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
//! Cortes da fase 1 (aditivos depois): inline-flow rico multi-run, margin-collapse,
//! `display:horizontal/grid` (chega já a seguir), float/position. O objetivo da
//! fatia é provar a TUBULAÇÃO DOM→layout→display-list→paint com box model block.

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
    Text { x: f32, y: f32, text: String, color: u32, size: f32, mono: bool },
}

/// A saída do layout: a lista plana de itens de pintura, em z-order. É o ÚNICO
/// que o backend de render consome. Sem nenhuma referência à árvore — o layout já
/// consumiu a topologia (herança/cascade/box model) ao produzir esta lista.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct DisplayList {
    pub items: Vec<DisplayItem>,
    /// Altura total ocupada pelo conteúdo (para o backend dimensionar o scroll).
    pub content_height: f32,
}

/// Abstração de MEDIÇÃO de texto (largura/altura de uma string num tamanho/peso).
/// Vive aqui (no `rts-dom`) e é IMPLEMENTADA pelo backend (o egui mede via galley);
/// reimplementar largura de glifo no `rts-dom` é a armadilha que o roadmap alertou.
/// O layout depende SÓ deste trait — continua egui-free e testável com um mock.
pub trait TextMeasurer {
    /// Largura em pontos de `text` renderizado em `size` (mono ou proporcional).
    fn text_width(&self, text: &str, size: f32, mono: bool) -> f32;
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
    fn text_width(&self, text: &str, size: f32, mono: bool) -> f32 {
        let per = if mono { 0.6 } else { 0.5 };
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
    let mut cursor_y = 0.0f32;
    let root = dom.node(dom.root);
    for &child in &root.children {
        let (_, h) = layout_block(dom, child, 0.0, cursor_y, ctx.viewport_w, ctx, &mut list);
        cursor_y += h;
    }
    list.content_height = cursor_y;
    list
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
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> (f32, f32) {
    // Nós não-elemento no nível de bloco (texto solto, comentário): trata o texto
    // como uma linha; comentário não pinta.
    let css = match &dom.node(id).kind {
        NodeKind::Element { tag } => {
            // `<style>`/`<script>`: conteúdo não-renderável — pula a subárvore.
            if tag == "style" || tag == "script" {
                return (0.0, 0.0);
            }
            dom.computed_style_idx(id).unwrap_or_default()
        }
        NodeKind::Text(t) => {
            let size = DEFAULT_FONT_SIZE;
            let lh = ctx.measurer.line_height(size);
            let tw = ctx.measurer.text_width(t, size, false);
            list.items.push(DisplayItem::Text {
                x,
                y,
                text: t.clone(),
                color: 0x000000FF,
                size,
                mono: false,
            });
            return (tw, lh);
        }
        _ => return (0.0, 0.0), // Comment / Document aninhado: não pinta.
    };

    // ── Box model (content-box): resolve as bordas/espaços absolutos ─────────────
    let margin = css.margin.unwrap_or(0.0);
    let padding = css.padding.unwrap_or(0.0);
    let border = css.border_width.unwrap_or(0.0);

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
    let frame = 2.0 * (margin + border + padding);
    let content_w = match css.width.and_then(|d| d.resolve(&resolve)) {
        Some(w) => w,
        None => (avail_w - frame).max(0.0),
    };

    // Posição do content-box (canto sup-esq), deslocado por margin+border+padding.
    let content_x = x + margin + border + padding;
    let content_y = y + margin + border + padding;

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
    let content_h = if display == crate::block::DISPLAY_HORIZONTAL {
        layout_children_horizontal(dom, id, content_x, content_y, content_w, &css, font_size, ctx, list)
    } else {
        layout_children_vertical(dom, id, content_x, content_y, content_w, &css, font_size, ctx, list)
    };

    // ── Insere a CAIXA (fundo + borda) no índice reservado, ATRÁS dos filhos ─────
    // A caixa cobre content + padding + border (NÃO a margin — esta é espaço
    // externo). `insert` no `box_index` põe o fundo antes dos itens dos filhos.
    if css.has_box() {
        let box_rect = Rect::new(
            x + margin,
            y + margin,
            content_w + 2.0 * (border + padding),
            content_h + 2.0 * (border + padding),
        );
        let radius = css.corner_radius.unwrap_or(0.0);
        // Insere na ordem: primeiro o fundo, depois a borda por cima dele (ambos
        // atrás dos filhos). `insert` desloca os filhos para a frente.
        let mut at = box_index;
        if let Some(color) = css.bg {
            list.items.insert(at, DisplayItem::SolidRect { rect: box_rect, color, radius });
            at += 1;
        }
        if border > 0.0 {
            let color = css.border_color.unwrap_or(0x808080FF);
            list.items.insert(at, DisplayItem::Border { rect: box_rect, width: border, color, radius });
        }
    }

    // Tamanho EXTERNO da caixa (outer = content + padding + border + margin), nos
    // dois eixos — o pai usa a altura (modo vertical) ou a largura (horizontal).
    let outer_w = content_w + 2.0 * (border + padding + margin);
    let outer_h = content_h + 2.0 * (border + padding + margin);
    (outer_w, outer_h)
}

/// O código de `display` de um nó (do `BlockDef` da tag), ou vertical (0) se a tag
/// não define bloco. É o eixo de empilhamento dos filhos.
fn css_display(dom: &Dom, id: NodeIdx) -> i64 {
    match &dom.node(id).kind {
        NodeKind::Element { tag } => {
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
    for &child in &dom.node(id).children {
        match &dom.node(child).kind {
            NodeKind::Element { tag } if crate::block::lookup(tag).is_some() => {
                let (_, h) = layout_block(dom, child, content_x, child_y, content_w, ctx, list);
                child_y += h;
            }
            _ => {
                child_y = layout_inline_line(dom, child, content_x, child_y, css, font_size, ctx, list);
            }
        }
    }
    (child_y - content_y).max(0.0)
}

/// Dispõe os filhos HORIZONTAL (cada um à direita do anterior). A altura do content
/// é a do filho MAIS ALTO (inline-axis stacking). Devolve essa altura.
fn layout_children_horizontal(
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
    // A regra do CSS `display:flex` SEM `flex-wrap` é: os filhos ENCOLHEM para
    // caber (flex-shrink:1), nunca transbordam a tela. Implementamos isso CLAMPANDO
    // a largura de cada filho ao espaço que RESTA da linha: o `width:%` resolve
    // contra o container (largura certa quando cabe), mas a largura efetiva nunca
    // passa do restante — então nada sai da janela. (O shrink proporcional exato
    // entre todos os filhos é refinamento posterior; o clamp já barra o overflow.)
    let mut child_x = content_x;
    let mut max_h = 0.0f32;
    let right_edge = content_x + content_w;
    for &child in &dom.node(id).children {
        let remaining = (right_edge - child_x).max(0.0);
        match &dom.node(child).kind {
            NodeKind::Element { tag } if crate::block::lookup(tag).is_some() => {
                // O `width:%` do filho resolve contra o CONTAINER (`content_w`) —
                // assim 3×30% dão 270 cada (não 30% do restante). A largura real
                // produzida é então CLAMPADA ao espaço que resta (`remaining`) para
                // nada sair da tela: o avanço de `child_x` nunca passa da borda.
                let (w, h) = layout_block(dom, child, child_x, content_y, content_w, ctx, list);
                child_x += w.min(remaining);
                max_h = max_h.max(h);
            }
            _ => {
                let text = collect_text(dom, child);
                if text.trim().is_empty() {
                    continue;
                }
                let color = css.color.unwrap_or(0x000000FF);
                let tw = ctx.measurer.text_width(&text, font_size, false).min(remaining);
                let lh = ctx.measurer.line_height(font_size);
                list.items.push(DisplayItem::Text {
                    x: child_x,
                    y: content_y,
                    text,
                    color,
                    size: font_size,
                    mono: false,
                });
                child_x += tw;
                max_h = max_h.max(lh);
            }
        }
    }
    max_h
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
    parent_css: &ComputedStyle,
    font_size: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    let text = collect_text(dom, id);
    if text.trim().is_empty() {
        return y;
    }
    let color = parent_css.color.unwrap_or(0x000000FF);
    let lh = ctx.measurer.line_height(font_size);
    list.items.push(DisplayItem::Text { x, y, text, color, size: font_size, mono: false });
    y + lh
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
}
