//! Motor de LAYOUT — calcula a geometria (x, y, largura, altura) de cada nó e
//! emite uma DISPLAY LIST plana que o backend de render só PINTA. EGUI-FREE.
//!
//! Esta é a virada arquitetural decidida em 2026-06-27 ("processar tudo no DOM e
//! o egui só lê e exibe"): o `rts-dom` deixa de só guardar a árvore/estilo e passa
//! a CALCULAR onde cada caixa fica, seguindo a lógica do CSS (fluxo normal, box
//! model content-box). O `rts-egui` (ou qualquer backend futuro: web/png/canvas)
//! recebe a [`DisplayList`] pronta — uma lista de "pinte retângulo/texto em
//! (x,y,w,h)" — e só desenha. **O backend nunca decide layout.**
//!
//! ## Modelo (fluxo normal block, fase 1)
//!
//! - **Block empilha vertical**, cada caixa ocupando a largura do container por
//!   padrão (MDN CSS Flow Layout). `width` explícito (px/%) encolhe; `%` resolve
//!   contra o content-box do PAI (containing block), TARDE, aqui no layout.
//! - **Box model content-box** (MDN): `outer_w = margin + border + padding +
//!   content_w`. O `width` do CSS é a largura do CONTENT; padding/border/margin
//!   somam por fora.
//! - **Texto** é medido por um [`TextMeasurer`] (a largura/altura do glifo é o
//!   único dado que o `rts-dom` não tem sozinho — o backend mede; ver o trait).
//!   Fase 1 usa uma medida aproximada ([`ApproxMeasurer`]); o egui pluga a real.
//!
//! Cortes da fase 1 (aditivos depois): inline-flow rico multi-run, margin-collapse
//! pai-filho, `display:grid`, float/position. O objetivo da fatia é provar a
//! TUBULAÇÃO DOM→layout→display-list→paint com box model block.
//!
//! ## Flexbox (gap/justify-content/align-items) — cortes CONSCIENTES
//!
//! Implementado: `display:flex` (row) + `flex-wrap`, `gap`/`row-gap`/`column-gap`,
//! `justify-content` (todas as formas, fiel à CSS Box Alignment L3 incl. fallback
//! de overflow), `align-items` (flex-start/center/flex-end). Cortes documentados:
//! - **`align-items:stretch` NÃO estica de fato** — trata como flex-start (cada
//!   item mantém sua altura natural). Stretch é o DEFAULT do flex, então um card
//!   sem `align-items` explícito não preenche a altura da linha (o browser
//!   esticaria). Esticar real exige passar altura imposta ao `layout_block`
//!   (fase futura — ver `align_offset`).
//! - **`flex-direction` só Row** — `column`/`row-reverse`/`column-reverse` são
//!   parseados e guardados (cascade pronta) mas o layout SEMPRE dispõe em row. Uma
//!   fatia futura generaliza `layout_children_horizontal` por eixo (`column` =
//!   main vertical, justify no Y). `flex-grow`/`shrink`/`basis` também fora.

use crate::dom::{Dom, NodeIdx, NodeKind};
use crate::style::{ComputedStyle, ResolveCtx};

/// Um retângulo em coordenadas de conteúdo (a origem é o canto da área de render;
/// o backend soma seu próprio offset de tela ao pintar). Unidade: pontos (f32).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }
}

/// UM item da display list — uma instrução de pintura ATÔMICA e já posicionada. O
/// backend percorre a lista em ordem (a ordem É o z-order: o que vem depois pinta
/// por cima) e desenha cada item, sem nenhuma decisão de layout. Egui-free: cor é
/// `u32` RGBA, posição é `f32` — nenhum tipo de backend.
#[derive(Clone, PartialEq, Debug)]
pub enum DisplayItem {
    /// Retângulo preenchido (fundo de uma caixa). `radius` arredonda os cantos.
    SolidRect { rect: Rect, color: u32, radius: f32 },
    /// Borda (contorno) de uma caixa, espessura `width`, na cor dada.
    Border { rect: Rect, width: f32, color: u32, radius: f32 },
    /// Texto numa posição (canto superior-esquerdo). `mono` escolhe a família
    /// monoespaçada. O backend resolve a fonte/atlas; aqui só o necessário.
    Text { x: f32, y: f32, text: String, color: u32, size: f32, mono: bool, bold: bool },
    /// Começa a RECORTAR a um retângulo (scroll container interno): os itens
    /// seguintes, até o `EndClip`, só pintam DENTRO deste rect E são transladados por
    /// `(offset_x, offset_y)` (o quanto a região rolou). O backend aplica o clip
    /// (egui: `painter.with_clip_rect`) e soma o offset. `node` liga ao `ScrollRegion`
    /// (o backend injeta o offset aqui antes de pintar). Empilha — pode aninhar.
    BeginClip { rect: Rect, node: NodeIdx, offset_x: f32, offset_y: f32 },
    /// Fecha o clip mais recente, restaurando o anterior.
    EndClip,
}

/// Um CONTAINER ROLÁVEL interno (uma `<div>` com `overflow:auto/scroll` e tamanho
/// definido): o conteúdo é maior que a caixa, então o backend recorta no `visible`,
/// rola por um offset próprio e mostra barra(s) dentro dela. Produzido pelo layout,
/// consumido pelo backend. Distinto do scroll da PÁGINA (que é a viewport inteira).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ScrollRegion {
    /// Qual nó é o container (chave do offset por-região no backend).
    pub node_idx: NodeIdx,
    /// Rect VISÍVEL (content-box do container, coords de conteúdo da página).
    pub visible: Rect,
    /// Largura REAL do conteúdo (pode exceder `visible.w` → rola em X).
    pub content_w: f32,
    /// Altura REAL do conteúdo (pode exceder `visible.h` → rola em Y).
    pub content_h: f32,
    /// overflow de cada eixo (auto/scroll rolam; hidden corta; visible não recorta).
    pub overflow_x: crate::scrollbar::Overflow,
    pub overflow_y: crate::scrollbar::Overflow,
}

/// A saída do layout: a lista plana de itens de pintura, em z-order. É o ÚNICO
/// que o backend de render consome. Sem nenhuma referência à árvore — o layout já
/// consumiu a topologia (herança/cascade/box model) ao produzir esta lista.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct DisplayList {
    pub items: Vec<DisplayItem>,
    /// Altura total ocupada pelo conteúdo (para o backend dimensionar o scroll).
    pub content_height: f32,
    /// Geometria por NÓ (border-box, em coordenadas de conteúdo) — a base do
    /// `element.getBoundingClientRect()`/`offsetWidth`/etc. Preenchido durante o
    /// layout: cada bloco registra seu retângulo (margin EXCLUÍDA — border-box, como
    /// o `getBoundingClientRect` do browser). Nós inline/texto não entram (a API só
    /// dá rect de elementos; um inline teria múltiplos rects — fase futura).
    pub node_rects: std::collections::HashMap<NodeIdx, Rect>,
    /// Containers roláveis internos (divs com `overflow`) — o backend gerencia o
    /// offset de cada um e recorta. Vazio quando a página não tem scroll interno.
    pub scroll_regions: Vec<ScrollRegion>,
}

/// Abstração de MEDIÇÃO de texto (largura/altura de uma string num tamanho/peso).
/// Vive aqui (no `rts-dom`) e é IMPLEMENTADA pelo backend (o egui mede via galley);
/// reimplementar largura de glifo no `rts-dom` é a armadilha que o roadmap alertou.
/// O layout depende SÓ deste trait — continua egui-free e testável com um mock.
pub trait TextMeasurer {
    /// Largura em pontos de `text` renderizado em `size` (mono ou proporcional,
    /// regular ou `bold`). O peso importa: a fonte bold é mais larga — medir regular
    /// e pintar bold faz o texto estourar a linha (quebra a mais).
    fn text_width(&self, text: &str, size: f32, mono: bool, bold: bool) -> f32;
    /// Altura de UMA linha em `size` (line-height). Aproximação aceitável: `size *
    /// fator`; o backend pode dar o valor exato da fonte.
    fn line_height(&self, size: f32) -> f32;
}

/// Medidor APROXIMADO, sem backend — para teste e para o caminho headless puro
/// (gerar layout sem janela). Largura ≈ `n_chars * size * 0.5` (média de fonte
/// proporcional latina); altura ≈ `size * 1.3`. Não é exato (o egui dá o real),
/// mas é determinístico e suficiente para block-flow (onde a largura do texto não
/// decide a da caixa — a caixa ocupa o container).
pub struct ApproxMeasurer;

impl TextMeasurer for ApproxMeasurer {
    fn text_width(&self, text: &str, size: f32, mono: bool, bold: bool) -> f32 {
        let mut per = if mono { 0.6 } else { 0.5 };
        if bold {
            per *= 1.06; // bold ~6% mais largo.
        }
        text.chars().count() as f32 * size * per
    }
    fn line_height(&self, size: f32) -> f32 {
        size * 1.3
    }
}

/// Tamanho de fonte default (pontos) quando o estilo não especifica — base de
/// `em`/`rem` e do texto sem `font-size`. Casa com o default do render antigo.
pub const DEFAULT_FONT_SIZE: f32 = 20.0;

/// O contexto de uma passada de layout: o viewport (para `vw`/`vh` e largura
/// inicial) e o medidor de texto. Imutável durante a passada.
pub struct LayoutCtx<'a> {
    pub viewport_w: f32,
    pub viewport_h: f32,
    pub measurer: &'a dyn TextMeasurer,
}

/// Calcula o layout de um `Dom` inteiro e devolve a [`DisplayList`]. Ponto de
/// entrada do motor: percorre os filhos de `#document` como blocos empilhados na
/// largura do viewport, resolvendo box model e emitindo os itens de pintura.
pub fn layout_document(dom: &Dom, ctx: &LayoutCtx) -> DisplayList {
    let mut list = DisplayList::default();
    // PROPAGAÇÃO DO FUNDO do <body>/<html> (regra especial do CSS): o background
    // desses dois elementos "vaza" para o VIEWPORT inteiro, não só a caixa deles.
    // Pintamos PRIMEIRO (atrás de tudo) um retângulo do tamanho do viewport com a cor
    // do body. (Reserva uma altura generosa; o egui faz clip na sua área.)
    if let Some(bg) = body_background(dom) {
        let h = ctx.viewport_h.max(4000.0); // cobre bem além do conteúdo
        list.items.push(DisplayItem::SolidRect {
            rect: Rect::new(0.0, 0.0, ctx.viewport_w, h),
            color: bg,
            radius: 0.0,
        });
    }
    let mut cursor_y = 0.0f32;
    let root = dom.node(dom.root);
    for &child in &root.children {
        let (_, h) = layout_block(dom, child, 0.0, cursor_y, ctx.viewport_w, false, ctx, &mut list);
        cursor_y += h;
    }
    list.content_height = cursor_y;
    list
}

/// Emite os retângulos da SCROLLBAR (track + thumb) na DisplayList — a BARRA é
/// preparada pelo DOM, não pelo backend (o egui só pinta `SolidRect`, mantendo-se
/// burro e substituível). Dados de geometria: `viewport_w/h` (área visível),
/// `content_h` (altura total do conteúdo), `offset_y` (quanto já rolou). Estilo:
/// `sb` (cor/largura/radius do CSS). Só emite a barra VERTICAL (a horizontal segue
/// o mesmo molde quando precisar). Coordenadas em espaço de CONTEÚDO já rolado: a
/// barra é desenhada FIXA na viewport, então some o `offset_y` (o backend translada
/// o conteúdo por -offset; somar offset à barra a mantém na tela).
///
/// Não faz nada se o conteúdo cabe (sem overflow) e a barra não é forçada.
pub fn emit_scrollbar(
    list: &mut DisplayList,
    viewport_w: f32,
    viewport_h: f32,
    content_h: f32,
    offset_y: f32,
    sb: &crate::scrollbar::ScrollbarStyle,
    force: bool,
) {
    use crate::scrollbar::BarWidth;
    // precisa rolar? (conteúdo maior que a viewport) ou barra forçada (overflow:scroll).
    let overflow = content_h > viewport_h + 0.5;
    if !overflow && !force {
        return;
    }
    // largura da barra (px): thin=8, none=0 (não desenha), px direto, senão 12.
    let bar_w = match sb.width {
        Some(BarWidth::None) => return,
        Some(BarWidth::Thin) => 8.0,
        Some(BarWidth::Px(px)) => px,
        _ => 12.0,
    };
    // cores default fiéis a um browser escuro (sobrescritas pelo CSS).
    let track_color = sb.track.unwrap_or(0x1e1e1eff);
    let thumb_color = sb.thumb.unwrap_or(0x6b6b6bff);
    let radius = sb.thumb_radius.unwrap_or(bar_w / 2.0);
    let bar_x = viewport_w - bar_w;
    // o thumb: tamanho proporcional à fração visível; posição proporcional ao offset.
    let frac = (viewport_h / content_h).clamp(0.0, 1.0);
    let thumb_h = (viewport_h * frac).max(24.0); // mínimo p/ pegar com o mouse
    let max_off = (content_h - viewport_h).max(1.0);
    let scroll_frac = (offset_y / max_off).clamp(0.0, 1.0);
    let thumb_y = scroll_frac * (viewport_h - thumb_h);
    // FIXA na viewport: soma offset_y (o backend translada tudo por -offset).
    let vy = offset_y;
    // track (faixa direita inteira) — atrás do thumb.
    list.items.push(DisplayItem::SolidRect {
        rect: Rect::new(bar_x, vy, bar_w, viewport_h),
        color: track_color,
        radius: 0.0,
    });
    // thumb (handle).
    list.items.push(DisplayItem::SolidRect {
        rect: Rect::new(bar_x, vy + thumb_y, bar_w, thumb_h),
        color: thumb_color,
        radius,
    });
}

/// Emite as barras (x e/ou y) DENTRO de um scroll container interno (#1744), no rect
/// visível dele (coords de conteúdo da página). Diferente de `emit_scrollbar` (que é
/// a viewport): aqui as barras ficam nas bordas da DIV. Emitidas APÓS o `EndClip`
/// (fora do recorte), então não rolam — ficam fixas na div. `offset_*` é o quanto a
/// região rolou (posiciona o thumb).
pub fn emit_scrollbar_in(
    list: &mut DisplayList,
    region: &ScrollRegion,
    offset_x: f32,
    offset_y: f32,
    sb: &crate::scrollbar::ScrollbarStyle,
) {
    use crate::scrollbar::BarWidth;
    let bar_w = match sb.width {
        Some(BarWidth::None) => return,
        Some(BarWidth::Thin) => 8.0,
        Some(BarWidth::Px(px)) => px,
        _ => 12.0,
    };
    let track_color = sb.track.unwrap_or(0x1e1e1eff);
    let thumb_color = sb.thumb.unwrap_or(0x6b6b6bff);
    let radius = sb.thumb_radius.unwrap_or(bar_w / 2.0);
    let v = region.visible;
    let need_y = region.overflow_y.scrollable() && region.content_h > v.h + 0.5;
    let need_x = region.overflow_x.scrollable() && region.content_w > v.w + 0.5;
    // barra VERTICAL (borda direita da div).
    if need_y {
        let track_h = if need_x { v.h - bar_w } else { v.h };
        let frac = (track_h / region.content_h).clamp(0.0, 1.0);
        let thumb_h = (track_h * frac).max(24.0);
        let max_off = (region.content_h - v.h).max(1.0);
        let thumb_y = (offset_y / max_off).clamp(0.0, 1.0) * (track_h - thumb_h);
        let bx = v.x + v.w - bar_w;
        list.items.push(DisplayItem::SolidRect { rect: Rect::new(bx, v.y, bar_w, track_h), color: track_color, radius: 0.0 });
        list.items.push(DisplayItem::SolidRect { rect: Rect::new(bx, v.y + thumb_y, bar_w, thumb_h), color: thumb_color, radius });
    }
    // barra HORIZONTAL (borda inferior da div).
    if need_x {
        let track_w = if need_y { v.w - bar_w } else { v.w };
        let frac = (track_w / region.content_w).clamp(0.0, 1.0);
        let thumb_w = (track_w * frac).max(24.0);
        let max_off = (region.content_w - v.w).max(1.0);
        let thumb_x = (offset_x / max_off).clamp(0.0, 1.0) * (track_w - thumb_w);
        let by = v.y + v.h - bar_w;
        list.items.push(DisplayItem::SolidRect { rect: Rect::new(v.x, by, track_w, bar_w), color: track_color, radius: 0.0 });
        list.items.push(DisplayItem::SolidRect { rect: Rect::new(v.x + thumb_x, by, thumb_w, bar_w), color: thumb_color, radius });
    }
}

/// O `background` do `<body>` (ou, se ausente, do `<html>`) — a cor que o CSS
/// PROPAGA para o viewport inteiro. `None` se nenhum dos dois tem fundo.
fn body_background(dom: &Dom) -> Option<u32> {
    // procura body e html entre os descendentes da raiz.
    for &child in &dom.node(dom.root).children {
        if let Some(bg) = bg_of_tag(dom, child, "body") {
            return Some(bg);
        }
        if let Some(bg) = bg_of_tag(dom, child, "html") {
            // o html pode ter o body dentro; tenta o body primeiro.
            if let Some(body_bg) = find_body_bg(dom, child) {
                return Some(body_bg);
            }
            return Some(bg);
        }
    }
    None
}

/// O bg de `idx` se sua tag é `tag` e tem background computado.
fn bg_of_tag(dom: &Dom, idx: NodeIdx, tag: &str) -> Option<u32> {
    match &dom.node(idx).kind {
        NodeKind::Element { tag: t } if t == tag => dom.computed_style_idx(idx).and_then(|c| c.bg),
        _ => None,
    }
}

/// Procura um `<body>` com bg na subárvore de `idx` (ex: html>body).
fn find_body_bg(dom: &Dom, idx: NodeIdx) -> Option<u32> {
    for &child in &dom.node(idx).children {
        if let Some(bg) = bg_of_tag(dom, child, "body") {
            return Some(bg);
        }
    }
    None
}

/// O retângulo (border-box) de um nó, computando o layout do documento na largura
/// dada — a base de `element.getBoundingClientRect()`. `None` se o nó não é um
/// bloco renderável (texto/inline/`display:none`/metadata não têm rect próprio).
/// Roda o layout inteiro (O(n)); para várias consultas no mesmo frame, reuse a
/// `DisplayList` de `layout_document` e leia `node_rects` direto.
pub fn bounding_rect(dom: &Dom, node: NodeIdx, ctx: &LayoutCtx) -> Option<Rect> {
    layout_document(dom, ctx).node_rects.get(&node).copied()
}

/// Faz o layout de UM nó-bloco a partir de `(x, y)`, com `avail_w` de largura
/// disponível (a do container). Emite os itens (fundo/borda/texto/filhos) na
/// `list` e devolve o TAMANHO EXTERNO `(outer_w, outer_h)` da caixa (incluindo
/// padding/border/margin) — o pai usa a altura (empilhamento vertical) ou a
/// largura (horizontal) para posicionar o irmão seguinte. Texto solto e nós inline
/// são desenhados como linhas dentro do content-box.
fn layout_block(
    dom: &Dom,
    id: NodeIdx,
    x: f32,
    y: f32,
    avail_w: f32,
    // `shrink_to_fit`: quando true, um bloco SEM `width` explícito dimensiona pela
    // largura do CONTEÚDO (como `inline-block`/item flex), não ocupa a largura
    // disponível. É o que faz badges num container horizontal não esticarem para a
    // linha toda. No fluxo vertical normal é false (block ocupa a largura — MDN).
    shrink_to_fit: bool,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> (f32, f32) {
    // Nós não-elemento no nível de bloco (texto solto, comentário): trata o texto
    // como uma linha; comentário não pinta.
    let css = match &dom.node(id).kind {
        NodeKind::Element { tag } => {
            // Metadata não-renderável (`<head>` e seu conteúdo, `<style>`,
            // `<script>`): pula a subárvore inteira — não pinta nada. Permite
            // carregar um HTML COMPLETO (com <head><title><meta>) e renderizar só
            // o que é visível (o <body> e seus filhos).
            if is_non_rendered_tag(tag) {
                return (0.0, 0.0);
            }
            let css = dom.computed_style_idx(id).unwrap_or_default();
            // `display:none` — não renderiza nem ocupa espaço (some da árvore visual).
            if css.effective_display() == Some(crate::style::DisplayKind::None) {
                return (0.0, 0.0);
            }
            css
        }
        NodeKind::Text(t) => {
            let size = DEFAULT_FONT_SIZE;
            let lh = ctx.measurer.line_height(size);
            let tw = ctx.measurer.text_width(t, size, false, false);
            list.items.push(DisplayItem::Text {
                x,
                y,
                text: t.clone(),
                color: 0x000000FF,
                size,
                mono: false,
                bold: false,
            });
            return (tw, lh);
        }
        _ => return (0.0, 0.0), // Comment / Document aninhado: não pinta.
    };

    // ── Box model (content-box): resolve as bordas/espaços absolutos ─────────────
    // Margin/padding POR LADO (Edges). O `margin_v` (UA-stylesheet, só vertical) é
    // somado ao top/bottom. `margin_left/right` deslocam no x; o vertical empilha.
    let m = &css.margin;
    let p = &css.padding;
    let mut margin_left = m.left.px().unwrap_or(0.0);
    let mut margin_right = m.right.px().unwrap_or(0.0);
    let margin_v_extra = css.margin_v.unwrap_or(0.0);
    let margin_top = m.top.px().unwrap_or(0.0) + margin_v_extra;
    let margin_bottom = m.bottom.px().unwrap_or(0.0) + margin_v_extra;
    let pad_left = p.left.px().unwrap_or(0.0);
    let pad_right = p.right.px().unwrap_or(0.0);
    let pad_top = p.top.px().unwrap_or(0.0);
    let pad_bottom = p.bottom.px().unwrap_or(0.0);
    let border = css.border_width.unwrap_or(0.0);
    // Atalhos para o eixo (horizontal = left+right): a maioria do box model usa o
    // total por eixo. (`margin_h`/`padding_h` = soma do eixo horizontal.)
    let margin_h = margin_left + margin_right;
    let padding_h = pad_left + pad_right;

    // Largura do CONTENT: `width` explícito (px/% resolvido contra `avail_w`), ou
    // o que sobra do container menos margin+border+padding dos dois lados (block
    // ocupa a largura disponível por padrão — MDN flow layout).
    let resolve = ResolveCtx {
        parent_content_w: avail_w,
        node_font_size: css.font_size.unwrap_or(DEFAULT_FONT_SIZE),
        root_font_size: DEFAULT_FONT_SIZE,
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    // `frame` horizontal = o que cerca o content no eixo X (margin+border+padding
    // dos DOIS lados). border conta 2× (left+right); padding/margin já são a soma.
    let frame = margin_h + 2.0 * border + padding_h;
    let font_for_content = css.font_size.unwrap_or(DEFAULT_FONT_SIZE);
    let border_box = css.border_box.unwrap_or(false);
    let content_w = match css.width.and_then(|d| d.resolve(&resolve)) {
        // `width` explícito. Em `border-box`, o `width` INCLUI padding+border —
        // então o content é `width - (padding_h + 2*border)`. Em content-box
        // (default), o `width` JÁ é o content.
        Some(w) if border_box => (w - (padding_h + 2.0 * border)).max(0.0),
        Some(w) => w,
        // Sem width: shrink-to-fit → largura do conteúdo (limitada ao disponível);
        // senão (fluxo block normal) → ocupa a largura disponível.
        None if shrink_to_fit => {
            content_natural_width(dom, id, font_for_content, ctx).min((avail_w - frame).max(0.0))
        }
        None => (avail_w - frame).max(0.0),
    };
    // CLAMP min/max-width (#1751): `used = clamp(min, width, max)`. min/max são sobre
    // a CAIXA (border-box) na spec — descontamos o frame p/ aplicar ao content quando
    // border-box; em content-box o min/max já são do content. (aprox: aplicamos ao
    // content, descontando pad+border só no border-box.)
    let mnw = css.min_width.and_then(|d| d.resolve(&resolve)).map(|v| {
        if border_box { (v - (padding_h + 2.0 * border)).max(0.0) } else { v }
    });
    let mxw = css.max_width.and_then(|d| d.resolve(&resolve)).map(|v| {
        if border_box { (v - (padding_h + 2.0 * border)).max(0.0) } else { v }
    });
    let content_w = crate::style::clamp_size(content_w, mnw, mxw);

    // `margin: 0 auto` (#1745): se o margin-left/right é `auto` E o bloco tem largura
    // definida (não ocupa o pai inteiro), o espaço livre se distribui pelos lados
    // auto — centralizando (ambos auto) ou empurrando (um só auto). Resolvido AQUI,
    // depois de saber o content_w. Só quando há largura explícita (senão o bloco já
    // ocupa avail_w e não há espaço a distribuir).
    let has_width = css.width.is_some() || css.max_width.is_some();
    if has_width {
        let box_outer = content_w + padding_h + 2.0 * border; // sem a margin
        let free = (avail_w - box_outer).max(0.0);
        match (m.left.is_auto(), m.right.is_auto()) {
            (true, true) => {
                margin_left = free / 2.0;
                margin_right = free / 2.0;
            }
            (true, false) => margin_left = (free - margin_right).max(0.0),
            (false, true) => margin_right = (free - margin_left).max(0.0),
            (false, false) => {}
        }
    }

    // Posição do content-box (canto sup-esq): deslocado pelo lado ESQUERDO/TOPO
    // (margin+border+padding daquele lado), não a soma do eixo.
    let content_x = x + margin_left + border + pad_left;
    let content_y = y + margin_top + border + pad_top;

    // Z-ORDER: o fundo/borda da caixa precisam ficar ATRÁS dos filhos. Como a
    // display list é pintada em ordem, reservamos AGORA o índice onde a caixa será
    // inserida (antes de qualquer filho), descemos nos filhos (que dão append no
    // fim), e só DEPOIS — conhecendo a altura — inserimos o fundo nesse índice.
    let box_index = list.items.len();

    // ── Filhos: o EIXO depende do `display` do bloco ─────────────────────────────
    // vertical (default): cada filho ABAIXO do anterior, ocupando a largura.
    // horizontal (`display:horizontal`/flex-row): cada filho À DIREITA do anterior,
    // a altura do content = a do filho mais alto (MDN flow: inline-axis stacking).
    let display = css_display(dom, id);
    let font_size = css.font_size.unwrap_or(DEFAULT_FONT_SIZE);

    // SCROLL CONTAINER (#1744): uma div com `overflow-x:auto/scroll` NÃO comprime os
    // filhos — eles transbordam e a div rola. Nesse caso layoutamos os filhos com a
    // largura NATURAL do conteúdo (intrinsic), não a do container. (overflow-y já não
    // comprime: o vertical empilha e a altura é a soma — só precisamos do clip+barra.)
    let ov_x = css.overflow_x.unwrap_or(crate::scrollbar::Overflow::Visible);
    let ov_y = css.overflow_y.unwrap_or(crate::scrollbar::Overflow::Visible);
    let scrolls_x = ov_x.scrollable() || ov_x == crate::scrollbar::Overflow::Hidden;
    let children_w = if scrolls_x {
        // largura que o conteúdo QUER (sem comprimir) — pode exceder content_w.
        intrinsic_content_width(dom, id, font_size, ctx).max(content_w)
    } else {
        content_w
    };

    let content_h = match display {
        // horizontal (flex-row sem wrap): lado a lado, encolhe pra caber, não quebra.
        d if d == crate::block::DISPLAY_HORIZONTAL => {
            layout_children_horizontal(dom, id, content_x, content_y, children_w, &css, font_size, false, ctx, list)
        }
        // wrap (inline-block flow): lado a lado E QUEBRA linha quando enche.
        d if d == crate::block::DISPLAY_WRAP => {
            layout_children_horizontal(dom, id, content_x, content_y, children_w, &css, font_size, true, ctx, list)
        }
        // vertical (block): empilha.
        _ => layout_children_vertical(dom, id, content_x, content_y, children_w, &css, font_size, ctx, list),
    };
    // a altura REAL do conteúdo (antes de `height` explícito a cortar) — p/ o scroll-Y.
    let content_h_natural = content_h;

    // `height` explícito SOBRESCREVE a altura do conteúdo (a caixa tem essa altura,
    // mesmo que o conteúdo seja menor). Em border-box, o height inclui pad+border —
    // o content_h é o height menos pad_v+2border. Em content-box, height JÁ é o content.
    let content_h = match css.height.and_then(|d| d.resolve(&resolve)) {
        Some(h) if border_box => (h - (pad_top + pad_bottom + 2.0 * border)).max(0.0),
        Some(h) => h,
        None => content_h,
    };
    // CLAMP min/max-height (#1751): used = clamp(min, height, max).
    let frame_v = pad_top + pad_bottom + 2.0 * border;
    let mnh = css.min_height.and_then(|d| d.resolve(&resolve)).map(|v| {
        if border_box { (v - frame_v).max(0.0) } else { v }
    });
    let mxh = css.max_height.and_then(|d| d.resolve(&resolve)).map(|v| {
        if border_box { (v - frame_v).max(0.0) } else { v }
    });
    let content_h = crate::style::clamp_size(content_h, mnh, mxh);

    // ── Insere a CAIXA (fundo + borda) no índice reservado, ATRÁS dos filhos ─────
    // O BORDER-BOX do nó: content + padding + border (NÃO a margin — esta é espaço
    // externo). É o retângulo que `getBoundingClientRect()` reporta.
    let box_rect = Rect::new(
        x + margin_left,
        y + margin_top,
        content_w + padding_h + 2.0 * border,
        content_h + pad_top + pad_bottom + 2.0 * border,
    );
    // Registra a geometria deste nó (base do getBoundingClientRect/offsetWidth).
    list.node_rects.insert(id, box_rect);

    // Pinta a CAIXA (fundo/borda) ATRÁS dos filhos. `insert` no `box_index` põe o
    // fundo antes dos itens dos filhos (z-order).
    if css.has_box() {
        let radius = css.corner_radius.unwrap_or(0.0);
        // Insere na ordem: primeiro o fundo, depois a borda por cima dele (ambos
        // atrás dos filhos). `insert` desloca os filhos para a frente.
        let mut at = box_index;
        if let Some(color) = css.bg {
            list.items.insert(at, DisplayItem::SolidRect { rect: box_rect, color, radius });
            at += 1;
        }
        // A borda só pinta se tem largura E um `border-style` VISÍVEL. O default
        // CSS de border-style é `none` → sem `border-style` declarado, NÃO pinta
        // (fiel ao Chrome: `border-width:2px` sozinho dá borda invisível).
        let style_visible = css.border_style.map(|s| s.is_visible()).unwrap_or(false);
        if border > 0.0 && style_visible {
            let color = css.border_color.unwrap_or(0x808080FF);
            list.items.insert(at, DisplayItem::Border { rect: box_rect, width: border, color, radius });
        }
    }

    // ── SCROLL CONTAINER interno (#1744): se a div rola (overflow-x/y) e o conteúdo
    // excede a caixa, (1) RECORTA os itens dos filhos ao content-box (BeginClip já
    // emitido depois da caixa, EndClip no fim), (2) registra a ScrollRegion p/ o
    // backend gerenciar o offset + pintar as barras. `hidden` também recorta (corta o
    // excesso, sem barra). `visible` não faz nada (transborda, como hoje).
    let clips = ov_x != crate::scrollbar::Overflow::Visible
        || ov_y != crate::scrollbar::Overflow::Visible;
    if clips {
        let content_rect = Rect::new(content_x, content_y, content_w, content_h);
        // BeginClip no índice onde os FILHOS começam (logo após os itens de caixa que
        // foram inseridos em `box_index`); EndClip no fim. Quantos itens de caixa:
        // fundo (se bg) + borda (se visível).
        let style_visible = css.border_style.map(|s| s.is_visible()).unwrap_or(false);
        let box_items = if css.has_box() {
            css.bg.is_some() as usize + (border > 0.0 && style_visible) as usize
        } else {
            0
        };
        let children_start = box_index + box_items;
        // offset 0 aqui; o backend injeta o offset rolado por região antes de pintar.
        list.items.insert(
            children_start,
            DisplayItem::BeginClip { rect: content_rect, node: id, offset_x: 0.0, offset_y: 0.0 },
        );
        list.items.push(DisplayItem::EndClip);
        // só registra como rolável (com barra) se de fato rola (auto/scroll), não hidden.
        if ov_x.scrollable() || ov_y.scrollable() {
            list.scroll_regions.push(ScrollRegion {
                node_idx: id,
                visible: content_rect,
                content_w: children_w.max(content_w),
                content_h: content_h_natural,
                overflow_x: ov_x,
                overflow_y: ov_y,
            });
        }
    }

    // Tamanho EXTERNO da caixa (outer = content + padding + border + margin) — cada
    // componente já é a SOMA do seu eixo (padding_h = left+right; margin_h idem;
    // border conta 2× pelos dois lados). Não multiplicar margin/padding por 2.
    let outer_w = content_w + padding_h + 2.0 * border + margin_h;
    let outer_h = content_h + pad_top + pad_bottom + 2.0 * border + margin_top + margin_bottom;
    (outer_w, outer_h)
}

/// Largura NATURAL do conteúdo de um nó (sem `width` explícito): a maior largura
/// de uma linha de texto entre os descendentes. É o "preferred width" do
/// shrink-to-fit (item flex / inline-block). Para um filho-bloco com `width`, usa
/// esse width (+ frame); para texto, a largura medida. Aproximação do max-content
/// (o inline-flow exato — palavras quebrando — vem na fatia de inline).
fn content_natural_width(dom: &Dom, id: NodeIdx, font: f32, ctx: &LayoutCtx) -> f32 {
    intrinsic_content_width(dom, id, font, ctx)
}

/// LARGURA INTRÍNSECA do CONTEÚDO de um elemento (max-content): quanto o conteúdo
/// QUER de largura sem quebrar. É a BASE de toda medição (shrink-to-fit, item flex,
/// inline-block, container flex). CONSCIENTE DO DISPLAY dos filhos:
/// - flex-ROW (horizontal/wrap): SOMA as larguras outer dos filhos + os gaps (eles
///   ficam lado a lado). Era o bug do navbar: `.logo`/`.links` (flex) mediam pelo
///   MAX, dando ~0.
/// - block (vertical): MAX das larguras dos filhos (empilham).
/// - texto: a largura do texto concatenado.
/// Recursivo: a largura de um filho é a SUA intrínseca + frame (ou seu `width` fixo).
fn intrinsic_content_width(dom: &Dom, id: NodeIdx, font: f32, ctx: &LayoutCtx) -> f32 {
    // folha de texto puro → largura do texto.
    let own_text = collect_text(dom, id);
    let only_text = !dom.node(id).children.is_empty()
        && dom.node(id).children.iter().all(|&c| matches!(dom.node(c).kind, NodeKind::Text(_)));
    if (dom.node(id).children.is_empty() || only_text) && !own_text.trim().is_empty() {
        let css = dom.computed_style_idx(id);
        let mono = css
            .as_ref()
            .and_then(|c| c.font_family.as_ref())
            .map(|f| crate::style::is_mono_family(f))
            .unwrap_or(false);
        // o peso importa p/ a largura natural: medir regular mas o wrap/paint usar bold
        // (mais largo) faz o conteúdo não caber na largura natural → quebra indevida.
        let bold = css.as_ref().and_then(|c| c.bold).unwrap_or(false);
        return ctx.measurer.text_width(&own_text, font, mono, bold);
    }

    // o EIXO em que os filhos se dispõem decide SOMA vs MAX.
    let display = css_display(dom, id);
    let is_row = display == crate::block::DISPLAY_HORIZONTAL || display == crate::block::DISPLAY_WRAP;
    let gap = if is_row {
        let resolve = ResolveCtx {
            parent_content_w: ctx.viewport_w,
            node_font_size: font,
            root_font_size: DEFAULT_FONT_SIZE,
            viewport_w: ctx.viewport_w,
            viewport_h: ctx.viewport_h,
        };
        dom.computed_style_idx(id)
            .and_then(|c| c.gap)
            .and_then(|d| d.resolve(&resolve))
            .unwrap_or(0.0)
            .max(0.0)
    } else {
        0.0
    };

    let mut sum = 0.0f32;
    let mut max = 0.0f32;
    let mut count: usize = 0;
    for &child in &dom.node(id).children {
        let w = intrinsic_outer_width(dom, child, font, ctx);
        if w > 0.0 {
            count += 1;
        }
        sum += w;
        max = max.max(w);
    }
    if is_row {
        // soma + gaps entre os itens.
        sum + (count.saturating_sub(1)) as f32 * gap
    } else {
        max
    }
}

/// A largura OUTER intrínseca de UM filho (max-content): seu `width` fixo (+ frame),
/// senão a intrínseca do seu conteúdo (+ frame). Texto → largura do texto.
fn intrinsic_outer_width(dom: &Dom, id: NodeIdx, parent_font: f32, ctx: &LayoutCtx) -> f32 {
    match &dom.node(id).kind {
        NodeKind::Element { .. } => {
            // metadata (head/style/script) não conta.
            if let NodeKind::Element { tag } = &dom.node(id).kind {
                if is_non_rendered_tag(tag) {
                    return 0.0;
                }
            }
            let css = dom.computed_style_idx(id).unwrap_or_default();
            let f = css.font_size.unwrap_or(parent_font);
            let border_box = css.border_box.unwrap_or(false);
            let frame = css.margin.horizontal_px() + 2.0 * css.border_width.unwrap_or(0.0) + css.padding.horizontal_px();
            let resolve = ResolveCtx {
                parent_content_w: ctx.viewport_w,
                node_font_size: f,
                root_font_size: DEFAULT_FONT_SIZE,
                viewport_w: ctx.viewport_w,
                viewport_h: ctx.viewport_h,
            };
            // width fixo: a caixa tem essa largura.
            if let Some(w) = css.width.and_then(|d| d.resolve(&resolve)) {
                return if border_box { w + css.margin.horizontal_px() } else { w + frame };
            }
            // senão: a intrínseca do conteúdo + frame.
            intrinsic_content_width(dom, id, f, ctx) + frame
        }
        NodeKind::Text(t) => ctx.measurer.text_width(t, parent_font, false, false),
        _ => 0.0,
    }
}

/// `true` se um nó-elemento deve ser tratado como BLOCO no layout (entra em
/// `layout_block`, com sua própria caixa/eixo) — em vez de inline (texto corrido).
/// É bloco se: tem `display` no CSS (qualquer um define caixa própria), OU tem um
/// default de display registrado (`block::lookup` = defineBlock, alimentado pela
/// UA-stylesheet `ua.ts` para div/p/… e pelo autor). Tags inline puras (sem nada
/// disso) fluem como texto. O motor NÃO nomeia tags HTML — os defaults são dados
/// do prelude TS.
fn is_block_level(dom: &Dom, id: NodeIdx) -> bool {
    match &dom.node(id).kind {
        NodeKind::Element { tag } => {
            let css = dom.computed_style_idx(id);
            css.as_ref().and_then(|c| c.effective_display()).is_some()
                || crate::block::lookup(tag).is_some()
                // INLINE-BLOCK de fato: um elemento inline (`<a>`/`<span>`/`<button>`)
                // que tem CAIXA própria (fundo/borda/padding/width/height) precisa de
                // layout_block p/ pintar essa caixa e respeitar o padding — senão o
                // botão fica sem fundo/borda. (`has_box` cobre bg/pad/margin/border/
                // radius/width; +height.)
                || css.as_ref().map(|c| c.has_box() || c.height.is_some()).unwrap_or(false)
        }
        _ => false,
    }
}

/// `true` se o elemento é INLINE-BLOCK: tem caixa (vira bloco p/ pintar) MAS é inline
/// por natureza (`<a>`/`<span>`/`<button>`/etc., não uma tag block) e SEM width que
/// ocupe o pai → dimensiona pelo CONTEÚDO (shrink-to-fit), como o pill/botão. Tags
/// block conhecidas (div/p/section…) NÃO são inline-block (ocupam o pai).
fn is_inline_block(dom: &Dom, id: NodeIdx) -> bool {
    match &dom.node(id).kind {
        NodeKind::Element { tag } => {
            // tag block conhecida OU display de bloco explícito → NÃO é inline-block.
            let css = dom.computed_style_idx(id);
            let explicit_block = css
                .as_ref()
                .and_then(|c| c.effective_display())
                .map(|d| d != crate::style::DisplayKind::Inline)
                .unwrap_or(false);
            if crate::block::lookup(tag).is_some() || explicit_block {
                return false;
            }
            // é inline-com-box (tem caixa mas é tag inline) → inline-block.
            css.as_ref().map(|c| c.has_box() || c.height.is_some()).unwrap_or(false)
        }
        _ => false,
    }
}

/// `true` se a tag NÃO é renderável — metadata do documento (`<head>` e o que vive
/// nele: `<title>`, `<meta>`, `<link>`, `<base>`) e os recursos `<style>`/`<script>`
/// (o CSS já virou stylesheet no parse; JS não executamos). Permite carregar um HTML
/// COMPLETO e pintar só o conteúdo visível (`<body>`). `<html>`/`<body>` SÃO
/// renderáveis (transparentes — fluxo block normal dos filhos).
fn is_non_rendered_tag(tag: &str) -> bool {
    matches!(tag, "head" | "title" | "meta" | "link" | "base" | "style" | "script")
}

/// O código de `display` de um nó: o CSS (`display:` parseado) VENCE; se não
/// declarado, cai no default da tag (`block::lookup`, a UA-stylesheet via
/// defineBlock); senão vertical. É o eixo de empilhamento dos filhos.
/// Códigos: 0=vertical/block, 1=wrap, 2=horizontal/flex, -1=none.
fn css_display(dom: &Dom, id: NodeIdx) -> i64 {
    match &dom.node(id).kind {
        NodeKind::Element { tag } => {
            // 1) CSS explícito (display:flex/block/inline/none) tem prioridade.
            if let Some(css) = dom.computed_style_idx(id) {
                if let Some(kind) = css.effective_display() {
                    return kind.to_display_code();
                }
            }
            // 2) default da tag: defineBlock (UA-stylesheet do TS) tem prioridade;
            // senão as tags HTML block conhecidas (div/p/…) são vertical; o resto
            // também cai em vertical (default seguro para um container).
            crate::block::lookup(tag).map(|d| d.display).unwrap_or(crate::block::DISPLAY_VERTICAL)
        }
        _ => crate::block::DISPLAY_VERTICAL,
    }
}

/// Empilha os filhos VERTICAL (cada um abaixo do anterior), ocupando a largura do
/// content. Devolve a altura TOTAL do content (soma das alturas dos filhos).
fn layout_children_vertical(
    dom: &Dom,
    id: NodeIdx,
    content_x: f32,
    content_y: f32,
    content_w: f32,
    css: &ComputedStyle,
    font_size: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    let mut child_y = content_y;
    // MARGIN-COLLAPSE (versão simples, fiel ao caso comum): margins verticais de
    // blocos ADJACENTES colapsam para o MAIOR, não somam (regra do CSS). Como o
    // `outer_h` de cada bloco já inclui seu margin nos dois lados, ao empilhar dois
    // blocos a soma conta `margin_bottom_anterior + margin_top_atual`; subtraímos o
    // overlap = min(dos dois) para virar max(dos dois). `prev_margin` rastreia o
    // margin do último bloco posto.
    let mut prev_margin = 0.0f32;
    for &child in &dom.node(id).children {
        match &dom.node(child).kind {
            // Metadata não-renderável (`<head>`/`<title>`/`<style>`/`<script>`):
            // pula — NÃO coleta seu texto como inline (senão o título e o CSS cru
            // vazam pra tela). Checado ANTES do caminho inline.
            NodeKind::Element { tag } if is_non_rendered_tag(tag) => {}
            NodeKind::Element { .. } if is_block_level(dom, child) => {
                // margin VERTICAL TOP do filho (para o collapse com o anterior):
                // margin.top + margin_v da UA.
                let m = dom.computed_style_idx(child)
                    .map(|c| c.margin.top.px().unwrap_or(0.0) + c.margin_v.unwrap_or(0.0))
                    .unwrap_or(0.0);
                // Colapsa com o bloco anterior: recua o overlap antes de posicionar.
                child_y -= prev_margin.min(m);
                // INLINE-BLOCK (pill/botão solto): dimensiona pelo conteúdo (shrink) e
                // posiciona conforme o text-align do PAI (center/right desloca o x).
                let inline_block = is_inline_block(dom, child);
                let child_x = if inline_block {
                    // mede a largura desejada (shrink) numa lista descartável p/ achar
                    // o offset do text-align ANTES de pintar de verdade.
                    let mut scratch = DisplayList::default();
                    let (w, _) = layout_block(dom, child, content_x, child_y, content_w, true, ctx, &mut scratch);
                    let free = (content_w - w).max(0.0);
                    match css.text_align {
                        Some(crate::style::TextAlign::Center) => content_x + free / 2.0,
                        Some(crate::style::TextAlign::Right) => content_x + free,
                        _ => content_x,
                    }
                } else {
                    content_x
                };
                let (_, h) = layout_block(dom, child, child_x, child_y, content_w, inline_block, ctx, list);
                child_y += h;
                prev_margin = m;
            }
            _ => {
                child_y = layout_inline_line(dom, child, content_x, child_y, content_w, css, font_size, ctx, list);
                prev_margin = 0.0; // texto inline quebra a sequência de collapse.
            }
        }
    }
    (child_y - content_y).max(0.0)
}

/// Um item medido do flex (pré-pass), com a referência ao nó e seu tamanho OUTER
/// desejado nos dois eixos (para justify no main e align no cross).
struct FlexItem {
    node: NodeIdx,
    /// largura outer desejada (eixo principal em row).
    w: f32,
    /// altura outer desejada (eixo cruzado em row).
    h: f32,
    /// `true` se é um nó de texto solto (pintado direto, não via layout_block).
    is_text: bool,
}

/// Dispõe os filhos HORIZONTAL (flex-row). Implementa gap, justify-content (eixo
/// principal) e align-items (eixo cruzado). Devolve a altura total do content.
///
/// - `wrap = false` (flex sem wrap): tudo numa linha; justify distribui o espaço
///   livre; em overflow, cai para flex-start (transborda no fim).
/// - `wrap = true` (inline-block/flex-wrap): quebra para a próxima linha quando não
///   cabe; justify/align aplicam POR LINHA.
fn layout_children_horizontal(
    dom: &Dom,
    id: NodeIdx,
    content_x: f32,
    content_y: f32,
    content_w: f32,
    css: &ComputedStyle,
    font_size: f32,
    wrap: bool,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    // gap/row-gap resolvidos do CSS (px/%/… contra o content do container).
    let resolve = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font_size,
        root_font_size: DEFAULT_FONT_SIZE,
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let gap = css.gap.and_then(|d| d.resolve(&resolve)).unwrap_or(0.0).max(0.0);
    let row_gap = css.row_gap.and_then(|d| d.resolve(&resolve)).unwrap_or(0.0).max(0.0);
    let justify = css.justify.unwrap_or(crate::style::JustifyContent::FlexStart);
    let align = css.align_items.unwrap_or(crate::style::AlignItems::Stretch);
    // altura do CONTENT do container (se `height` explícito) — referência do cross-axis
    // para align-items numa linha única. `0` = sem height (usa o max dos itens).
    let container_cross_h = match css.height.and_then(|d| d.resolve(&resolve)) {
        Some(h) if css.border_box.unwrap_or(false) => {
            (h - (css.padding.vertical_px() + 2.0 * css.border_width.unwrap_or(0.0))).max(0.0)
        }
        Some(h) => h,
        None => 0.0,
    };

    // ── PRÉ-PASS: mede cada filho renderável e agrupa em LINHAS (wrap) ────────────
    let mut lines: Vec<Vec<FlexItem>> = vec![Vec::new()];
    let mut line_w = 0.0f32; // largura consumida da linha atual (itens + gaps)
    for &child in &dom.node(id).children {
        if let NodeKind::Element { tag } = &dom.node(child).kind {
            if is_non_rendered_tag(tag) {
                continue;
            }
        }
        let is_block = is_block_level(dom, child);
        if !is_block {
            // texto solto: largura medida; vazio é ignorado.
            let text = collect_text(dom, child);
            if text.trim().is_empty() {
                continue;
            }
            let w = ctx.measurer.text_width(&text, font_size, false, false);
            let h = ctx.measurer.line_height(font_size);
            let cur = lines.last_mut().unwrap();
            let with_gap = if cur.is_empty() { 0.0 } else { gap };
            if wrap && !cur.is_empty() && line_w + with_gap + w > content_w {
                lines.push(Vec::new());
                line_w = w;
            } else {
                line_w += with_gap + w;
            }
            lines.last_mut().unwrap().push(FlexItem { node: child, w, h, is_text: true });
            continue;
        }
        let w = child_outer_width(dom, child, content_w, font_size, ctx);
        let h = child_outer_height(dom, child, content_w, font_size, ctx);
        let cur = lines.last_mut().unwrap();
        let with_gap = if cur.is_empty() { 0.0 } else { gap };
        if wrap && !cur.is_empty() && line_w + with_gap + w > content_w {
            lines.push(Vec::new());
            line_w = w;
        } else {
            line_w += with_gap + w;
        }
        lines.last_mut().unwrap().push(FlexItem { node: child, w, h, is_text: false });
    }

    // ── POSICIONAMENTO: por linha, aplica justify (main) + align (cross) ─────────
    let mut line_y = content_y;
    for line in &lines {
        if line.is_empty() {
            continue;
        }
        let n = line.len();
        let sum_w: f32 = line.iter().map(|it| it.w).sum();
        let total_gap = (n.saturating_sub(1)) as f32 * gap;
        let free = content_w - sum_w - total_gap;
        // Cross-size de referência da linha = max das alturas dos itens, MAS se o
        // container tem `height` explícito e a linha é única (no-wrap), o cross-size
        // é a ALTURA DO CONTENT do container (fiel ao Chrome: align-items:center num
        // bar height:60 com botões height:40 → botão em y=10, não y=0). Em wrap, cada
        // linha usa seu próprio max (height do container reparte entre linhas — corte).
        let items_h = line.iter().fold(0.0f32, |a, it| a.max(it.h));
        let line_h = if !wrap && container_cross_h > items_h {
            container_cross_h
        } else {
            items_h
        };

        // justify-content → (leading, between extra). Em overflow (free<=0) ou n==1
        // os space-* caem para flex-start/center (ver justify_offsets).
        let (leading, between) = justify_offsets(justify, free, n);

        let mut x = content_x + leading;
        for (j, it) in line.iter().enumerate() {
            if j > 0 {
                x += gap + between;
            }
            // align-items → offset no eixo cruzado dentro da altura da linha.
            let off_cross = align_offset(align, line_h, it.h);
            let item_y = line_y + off_cross;
            if it.is_text {
                let text = collect_text(dom, it.node);
                let color = css.color.unwrap_or(0x000000FF);
                list.items.push(DisplayItem::Text {
                    x,
                    y: item_y,
                    text,
                    color,
                    size: font_size,
                    mono: false,
                    bold: css.bold.unwrap_or(false),
                });
            } else {
                // o filho resolve seu próprio width contra o container (%).
                layout_block(dom, it.node, x, item_y, content_w, true, ctx, list);
            }
            x += it.w;
        }
        line_y += line_h + row_gap;
    }
    // desconta o último row_gap (só ENTRE linhas, não após a última).
    let total_h = (line_y - row_gap - content_y).max(0.0);
    total_h
}

/// Calcula (leading, between) do justify-content dado o espaço livre `free` e o nº
/// de itens `n`. `leading` = offset inicial; `between` = espaço EXTRA entre itens
/// (além do gap).
///
/// OVERFLOW (free<=0): VALIDADO contra o Chrome (com `flex-shrink:0` para forçar
/// overflow real — sem isso o flex-shrink encolhe os itens e não há overflow). Os
/// três distribuidores `space-*` caem para FLEX-START ([0,100,200] no teste), e só
/// `center`/`flex-end` mantêm o leading (negativo = transborda dos dois lados/start).
/// NB: a verificação adversarial sugeriu around/evenly→center, mas o Chrome real os
/// trata como flex-start — a medição no browser desempatou.
fn justify_offsets(j: crate::style::JustifyContent, free: f32, n: usize) -> (f32, f32) {
    use crate::style::JustifyContent as J;
    if free <= 0.0 {
        return match j {
            J::Center => (free / 2.0, 0.0), // leading negativo = transbordo centrado
            J::FlexEnd => (free, 0.0),      // todo o overflow no start
            // flex-start E os space-* → flush no start (fiel ao Chrome em overflow).
            J::FlexStart | J::SpaceBetween | J::SpaceAround | J::SpaceEvenly => (0.0, 0.0),
        };
    }
    match j {
        J::FlexStart => (0.0, 0.0),
        J::FlexEnd => (free, 0.0),
        J::Center => (free / 2.0, 0.0),
        J::SpaceBetween => {
            if n > 1 { (0.0, free / (n - 1) as f32) } else { (0.0, 0.0) }
        }
        J::SpaceAround => {
            if n >= 1 { (free / (2 * n) as f32, free / n as f32) } else { (0.0, 0.0) }
        }
        J::SpaceEvenly => (free / (n + 1) as f32, free / (n + 1) as f32),
    }
}

/// Offset no eixo cruzado de um item, dado o align-items, a altura da linha `line_h`
/// e a altura outer do item `item_h`. (stretch é tratado como flex-start aqui — o
/// esticar real exige passar altura imposta ao layout_block, fase futura.)
fn align_offset(a: crate::style::AlignItems, line_h: f32, item_h: f32) -> f32 {
    use crate::style::AlignItems as A;
    let free = line_h - item_h;
    match a {
        A::Stretch | A::FlexStart => 0.0,
        A::FlexEnd => free,
        A::Center => free / 2.0,
    }
}

/// Altura OUTER que um filho QUER, para o align-items/cross-axis. Para nós-bloco,
/// MEDE chamando o `layout_block` real numa `DisplayList` DESCARTÁVEL — assim a
/// altura medida é EXATAMENTE a que será pintada (inclui height explícito, frame,
/// recursão nos filhos, %). Sem aproximação: a verificação adversarial pegou que a
/// estimativa por "nº de linhas × line-height" divergia da pintura quando o filho
/// tinha frame próprio ou múltiplas linhas, errando a centralização cross-axis.
fn child_outer_height(dom: &Dom, id: NodeIdx, container_w: f32, parent_font: f32, ctx: &LayoutCtx) -> f32 {
    match &dom.node(id).kind {
        NodeKind::Element { .. } if is_block_level(dom, id) => {
            // layout de teste numa lista descartável: o (_, outer_h) é a altura real.
            let mut scratch = DisplayList::default();
            let (_, outer_h) = layout_block(dom, id, 0.0, 0.0, container_w, true, ctx, &mut scratch);
            outer_h
        }
        NodeKind::Text(_) => ctx.measurer.line_height(parent_font),
        _ => 0.0,
    }
}

/// Largura OUTER que um filho QUER (sem pintar), para decidir a quebra de linha no
/// modo wrap. Bloco com `width`: esse width (+ frame); sem width: largura natural
/// do conteúdo (+ frame); texto solto: a largura do texto.
fn child_outer_width(dom: &Dom, id: NodeIdx, container_w: f32, parent_font: f32, ctx: &LayoutCtx) -> f32 {
    match &dom.node(id).kind {
        NodeKind::Element { .. } if is_block_level(dom, id) => {
            let css = dom.computed_style_idx(id).unwrap_or_default();
            let font = css.font_size.unwrap_or(parent_font);
            // frame horizontal = margin_h + 2*border + padding_h (cada já é o eixo).
            let frame = css.margin.horizontal_px() + 2.0 * css.border_width.unwrap_or(0.0) + css.padding.horizontal_px();
            let resolve = ResolveCtx {
                parent_content_w: container_w,
                node_font_size: font,
                root_font_size: DEFAULT_FONT_SIZE,
                viewport_w: ctx.viewport_w,
                viewport_h: ctx.viewport_h,
            };
            // Em border-box, o `width` declarado JÁ é a caixa (outer sem margin) —
            // não soma pad/border de novo; só a margin. Em content-box, soma o frame.
            match css.width.and_then(|d| d.resolve(&resolve)) {
                Some(w) if css.border_box.unwrap_or(false) => {
                    w + css.margin.horizontal_px()
                }
                Some(w) => w + frame,
                None => content_natural_width(dom, id, font, ctx) + frame,
            }
        }
        NodeKind::Text(t) => ctx.measurer.text_width(t, parent_font, false, false),
        _ => 0.0,
    }
}

/// Desenha um nó como UMA linha de texto (texto solto ou inline simples), herdando
/// cor/tamanho do bloco pai, e devolve o `y` abaixo da linha. Concatena o texto de
/// todos os descendentes (inline-flow rico — spans em cores diferentes na mesma
/// linha — fica para a fatia de inline).
fn layout_inline_line(
    dom: &Dom,
    id: NodeIdx,
    x: f32,
    y: f32,
    content_w: f32,
    parent_css: &ComputedStyle,
    font_size: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    // INLINE-FLOW RICO: coleta os RUNS (cada pedaço de texto com a SUA cor/bold/
    // italic herdada do span que o contém), em ordem de documento. Assim
    // <h1>Seus dados <span style=color:roxo>insight</span> fim</h1> vira 3 runs com
    // cores diferentes que FLUEM numa linha contínua (não 1 linha por nó).
    let runs = collect_runs(dom, id, parent_css);
    if runs.iter().all(|r| r.text.trim().is_empty()) {
        return y;
    }
    let mono = parent_css
        .font_family
        .as_deref()
        .map(crate::style::is_mono_family)
        .unwrap_or(false);
    // line-height: do CSS (multiplicador ou px), senão o default do measurer — #1749.
    let lh = parent_css
        .line_height
        .map(|l| l.resolve(font_size))
        .unwrap_or_else(|| ctx.measurer.line_height(font_size));
    let nowrap = matches!(
        parent_css.white_space,
        Some(crate::style::WhiteSpace::Nowrap | crate::style::WhiteSpace::Pre)
    );
    let wrap_w = if nowrap { f32::INFINITY } else { content_w };
    // quebra os runs em LINHAS, cada linha = sequência de pedaços coloridos (word).
    let lines = wrap_runs(&runs, wrap_w, font_size, mono, ctx.measurer);
    let mut cy = y;
    for line in &lines {
        // largura total da linha (soma dos pedaços, cada um no SEU peso) p/ text-align.
        let line_w: f32 = line
            .iter()
            .map(|seg| ctx.measurer.text_width(&seg.text, font_size, mono, seg.bold))
            .sum();
        let free = (content_w - line_w).max(0.0);
        let mut seg_x = match parent_css.text_align {
            Some(crate::style::TextAlign::Right) => x + free,
            Some(crate::style::TextAlign::Center) => x + free / 2.0,
            _ => x, // left/justify
        };
        // pinta cada pedaço NA SUA COR e PESO, avançando o x.
        for seg in line {
            let w = ctx.measurer.text_width(&seg.text, font_size, mono, seg.bold);
            list.items.push(DisplayItem::Text {
                x: seg_x,
                y: cy,
                text: seg.text.clone(),
                color: seg.color,
                size: font_size,
                mono,
                bold: seg.bold,
            });
            seg_x += w;
        }
        cy += lh;
    }
    cy
}

/// Um pedaço de texto inline com seu estilo resolvido (cor/peso herdados do span pai).
struct InlineRun {
    text: String,
    color: u32,
    bold: bool,
}

/// Coleta os RUNS de texto de `id` em ordem de documento, cada um com a COR efetiva
/// do elemento inline que o contém (um `<span style=color:x>` muda a cor do seu
/// texto). Aplica text-transform por run. A cor vem do `computed_style_idx` do nó
/// inline (que já herda do pai via a cascade) — é por isso que o style do span passa
/// a valer no texto.
fn collect_runs(dom: &Dom, id: NodeIdx, parent_css: &ComputedStyle) -> Vec<InlineRun> {
    let mut runs = Vec::new();
    walk(
        dom,
        id,
        parent_css.color.unwrap_or(0x000000FF),
        parent_css.text_transform,
        parent_css.bold.unwrap_or(false),
        &mut runs,
    );
    return runs;

    fn walk(
        dom: &Dom,
        id: NodeIdx,
        inherited_color: u32,
        inherited_tt: Option<crate::style::TextTransform>,
        inherited_bold: bool,
        out: &mut Vec<InlineRun>,
    ) {
        match &dom.node(id).kind {
            NodeKind::Text(t) => {
                let text = match inherited_tt {
                    Some(tt) => tt.apply(t),
                    None => t.clone(),
                };
                out.push(InlineRun { text, color: inherited_color, bold: inherited_bold });
            }
            NodeKind::Element { .. } => {
                // a cor/text-transform/peso DESTE inline (se declarar) vence p/ os filhos.
                let css = dom.computed_style_idx(id);
                let color = css.as_ref().and_then(|c| c.color).unwrap_or(inherited_color);
                let tt = css.as_ref().and_then(|c| c.text_transform).or(inherited_tt);
                let bold = css.as_ref().and_then(|c| c.bold).unwrap_or(inherited_bold);
                for &c in &dom.node(id).children {
                    walk(dom, c, color, tt, bold, out);
                }
            }
            _ => {}
        }
    }
}

/// Um segmento de texto colorido/pesado posicionado numa linha (após o wrap).
struct Segment {
    text: String,
    color: u32,
    bold: bool,
}

/// Quebra uma sequência de RUNS coloridos em LINHAS por palavra (word-wrap), juntando
/// runs adjacentes na mesma linha. Cada linha é um vetor de [`Segment`] (pedaços
/// coloridos contíguos). Uma palavra que não cabe começa nova linha; preserva a cor
/// de cada palavra conforme o run de origem.
fn wrap_runs(
    runs: &[InlineRun],
    max_w: f32,
    font_size: f32,
    mono: bool,
    m: &dyn TextMeasurer,
) -> Vec<Vec<Segment>> {
    let space_w = m.text_width(" ", font_size, mono, false);
    let mut lines: Vec<Vec<Segment>> = Vec::new();
    let mut cur: Vec<Segment> = Vec::new();
    let mut cur_w = 0.0f32;
    let mut at_line_start = true;

    for run in runs {
        for word in run.text.split_whitespace() {
            let ww = m.text_width(word, font_size, mono, run.bold);
            let need = if at_line_start { ww } else { space_w + ww };
            if !at_line_start && cur_w + need > max_w {
                // não cabe: fecha a linha.
                lines.push(std::mem::take(&mut cur));
                cur_w = 0.0;
                at_line_start = true;
            }
            // adiciona a palavra (com espaço antes, se não for início de linha).
            let piece = if at_line_start { word.to_string() } else { format!(" {word}") };
            // junta no último segmento se mesma cor E peso, senão novo segmento.
            if let Some(last) = cur.last_mut() {
                if last.color == run.color && last.bold == run.bold {
                    last.text.push_str(&piece);
                } else {
                    cur.push(Segment { text: piece, color: run.color, bold: run.bold });
                }
            } else {
                cur.push(Segment { text: piece, color: run.color, bold: run.bold });
            }
            cur_w += need;
            at_line_start = false;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.is_empty() {
        lines.push(vec![Segment { text: String::new(), color: 0, bold: false }]);
    }
    lines
}

/// Quebra `text` em LINHAS que cabem em `max_w` (word-wrap do CSS `white-space:
/// normal`): acumula palavras separadas por espaço; quando a próxima não cabe,
/// fecha a linha e começa outra. Uma palavra maior que `max_w` fica sozinha na
/// linha (não quebra no meio da palavra — `overflow-wrap:normal`).
fn wrap_text(text: &str, max_w: f32, font_size: f32, mono: bool, m: &dyn TextMeasurer) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_w = 0.0f32;
    let space_w = m.text_width(" ", font_size, mono, false);
    for word in text.split_whitespace() {
        let word_w = m.text_width(word, font_size, mono, false);
        if current.is_empty() {
            current.push_str(word);
            current_w = word_w;
        } else if current_w + space_w + word_w <= max_w {
            current.push(' ');
            current.push_str(word);
            current_w += space_w + word_w;
        } else {
            // não cabe: fecha a linha atual e começa nova com a palavra.
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
            current_w = word_w;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Concatena o texto de todos os descendentes de `id` (ordem de documento).
fn collect_text(dom: &Dom, id: NodeIdx) -> String {
    let mut out = String::new();
    collect_into(dom, id, &mut out);
    return out;

    fn collect_into(dom: &Dom, id: NodeIdx, out: &mut String) {
        match &dom.node(id).kind {
            NodeKind::Text(t) => out.push_str(t),
            _ => {
                for &c in &dom.node(id).children {
                    collect_into(dom, c, out);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::parse_html_to_dom;

    /// Registra `<div>` como bloco vertical (os testes precisam que a tag tenha
    /// layout de bloco para entrar no caminho `layout_block` dos filhos).
    fn def_div() {
        crate::block::define(
            "div",
            crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 },
        );
    }

    /// Layout determinístico com medidor aproximado e viewport fixo.
    fn layout(html: &str, vw: f32) -> DisplayList {
        def_div();
        let dom = parse_html_to_dom(html);
        let ctx = LayoutCtx { viewport_w: vw, viewport_h: 600.0, measurer: &ApproxMeasurer };
        layout_document(&dom, &ctx)
    }

    /// Primeiro `SolidRect` da lista (o fundo da 1ª caixa) — atalho de assert.
    fn first_rect(list: &DisplayList) -> Rect {
        list.items
            .iter()
            .find_map(|it| match it {
                DisplayItem::SolidRect { rect, .. } => Some(*rect),
                _ => None,
            })
            .expect("esperava ao menos um SolidRect")
    }

    #[test]
    fn block_ocupa_largura_do_container() {
        // <div> sem width: bloco ocupa a largura do viewport menos o frame.
        // Aqui só padding=10 (margin/border=0): content = 600 - 20 = 580; a CAIXA
        // (content+padding) = 600 (largura cheia).
        let list = layout("<div style='background:#112233; padding:10'>x</div>", 600.0);
        let r = first_rect(&list);
        assert_eq!(r.x, 0.0);
        assert_eq!(r.y, 0.0);
        assert_eq!(r.w, 600.0); // content(580) + padding(2×10) = 600
    }

    #[test]
    fn width_percent_resolve_contra_container() {
        // width:50% de um viewport 800 → content=400; sem padding/border a caixa=400.
        let list = layout("<div style='background:#111111; width:50%'>x</div>", 800.0);
        let r = first_rect(&list);
        assert_eq!(r.w, 400.0); // 50% de 800
        assert_eq!(r.x, 0.0);
    }

    #[test]
    fn blocos_empilham_vertical() {
        // dois <div> com altura de 1 linha (~26 = 20×1.3) empilham: o 2º começa
        // abaixo do 1º. Sem box (sem bg) — só checa o Y das linhas de texto.
        let list = layout("<div>um</div><div>dois</div>", 600.0);
        let texts: Vec<f32> = list
            .items
            .iter()
            .filter_map(|it| match it {
                DisplayItem::Text { y, .. } => Some(*y),
                _ => None,
            })
            .collect();
        assert_eq!(texts.len(), 2);
        assert_eq!(texts[0], 0.0); // primeiro no topo
        assert!(texts[1] >= 26.0, "segundo bloco abaixo do primeiro (y={})", texts[1]); // 20×1.3
    }

    #[test]
    fn fundo_vem_antes_do_texto_filho_no_zorder() {
        // O SolidRect (fundo) deve estar ANTES do Text na lista (pinta atrás).
        let list = layout("<div style='background:#222222; padding:8'>oi</div>", 600.0);
        let i_rect = list.items.iter().position(|it| matches!(it, DisplayItem::SolidRect { .. }));
        let i_text = list.items.iter().position(|it| matches!(it, DisplayItem::Text { .. }));
        assert!(i_rect < i_text, "fundo (idx {i_rect:?}) deve vir antes do texto (idx {i_text:?})");
    }

    #[test]
    fn box_model_content_box_offset_do_texto() {
        // content-box: o texto começa deslocado por margin+border+padding.
        // padding=14, border=2, margin=6 → offset = 22. (MDN: outer = m+b+p+content)
        let list = layout(
            "<div style='background:#111111; padding:14; border-width:2; margin:6'>z</div>",
            600.0,
        );
        let txt = list
            .items
            .iter()
            .find_map(|it| match it {
                DisplayItem::Text { x, y, .. } => Some((*x, *y)),
                _ => None,
            })
            .expect("texto");
        assert_eq!(txt.0, 22.0); // x = margin(6)+border(2)+padding(14)
        assert_eq!(txt.1, 22.0); // y idem
        // a caixa (fundo) NÃO inclui a margin: começa em (6,6).
        let r = first_rect(&list);
        assert_eq!(r.x, 6.0);
        assert_eq!(r.y, 6.0);
    }

    #[test]
    fn tres_cards_empilham_no_vertical() {
        // <div> vertical (default): 3 cards empilham — mesmo x, Y crescente, cada
        // um com sua caixa de 30% de 900 = 270.
        let list = layout(
            "<div style='background:#111;width:30%'>a</div>\
             <div style='background:#222;width:30%'>b</div>\
             <div style='background:#333;width:30%'>c</div>",
            900.0,
        );
        let rects: Vec<Rect> = list
            .items
            .iter()
            .filter_map(|it| match it {
                DisplayItem::SolidRect { rect, .. } => Some(*rect),
                _ => None,
            })
            .collect();
        assert_eq!(rects.len(), 3);
        assert!(rects.iter().all(|r| r.x == 0.0)); // mesmo x (vertical)
        assert!(rects.iter().all(|r| (r.w - 270.0).abs() < 0.01)); // 30% de 900
        assert!(rects[0].y < rects[1].y && rects[1].y < rects[2].y); // Y crescente
    }

    #[test]
    fn cards_lado_a_lado_no_horizontal() {
        // <row display:horizontal> com 3 <div> cada 30% → ficam LADO A LADO: X
        // crescente, MESMO y (topo), cada caixa 270 de largura. (O caso do
        // stat-card: era isto que o egui colapsava; agora o layout do DOM resolve.)
        crate::block::define(
            "row",
            crate::block::BlockDef { display: 2, indent: 0.0, prefix: 0, flags: 0 },
        );
        crate::block::define(
            "div",
            crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 },
        );
        let dom = parse_html_to_dom(
            "<row>\
               <div style='background:#111;width:30%'>a</div>\
               <div style='background:#222;width:30%'>b</div>\
               <div style='background:#333;width:30%'>c</div>\
             </row>",
        );
        let ctx = LayoutCtx { viewport_w: 900.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let rects: Vec<Rect> = list
            .items
            .iter()
            .filter_map(|it| match it {
                DisplayItem::SolidRect { rect, .. } => Some(*rect),
                _ => None,
            })
            .collect();
        assert_eq!(rects.len(), 3);
        // mesmo Y (lado a lado, não empilhado).
        assert!(rects.iter().all(|r| r.y == rects[0].y), "todos no mesmo topo: {rects:?}");
        // X crescente: card 2 à direita do 1, card 3 à direita do 2.
        assert!(rects[0].x < rects[1].x && rects[1].x < rects[2].x, "X crescente: {rects:?}");
        // cada caixa 30% de 900 = 270 (a % resolve contra o content do <row>).
        assert!(rects.iter().all(|r| (r.w - 270.0).abs() < 1.0), "largura ~270: {rects:?}");
        // o 2º começa onde o 1º termina (sem sobrepor): x[1] ≈ x[0] + w[0].
        assert!((rects[1].x - (rects[0].x + rects[0].w)).abs() < 1.0, "encostados: {rects:?}");
    }

    #[test]
    fn cards_com_filhos_nao_esticam_o_ultimo() {
        // REGRESSÃO (bug visto na tela): 3 cards width:32% COM filhos (<p>) num <row>
        // largo — o ÚLTIMO não pode esticar até a borda. Cada um = 32% da largura,
        // o resto fica vazio à direita (como no navegador). p=wrap pra bater o real.
        crate::block::define("row", crate::block::BlockDef { display: 2, indent: 0.0, prefix: 0, flags: 0 });
        crate::block::define("div", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        crate::block::define("p", crate::block::BlockDef { display: 1, indent: 0.0, prefix: 0, flags: 0 });
        let dom = parse_html_to_dom(
            "<row>\
               <div style='background:#111;width:32%'><p>256</p><p>testes</p></div>\
               <div style='background:#222;width:32%'><p>31%</p><p>paridade</p></div>\
               <div style='background:#333;width:32%'><p>5</p><p>fases</p></div>\
             </row>",
        );
        let ctx = LayoutCtx { viewport_w: 1000.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let rects: Vec<Rect> = list.items.iter().filter_map(|it| match it {
            DisplayItem::SolidRect { rect, .. } => Some(*rect),
            _ => None,
        }).collect();
        assert_eq!(rects.len(), 3);
        // TODOS com a MESMA largura = 32% de 1000 = 320 (o 3º NÃO estica).
        for (i, r) in rects.iter().enumerate() {
            assert!((r.w - 320.0).abs() < 1.0, "card[{i}] devia ter 320 (32%), tem {}: {rects:?}", r.w);
        }
        // o último termina BEM antes da borda (3×320=960 < 1000), sobra vazio.
        let last = rects[2];
        assert!(last.x + last.w <= 1000.0, "último não passa da borda: {last:?}");
    }

    #[test]
    fn border_box_faz_3_cards_caberem() {
        // box-sizing:border-box: width:32% INCLUI padding+border → a CAIXA é 32%,
        // 3 cards = 96% (cabem, sobra ~4%). Sem border-box (content-box) cada caixa
        // seria 32%+frame e estouraria. Prova a propriedade real do CSS.
        crate::block::define("row", crate::block::BlockDef { display: 2, indent: 0.0, prefix: 0, flags: 0 });
        crate::block::define("div", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        let dom = parse_html_to_dom(
            "<style>.card{box-sizing:border-box;width:32%;padding:14;border-width:2;background:#1a2030}</style>\
             <row>\
               <div class='card'>a</div><div class='card'>b</div><div class='card'>c</div>\
             </row>",
        );
        let ctx = LayoutCtx { viewport_w: 1000.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let rects: Vec<Rect> = list.items.iter().filter_map(|it| match it {
            DisplayItem::SolidRect { rect, .. } => Some(*rect),
            _ => None,
        }).collect();
        assert_eq!(rects.len(), 3);
        // cada CAIXA = 32% de 1000 = 320 (border-box: o width É a caixa inteira).
        for (i, r) in rects.iter().enumerate() {
            assert!((r.w - 320.0).abs() < 1.0, "card[{i}] caixa=320 (border-box): {rects:?}");
        }
        // 3×320=960 < 1000: cabem com folga (sobra ~40 = 4%).
        let last = rects[2];
        assert!(last.x + last.w <= 1000.0, "cabem todos: {rects:?}");
        assert!(1000.0 - (last.x + last.w) >= 30.0, "sobra espaço à direita: {rects:?}");
    }

    #[test]
    fn min_max_width_clamp() {
        // VALIDADO no Chrome: used_width = clamp(min, width, max) (#1751).
        crate::block::define("div", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        let cases = [
            ("width:500px;max-width:300px", 300.0),  // max limita
            ("width:50px;min-width:200px", 200.0),   // min eleva
            ("width:1000px;max-width:400px;min-width:100px", 400.0), // clamp
            ("width:600px;max-width:50%", 400.0),    // % de 800
        ];
        for (style, expected) in cases {
            let dom = parse_html_to_dom(&format!("<div id=\"t\" style=\"{style}\">x</div>"));
            let t = dom.query("#t").unwrap();
            let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
            let rect = bounding_rect(&dom, dom.resolve(t).unwrap(), &ctx).unwrap();
            assert!((rect.w - expected).abs() < 1.0, "{style}: w={} esperado {expected}", rect.w);
        }
    }

    #[test]
    fn min_max_height_clamp() {
        crate::block::define("div", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        // height:500 max-height:200 → caixa de 200.
        let dom = parse_html_to_dom("<div id=\"t\" style=\"height:500px;max-height:200px;width:100px\">x</div>");
        let t = dom.query("#t").unwrap();
        let ctx = LayoutCtx { viewport_w: 600.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let rect = bounding_rect(&dom, dom.resolve(t).unwrap(), &ctx).unwrap();
        assert!((rect.h - 200.0).abs() < 1.0, "max-height: h={}", rect.h);
        // min-height:300 num conteúdo pequeno → caixa de 300.
        let dom2 = parse_html_to_dom("<div id=\"t\" style=\"min-height:300px;width:100px\">x</div>");
        let t2 = dom2.query("#t").unwrap();
        let rect2 = bounding_rect(&dom2, dom2.resolve(t2).unwrap(), &ctx).unwrap();
        assert!(rect2.h >= 300.0, "min-height: h={}", rect2.h);
    }

    #[test]
    fn text_align_desloca_o_texto() {
        // text-align center/right desloca o texto pelo espaço livre (#1749).
        let dom = parse_html_to_dom("<style>#c{text-align:center;width:400px}#r{text-align:right;width:400px}</style><div id=\"c\">x</div><div id=\"r\">y</div>");
        let ctx = LayoutCtx { viewport_w: 600.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let texts: Vec<(String, f32)> = list.items.iter().filter_map(|it| match it {
            DisplayItem::Text { text, x, .. } => Some((text.clone(), *x)),
            _ => None,
        }).collect();
        // "x" (1 char, ~10px de largura) centrado em 400 → x ≈ (400-10)/2 = 195.
        let cx = texts.iter().find(|(t, _)| t == "x").unwrap().1;
        assert!((cx - 195.0).abs() < 2.0, "center: {cx}");
        // "y" à direita → x ≈ 400-10 = 390.
        let rx = texts.iter().find(|(t, _)| t == "y").unwrap().1;
        assert!((rx - 390.0).abs() < 2.0, "right: {rx}");
    }

    #[test]
    fn line_height_e_text_transform() {
        // line-height do CSS respeitado + text-transform aplicado (#1749). Usa <div>
        // (sem margin default da UA, ao contrário de <p>) p/ isolar o line-height.
        crate::block::define("div", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        let dom = parse_html_to_dom("<style>div{line-height:3;text-transform:uppercase}</style><div>oi</div><div>tchau</div>");
        let ctx = LayoutCtx { viewport_w: 600.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let texts: Vec<(String, f32)> = list.items.iter().filter_map(|it| match it {
            DisplayItem::Text { text, y, .. } => Some((text.clone(), *y)),
            _ => None,
        }).collect();
        // uppercase aplicado.
        assert!(texts.iter().any(|(t, _)| t == "OI"));
        assert!(texts.iter().any(|(t, _)| t == "TCHAU"));
        // line-height:3 = 3×20 = 60px entre as linhas (div sem margin).
        let y_oi = texts.iter().find(|(t, _)| t == "OI").unwrap().1;
        let y_tchau = texts.iter().find(|(t, _)| t == "TCHAU").unwrap().1;
        assert!((y_tchau - y_oi - 60.0).abs() < 5.0, "line-height: {y_oi} → {y_tchau}");
    }

    #[test]
    fn display_vem_do_css_nao_do_defineblock() {
        // O `display:flex` no <style> faz <row> dispor os filhos LADO A LADO, sem
        // precisar de defineBlock. `display:none` some. É o motor lendo o display DO
        // CSS. (`<div>` é block via a UA-stylesheet `ua.ts` em produção; nos testes
        // unitários — sem o prelude TS — registramos o default à mão.)
        crate::block::define("div", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        let dom = parse_html_to_dom(
            "<style>row{display:flex} hide{display:none} \
                    .c{width:30%;background:#111}</style>\
             <row>\
               <div class='c'>a</div><div class='c'>b</div><div class='c'>c</div>\
             </row>\
             <hide>invisível</hide>",
        );
        let ctx = LayoutCtx { viewport_w: 900.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let rects: Vec<Rect> = list.items.iter().filter_map(|it| match it {
            DisplayItem::SolidRect { rect, .. } => Some(*rect),
            _ => None,
        }).collect();
        assert_eq!(rects.len(), 3, "3 cards (o <hide> display:none não pinta)");
        // display:flex do CSS → lado a lado (X crescente, mesmo Y).
        assert!(rects[0].x < rects[1].x && rects[1].x < rects[2].x, "lado a lado: {rects:?}");
        assert!(rects.iter().all(|r| r.y == rects[0].y), "mesma linha: {rects:?}");
        // display:none → o texto "invisível" NÃO está na lista.
        let has_invisivel = list.items.iter().any(|it| matches!(it, DisplayItem::Text { text, .. } if text.contains("invisível")));
        assert!(!has_invisivel, "display:none não renderiza o conteúdo");
    }

    #[test]
    fn margin_vertical_empilha_sem_deslocar_horizontal() {
        // margin_v (UA-stylesheet) separa blocos no VERTICAL mas NÃO empurra no
        // eixo horizontal (como `margin: Npx 0` do navegador para h1/p). Dois
        // parágrafos com margin_v: o 2º começa mais abaixo, mas ambos em x=0.
        crate::block::define("p", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        crate::style::define_style("p", crate::style::SLOT_MARGIN_V, 16);
        let dom = parse_html_to_dom("<p>um</p><p>dois</p>");
        let ctx = LayoutCtx { viewport_w: 600.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let texts: Vec<(f32, f32)> = list.items.iter().filter_map(|it| match it {
            DisplayItem::Text { x, y, .. } => Some((*x, *y)),
            _ => None,
        }).collect();
        assert_eq!(texts.len(), 2);
        // X: ambos em 0 (margin VERTICAL não desloca horizontal).
        assert_eq!(texts[0].0, 0.0, "1º texto em x=0: {texts:?}");
        assert_eq!(texts[1].0, 0.0, "2º texto em x=0 (margin não empurrou): {texts:?}");
        // Y: o 2º bem abaixo (margin colapsado entre eles + altura da linha).
        assert!(texts[1].1 > texts[0].1 + 20.0, "2º empilhado abaixo: {texts:?}");
    }

    #[test]
    fn bounding_rect_dos_cards() {
        // getBoundingClientRect: o border-box de cada nó-bloco. Os 3 cards (flex,
        // 32% border-box) têm os MESMOS rects que o dump mostra (x=20/322/624, w=302).
        crate::block::define("row", crate::block::BlockDef { display: 2, indent: 0.0, prefix: 0, flags: 0 });
        crate::block::define("div", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        let dom = parse_html_to_dom(
            "<style>.card{box-sizing:border-box;width:32%;padding:14;border-width:2;background:#1a2030}</style>\
             <row>\
               <div class='card' id='a'>1</div><div class='card' id='b'>2</div><div class='card' id='c'>3</div>\
             </row>",
        );
        let ctx = LayoutCtx { viewport_w: 1000.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        // resolve os NodeIdx dos 3 cards e mede cada um.
        let a = dom.resolve(dom.query("#a").unwrap()).unwrap();
        let b = dom.resolve(dom.query("#b").unwrap()).unwrap();
        let c = dom.resolve(dom.query("#c").unwrap()).unwrap();
        let ra = bounding_rect(&dom, a, &ctx).expect("card a tem rect");
        let rb = bounding_rect(&dom, b, &ctx).expect("card b tem rect");
        let rc = bounding_rect(&dom, c, &ctx).expect("card c tem rect");
        // border-box = 32% de 1000 = 320 cada; lado a lado.
        assert!((ra.w - 320.0).abs() < 1.0, "largura ~320: {ra:?}");
        assert!((rb.w - 320.0).abs() < 1.0);
        assert!((rc.w - 320.0).abs() < 1.0);
        assert_eq!(ra.x, 0.0); // (sem padding no body de teste, x começa em 0)
        assert!(rb.x > ra.x && rc.x > rb.x, "X crescente: {ra:?} {rb:?} {rc:?}");
        assert_eq!(ra.y, rb.y); // mesma linha (flex)
    }

    #[test]
    fn bounding_rect_none_para_texto() {
        // texto/inline não tem rect próprio (a API só dá rect de elemento-bloco).
        let dom = parse_html_to_dom("<p>oi</p>");
        crate::block::define("p", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        let ctx = LayoutCtx { viewport_w: 600.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let p = dom.resolve(dom.query("p").unwrap()).unwrap();
        // o <p> (bloco) TEM rect.
        assert!(bounding_rect(&dom, p, &ctx).is_some());
        // o nó de texto filho NÃO tem (não é bloco).
        let txt = dom.node(p).children[0];
        assert!(bounding_rect(&dom, txt, &ctx).is_none());
    }

    /// Helper: layout de um HTML num row flex e os rects (x ordenado) dos N cards.
    fn flex_card_rects(style: &str, n_cards: usize, vw: f32) -> Vec<Rect> {
        crate::block::define("row", crate::block::BlockDef { display: 2, indent: 0.0, prefix: 0, flags: 0 });
        crate::block::define("div", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        let mut html = format!("<style>row{{display:flex;{style}}} .c{{width:100px;background:#111}}</style><row>");
        for i in 0..n_cards {
            html.push_str(&format!("<div class='c' id='c{i}'>x</div>"));
        }
        html.push_str("</row>");
        let dom = parse_html_to_dom(&html);
        let ctx = LayoutCtx { viewport_w: vw, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let mut rects: Vec<Rect> = list.items.iter().filter_map(|it| match it {
            DisplayItem::SolidRect { rect, .. } => Some(*rect),
            _ => None,
        }).collect();
        rects.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
        rects
    }

    #[test]
    fn flex_gap_separa_itens() {
        // gap:20px entre 3 cards de 100px: x = 0, 120, 240.
        let r = flex_card_rects("gap:20px", 3, 600.0);
        assert_eq!(r.len(), 3);
        assert!((r[0].x - 0.0).abs() < 0.5, "{r:?}");
        assert!((r[1].x - 120.0).abs() < 0.5, "card2 em 100+20: {r:?}");
        assert!((r[2].x - 240.0).abs() < 0.5, "card3 em 220+20: {r:?}");
    }

    #[test]
    fn flex_justify_content() {
        // 3 cards de 100 num container de 600 → free = 600-300 = 300.
        // space-between: x = 0, 100+150=250, 200+300=500.
        let r = flex_card_rects("justify-content:space-between", 3, 600.0);
        assert!((r[0].x - 0.0).abs() < 0.5, "{r:?}");
        assert!((r[1].x - 250.0).abs() < 0.5, "between=150: {r:?}");
        assert!((r[2].x - 500.0).abs() < 0.5, "flush no fim: {r:?}");
        // center: leading = 150 → x = 150, 250, 350.
        let r = flex_card_rects("justify-content:center", 3, 600.0);
        assert!((r[0].x - 150.0).abs() < 0.5, "center leading=150: {r:?}");
        assert!((r[2].x - 350.0).abs() < 0.5, "{r:?}");
        // flex-end: leading = 300 → x = 300, 400, 500.
        let r = flex_card_rects("justify-content:flex-end", 3, 600.0);
        assert!((r[0].x - 300.0).abs() < 0.5, "flex-end leading=300: {r:?}");
        // space-evenly: leading = between = 300/4 = 75 → x = 75, 250, 425.
        let r = flex_card_rects("justify-content:space-evenly", 3, 600.0);
        assert!((r[0].x - 75.0).abs() < 0.5, "evenly leading=75: {r:?}");
        assert!((r[1].x - 250.0).abs() < 0.5, "{r:?}");
    }

    #[test]
    fn flex_justify_overflow() {
        // 3 cards de 100 em 200 (overflow real = -100). VALIDADO contra Chrome
        // (flex-shrink:0): os space-* caem para flex-start → x = 0, 100, 200.
        for jc in ["space-between", "space-around", "space-evenly", "flex-start"] {
            let r = flex_card_rects(&format!("justify-content:{jc}"), 3, 200.0);
            assert!((r[0].x - 0.0).abs() < 0.5, "{jc} overflow→start: {r:?}");
            assert!((r[1].x - 100.0).abs() < 0.5, "{jc}: {r:?}");
            assert!((r[2].x - 200.0).abs() < 0.5, "{jc}: {r:?}");
        }
        // center em overflow: leading = free/2 = -50 → x = -50, 50, 150 (Chrome).
        let r = flex_card_rects("justify-content:center", 3, 200.0);
        assert!((r[0].x + 50.0).abs() < 0.5, "center overflow leading=-50: {r:?}");
        assert!((r[2].x - 150.0).abs() < 0.5, "{r:?}");
    }

    #[test]
    fn flex_align_center_usa_altura_do_container() {
        // VALIDADO no Chrome: bar height:80, cards height:40, align-items:center
        // → cards em y=20 (centrados na altura DO CONTAINER, não na linha de 40).
        crate::block::define("bar", crate::block::BlockDef { display: 2, indent: 0.0, prefix: 0, flags: 0 });
        crate::block::define("div", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        let dom = parse_html_to_dom(
            "<style>bar{display:flex;align-items:center;height:80px} .c{width:100px;height:40px;background:#ff0000}</style>\
             <bar><div class='c'>a</div><div class='c'>b</div></bar>",
        );
        let ctx = LayoutCtx { viewport_w: 600.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let ys: Vec<f32> = list.items.iter().filter_map(|it| match it {
            DisplayItem::SolidRect { rect, .. } => Some(rect.y),
            _ => None,
        }).collect();
        assert!(ys.iter().all(|&y| (y - 20.0).abs() < 0.5), "cards centrados em y=20: {ys:?}");
    }

    #[test]
    fn flex_align_items_center() {
        // 1 card baixo + 1 alto: com align-items:center o baixo desce metade da folga.
        crate::block::define("row", crate::block::BlockDef { display: 2, indent: 0.0, prefix: 0, flags: 0 });
        crate::block::define("div", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        let dom = parse_html_to_dom(
            "<style>row{display:flex;align-items:center} .a{height:20px;width:50px;background:#111111} .b{height:60px;width:50px;background:#222222}</style>\
             <row><div class='a' id='a'>x</div><div class='b' id='b'>y</div></row>",
        );
        let ctx = LayoutCtx { viewport_w: 600.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let rects: Vec<(f32, f32)> = list.items.iter().filter_map(|it| match it {
            DisplayItem::SolidRect { rect, .. } => Some((rect.x, rect.y)),
            _ => None,
        }).collect();
        // ordena por x: o card 'a' (baixo, x menor) deve ter y MAIOR que o 'b' (alto).
        let mut s = rects.clone(); s.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap());
        assert!(s[0].1 > s[1].1, "card baixo centralizado desce: {s:?}");
    }

    #[test]
    fn badges_fluem_e_quebram_linha_no_wrap() {
        // <tags display:wrap> com badges: fluem lado a lado e QUEBRAM para a próxima
        // linha quando não cabem (inline-block flow). Cada badge dimensiona pelo
        // conteúdo (shrink-to-fit), não estica para a largura toda.
        crate::block::define(
            "tags",
            crate::block::BlockDef { display: 1, indent: 0.0, prefix: 0, flags: 0 },
        );
        crate::block::define(
            "badge",
            crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 },
        );
        // 4 badges; numa largura estreita (200) eles não cabem todos numa linha.
        let dom = parse_html_to_dom(
            "<tags>\
               <badge style='background:#111;padding:6'>rust</badge>\
               <badge style='background:#222;padding:6'>cranelift</badge>\
               <badge style='background:#333;padding:6'>typescript</badge>\
               <badge style='background:#444;padding:6'>egui</badge>\
             </tags>",
        );
        let ctx = LayoutCtx { viewport_w: 200.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let rects: Vec<Rect> = list
            .items
            .iter()
            .filter_map(|it| match it {
                DisplayItem::SolidRect { rect, .. } => Some(*rect),
                _ => None,
            })
            .collect();
        assert_eq!(rects.len(), 4);
        // shrink-to-fit: nenhum badge ocupa a largura toda (200) — cada um é estreito.
        assert!(rects.iter().all(|r| r.w < 150.0), "badges estreitos (conteúdo): {rects:?}");
        // QUEBROU linha: há pelo menos 2 valores distintos de Y (não todos na mesma linha).
        let ys: std::collections::BTreeSet<i32> = rects.iter().map(|r| r.y as i32).collect();
        assert!(ys.len() >= 2, "deve haver quebra de linha (Ys distintos): {rects:?}");
        // o primeiro badge começa no canto (x=0).
        assert_eq!(rects[0].x, 0.0);
    }
}
