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
    /// SOMBRA de caixa (`box-shadow`): pintada ATRÁS da caixa. `dx`/`dy` deslocam,
    /// `blur` amacia a borda, `spread` cresce/encolhe o rect, `color` é a cor (com
    /// alpha). O backend usa o blur real do egui (`epaint::Shadow`).
    Shadow { rect: Rect, dx: f32, dy: f32, blur: f32, spread: f32, color: u32, radius: f32 },
    /// Retângulo com GRADIENTE LINEAR (`background: linear-gradient(...)`). Interpola
    /// `c0`→`c1` ao longo do ângulo `angle_deg` (0=para cima, 90=para a direita, como
    /// o CSS). O backend pinta como mesh de 4 vértices coloridos. `radius` arredonda
    /// (aproximado — o mesh não recorta os cantos; suficiente p/ heros/botões).
    GradientRect { rect: Rect, c0: u32, c1: u32, angle_deg: f32, radius: f32 },
    /// Borda (contorno) de uma caixa, espessura `width`, na cor dada.
    Border { rect: Rect, width: f32, color: u32, radius: f32 },
    /// IMAGEM (`<img>` / background-image) — um bitmap RGBA8 já decodificado. O
    /// `pixels_handle` é um Buffer no HandleTable com `img_w*img_h*4` bytes RGBA
    /// (a partir do offset `pixels_off`); o backend sobe como textura e pinta no
    /// `rect` (escalando). Decodificação/download acontecem ANTES (no browser .ts,
    /// via fetchBytes+imgdec); o rts-dom só carrega o handle+dims — segue wasm-safe.
    Image { rect: Rect, pixels_handle: u64, pixels_off: u32, img_w: u32, img_h: u32 },
    /// Texto numa posição (canto superior-esquerdo). `mono` escolhe a família
    /// monoespaçada. `letter_spacing` = espaço extra entre glifos (px). `decoration`
    /// = linha decorativa (0=nenhuma, 1=underline, 2=line-through, 3=overline). O
    /// backend resolve a fonte/atlas; aqui só o necessário.
    Text {
        x: f32,
        y: f32,
        text: String,
        color: u32,
        size: f32,
        mono: bool,
        bold: bool,
        letter_spacing: f32,
        decoration: u8,
    },
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

impl DisplayList {
    /// HIT-TEST: o nó sob o ponto `(x, y)` em COORDENADAS DE CONTEÚDO (o backend
    /// converte tela→conteúdo somando o offset de scroll antes de chamar). Entre
    /// todos os `node_rects` que contêm o ponto, vence o de MENOR ÁREA — numa
    /// árvore DOM os rects dos descendentes estão contidos nos dos ancestrais,
    /// então menor área = nó mais profundo = top-most (o critério do north-star
    /// §3, "ordem reversa da display list", sem depender de ordem num HashMap).
    /// Limite documentado: irmãos sobrepostos por position/z-index podem resolver
    /// pro rect menor em vez do maior z — aceito na v1 (raro em fluxo normal).
    ///
    /// Sem desempate — ver [`DisplayList::hit_test_by`] para o caso (comum!) de
    /// ancestral e descendente com rects IDÊNTICOS.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<NodeIdx> {
        self.hit_test_by(x, y, |_| 0)
    }

    /// [`DisplayList::hit_test`] com DESEMPATE por profundidade: `depth_of` devolve
    /// a profundidade do nó na árvore, e num empate de área vence o MAIS PROFUNDO.
    ///
    /// O desempate não é um refinamento cosmético — sem ele o hit-test é
    /// NÃO-DETERMINÍSTICO no caso mais comum que existe. Um `<a>` de bloco cujo
    /// único filho é um `<span>` de bloco produz rects IDÊNTICOS para os dois (e
    /// para o `<div>` que os envolve): mesma origem, mesma largura de linha, mesma
    /// altura. Com áreas iguais, o `<` estrito nunca troca o melhor, então quem
    /// vence é simplesmente quem o `HashMap` iterou primeiro — ordem que a std
    /// deliberadamente não define e que muda entre execuções. Medido: a mesma
    /// página resolvia ora para o `<a>`, ora para o `<body>`.
    ///
    /// Profundidade é o critério certo porque reproduz o que "menor área" só
    /// aproxima: o alvo de um evento é o nó mais interno que contém o ponto.
    pub fn hit_test_by(&self, x: f32, y: f32, depth_of: impl Fn(NodeIdx) -> u32) -> Option<NodeIdx> {
        let mut best: Option<(NodeIdx, f32, u32)> = None;
        for (&idx, r) in &self.node_rects {
            if x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h {
                let area = r.w * r.h;
                let better = match best {
                    None => true,
                    // Área estritamente menor vence; área igual (dentro de um épsilon
                    // relativo a f32) é desempatada pela profundidade.
                    Some((_, a, d)) => {
                        if (area - a).abs() <= a * 1e-6 {
                            depth_of(idx) > d
                        } else {
                            area < a
                        }
                    }
                };
                if better {
                    best = Some((idx, area, depth_of(idx)));
                }
            }
        }
        best.map(|(idx, _, _)| idx)
    }
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
/// `em`/`rem` e do texto sem `font-size`. **16px, o default de todo browser**
/// (era 20, o que inflava cada `em`/`rem` em 25% — `max-width:42em` dava 840 em
/// vez dos 672 do Chrome; validado número-a-número no cover).
pub const DEFAULT_FONT_SIZE: f32 = 16.0;

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
    // informa o viewport à CASCADE (base de vw/vh no font-size fluido/calc; o
    // memo de estilo do Dom invalida sozinho se mudou).
    dom.set_viewport(ctx.viewport_w, ctx.viewport_h);
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
        // o containing block da raiz é a VIEWPORT: `height:100%` no <html> resolve
        // contra a altura da janela (base do `h-100` de páginas reais).
        let (_, h) =
            layout_block(dom, child, 0.0, cursor_y, ctx.viewport_w, Some(ctx.viewport_h), None, None, false, ctx, &mut list);
        cursor_y += h;
    }
    list.content_height = cursor_y;
    // ── PASSADA OUT-OF-FLOW: `position:absolute/fixed` saíram do fluxo (não
    // ocuparam espaço); pinta cada um contra o VIEWPORT com top/right/bottom/left,
    // por cima do fluxo (apêndice da lista = z maior; sem z-index real). V1: o
    // containing block é sempre a viewport (o de `absolute` — ancestral positioned
    // — e o "fica fixo ao rolar" do `fixed` são a v2).
    let mut out_of_flow = Vec::new();
    collect_out_of_flow(dom, dom.root, &mut out_of_flow);
    // Z-INDEX: ordena por z-index (menor pinta primeiro = fica atrás). Sort ESTÁVEL:
    // z-index igual (ou ambos auto=0) preserva a ordem do documento. Cobre o caso
    // comum (modais/dropdowns/overlays posicionados que se sobrepõem).
    out_of_flow.sort_by_key(|&id| {
        dom.computed_style_idx(id).and_then(|c| c.z_index).unwrap_or(0)
    });
    // O rect do containing block de cada abs é lido do `node_rects` JÁ preenchido
    // pelo fluxo normal (o ancestral positioned já foi pintado). Clona antes do
    // empréstimo mutável de `list`.
    let flow_rects = list.node_rects.clone();
    for id in out_of_flow {
        layout_out_of_flow(dom, id, ctx, &flow_rects, &mut list);
    }
    list
}

/// O rect do CONTAINING BLOCK de um `position:absolute` = o ancestral mais próximo
/// com `position != static` (relative/absolute/fixed), lido do `node_rects` do
/// fluxo. `None` = nenhum ancestral positioned → o containing block é a viewport
/// (a raiz inicial). Um `fixed` sempre usa a viewport (tratado no caller).
fn containing_block_rect(
    dom: &Dom,
    id: NodeIdx,
    flow_rects: &std::collections::HashMap<NodeIdx, Rect>,
) -> Option<Rect> {
    let mut cur = dom.node(id).parent;
    while let Some(p) = cur {
        let positioned = dom
            .computed_style_idx(p)
            .and_then(|c| c.position)
            .map(|pos| pos != crate::style::Position::Static)
            .unwrap_or(false);
        if positioned {
            if let Some(r) = flow_rects.get(&p) {
                return Some(*r);
            }
        }
        cur = dom.node(p).parent;
    }
    None
}

/// DFS que coleta os nós `position:absolute/fixed`. Não desce DENTRO de um
/// out-of-flow (os filhos dele pertencem ao layout dele; abs-dentro-de-abs = v2).
fn collect_out_of_flow(dom: &Dom, id: NodeIdx, out: &mut Vec<NodeIdx>) {
    for &child in &dom.node(id).children {
        if is_out_of_flow(dom, child) {
            out.push(child);
        } else {
            collect_out_of_flow(dom, child, out);
        }
    }
}

/// Layouta UM nó fora do fluxo contra o viewport: mede shrink-to-fit e posiciona
/// pelos offsets (`left` OU `right`−largura; `top` OU `bottom`−altura; sem nenhum
/// dos dois no eixo → 0).
fn layout_out_of_flow(
    dom: &Dom,
    id: NodeIdx,
    ctx: &LayoutCtx,
    flow_rects: &std::collections::HashMap<NodeIdx, Rect>,
    list: &mut DisplayList,
) {
    let css = dom.computed_style_idx(id).unwrap_or_default();
    // CONTAINING BLOCK: `absolute` posiciona contra o ancestral positioned mais
    // próximo (o Google ancora os ícones no canto direito da CAIXA DE BUSCA, não
    // da tela); `fixed` sempre contra a viewport. Sem ancestral positioned →
    // viewport. `cb` = (origem_x, origem_y, largura, altura) do container.
    let is_fixed = matches!(css.position, Some(crate::style::Position::Fixed));
    let cb = if is_fixed {
        Rect::new(0.0, 0.0, ctx.viewport_w, ctx.viewport_h)
    } else {
        containing_block_rect(dom, id, flow_rects)
            .unwrap_or_else(|| Rect::new(0.0, 0.0, ctx.viewport_w, ctx.viewport_h))
    };
    let resolve = ResolveCtx {
        parent_content_w: cb.w,
        node_font_size: font_px(&css, DEFAULT_FONT_SIZE),
        root_font_size: DEFAULT_FONT_SIZE,
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    // mede (w, h) numa lista descartável para resolver right/bottom.
    let mut scratch = DisplayList::default();
    let (w, h) = layout_block(dom, id, 0.0, 0.0, cb.w, Some(cb.h), None, None, true, ctx, &mut scratch);
    let left = resolve_inset(css.inset_left, cb.w, &resolve);
    let right = resolve_inset(css.inset_right, cb.w, &resolve);
    let top = resolve_inset(css.inset_top, cb.h, &resolve);
    let bottom = resolve_inset(css.inset_bottom, cb.h, &resolve);
    // Os offsets são RELATIVOS ao container: soma a origem do containing block.
    let x = match (left, right) {
        (Some(l), _) => cb.x + l,
        (None, Some(r)) => cb.x + cb.w - w - r,
        (None, None) => cb.x,
    };
    let y = match (top, bottom) {
        (Some(t), _) => cb.y + t,
        (None, Some(b)) => cb.y + cb.h - h - b,
        (None, None) => cb.y,
    };
    layout_block(dom, id, x, y, cb.w, Some(cb.h), None, None, true, ctx, list);
}

/// Resolve um offset de posicionamento (`top`/`left`/…): px SEM clamp (negativo
/// desloca para fora — badges/tooltips); `%` contra o eixo do viewport dado.
fn resolve_inset(d: Option<crate::style::Dimension>, axis: f32, ctx: &ResolveCtx) -> Option<f32> {
    match d? {
        crate::style::Dimension::Px(v) => Some(v),
        crate::style::Dimension::Percent(p) => Some(axis * p / 100.0),
        other => other.resolve(ctx),
    }
}

/// `true` se o nó SAI do fluxo (`position: absolute/fixed`) — não ocupa espaço
/// entre os irmãos; pintado na passada out-of-flow de [`layout_document`].
fn is_out_of_flow(dom: &Dom, id: NodeIdx) -> bool {
    matches!(&dom.node(id).kind, NodeKind::Element { .. })
        && dom
            .computed_style_idx(id)
            .and_then(|c| c.position)
            .map(|p| p.out_of_flow())
            .unwrap_or(false)
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
    // Altura do CONTENT do containing block, quando DEFINIDA (height explícito no
    // pai / viewport na raiz): a base de `height: %` — que resolve contra a ALTURA
    // do pai (antes resolvia errado contra a largura; `h-100` não funcionava).
    // `None` = pai com altura auto → `height: %` vira auto (fiel ao browser).
    avail_h: Option<f32>,
    // Largura OUTER IMPOSTA (com margem) — o flex resolveu grow/shrink e DITA o
    // main size do item; vence width/min-max/shrink-to-fit (v1: clamp min/max no
    // resolve flex fica como corte documentado). `None` = fluxo normal.
    forced_outer_w: Option<f32>,
    // Altura OUTER IMPOSTA (com margem) — o `align-items/self: stretch` do flex.
    // O caller só passa para item SEM height explícito. `None` = altura natural.
    forced_outer_h: Option<f32>,
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
            // `<input>`/`<textarea>` editável (mini-browser): void, sem filhos — o
            // "conteúdo" é o texto do value/placeholder + cursor. Caminho próprio,
            // fora do fluxo de bloco genérico (que desceria em filhos inexistentes).
            if is_text_input_tag(tag) {
                let itype = dom
                    .node(id)
                    .attr("type")
                    .map(|t| t.to_ascii_lowercase())
                    .unwrap_or_default();
                // `type=hidden`: invisível e sem espaço (o form legado do google
                // tem 5 — viravam caixas de texto fantasmas).
                if itype == "hidden" {
                    return (0.0, 0.0);
                }
                // `type=submit/button/reset`: BOTÃO — caixa cinza UA com o value
                // como rótulo (não editável). O suficiente p/ o "Pesquisa Google".
                if matches!(itype.as_str(), "submit" | "button" | "reset") {
                    return layout_button(dom, id, &css, x, y, ctx, list);
                }
                return layout_input(dom, id, &css, x, y, avail_w, forced_outer_w, ctx, list);
            }
            // `<img>` com pixels decodificados: emite a imagem no rect (tamanho do CSS
            // width/height, senão o natural da imagem). Void — sem filhos.
            if tag == "img" {
                if let Some(img) = layout_image(dom, id, &css, x, y, avail_w, ctx, list) {
                    return img;
                }
                // sem pixels ainda (não baixou/decodificou): ocupa 0 (não pinta nada).
            }
            // `<svg>` é um REPLACED element: não desenhamos o vetor, mas RESERVAMOS
            // a caixa (dimensões do CSS width/height, dos atributos, ou da razão do
            // `viewBox`) e pintamos um placeholder cinza — assim a estrutura da
            // página fica correta mesmo sem o SVG (logo/ícones do google ocupam o
            // espaço certo em vez de colapsar pra 0×0).
            if tag == "svg" {
                if let Some(r) = layout_svg_placeholder(dom, id, &css, x, y, avail_w, ctx, list) {
                    return r;
                }
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
                letter_spacing: 0.0,
                decoration: 0,
            });
            return (tw, lh);
        }
        _ => return (0.0, 0.0), // Comment / Document aninhado: não pinta.
    };

    // ── Box model (content-box): resolve as bordas/espaços absolutos ─────────────
    // O contexto de RESOLUÇÃO tardia primeiro (margens/paddings agora aceitam
    // unidades relativas — `p-3` = 1rem do Bootstrap — e resolvem AQUI, como width).
    let resolve = ResolveCtx {
        parent_content_w: avail_w,
        node_font_size: font_px(&css, DEFAULT_FONT_SIZE),
        root_font_size: DEFAULT_FONT_SIZE,
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    // Margin/padding POR LADO (Edges). O `margin_v` (UA-stylesheet, só vertical) é
    // somado ao top/bottom. Margens são SIGNED (negativa puxa — gutters `.row`);
    // padding é clampado ≥ 0 (padding negativo não existe no CSS).
    let m = &css.margin;
    let p = &css.padding;
    let mut margin_left = m.left.resolve(&resolve).unwrap_or(0.0);
    let mut margin_right = m.right.resolve(&resolve).unwrap_or(0.0);
    // margin_v (UA-stylesheet) só vale no lado que o AUTOR NÃO declarou — um
    // `margin-top: 0` explícito ANULA o default da UA naquele lado (era o brand
    // do cover descendo 16px apesar do `h3 { margin-top: 0 }` do Bootstrap).
    let margin_v_extra = css.margin_v.unwrap_or(0.0);
    let mv_top = if m.top == crate::style::Side::Unset { margin_v_extra } else { 0.0 };
    let mv_bottom = if m.bottom == crate::style::Side::Unset { margin_v_extra } else { 0.0 };
    let margin_top = m.top.resolve(&resolve).unwrap_or(0.0) + mv_top;
    let margin_bottom = m.bottom.resolve(&resolve).unwrap_or(0.0) + mv_bottom;
    let pad_left = p.left.resolve(&resolve).unwrap_or(0.0).max(0.0);
    let pad_right = p.right.resolve(&resolve).unwrap_or(0.0).max(0.0);
    let pad_top = p.top.resolve(&resolve).unwrap_or(0.0).max(0.0);
    let pad_bottom = p.bottom.resolve(&resolve).unwrap_or(0.0).max(0.0);
    let border = css.border_width.unwrap_or(0.0);
    // Atalhos para o eixo (horizontal = left+right): a maioria do box model usa o
    // total por eixo. (`margin_h`/`padding_h` = soma do eixo horizontal.)
    let margin_h = margin_left + margin_right;
    let padding_h = pad_left + pad_right;
    // `frame` horizontal = o que cerca o content no eixo X (margin+border+padding
    // dos DOIS lados). border conta 2× (left+right); padding/margin já são a soma.
    let frame = margin_h + 2.0 * border + padding_h;
    let font_for_content = font_px(&css, DEFAULT_FONT_SIZE);
    let border_box = css.border_box.unwrap_or(false);
    let content_w = if let Some(fw) = forced_outer_w {
        // main size do FLEX (grow/shrink já resolvidos): outer imposto → content =
        // outer - frame (o frame já soma margem+borda+padding dos dois lados).
        // Vence width/min-max (o clamp no resolve flex é corte documentado).
        (fw - frame).max(0.0)
    } else {
        let base = match css.width.and_then(|d| d.resolve(&resolve)) {
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
        // CLAMP min/max-width (#1751): `used = clamp(min, width, max)`. min/max são
        // sobre a CAIXA (border-box) na spec — descontamos o frame p/ aplicar ao
        // content quando border-box; em content-box já são do content.
        let mnw = css.min_width.and_then(|d| d.resolve(&resolve)).map(|v| {
            if border_box { (v - (padding_h + 2.0 * border)).max(0.0) } else { v }
        });
        let mxw = css.max_width.and_then(|d| d.resolve(&resolve)).map(|v| {
            if border_box { (v - (padding_h + 2.0 * border)).max(0.0) } else { v }
        });
        crate::style::clamp_size(base, mnw, mxw)
    };

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
    let font_size = font_px(&css, DEFAULT_FONT_SIZE);

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

    // `height` EXPLÍCITO resolve ANTES dos filhos (não depende deles): eles o
    // recebem como containing-block height (base do `height:%` deles), e o flex
    // COLUMN o usa como referência do eixo principal (justify/margin-auto).
    let frame_v = pad_top + pad_bottom + 2.0 * border;
    let explicit_content_h = resolve_height(css.height, avail_h, &resolve)
        .map(|h| if border_box { (h - frame_v).max(0.0) } else { h })
        // `aspect-ratio`: sem height explícito, a altura vem da largura / razão. Só
        // quando há largura resolvida (content_w) e uma razão > 0.
        .or_else(|| {
            css.aspect_ratio
                .filter(|r| *r > 0.0)
                .map(|r| (content_w / r).max(0.0))
        })
        // ALTURA IMPOSTA pelo flex (grow/stretch): o `forced_outer_h` é a altura
        // OUTER do item — o content-box é ela menos margem-v/frame. Vira o
        // containing block dos filhos (um filho `height:100%` resolve contra ela),
        // resolvendo o logo/caixa do google que crescem via flex-grow vertical.
        .or_else(|| {
            forced_outer_h.map(|oh| {
                let mv = margin_top + margin_bottom;
                (oh - mv - frame_v).max(0.0)
            })
        });

    // Altura que serve de CONTAINING BLOCK aos filhos (`height:%`): o height
    // explícito, senão um `max-height` conhecido (o Google dá ao container do
    // logo `height:calc(100% - 560px); max-height:290px` — o max é a altura
    // efetiva; sem isso o filho `height:100%` resolvia contra o conteúdo e
    // inflava). Calculado ANTES de layoutar os filhos (a resolução do `%` do
    // filho é top-down; a spec exige o CB conhecido).
    let mnh_pre = resolve_height(css.min_height, avail_h, &resolve)
        .map(|v| if border_box { (v - frame_v).max(0.0) } else { v });
    let mxh_pre = resolve_height(css.max_height, avail_h, &resolve)
        .map(|v| if border_box { (v - frame_v).max(0.0) } else { v });
    let avail_children = explicit_content_h.or(mxh_pre);

    // `flex-direction: column` — o eixo PRINCIPAL do flex vira o vertical: os itens
    // empilham (sem margin-collapse, que flex não tem), gap/justify/margin-auto
    // atuam no Y e align-items no X (stretch = ocupar a largura, o default).
    let is_column = css.flex_direction.map(|f| f.is_column()).unwrap_or(false);
    let is_flex = display == crate::block::DISPLAY_HORIZONTAL || display == crate::block::DISPLAY_WRAP;
    let content_h = match display {
        // flex column (com ou sem wrap — multi-coluna do wrap é corte documentado).
        _ if is_flex && is_column => {
            layout_children_column(dom, id, content_x, content_y, children_w, avail_children, &css, font_size, ctx, list)
        }
        // horizontal (flex-row sem wrap): lado a lado, encolhe pra caber, não quebra.
        d if d == crate::block::DISPLAY_HORIZONTAL => {
            layout_children_horizontal(dom, id, content_x, content_y, children_w, avail_children, &css, font_size, false, None, ctx, list)
        }
        // GRID REAL: track-sizing (px/fr/auto/%) + auto-placement row-by-row +
        // alinhamento de célula (align-items/justify-items). Só quando é
        // `display:grid` de fato; senão o wrap horizontal (inline-block flow).
        _ if css.effective_display() == Some(crate::style::DisplayKind::Grid) => {
            layout_children_grid(dom, id, content_x, content_y, children_w, avail_children, &css, font_size, ctx, list)
        }
        // wrap (inline-block flow): lado a lado E QUEBRA linha quando enche.
        d if d == crate::block::DISPLAY_WRAP => {
            layout_children_horizontal(dom, id, content_x, content_y, children_w, avail_children, &css, font_size, true, None, ctx, list)
        }
        // vertical (block): empilha.
        _ => layout_children_vertical(dom, id, content_x, content_y, children_w, avail_children, &css, font_size, ctx, list),
    };
    // a altura REAL do conteúdo (antes de `height` explícito a cortar) — p/ o scroll-Y.
    let content_h_natural = content_h;

    // `height` explícito SOBRESCREVE a altura do conteúdo (a caixa tem essa altura,
    // mesmo que o conteúdo seja menor) — já resolvido antes dos filhos.
    let content_h = explicit_content_h.unwrap_or(content_h);
    // CLAMP min/max-height (#1751): used = clamp(min, height, max) — eixo vertical
    // (`%` contra a ALTURA do containing block, como o height).
    let content_h = crate::style::clamp_size(content_h, mnh_pre, mxh_pre);
    // STRETCH do flex: altura OUTER imposta pelo container (align-items/self:
    // stretch) → content = outer - margens - frame_v; nunca ENCOLHE o conteúdo
    // (max com o natural — um item mais alto que a linha não é cortado).
    let content_h = match forced_outer_h {
        Some(fh) => (fh - margin_top - margin_bottom - frame_v).max(content_h),
        None => content_h,
    };

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
        // `opacity` do elemento: multiplica o ALPHA das cores próprias (fundo/borda).
        // Cobre o caso comum (card/botão/overlay com fade) sem grupo de compositing.
        let op = css.opacity.unwrap_or(1.0);
        // Insere na ordem: primeiro o fundo, depois a borda por cima dele (ambos
        // atrás dos filhos). `insert` desloca os filhos para a frente.
        let mut at = box_index;
        // SOMBRA primeiro (atrás de tudo): box-shadow.
        if let Some(sh) = css.box_shadow {
            list.items.insert(at, DisplayItem::Shadow {
                rect: box_rect,
                dx: sh.dx,
                dy: sh.dy,
                blur: sh.blur,
                spread: sh.spread,
                color: apply_opacity(sh.color, op),
                radius,
            });
            at += 1;
        }
        // FUNDO: gradiente (se houver) OU cor sólida.
        if let Some(g) = css.gradient {
            list.items.insert(at, DisplayItem::GradientRect {
                rect: box_rect,
                c0: apply_opacity(g.c0, op),
                c1: apply_opacity(g.c1, op),
                angle_deg: g.angle_deg,
                radius,
            });
            at += 1;
        } else if let Some(color) = css.bg {
            let color = apply_opacity(color, op);
            list.items.insert(at, DisplayItem::SolidRect { rect: box_rect, color, radius });
            at += 1;
        }
        // A borda só pinta se tem largura E um `border-style` VISÍVEL. O default
        // CSS de border-style é `none` → sem `border-style` declarado, NÃO pinta
        // (fiel ao Chrome: `border-width:2px` sozinho dá borda invisível).
        let style_visible = css.border_style.map(|s| s.is_visible()).unwrap_or(false);
        if border > 0.0 && style_visible {
            let color = apply_opacity(css.border_color.unwrap_or(0x808080FF), op);
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
            // MESMA contagem da emissão acima: sombra + (gradiente OU bg) + borda.
            css.box_shadow.is_some() as usize
                + (css.gradient.is_some() || css.bg.is_some()) as usize
                + (border > 0.0 && style_visible) as usize
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

    // ── TRANSFORM (translate/scale/rotate): pós-processa os itens DESTE elemento e
    // seus descendentes (o range `[box_index..]`), em torno do CENTRO do border-box.
    // Aplicado por último (não afeta o fluxo/tamanho — como no CSS, transform é visual).
    if let Some(tf) = css.transform {
        if !tf.is_identity() {
            let cx = box_rect.x + box_rect.w / 2.0;
            let cy = box_rect.y + box_rect.h / 2.0;
            // translate em px + fração do tamanho do elemento (translate(-50%,-50%)).
            let tx = tf.tx + tf.tx_pct * box_rect.w;
            let ty = tf.ty + tf.ty_pct * box_rect.h;
            let (sin, cos) = tf.rot_deg.to_radians().sin_cos();
            for it in list.items[box_index..].iter_mut() {
                apply_transform_to_item(it, cx, cy, tx, ty, tf.sx, tf.sy, sin, cos);
            }
        }
    }

    // Tamanho EXTERNO da caixa (outer = content + padding + border + margin) — cada
    // componente já é a SOMA do seu eixo (padding_h = left+right; margin_h idem;
    // border conta 2× pelos dois lados). Não multiplicar margin/padding por 2.
    let outer_w = content_w + padding_h + 2.0 * border + margin_h;
    let outer_h = content_h + pad_top + pad_bottom + 2.0 * border + margin_top + margin_bottom;
    (outer_w, outer_h)
}

/// Aplica um transform (translate `tx,ty` + escala `sx,sy` + rotação `sin,cos`) em
/// torno do centro `(cx,cy)` a um DisplayItem, mutando suas coords. Rects escalam de
/// tamanho; a rotação move o canto (aproximação: rotaciona a posição, não o próprio
/// rect — cobre o uso comum sem mesh rotacionado). Texto/pos: rotaciona+escala o ponto.
#[allow(clippy::too_many_arguments)]
fn apply_transform_to_item(
    it: &mut DisplayItem,
    cx: f32,
    cy: f32,
    tx: f32,
    ty: f32,
    sx: f32,
    sy: f32,
    sin: f32,
    cos: f32,
) {
    // transforma UM ponto: escala em torno do centro, rotaciona, translada.
    let xf = |px: f32, py: f32| -> (f32, f32) {
        let (mut dx, mut dy) = (px - cx, py - cy);
        dx *= sx;
        dy *= sy;
        let rx = dx * cos - dy * sin;
        let ry = dx * sin + dy * cos;
        (cx + rx + tx, cy + ry + ty)
    };
    match it {
        DisplayItem::SolidRect { rect, .. }
        | DisplayItem::Border { rect, .. }
        | DisplayItem::GradientRect { rect, .. }
        | DisplayItem::Shadow { rect, .. }
        | DisplayItem::Image { rect, .. } => {
            let (nx, ny) = xf(rect.x, rect.y);
            rect.x = nx;
            rect.y = ny;
            rect.w *= sx;
            rect.h *= sy;
        }
        DisplayItem::Text { x, y, size, .. } => {
            let (nx, ny) = xf(*x, *y);
            *x = nx;
            *y = ny;
            *size *= sy; // escala o texto na vertical (aproxima).
        }
        DisplayItem::BeginClip { rect, .. } => {
            let (nx, ny) = xf(rect.x, rect.y);
            rect.x = nx;
            rect.y = ny;
            rect.w *= sx;
            rect.h *= sy;
        }
        DisplayItem::EndClip => {}
    }
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
        // fora do fluxo não contribui para a largura intrínseca do container.
        if is_out_of_flow(dom, child) {
            continue;
        }
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
            let f = font_px(&css, parent_font);
            let border_box = css.border_box.unwrap_or(false);
            let resolve = ResolveCtx {
                parent_content_w: ctx.viewport_w,
                node_font_size: f,
                root_font_size: DEFAULT_FONT_SIZE,
                viewport_w: ctx.viewport_w,
                viewport_h: ctx.viewport_h,
            };
            let frame = css.margin.resolve_h(&resolve)
                + 2.0 * css.border_width.unwrap_or(0.0)
                + css.padding.resolve_h(&resolve);
            // width fixo: a caixa tem essa largura.
            if let Some(w) = css.width.and_then(|d| d.resolve(&resolve)) {
                return if border_box { w + css.margin.resolve_h(&resolve) } else { w + frame };
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
            // `<img>` com imagem decodificada é um elemento REPLACED (conteúdo visual
            // intrínseco) → precisa de layout_block p/ emitir o DisplayItem::Image,
            // mesmo sem CSS de caixa.
            if tag == "img" && dom.image_of(id).is_some() {
                return true;
            }
            // `<svg>` é replaced: layout_svg_placeholder reserva a caixa (logo/
            // ícones do google ocupam o espaço certo em vez de colapsar).
            if tag == "svg" {
                return true;
            }
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
            let css = dom.computed_style_idx(id);
            let explicit_block = css
                .as_ref()
                .and_then(|c| c.effective_display())
                .map(|d| d != crate::style::DisplayKind::Inline)
                .unwrap_or(false);
            // `<input>`/`<button>`/`<select>`/`<textarea>` são inline-block por
            // DEFAULT do browser (fluem lado a lado — os botões do google), a não
            // ser que o CSS do autor force um display de bloco. É o que faz os 2
            // `<input type=submit>` irmãos ficarem na MESMA linha.
            if matches!(tag.as_str(), "input" | "button" | "select" | "textarea") {
                return !explicit_block;
            }
            // tag block conhecida OU display de bloco explícito → NÃO é inline-block.
            if crate::block::lookup(tag).is_some() || explicit_block {
                return false;
            }
            // é inline-com-box (tem caixa mas é tag inline) → inline-block.
            css.as_ref().map(|c| c.has_box() || c.height.is_some()).unwrap_or(false)
        }
        _ => false,
    }
}

/// Código de decoração de texto p/ o `DisplayItem::Text` a partir do estilo:
/// 0=nenhuma, 1=underline, 2=line-through, 3=overline.
fn decoration_code(css: &ComputedStyle) -> u8 {
    match css.text_decoration {
        Some(crate::style::values::TextDecoration::Underline) => 1,
        Some(crate::style::values::TextDecoration::LineThrough) => 2,
        Some(crate::style::values::TextDecoration::Overline) => 3,
        _ => 0,
    }
}

/// Multiplica o ALPHA de uma cor `0xRRGGBBAA` por `opacity` ∈ [0,1] (o RGB fica
/// intacto; só o canal alpha escala). `opacity >= 1` devolve a cor inalterada.
fn apply_opacity(color: u32, opacity: f32) -> u32 {
    if opacity >= 1.0 {
        return color;
    }
    let op = opacity.clamp(0.0, 1.0);
    let a = (color & 0xFF) as f32;
    let new_a = (a * op).round().clamp(0.0, 255.0) as u32;
    (color & 0xFFFF_FF00) | new_a
}

/// `true` se a tag é um campo de TEXTO editável (mini-browser): `<input>` (tipos
/// textuais) ou `<textarea>`. Um `<input type=checkbox/radio/...>` não conta (v1
/// só faz texto). Sem `type` → texto (o default do HTML).
fn is_text_input_tag(tag: &str) -> bool {
    matches!(tag, "input" | "textarea")
}

/// Layout de um `<input>`/`<textarea>` editável: emite a CAIXA (fundo+borda), o
/// TEXTO (o valor digitado, ou o `placeholder` apagado se vazio) e, se o campo tem
/// o FOCO, um CURSOR (barrinha) após o texto. Void (sem filhos) — o egui só recebe
/// SolidRect+Text+SolidRect e pinta burramente. Retorna `(outer_w, outer_h)`.
#[allow(clippy::too_many_arguments)]
/// Layout de um `<img>` com pixels já decodificados: emite `DisplayItem::Image` no
/// rect. Tamanho: `width`/`height` do CSS se houver; senão o natural da imagem (mas
/// limitado à largura disponível, preservando a proporção). `None` se o `<img>` ainda
/// não tem imagem setada (nada a pintar). Retorna `(outer_w, outer_h)`.
#[allow(clippy::too_many_arguments)]
/// Reserva a CAIXA de um `<svg>` (replaced element) sem desenhar o vetor: usa
/// `width`/`height` do CSS ou dos atributos; se só um lado é dado e há `viewBox`,
/// deriva o outro pela razão de aspecto; se nada, cai numa proporção do viewBox
/// ou num tamanho default. Pinta um placeholder cinza-claro (a "caixa" do ícone/
/// logo) no rect. `None` se não dá pra dimensionar (colapsa como antes).
fn layout_svg_placeholder(
    dom: &Dom,
    id: NodeIdx,
    css: &ComputedStyle,
    x: f32,
    y: f32,
    avail_w: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> Option<(f32, f32)> {
    let font = font_px(css, DEFAULT_FONT_SIZE);
    let resolve = ResolveCtx {
        parent_content_w: avail_w,
        node_font_size: font,
        root_font_size: DEFAULT_FONT_SIZE,
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let node = dom.node(id);
    let attr_px = |name: &str| -> Option<f32> {
        node.attr(name).and_then(|v| {
            let v = v.trim().trim_end_matches("px");
            v.parse::<f32>().ok().filter(|n| *n > 0.0)
        })
    };
    // razão de aspecto do viewBox ("0 0 W H" → W/H).
    let vb_ratio = node.attr("viewBox").and_then(|vb| {
        let n: Vec<f32> = vb.split_whitespace().filter_map(|s| s.parse().ok()).collect();
        if n.len() == 4 && n[3] > 0.0 { Some(n[2] / n[3]) } else { None }
    });
    let css_w = css.width.and_then(|d| d.resolve(&resolve)).filter(|w| *w > 0.0);
    let css_h = css.height.and_then(|d| resolve_height(Some(d), None, &resolve)).filter(|h| *h > 0.0);
    let w0 = css_w.or_else(|| attr_px("width"));
    let h0 = css_h.or_else(|| attr_px("height"));
    // resolve (w, h): ambos dados usa-os; só um + viewBox deriva o outro; nada →
    // um ícone default (24×24) ou o viewBox escalado a 24 de altura.
    let (w, h) = match (w0, h0) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, vb_ratio.map(|r| w / r).unwrap_or(w)),
        (None, Some(h)) => (vb_ratio.map(|r| h * r).unwrap_or(h), h),
        (None, None) => {
            let h = 24.0;
            (vb_ratio.map(|r| h * r).unwrap_or(h), h)
        }
    };
    let w = w.min(avail_w.max(1.0));
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    // placeholder cinza-claro (a caixa do ícone) — só quando não é minúsculo demais.
    list.items.push(DisplayItem::SolidRect {
        rect: Rect::new(x, y, w, h),
        color: 0xE8EAEDFF,
        radius: 2.0,
    });
    list.node_rects.insert(id, Rect::new(x, y, w, h));
    Some((w, h))
}

fn layout_image(
    dom: &Dom,
    id: NodeIdx,
    css: &ComputedStyle,
    x: f32,
    y: f32,
    avail_w: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> Option<(f32, f32)> {
    let (handle, off, iw, ih) = dom.image_of(id)?;
    if handle == 0 || iw == 0 || ih == 0 {
        return None;
    }
    let font = font_px(css, DEFAULT_FONT_SIZE);
    let resolve = ResolveCtx {
        parent_content_w: avail_w,
        node_font_size: font,
        root_font_size: DEFAULT_FONT_SIZE,
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    // margens (respeita o CSS); a imagem em si é o content (sem padding/borda v1).
    let m = &css.margin;
    let margin_left = m.left.resolve(&resolve).unwrap_or(0.0);
    let margin_right = m.right.resolve(&resolve).unwrap_or(0.0);
    let margin_top = m.top.resolve(&resolve).unwrap_or(0.0);
    let margin_bottom = m.bottom.resolve(&resolve).unwrap_or(0.0);
    // largura/altura: CSS explícito, senão o atributo HTML `width`/`height` (comum em
    // `<img width=100 height=100>`), senão o natural. Se só um é dado, mantém a razão.
    let attr_px = |name: &str| -> Option<f32> {
        dom.node(id)
            .attr(name)
            .and_then(|v| v.trim().trim_end_matches("px").trim().parse::<f32>().ok())
            .filter(|v| *v >= 0.0)
    };
    let css_w = css.width.and_then(|d| d.resolve(&resolve)).or_else(|| attr_px("width"));
    let css_h = css.height.and_then(|d| d.resolve(&resolve)).or_else(|| attr_px("height"));
    let (nat_w, nat_h) = (iw as f32, ih as f32);
    let (mut w, mut h) = match (css_w, css_h) {
        (Some(cw), Some(ch)) => (cw, ch),
        (Some(cw), None) => (cw, cw * nat_h / nat_w),
        (None, Some(ch)) => (ch * nat_w / nat_h, ch),
        (None, None) => (nat_w, nat_h),
    };
    // não estoura a largura disponível (encolhe mantendo a razão).
    let max_w = (avail_w - margin_left - margin_right).max(0.0);
    if w > max_w && w > 0.0 {
        h = h * max_w / w;
        w = max_w;
    }
    let rect = Rect::new(x + margin_left, y + margin_top, w, h);
    list.node_rects.insert(id, rect);
    list.items.push(DisplayItem::Image {
        rect,
        pixels_handle: handle,
        pixels_off: off,
        img_w: iw,
        img_h: ih,
    });
    Some((w + margin_left + margin_right, h + margin_top + margin_bottom))
}

/// Layout de um `<input type=submit/button/reset>`: BOTÃO estilo UA — caixa
/// cinza-clara com borda e o `value` como rótulo (shrink-to-fit no texto). O CSS
/// do autor (bg/cor/padding) vence os defaults. Não editável, não focável (v1).
fn layout_button(
    dom: &Dom,
    id: NodeIdx,
    css: &ComputedStyle,
    x: f32,
    y: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> (f32, f32) {
    let font = font_px(css, DEFAULT_FONT_SIZE - 3.0);
    let label = dom.node(id).attr("value").unwrap_or("").to_string();
    let tw = ctx.measurer.text_width(&label, font, false, false);
    let lh = ctx.measurer.line_height(font);
    let (pad_h, pad_v) = (12.0, 5.0);
    let w = tw + 2.0 * pad_h;
    let h = lh + 2.0 * pad_v;
    let bg = css.bg.unwrap_or(0xF8F9FAFF); // cinza-claro UA (o do botão do google)
    let fg = css.color.unwrap_or(0x3C4043FF);
    list.items.push(DisplayItem::SolidRect {
        rect: Rect::new(x, y, w, h),
        color: bg,
        radius: css.corner_radius.unwrap_or(4.0),
    });
    list.items.push(DisplayItem::Border {
        rect: Rect::new(x, y, w, h),
        width: css.border_width.unwrap_or(1.0),
        color: css.border_color.unwrap_or(0xDADCE0FF),
        radius: css.corner_radius.unwrap_or(4.0),
    });
    list.items.push(DisplayItem::Text {
        x: x + pad_h,
        y: y + pad_v,
        text: label,
        color: fg,
        size: font,
        mono: false,
        bold: false,
        letter_spacing: 0.0,
        decoration: 0,
    });
    list.node_rects.insert(id, Rect::new(x, y, w, h));
    (w + 6.0, h + 4.0) // margenzinha UA entre botões
}

fn layout_input(
    dom: &Dom,
    id: NodeIdx,
    css: &ComputedStyle,
    x: f32,
    y: f32,
    avail_w: f32,
    forced_outer_w: Option<f32>,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> (f32, f32) {
    let font = font_px(css, DEFAULT_FONT_SIZE);
    let resolve = ResolveCtx {
        parent_content_w: avail_w,
        node_font_size: font,
        root_font_size: DEFAULT_FONT_SIZE,
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    // Box model do input. Padding default do browser (~2px 4px) quando o autor não
    // declara; borda 1px cinza default. margem respeita o CSS.
    let m = &css.margin;
    let p = &css.padding;
    let margin_left = m.left.resolve(&resolve).unwrap_or(0.0);
    let margin_right = m.right.resolve(&resolve).unwrap_or(0.0);
    let margin_top = m.top.resolve(&resolve).unwrap_or(0.0);
    let margin_bottom = m.bottom.resolve(&resolve).unwrap_or(0.0);
    let pad_left = p.left.resolve(&resolve).unwrap_or(4.0).max(0.0);
    let pad_right = p.right.resolve(&resolve).unwrap_or(4.0).max(0.0);
    let pad_top = p.top.resolve(&resolve).unwrap_or(3.0).max(0.0);
    let pad_bottom = p.bottom.resolve(&resolve).unwrap_or(3.0).max(0.0);
    let border = css.border_width.unwrap_or(1.0).max(0.0);
    let padding_h = pad_left + pad_right;
    let frame = margin_left + margin_right + 2.0 * border + padding_h;

    // Largura do CONTENT: `width` do CSS; senão o main-size imposto pelo flex; senão
    // um default de campo (~180px de content). Nunca excede o disponível.
    let border_box = css.border_box.unwrap_or(false);
    let content_w = if let Some(fw) = forced_outer_w {
        (fw - frame).max(0.0)
    } else if let Some(w) = css.width.and_then(|d| d.resolve(&resolve)) {
        if border_box { (w - (padding_h + 2.0 * border)).max(0.0) } else { w }
    } else {
        180.0_f32.min((avail_w - frame).max(0.0))
    };

    // Altura do CONTENT: `height` do CSS; senão uma linha de texto (line-height).
    let line_h = ctx.measurer.line_height(font);
    let content_h = css
        .height
        .and_then(|d| d.resolve(&resolve))
        .map(|h| if border_box { (h - (pad_top + pad_bottom + 2.0 * border)).max(0.0) } else { h })
        .unwrap_or(line_h);

    let box_rect = Rect::new(
        x + margin_left,
        y + margin_top,
        content_w + padding_h + 2.0 * border,
        content_h + pad_top + pad_bottom + 2.0 * border,
    );
    list.node_rects.insert(id, box_rect);

    // Fundo: o `background` do CSS, senão branco (campo de texto clássico).
    let radius = css.corner_radius.unwrap_or(0.0);
    let bg = css.bg.unwrap_or(0xFFFFFFFF);
    list.items.push(DisplayItem::SolidRect { rect: box_rect, color: bg, radius });
    // Borda: sempre desenha (o input tem contorno por padrão). Cor do CSS ou cinza.
    // Se o campo tem foco, realça a borda (azul), como o browser.
    let focused = dom.focused_input() == Some(id);
    let border_color = if focused {
        0x3B82F6FF // azul de foco
    } else {
        css.border_color.unwrap_or(0x9AA0A6FF)
    };
    let bw = if border > 0.0 { border } else { 1.0 };
    list.items.push(DisplayItem::Border { rect: box_rect, width: bw, color: border_color, radius });

    // Texto: o valor digitado, ou o placeholder apagado. Posicionado no content-box.
    let text_x = x + margin_left + bw + pad_left;
    let text_y = y + margin_top + bw + pad_top;
    let (shown, tcolor) = if dom.input_is_empty(id) {
        let ph = dom.node(id).attr("placeholder").unwrap_or("").to_string();
        (ph, 0x9AA0A6FF) // cinza apagado
    } else {
        (dom.input_value(id), css.color.unwrap_or(0x111111FF))
    };
    if !shown.is_empty() {
        list.items.push(DisplayItem::Text {
            x: text_x,
            y: text_y,
            text: shown.clone(),
            color: tcolor,
            size: font,
            mono: false,
            bold: false,
            letter_spacing: 0.0,
            decoration: 0,
        });
    }
    // Cursor: barrinha vertical após o texto do VALOR (não do placeholder), só com foco.
    if focused {
        let val = dom.input_value(id);
        let caret_x = text_x + ctx.measurer.text_width(&val, font, false, false) + 1.0;
        let caret = Rect::new(caret_x, text_y, 1.5, line_h.min(content_h.max(line_h)));
        list.items.push(DisplayItem::SolidRect { rect: caret, color: 0x111111FF, radius: 0.0 });
    }

    (
        box_rect.w + margin_left + margin_right,
        box_rect.h + margin_top + margin_bottom,
    )
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

/// O `font-size` COMPUTADO em pontos: a CASCADE já resolveu a forma para `Px`
/// (dom.rs — base de em/% é o pai; rem/vw/vh contra root/viewport), então aqui é
/// só extrair; `fallback` cobre o não-declarado (herda) e formas não-resolvidas.
fn font_px(css: &ComputedStyle, fallback: f32) -> f32 {
    match css.font_size {
        Some(crate::style::Dimension::Px(v)) => v,
        _ => fallback,
    }
}

/// Resolve uma dimensão do EIXO VERTICAL (`height`/`min-height`/`max-height`):
/// `%` resolve contra a ALTURA do containing block (não a largura — era o bug que
/// fazia `height:100%` virar 100% da largura do pai); as demais unidades usam o
/// ctx normal. `avail_h = None` (pai com altura auto) → `%` vira auto (`None`),
/// fiel ao browser.
fn resolve_height(
    d: Option<crate::style::Dimension>,
    avail_h: Option<f32>,
    ctx: &ResolveCtx,
) -> Option<f32> {
    match d? {
        crate::style::Dimension::Percent(p) => avail_h.map(|h| (h * p / 100.0).max(0.0)),
        // `calc(...)` num contexto de ALTURA: o componente `%` resolve contra a
        // ALTURA do containing block (avail_h), NÃO a largura — o `resolve`
        // genérico usa parent_content_w e daria `calc(100% - 560px)` = 1000-560
        // (largura) em vez de 800-560 (altura). Reconstrói a soma no eixo certo.
        crate::style::Dimension::Calc(c) => {
            let h = avail_h?;
            let v = c.px + h * c.pct / 100.0
                + ctx.node_font_size * c.em
                + ctx.root_font_size * c.rem
                + ctx.viewport_w * c.vw / 100.0
                + ctx.viewport_h * c.vh / 100.0;
            Some(v.max(0.0))
        }
        other => other.resolve(ctx),
    }
}

/// Empilha os filhos VERTICAL (cada um abaixo do anterior), ocupando a largura do
/// content. Devolve a altura TOTAL do content (soma das alturas dos filhos).
/// `avail_h` = altura do content DESTE container quando explícita (containing
/// block dos filhos p/ `height:%`).
// as macros de estado (close_floats!/flush_inline!) resetam as variáveis a cada
// fechamento — a ÚLTIMA atribuição (no flush final) é estruturalmente morta, o
// que dispara unused_assignments sem haver bug.
#[allow(unused_assignments)]
fn layout_children_vertical(
    dom: &Dom,
    id: NodeIdx,
    content_x: f32,
    content_y: f32,
    content_w: f32,
    avail_h: Option<f32>,
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
    // ── FLOAT LINE (v1): floats CONSECUTIVOS dividem a mesma linha — left encosta
    // à esquerda, right à direita (o header brand+nav do Bootstrap). Um irmão
    // NÃO-float fecha a linha (começa abaixo do float mais alto). Ver FloatSide.
    let mut float_top: Option<f32> = None; // y do topo da linha de floats
    let mut float_left_x = content_x;
    let mut float_right_x = content_x + content_w;
    let mut float_h = 0.0f32; // altura da linha (max dos floats)
    // fecha a float line corrente (chamado antes de um não-float e no fim).
    macro_rules! close_floats {
        ($y:expr) => {
            if let Some(top) = float_top.take() {
                $y = $y.max(top + float_h);
                float_left_x = content_x;
                float_right_x = content_x + content_w;
                float_h = 0.0;
            }
        };
    }
    // ── CONTEXTO INLINE (P4): irmãos inline CONSECUTIVOS (texto + <a>/<b>/<span>)
    // fluem JUNTOS numa sequência de linhas — acumulados aqui e descarregados por
    // `flush_inline!` quando um bloco/float/fim interrompe o fluxo.
    let mut inline_group: Vec<NodeIdx> = Vec::new();
    // Corrida de INLINE-BLOCKS consecutivos (botões/pills lado a lado). Pintada
    // por `flush_ib` — mede cada um (shrink), põe lado a lado quebrando linha ao
    // encher, e alinha a linha pelo text-align do pai (center do google).
    let mut ib_run: Vec<NodeIdx> = Vec::new();
    macro_rules! flush_ib {
        ($y:expr) => {
            if !ib_run.is_empty() {
                $y = layout_inline_block_line(
                    dom, &ib_run, content_x, $y, content_w, avail_h, css, ctx, list,
                );
                ib_run.clear();
                prev_margin = 0.0;
            }
        };
    }
    macro_rules! flush_inline {
        ($y:expr) => {
            if !ib_run.is_empty() {
                flush_ib!($y);
            }
            if !inline_group.is_empty() {
                close_floats!($y);
                $y = layout_inline_flow(
                    dom, &inline_group, content_x, $y, content_w, css, font_size, ctx, list,
                );
                inline_group.clear();
                prev_margin = 0.0; // texto quebra a sequência de margin-collapse
            }
        };
    }
    for &child in &dom.node(id).children {
        match &dom.node(child).kind {
            // Metadata não-renderável (`<head>`/`<title>`/`<style>`/`<script>`):
            // pula — NÃO coleta seu texto como inline (senão o título e o CSS cru
            // vazam pra tela). Checado ANTES do caminho inline.
            NodeKind::Element { tag } if is_non_rendered_tag(tag) => {}
            // Fora do fluxo (`position:absolute/fixed`): não ocupa espaço aqui —
            // pintado na passada out-of-flow de layout_document.
            NodeKind::Element { .. } if is_out_of_flow(dom, child) => {}
            // FLOAT left/right: shrink-to-fit na linha de floats corrente.
            NodeKind::Element { .. } if float_of(dom, child) != crate::style::FloatSide::None => {
                flush_inline!(child_y);
                let side = float_of(dom, child);
                let top = *float_top.get_or_insert(child_y);
                let w = child_outer_width(dom, child, content_w, font_size, ctx);
                let h = child_outer_height(dom, child, content_w, avail_h, font_size, ctx);
                let x = if side == crate::style::FloatSide::Left {
                    let x = float_left_x;
                    float_left_x += w;
                    x
                } else {
                    float_right_x -= w;
                    float_right_x
                };
                layout_block(dom, child, x, top, content_w, avail_h, None, None, true, ctx, list);
                float_h = float_h.max(h);
                prev_margin = 0.0; // float quebra a sequência de collapse
            }
            NodeKind::Element { .. } if is_block_level(dom, child) && !is_inline_block(dom, child) => {
                flush_inline!(child_y);
                close_floats!(child_y);
                // margin VERTICAL TOP do filho (para o collapse com o anterior):
                // margin.top + margin_v da UA.
                let m = dom.computed_style_idx(child)
                    .map(|c| {
                        // margem TOP do filho p/ o collapse (unidades relativas
                        // resolvem contra o content deste container).
                        let r = ResolveCtx {
                            parent_content_w: content_w,
                            node_font_size: font_px(&c, font_size),
                            root_font_size: DEFAULT_FONT_SIZE,
                            viewport_w: ctx.viewport_w,
                            viewport_h: ctx.viewport_h,
                        };
                        let mv = if c.margin.top == crate::style::Side::Unset {
                            c.margin_v.unwrap_or(0.0)
                        } else {
                            0.0
                        };
                        c.margin.top.resolve(&r).unwrap_or(0.0) + mv
                    })
                    .unwrap_or(0.0);
                // Colapsa com o bloco anterior: recua o overlap antes de posicionar.
                child_y -= prev_margin.min(m);
                let (_, h) = layout_block(dom, child, content_x, child_y, content_w, avail_h, None, None, false, ctx, list);
                child_y += h;
                prev_margin = m;
            }
            // INLINE-BLOCK (pill/botão solto): NÃO pinta agora — acumula na
            // "linha de inline-blocks" corrente (irmãos consecutivos fluem LADO A
            // LADO, quebrando quando enche). Os botões 'Pesquisa Google'/'Estou
            // com sorte' do google são 2 inline-block irmãos que compartilham a
            // linha. Um texto/inline entre eles fecha a corrida (flush_inline).
            NodeKind::Element { .. } if is_inline_block(dom, child) => {
                // descarrega só o TEXTO inline pendente (não o ib_run — este b
                // continua a acumular os inline-blocks IRMÃOS na mesma corrida).
                if !inline_group.is_empty() {
                    close_floats!(child_y);
                    child_y = layout_inline_flow(
                        dom, &inline_group, content_x, child_y, content_w, css, font_size, ctx, list,
                    );
                    inline_group.clear();
                }
                ib_run.push(child);
                prev_margin = 0.0;
            }
            // Texto / elemento inline: entra no CONTEXTO INLINE corrente — flui
            // com os irmãos inline adjacentes (o flush pinta o grupo inteiro).
            _ => {
                flush_ib!(child_y); // fecha a corrida de inline-blocks
                inline_group.push(child);
            }
        }
    }
    // descarrega o fluxo inline pendente e fecha a float line: os floats
    // CONTRIBUEM para a altura (v1 = comportamento de BFC — correto p/ flex
    // items, o caso do header do cover).
    flush_inline!(child_y);
    close_floats!(child_y);
    (child_y - content_y).max(0.0)
}

/// Pinta uma CORRIDA de inline-blocks consecutivos (botões/pills irmãos) numa
/// sequência de linhas horizontais: mede cada um (shrink, numa lista descartável),
/// põe lado a lado enquanto cabe na `content_w`, quebra linha quando enche, e
/// alinha CADA linha pelo `text-align` do pai (center do google centra os botões).
/// Devolve o novo `y` (abaixo da última linha). Vazio → devolve `y`.
#[allow(clippy::too_many_arguments)]
fn layout_inline_block_line(
    dom: &Dom,
    run: &[NodeIdx],
    content_x: f32,
    y: f32,
    content_w: f32,
    avail_h: Option<f32>,
    parent_css: &ComputedStyle,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    // 1) mede a largura+altura desejada (shrink) de cada item numa lista descartável.
    let mut sizes: Vec<(NodeIdx, f32, f32)> = Vec::with_capacity(run.len());
    for &child in run {
        let mut scratch = DisplayList::default();
        let (w, h) = layout_block(
            dom, child, content_x, y, content_w, avail_h, None, None, true, ctx, &mut scratch,
        );
        sizes.push((child, w, h));
    }
    // 2) agrupa em LINHAS (soma das larguras ≤ content_w). Cada linha guarda os
    //    itens + a largura total (p/ o alinhamento).
    let mut lines: Vec<(Vec<(NodeIdx, f32, f32)>, f32)> = Vec::new();
    let mut cur: Vec<(NodeIdx, f32, f32)> = Vec::new();
    let mut cur_w = 0.0f32;
    for (child, w, h) in sizes {
        if !cur.is_empty() && cur_w + w > content_w {
            lines.push((std::mem::take(&mut cur), cur_w));
            cur_w = 0.0;
        }
        cur_w += w;
        cur.push((child, w, h));
    }
    if !cur.is_empty() {
        lines.push((cur, cur_w));
    }
    // 3) pinta cada linha: x inicial pelo text-align do pai, itens lado a lado;
    //    y avança pela ALTURA da linha (o item mais alto).
    let mut cy = y;
    for (items, line_w) in &lines {
        let free = (content_w - line_w).max(0.0);
        let mut x = match parent_css.text_align {
            Some(crate::style::TextAlign::Center) => content_x + free / 2.0,
            Some(crate::style::TextAlign::Right) => content_x + free,
            _ => content_x,
        };
        let mut line_h = 0.0f32;
        for &(child, w, h) in items {
            layout_block(dom, child, x, cy, content_w, avail_h, None, None, true, ctx, list);
            x += w;
            line_h = line_h.max(h);
        }
        cy += line_h;
    }
    cy
}

/// O `float` computado de um nó-elemento (None p/ não-elemento/sem estilo).
fn float_of(dom: &Dom, id: NodeIdx) -> crate::style::FloatSide {
    dom.computed_style_idx(id)
        .and_then(|c| c.float_side)
        .unwrap_or(crate::style::FloatSide::None)
}

/// Um item do flex (pré-pass), com a BASE no eixo principal (flex-basis/width/
/// conteúdo, outer com margem), o MAIN size final (após grow/shrink) e os
/// fatores de flexibilidade lidos do estilo.
struct FlexItem {
    node: NodeIdx,
    /// tamanho BASE outer no eixo principal (antes de grow/shrink).
    base: f32,
    /// main size FINAL outer (após grow/shrink) — começa igual à base.
    main: f32,
    /// altura outer (cross) — re-medida com o main final quando ele muda.
    h: f32,
    /// `true` se é um nó de texto solto (pintado direto, não via layout_block).
    is_text: bool,
    /// `flex-grow` (0 = não cresce).
    grow: f32,
    /// `flex-shrink` (1 = default do CSS; texto solto não encolhe).
    shrink: f32,
    /// `align-self` do item (None = usa o align-items do container).
    align_self: Option<crate::style::AlignItems>,
    /// `order` (menor primeiro; empate = ordem do documento — sort estável).
    order: i32,
    /// o item PODE ser esticado pelo stretch (sem `height` explícito).
    can_stretch: bool,
}

/// A BASE outer de um item flex no eixo principal: `flex-basis` explícita
/// (resolvida como o width — respeita box-sizing) + margens; `auto`/ausente →
/// width/conteúdo ([`child_outer_width`]). O `.col` do Bootstrap tem basis `0%`
/// → a base é só o frame (e o grow distribui o espaço).
fn flex_base_outer(dom: &Dom, id: NodeIdx, container_w: f32, parent_font: f32, ctx: &LayoutCtx) -> f32 {
    let css = dom.computed_style_idx(id).unwrap_or_default();
    let font = font_px(&css, parent_font);
    let resolve = ResolveCtx {
        parent_content_w: container_w,
        node_font_size: font,
        root_font_size: DEFAULT_FONT_SIZE,
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let basis = css.flex_basis.and_then(|d| match d {
        crate::style::Dimension::Auto => None,
        other => other.resolve(&resolve),
    });
    let Some(basis) = basis else {
        return child_outer_width(dom, id, container_w, parent_font, ctx);
    };
    let margin_h = css.margin.resolve_h(&resolve);
    if css.border_box.unwrap_or(false) {
        basis + margin_h // border-box: a basis JÁ é a caixa (pad+borda inclusos)
    } else {
        basis
            + margin_h
            + 2.0 * css.border_width.unwrap_or(0.0)
            + css.padding.resolve_h(&resolve)
    }
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
    // altura do CONTENT do container quando explícita (já resolvida pelo caller,
    // no eixo certo) — referência do cross-axis p/ align-items e containing block
    // dos filhos.
    container_content_h: Option<f32>,
    css: &ComputedStyle,
    font_size: f32,
    wrap: bool,
    // `Some(N)` quando `display:grid`: cada item vira uma coluna de largura fixa
    // `(content_w - (N-1)*gap)/N` e a linha quebra a cada N. `None` = flex/wrap normal.
    grid_cols: Option<i32>,
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
    // `0` = sem height explícito (o cross-size da linha usa o max dos itens).
    let container_cross_h = container_content_h.unwrap_or(0.0);

    // ── PRÉ-PASS: coleta cada filho renderável com a BASE flex + fatores ─────────
    let mut items: Vec<FlexItem> = Vec::new();
    for &child in &dom.node(id).children {
        if let NodeKind::Element { tag } = &dom.node(child).kind {
            if is_non_rendered_tag(tag) {
                continue;
            }
        }
        // fora do fluxo: não é item flex (pintado na passada out-of-flow).
        if is_out_of_flow(dom, child) {
            continue;
        }
        if !is_block_level(dom, child) {
            // texto solto: largura medida; vazio é ignorado. Não cresce nem encolhe.
            let text = collect_text(dom, child);
            if text.trim().is_empty() {
                continue;
            }
            let w = ctx.measurer.text_width(&text, font_size, false, false);
            let h = ctx.measurer.line_height(font_size);
            items.push(FlexItem {
                node: child,
                base: w,
                main: w,
                h,
                is_text: true,
                grow: 0.0,
                shrink: 0.0,
                align_self: None,
                order: 0,
                can_stretch: false,
            });
            continue;
        }
        let ccss = dom.computed_style_idx(child).unwrap_or_default();
        let base = flex_base_outer(dom, child, content_w, font_size, ctx);
        let h = child_outer_height(dom, child, content_w, container_content_h, font_size, ctx);
        items.push(FlexItem {
            node: child,
            base,
            main: base,
            h,
            is_text: false,
            grow: ccss.flex_grow.unwrap_or(0.0),
            shrink: ccss.flex_shrink.unwrap_or(1.0), // 1 é o default do CSS
            align_self: ccss.align_self,
            order: ccss.order.unwrap_or(0),
            can_stretch: ccss.height.is_none(),
        });
    }
    // `order` reordena ANTES do wrap (sort estável: empate = ordem do documento).
    items.sort_by_key(|it| it.order);

    // GRID: cada item (não-texto) vira uma coluna de largura fixa. Fixa base=main=col_w
    // e zera grow/shrink (a coluna não flui) → o wrap abaixo quebra a cada N colunas.
    if let Some(n) = grid_cols {
        let n = n.max(1) as f32;
        let col_w = ((content_w - (n - 1.0) * gap) / n).max(0.0);
        for it in items.iter_mut() {
            if it.is_text {
                continue;
            }
            it.base = col_w;
            it.main = col_w;
            it.grow = 0.0;
            it.shrink = 0.0;
        }
    }

    // agrupa em LINHAS pela BASE (o wrap decide pelas bases; grow/shrink POR linha).
    let mut lines: Vec<Vec<FlexItem>> = vec![Vec::new()];
    let mut line_w = 0.0f32;
    for it in items {
        let cur = lines.last_mut().unwrap();
        let with_gap = if cur.is_empty() { 0.0 } else { gap };
        if wrap && !cur.is_empty() && line_w + with_gap + it.base > content_w {
            lines.push(Vec::new());
            line_w = it.base;
        } else {
            line_w += with_gap + it.base;
        }
        lines.last_mut().unwrap().push(it);
    }

    // ── RESOLVE + POSICIONA por linha: grow/shrink (main), justify, align ────────
    let mut line_y = content_y;
    for line in &mut lines {
        if line.is_empty() {
            continue;
        }
        let n = line.len();
        let total_gap = (n.saturating_sub(1)) as f32 * gap;

        // GROW/SHRINK (spec flexbox §9.7 simplificada): espaço livre positivo
        // distribui ∝ flex-grow (o `.col { flex:1 0 0% }` divide igual); negativo
        // encolhe ∝ shrink × base (itens maiores cedem mais), clamp ≥ 0.
        let sum_base: f32 = line.iter().map(|it| it.base).sum();
        let free_pre = content_w - sum_base - total_gap;
        let sum_grow: f32 = line.iter().map(|it| it.grow).sum();
        if free_pre > 0.0 && sum_grow > 0.0 {
            for it in line.iter_mut() {
                it.main = it.base + free_pre * it.grow / sum_grow;
            }
        } else if free_pre < 0.0 {
            let weighted: f32 = line.iter().map(|it| it.shrink * it.base).sum();
            if weighted > 0.0 {
                for it in line.iter_mut() {
                    it.main = (it.base + free_pre * (it.shrink * it.base) / weighted).max(0.0);
                }
            }
        }
        // re-mede a ALTURA com o main final (mais largura → menos linhas de texto);
        // só quando o main mudou (senão a medição do pré-pass vale).
        for it in line.iter_mut() {
            if !it.is_text && (it.main - it.base).abs() > 0.5 {
                let mut scratch = DisplayList::default();
                let (_, h) = layout_block(
                    dom, it.node, 0.0, 0.0, content_w, container_content_h,
                    Some(it.main), None, true, ctx, &mut scratch,
                );
                it.h = h;
            }
        }

        // Cross-size de referência da linha = max das alturas dos itens, MAS se o
        // container tem `height` explícito e a linha é única (no-wrap), o cross-size
        // é a ALTURA DO CONTENT do container (fiel ao Chrome). Em wrap, cada linha
        // usa seu próprio max (repartir o height entre linhas — corte documentado).
        let items_h = line.iter().fold(0.0f32, |a, it| a.max(it.h));
        let line_h = if !wrap && container_cross_h > items_h {
            container_cross_h
        } else {
            items_h
        };

        // justify-content sobre o espaço restante PÓS-grow (com grow>0 o free é 0
        // e o justify é neutro — correto). Em overflow, ver justify_offsets.
        let sum_main: f32 = line.iter().map(|it| it.main).sum();
        let free = content_w - sum_main - total_gap;
        let (leading, between) = justify_offsets(justify, free, n);

        let mut x = content_x + leading;
        for (j, it) in line.iter().enumerate() {
            if j > 0 {
                x += gap + between;
            }
            // align por item: `align-self` vence o `align-items` do container;
            // STRETCH real: item sem height explícito ganha a ALTURA DA LINHA
            // (forced_outer_h) — os cards `.col` preenchem a linha.
            let item_align = it.align_self.unwrap_or(align);
            let stretches = item_align == crate::style::AlignItems::Stretch
                && it.can_stretch
                && !it.is_text
                && line_h > it.h;
            let off_cross = if stretches { 0.0 } else { align_offset(item_align, line_h, it.h) };
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
                    letter_spacing: css.letter_spacing.unwrap_or(0.0),
                    decoration: decoration_code(css),
                });
            } else {
                // o main resolvido é IMPOSTO ao item (grow/shrink venceram o
                // width); stretch impõe a altura da linha.
                let forced_h = if stretches { Some(line_h) } else { None };
                layout_block(
                    dom, it.node, x, item_y, content_w, container_content_h,
                    Some(it.main), forced_h, true, ctx, list,
                );
            }
            x += it.main;
        }
        line_y += line_h + row_gap;
    }
    // desconta o último row_gap (só ENTRE linhas, não após a última).
    let total_h = (line_y - row_gap - content_y).max(0.0);
    total_h
}

/// Dispõe os filhos como FLEX COLUMN (`display:flex; flex-direction:column`): o
/// eixo PRINCIPAL é o vertical. Diferenças do block vertical: SEM margin-collapse
/// (flex não colapsa margens), `gap` entre itens (em column o espaçamento main é o
/// `row-gap`; o shorthand `gap:` seta ambos), `justify-content` distribui o espaço
/// livre VERTICAL (só quando o container tem altura explícita), `margin-top/bottom:
/// auto` de um item ABSORVE o espaço livre (spec flexbox §8.1 — é o `mb-auto`/
/// `mt-auto` do Bootstrap empurrando header/footer para as pontas), e `align-items`
/// atua no X: `stretch` (default) = item ocupa a largura; start/center/end = item
/// shrink-to-fit deslocado. Devolve a altura natural do content.
/// ⚠️ Cortes: `column-reverse` dispõe como `column` (sem inverter); `flex-wrap` em
/// column (multi-coluna) trata como coluna única; `flex-grow/shrink/basis` ainda
/// fora (fatia própria).
/// GRID real (css-grid track-sizing simplificado): resolve as trilhas de coluna
/// (px/%/fr/auto) e de linha, faz auto-placement dos itens célula-a-célula
/// (row-by-row), e posiciona cada item na sua célula com `justify-items`
/// (horizontal) / `align-items` (vertical). Suporta o subset do MDN:
/// grid-template-columns/rows, grid-auto-rows, gap, repeat(N,...), minmax(→max),
/// fr. NÃO suporta: grid-column/row-span explícito, areas, auto-fill/fit reais,
/// dense. Um item sem placement explícito preenche a próxima célula livre.
#[allow(clippy::too_many_arguments)]
fn layout_children_grid(
    dom: &Dom,
    id: NodeIdx,
    content_x: f32,
    content_y: f32,
    content_w: f32,
    container_content_h: Option<f32>,
    css: &ComputedStyle,
    font_size: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    let resolve = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font_size,
        root_font_size: DEFAULT_FONT_SIZE,
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let col_gap = css.gap.or(css.row_gap).and_then(|d| d.resolve(&resolve)).unwrap_or(0.0).max(0.0);
    let row_gap = css.row_gap.or(css.gap).and_then(|d| d.resolve(&resolve)).unwrap_or(0.0).max(0.0);

    // ── COLUNAS: resolve as trilhas ──────────────────────────────────────────────
    // Sem grid-template-columns explícito → 1 coluna 1fr (o container-do-logo do
    // google: single-column grid). Com N colunas do grid_columns legado (repeat) →
    // N trilhas 1fr.
    let col_tracks: Vec<crate::style::GridTrack> = match &css.grid_template_columns {
        Some(t) => (**t).clone(),
        None => {
            let n = css.grid_columns.unwrap_or(1).max(1) as usize;
            vec![crate::style::GridTrack::Fr(1.0); n]
        }
    };
    let col_sizes = resolve_tracks(&col_tracks, content_w, col_gap, &resolve);
    let ncols = col_sizes.len().max(1);

    // ── ITENS: os filhos renderizáveis (auto-placement row-by-row) ───────────────
    let mut children: Vec<NodeIdx> = Vec::new();
    for &child in &dom.node(id).children {
        if let NodeKind::Element { tag } = &dom.node(child).kind {
            if is_non_rendered_tag(tag) {
                continue;
            }
        }
        if is_out_of_flow(dom, child) {
            continue;
        }
        if !is_block_level(dom, child) && collect_text(dom, child).trim().is_empty() {
            continue;
        }
        children.push(child);
    }
    if children.is_empty() {
        return 0.0;
    }
    let nrows = children.len().div_ceil(ncols);

    // ── LINHAS: altura de cada linha ─────────────────────────────────────────────
    // grid-template-rows explícito (px/%/fr/auto), senão grid-auto-rows, senão a
    // altura do conteúdo mais alto da linha. `fr`/`%` de linha precisam da altura
    // do container (container_content_h).
    let explicit_rows: Vec<crate::style::GridTrack> = css
        .grid_template_rows
        .as_ref()
        .map(|t| (**t).clone())
        .unwrap_or_default();
    // mede a altura de conteúdo de cada linha (o item mais alto medido em shrink).
    let mut content_row_h = vec![0.0f32; nrows];
    for (i, &child) in children.iter().enumerate() {
        let r = i / ncols;
        let ci = i % ncols;
        let cw = col_sizes[ci.min(col_sizes.len() - 1)];
        let mut scratch = DisplayList::default();
        let (_, h) = layout_block(dom, child, 0.0, 0.0, cw, container_content_h, None, None, true, ctx, &mut scratch);
        content_row_h[r] = content_row_h[r].max(h);
    }
    let auto_row = css.grid_auto_rows;
    let has_explicit_row_track = |r: usize| {
        explicit_rows.get(r).is_some() || auto_row.is_some()
    };
    let mut row_sizes: Vec<f32> = (0..nrows)
        .map(|r| {
            let track = explicit_rows.get(r).copied().or(auto_row);
            match track {
                Some(crate::style::GridTrack::Fixed(d)) => resolve_height(Some(d), container_content_h, &resolve).unwrap_or(content_row_h[r]),
                _ => content_row_h[r], // Auto/None/Fr → conteúdo por ora (ajuste abaixo)
            }
        })
        .collect();
    // Se o container tem ALTURA definida e as linhas NÃO têm track explícita (auto),
    // as linhas DIVIDEM a altura do container (uma row auto num grid de altura fixa
    // preenche o espaço — é o que dá a track de 240 pro logo centrar). Distribui o
    // espaço livre igualmente entre as linhas auto (aproximação; fr real seria por
    // peso — mas grid sem template-rows usa 1fr implícito quando há altura).
    if let Some(ch) = container_content_h {
        let auto_rows: Vec<usize> = (0..nrows).filter(|&r| !has_explicit_row_track(r)).collect();
        if !auto_rows.is_empty() {
            let fixed: f32 = (0..nrows).filter(|r| has_explicit_row_track(*r)).map(|r| row_sizes[r]).sum();
            let total_gap = (nrows.saturating_sub(1)) as f32 * row_gap;
            let free = (ch - fixed - total_gap).max(0.0);
            let each = free / auto_rows.len() as f32;
            for r in auto_rows {
                row_sizes[r] = row_sizes[r].max(each);
            }
        }
    }

    // ── POSICIONA cada item na sua célula ────────────────────────────────────────
    let justify = css.grid_justify_items.unwrap_or(crate::style::AlignItems::Stretch);
    let align = css.align_items.unwrap_or(crate::style::AlignItems::Stretch);
    // x acumulado de cada coluna, y de cada linha.
    let mut col_x = vec![content_x; ncols + 1];
    for c in 0..ncols {
        col_x[c + 1] = col_x[c] + col_sizes[c.min(col_sizes.len() - 1)] + col_gap;
    }
    let mut row_y = vec![content_y; nrows + 1];
    for r in 0..nrows {
        row_y[r + 1] = row_y[r] + row_sizes[r] + row_gap;
    }
    for (i, &child) in children.iter().enumerate() {
        let r = i / ncols;
        let c = i % ncols;
        let cell_x = col_x[c];
        let cell_y = row_y[r];
        let cell_w = col_sizes[c.min(col_sizes.len() - 1)];
        let cell_h = row_sizes[r];
        // mede o tamanho natural do item (shrink) p/ o alinhamento não-stretch.
        let stretch_x = justify == crate::style::AlignItems::Stretch;
        let stretch_y = align == crate::style::AlignItems::Stretch;
        let mut scratch = DisplayList::default();
        let (nat_w, nat_h) = layout_block(dom, child, 0.0, 0.0, cell_w, Some(cell_h), None, None, true, ctx, &mut scratch);
        let iw = if stretch_x { cell_w } else { nat_w.min(cell_w) };
        let ih = if stretch_y { cell_h } else { nat_h.min(cell_h) };
        let x = cell_x + cell_align_offset(justify, cell_w, iw);
        let y = cell_y + cell_align_offset(align, cell_h, ih);
        // pinta o item: stretch no eixo → forced size; senão shrink-to-fit.
        let forced_w = if stretch_x { None } else { Some(iw) };
        let forced_h = if stretch_y { Some(cell_h) } else { None };
        layout_block(dom, child, x, y, cell_w, Some(cell_h), forced_w, forced_h, !stretch_x, ctx, list);
    }
    // altura total = soma das linhas + gaps.
    let total_h: f32 = row_sizes.iter().sum::<f32>() + (nrows.saturating_sub(1)) as f32 * row_gap;
    total_h.max(0.0)
}

/// Resolve os tamanhos px de uma lista de trilhas dado o tamanho do container no
/// eixo e o gap: px/%/auto resolvem direto; o espaço livre restante é dividido
/// pelas trilhas `fr` na proporção dos pesos (css-grid track sizing simplificado).
fn resolve_tracks(
    tracks: &[crate::style::GridTrack],
    container: f32,
    gap: f32,
    ctx: &ResolveCtx,
) -> Vec<f32> {
    let n = tracks.len().max(1);
    let total_gap = (n.saturating_sub(1)) as f32 * gap;
    // 1ª passada: resolve px/%/auto; soma o consumido e os pesos fr.
    let mut sizes = vec![0.0f32; tracks.len()];
    let mut used = 0.0f32;
    let mut sum_fr = 0.0f32;
    for (i, t) in tracks.iter().enumerate() {
        match t {
            crate::style::GridTrack::Fixed(d) => {
                // % de trilha resolve contra o container (largura p/ colunas).
                let v = match d {
                    crate::style::Dimension::Percent(p) => container * p / 100.0,
                    other => other.resolve(ctx).unwrap_or(0.0),
                };
                sizes[i] = v.max(0.0);
                used += sizes[i];
            }
            crate::style::GridTrack::Fr(f) => sum_fr += f.max(0.0),
            crate::style::GridTrack::Auto => {} // auto: 0 na v1 (conteúdo dita a linha, não a coluna)
        }
    }
    // 2ª passada: distribui o espaço livre pelas trilhas fr.
    let free = (container - used - total_gap).max(0.0);
    if sum_fr > 0.0 {
        for (i, t) in tracks.iter().enumerate() {
            if let crate::style::GridTrack::Fr(f) = t {
                sizes[i] = free * f.max(0.0) / sum_fr;
            }
        }
    } else {
        // sem fr: uma trilha `auto` sozinha ocupa o espaço livre (single-column).
        let autos: Vec<usize> = tracks.iter().enumerate()
            .filter(|(_, t)| matches!(t, crate::style::GridTrack::Auto))
            .map(|(i, _)| i).collect();
        if !autos.is_empty() {
            let each = free / autos.len() as f32;
            for i in autos { sizes[i] = each; }
        }
    }
    sizes
}

/// Offset de alinhamento de um item de tamanho `item` dentro de uma célula de
/// tamanho `cell` (start=0, center=(cell-item)/2, end=cell-item; stretch=0).
fn cell_align_offset(a: crate::style::AlignItems, cell: f32, item: f32) -> f32 {
    match a {
        crate::style::AlignItems::Center => ((cell - item) / 2.0).max(0.0),
        crate::style::AlignItems::FlexEnd => (cell - item).max(0.0),
        _ => 0.0, // FlexStart / Stretch
    }
}

fn layout_children_column(
    dom: &Dom,
    id: NodeIdx,
    content_x: f32,
    content_y: f32,
    content_w: f32,
    // altura do CONTENT do container quando explícita — a referência do eixo
    // principal (justify/margin-auto) e o containing block dos filhos (height:%).
    container_content_h: Option<f32>,
    css: &ComputedStyle,
    font_size: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    let resolve = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font_size,
        root_font_size: DEFAULT_FONT_SIZE,
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    // Em column, o espaço entre itens no eixo principal é o ROW-gap; o shorthand
    // `gap: X` seta os dois, então row_gap cobre o caso comum. (Fallback ao `gap`
    // — column-gap — só quando row_gap não veio, cobrindo `column-gap` usado
    // "errado" sem quebrar o shorthand.)
    let main_gap = css
        .row_gap
        .or(css.gap)
        .and_then(|d| d.resolve(&resolve))
        .unwrap_or(0.0)
        .max(0.0);
    let justify = css.justify.unwrap_or(crate::style::JustifyContent::FlexStart);
    let align = css.align_items.unwrap_or(crate::style::AlignItems::Stretch);

    // ── PASSO 1: mede a altura outer desejada de cada filho + margens auto ───────
    struct ColItem {
        node: NodeIdx,
        h: f32,
        is_text: bool,
        mt_auto: bool,
        mb_auto: bool,
        grow: f32,
    }
    let mut items: Vec<ColItem> = Vec::new();
    for &child in &dom.node(id).children {
        if let NodeKind::Element { tag } = &dom.node(child).kind {
            if is_non_rendered_tag(tag) {
                continue;
            }
        }
        // fora do fluxo: não é item flex (pintado na passada out-of-flow).
        if is_out_of_flow(dom, child) {
            continue;
        }
        if !is_block_level(dom, child) {
            let text = collect_text(dom, child);
            if text.trim().is_empty() {
                continue;
            }
            items.push(ColItem {
                node: child,
                h: ctx.measurer.line_height(font_size),
                is_text: true,
                mt_auto: false,
                mb_auto: false,
                grow: 0.0,
            });
            continue;
        }
        let h = child_outer_height(dom, child, content_w, container_content_h, font_size, ctx);
        let (mt_auto, mb_auto, grow) = dom
            .computed_style_idx(child)
            .map(|c| (c.margin.top.is_auto(), c.margin.bottom.is_auto(), c.flex_grow.unwrap_or(0.0)))
            .unwrap_or((false, false, 0.0));
        items.push(ColItem { node: child, h, is_text: false, mt_auto, mb_auto, grow });
    }
    if items.is_empty() {
        return 0.0;
    }

    // ── PASSO 2: distribui o espaço livre do eixo principal (Y) ──────────────────
    let n = items.len();
    let sum_h: f32 = items.iter().map(|it| it.h).sum();
    let total_gap = (n.saturating_sub(1)) as f32 * main_gap;
    let free = container_content_h.map(|ch| ch - sum_h - total_gap).unwrap_or(0.0);
    // FLEX-GROW no eixo principal (css-flexbox §9.7): quando há espaço livre
    // positivo e algum item tem flex-grow, cada um cresce em proporção
    // `grow / soma_dos_grows * free` — dando ALTURA aos containers que os filhos
    // com `height:100%` resolvem (o logo/caixa do google centram assim). Consome
    // o `free` (o justify/margin-auto abaixo vê 0). margin:auto tem prioridade.
    let sum_grow: f32 = items.iter().map(|it| it.grow).sum();
    let any_auto = items.iter().any(|it| it.mt_auto || it.mb_auto);
    if free > 0.0 && sum_grow > 0.0 && !any_auto {
        for it in &mut items {
            if it.grow > 0.0 {
                it.h += it.grow / sum_grow * free;
            }
        }
    }
    let sum_h: f32 = items.iter().map(|it| it.h).sum();
    let free = container_content_h.map(|ch| ch - sum_h - total_gap).unwrap_or(0.0);
    let auto_count: usize = items.iter().map(|it| it.mt_auto as usize + it.mb_auto as usize).sum();
    // margin:auto no eixo main absorve TODO o espaço livre (o justify vira no-op) —
    // spec css-flexbox §8.1. Sem autos, o justify distribui.
    let auto_size = if free > 0.0 && auto_count > 0 { free / auto_count as f32 } else { 0.0 };
    let (leading, between) = if auto_count > 0 {
        (0.0, 0.0)
    } else {
        justify_offsets(justify, free, n)
    };

    // ── PASSO 3: posiciona e pinta ────────────────────────────────────────────────
    let mut y = content_y + leading;
    for (j, it) in items.iter().enumerate() {
        if j > 0 {
            y += main_gap + between;
        }
        if it.mt_auto {
            y += auto_size;
        }
        if it.is_text {
            let text = collect_text(dom, it.node);
            list.items.push(DisplayItem::Text {
                x: content_x,
                y,
                text,
                color: css.color.unwrap_or(0x000000FF),
                size: font_size,
                mono: false,
                bold: css.bold.unwrap_or(false),
                letter_spacing: css.letter_spacing.unwrap_or(0.0),
                decoration: decoration_code(css),
            });
        } else {
            // CROSS (X): stretch (default) → o item ocupa a largura do container
            // (layout normal de bloco); start/center/end → shrink-to-fit + offset.
            let stretch = align == crate::style::AlignItems::Stretch;
            let child_x = if stretch {
                content_x
            } else {
                let mut scratch = DisplayList::default();
                let (w, _) =
                    layout_block(dom, it.node, content_x, y, content_w, container_content_h, None, None, true, ctx, &mut scratch);
                let free_x = (content_w - w).max(0.0);
                content_x + align_offset(align, content_w, content_w - free_x)
            };
            // Um item que CRESCEU por flex-grow tem altura MAIOR que o conteúdo —
            // passa essa altura como containing block (avail_h) E como outer forçada
            // (forced_outer_h) para os filhos com `height:100%` resolverem contra ela.
            let (avail, forced_h) = if it.grow > 0.0 {
                (Some(it.h), Some(it.h))
            } else {
                (container_content_h, None)
            };
            layout_block(dom, it.node, child_x, y, content_w, avail, None, forced_h, !stretch, ctx, list);
        }
        y += it.h;
        if it.mb_auto {
            y += auto_size;
        }
    }
    (y - content_y).max(0.0)
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
fn child_outer_height(
    dom: &Dom,
    id: NodeIdx,
    container_w: f32,
    container_h: Option<f32>,
    parent_font: f32,
    ctx: &LayoutCtx,
) -> f32 {
    match &dom.node(id).kind {
        NodeKind::Element { .. } if is_block_level(dom, id) => {
            // layout de teste numa lista descartável: o (_, outer_h) é a altura real.
            let mut scratch = DisplayList::default();
            let (_, outer_h) = layout_block(dom, id, 0.0, 0.0, container_w, container_h, None, None, true, ctx, &mut scratch);
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
            let font = font_px(&css, parent_font);
            let resolve = ResolveCtx {
                parent_content_w: container_w,
                node_font_size: font,
                root_font_size: DEFAULT_FONT_SIZE,
                viewport_w: ctx.viewport_w,
                viewport_h: ctx.viewport_h,
            };
            // frame horizontal = margin_h + 2*border + padding_h (cada já é o eixo;
            // unidades relativas resolvidas contra o container).
            let frame = css.margin.resolve_h(&resolve)
                + 2.0 * css.border_width.unwrap_or(0.0)
                + css.padding.resolve_h(&resolve);
            // Em border-box, o `width` declarado JÁ é a caixa (outer sem margin) —
            // não soma pad/border de novo; só a margin. Em content-box, soma o frame.
            match css.width.and_then(|d| d.resolve(&resolve)) {
                Some(w) if css.border_box.unwrap_or(false) => {
                    w + css.margin.resolve_h(&resolve)
                }
                Some(w) => w + frame,
                None => content_natural_width(dom, id, font, ctx) + frame,
            }
        }
        NodeKind::Text(t) => ctx.measurer.text_width(t, parent_font, false, false),
        _ => 0.0,
    }
}

/// Desenha um nó como linha(s) de texto (texto solto ou inline simples), herdando
/// cor/tamanho do bloco pai, e devolve o `y` abaixo. Caso de UM nó do fluxo
/// inline — o caminho geral (irmãos inline fluindo juntos) é
/// [`layout_inline_flow`].
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
    layout_inline_flow(dom, &[id], x, y, content_w, parent_css, font_size, ctx, list)
}

/// O FLUXO INLINE RICO (P4): um GRUPO de irmãos inline consecutivos (nós de texto
/// + elementos inline como `<a>`/`<b>`/`<span>`) flui como UM contexto — os runs
/// de todos concatenam, quebram por palavra na largura, e cada pedaço pinta com a
/// SUA cor/peso. É o que faz `<p>texto <a>link</a>, fim</p>` virar UMA linha
/// (antes cada filho virava uma linha própria — o footer do Bootstrap cover saía
/// em 5 linhas).
fn layout_inline_flow(
    dom: &Dom,
    group: &[NodeIdx],
    x: f32,
    y: f32,
    content_w: f32,
    parent_css: &ComputedStyle,
    font_size: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    // coleta os RUNS (cada pedaço de texto com a SUA cor/bold herdada do span que
    // o contém) de TODOS os nós do grupo, em ordem de documento.
    let mut runs = Vec::new();
    for &id in group {
        runs.extend(collect_runs(dom, id, parent_css, ctx));
    }
    if runs.iter().all(|r| r.text.trim().is_empty() && r.widget.is_none()) {
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
        // largura total da linha (texto no SEU peso + widgets) p/ text-align.
        let line_w: f32 = line
            .iter()
            .map(|seg| match seg.widget {
                Some(_) => seg.ww,
                None => ctx.measurer.text_width(&seg.text, font_size, mono, seg.bold),
            })
            .sum();
        // altura da linha: o texto (lh) ou o widget mais alto nela.
        let line_h = line
            .iter()
            .filter(|s| s.widget.is_some())
            .map(|s| s.wh)
            .fold(lh, f32::max);
        let free = (content_w - line_w).max(0.0);
        let mut seg_x = match parent_css.text_align {
            Some(crate::style::TextAlign::Right) => x + free,
            Some(crate::style::TextAlign::Center) => x + free / 2.0,
            _ => x, // left/justify
        };
        // pinta cada pedaço NA SUA COR e PESO, avançando o x.
        for seg in line {
            if let Some(w_idx) = seg.widget {
                // WIDGET inline: pinta a caixa no lugar (botão via layout_button;
                // campo de texto via layout_input com o avail da linha).
                let wcss = dom.computed_style_idx(w_idx).unwrap_or_default();
                let itype = dom
                    .node(w_idx)
                    .attr("type")
                    .map(|t| t.to_ascii_lowercase())
                    .unwrap_or_default();
                if matches!(itype.as_str(), "submit" | "button" | "reset") {
                    layout_button(dom, w_idx, &wcss, seg_x, cy, ctx, list);
                } else {
                    layout_input(dom, w_idx, &wcss, seg_x, cy, seg.ww, None, ctx, list);
                }
                seg_x += seg.ww;
                continue;
            }
            let ls = parent_css.letter_spacing.unwrap_or(0.0);
            let w = ctx.measurer.text_width(&seg.text, font_size, mono, seg.bold)
                + ls * seg.text.chars().count() as f32;
            list.items.push(DisplayItem::Text {
                x: seg_x,
                y: cy,
                text: seg.text.clone(),
                color: seg.color,
                size: font_size,
                mono,
                bold: seg.bold,
                letter_spacing: ls,
                decoration: seg.deco,
            });
            // RECT DO INLINE (`<a>`, `<span>`, `<strong>`…): sem isto um link no
            // meio de um parágrafo não é clicável — `node_rects` só tinha blocos, o
            // hit-test resolvia para o `<p>` e a navegação nunca disparava numa
            // página real (onde `<a>` é inline por padrão).
            //
            // Um inline que quebra em várias linhas gera VÁRIOS segmentos; UNIMOS
            // os rects do mesmo dono (bounding box), em vez de sobrescrever, porque
            // sobrescrever deixaria só o último pedaço clicável. A união é uma
            // aproximação declarada: o browser mantém um rect POR linha, então um
            // link quebrado em duas linhas tem aqui uma caixa que cobre também o
            // vão entre elas. Suficiente para hit-test de clique; insuficiente para
            // `getClientRects()` fiel, que não existe ainda.
            if let Some(o) = seg.owner {
                let r = Rect::new(seg_x, cy, w, line_h);
                list.node_rects
                    .entry(o)
                    .and_modify(|e| {
                        let x0 = e.x.min(r.x);
                        let y0 = e.y.min(r.y);
                        let x1 = (e.x + e.w).max(r.x + r.w);
                        let y1 = (e.y + e.h).max(r.y + r.h);
                        *e = Rect::new(x0, y0, x1 - x0, y1 - y0);
                    })
                    .or_insert(r);
            }
            seg_x += w;
        }
        cy += line_h;
    }
    cy
}

/// Um pedaço de texto inline com seu estilo resolvido (cor/peso herdados do span pai).
/// `widget: Some(idx)` = um WIDGET inline (input botão/texto) em vez de texto — flui
/// como uma "palavra" inquebrável de `ww × wh` pontos (item 8 do handoff #1793;
/// os botões 'Pesquisa Google' do google legado vivem em span>span>input).
struct InlineRun {
    text: String,
    color: u32,
    bold: bool,
    /// decoração do RUN (0=none 1=underline 2=line-through) — vem do <a>/<span>
    /// que contém o texto, não do bloco pai (um <a> sublinha só o seu texto).
    deco: u8,
    widget: Option<NodeIdx>,
    ww: f32,
    wh: f32,
    /// O ELEMENTO inline mais interno que contém este run (`<a>`, `<span>`,
    /// `<strong>`…), ou `None` quando o texto é filho direto do bloco.
    ///
    /// Existe para o HIT-TEST: sem isto, um `<a>` no meio de um parágrafo não tem
    /// entrada em `node_rects` e um clique nele não acha nada — o alvo resolvido
    /// seria o `<p>` inteiro, e a navegação por link nunca dispararia numa página
    /// real (onde `<a>` é inline por padrão). Guardar o dono por RUN, e não por
    /// elemento, é o que permite um `<a>` que quebra em duas linhas registrar um
    /// rect por linha — que é como o browser trata inline (vários rects, não um).
    owner: Option<NodeIdx>,
}

/// Coleta os RUNS de texto de `id` em ordem de documento, cada um com a COR efetiva
/// do elemento inline que o contém (um `<span style=color:x>` muda a cor do seu
/// texto). Aplica text-transform por run. A cor vem do `computed_style_idx` do nó
/// inline (que já herda do pai via a cascade) — é por isso que o style do span passa
/// a valer no texto.
fn collect_runs(
    dom: &Dom,
    id: NodeIdx,
    parent_css: &ComputedStyle,
    ctx: &LayoutCtx,
) -> Vec<InlineRun> {
    let mut runs = Vec::new();
    walk(
        dom,
        ctx,
        id,
        parent_css.color.unwrap_or(0x000000FF),
        decoration_code(parent_css),
        parent_css.text_transform,
        parent_css.bold.unwrap_or(false),
        None,
        &mut runs,
    );
    return runs;

    fn walk(
        dom: &Dom,
        ctx: &LayoutCtx,
        id: NodeIdx,
        inherited_color: u32,
        inherited_deco: u8,
        inherited_tt: Option<crate::style::TextTransform>,
        inherited_bold: bool,
        owner: Option<NodeIdx>,
        out: &mut Vec<InlineRun>,
    ) {
        match &dom.node(id).kind {
            NodeKind::Text(t) => {
                let text = match inherited_tt {
                    Some(tt) => tt.apply(t),
                    None => t.clone(),
                };
                out.push(InlineRun {
                    text,
                    color: inherited_color,
                    bold: inherited_bold,
                    deco: inherited_deco,
                    widget: None,
                    ww: 0.0,
                    wh: 0.0,
                    owner,
                });
            }
            NodeKind::Element { tag } => {
                // `<script>`/`<style>`/head-etc DENTRO de um contexto inline (um
                // script dentro de <td>/<center> — google.com faz isso): o texto
                // cru NÃO é conteúdo renderável — sem este skip, o código JS era
                // PINTADO na página.
                if is_non_rendered_tag(tag) {
                    return;
                }
                // WIDGET inline: um `<input>` no meio do fluxo (botão/campo) vira
                // um run-widget com o tamanho pré-medido — o wrap o trata como
                // palavra inquebrável e a emissão pinta a caixa no lugar.
                if is_text_input_tag(tag) {
                    let itype = dom
                        .node(id)
                        .attr("type")
                        .map(|t| t.to_ascii_lowercase())
                        .unwrap_or_default();
                    if itype == "hidden" {
                        return;
                    }
                    let (ww, wh) = inline_widget_size(dom, id, &itype, ctx);
                    out.push(InlineRun {
                        text: String::new(),
                        color: inherited_color,
                        bold: false,
                        deco: 0,
                        widget: Some(id),
                        ww,
                        wh,
                        owner,
                    });
                    return;
                }
                // a cor/text-transform/peso/decoração DESTE inline (se declarar)
                // vence p/ os filhos (o <a> sublinha só o próprio texto).
                let css = dom.computed_style_idx(id);
                let color = css.as_ref().and_then(|c| c.color).unwrap_or(inherited_color);
                let tt = css.as_ref().and_then(|c| c.text_transform).or(inherited_tt);
                let bold = css.as_ref().and_then(|c| c.bold).unwrap_or(inherited_bold);
                let deco = match css.as_ref().map(decoration_code) {
                    Some(d) if d != 0 => d,
                    _ => inherited_deco,
                };
                for &c in &dom.node(id).children {
                    // ESTE elemento passa a ser o dono dos runs dos filhos — o mais
                    // interno vence, que e o alvo certo de um clique.
                    walk(dom, ctx, c, color, deco, tt, bold, Some(id), out);
                }
            }
            _ => {}
        }
    }
}

/// Tamanho OUTER de um widget inline (`<input>`): o MESMO cálculo que a emissão
/// usa (layout_button / layout_input), para o wrap reservar a largura exata.
fn inline_widget_size(dom: &Dom, id: NodeIdx, itype: &str, ctx: &LayoutCtx) -> (f32, f32) {
    let css = dom.computed_style_idx(id).unwrap_or_default();
    if matches!(itype, "submit" | "button" | "reset") {
        let font = font_px(&css, DEFAULT_FONT_SIZE - 3.0);
        let label = dom.node(id).attr("value").unwrap_or("").to_string();
        let tw = ctx.measurer.text_width(&label, font, false, false);
        let lh = ctx.measurer.line_height(font);
        return (tw + 24.0 + 6.0, lh + 10.0 + 4.0); // espelha layout_button
    }
    // campo de texto: content default 180 + frame aproximado (padding 4+4, borda 1+1).
    let font = font_px(&css, DEFAULT_FONT_SIZE);
    let lh = ctx.measurer.line_height(font);
    (190.0, lh + 8.0)
}

/// Um segmento de texto colorido/pesado posicionado numa linha (após o wrap).
/// `widget: Some(idx)` = um widget inline de `ww × wh` (pintado pela emissão).
struct Segment {
    text: String,
    color: u32,
    bold: bool,
    deco: u8,
    widget: Option<NodeIdx>,
    ww: f32,
    wh: f32,
    /// Elemento inline dono deste pedaco (ver `InlineRun::owner`) — propagado ate
    /// a emissao, que registra o rect do segmento em `node_rects` para o hit-test.
    owner: Option<NodeIdx>,
}

/// Quebra uma sequência de RUNS coloridos em LINHAS por palavra (word-wrap), juntando
/// runs adjacentes na mesma linha. Cada linha é um vetor de [`Segment`] (pedaços
/// coloridos contíguos). Uma palavra que não cabe começa nova linha. FIEL AOS
/// ESPAÇOS do fonte: um espaço só entra entre duas palavras quando o texto
/// ORIGINAL tinha whitespace ali (colapsado p/ 1) — inclusive ATRAVÉS de runs
/// (`<a>Bootstrap</a>, by` NÃO ganha espaço antes da vírgula; antes toda palavra
/// ganhava espaço e a pontuação descolava).
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
    // havia whitespace no ORIGINAL desde a última palavra? (carrega entre runs)
    let mut pending_space = false;

    for run in runs {
        // WIDGET: uma "palavra" inquebrável de run.ww pontos, segmento próprio.
        if let Some(w_idx) = run.widget {
            let with_space = pending_space && !at_line_start;
            let need = if with_space { space_w + run.ww } else { run.ww };
            if !at_line_start && cur_w + need > max_w {
                lines.push(std::mem::take(&mut cur));
                cur_w = 0.0;
                at_line_start = true;
            }
            cur.push(Segment {
                text: String::new(),
                color: run.color,
                bold: false,
                deco: 0,
                widget: Some(w_idx),
                ww: run.ww,
                wh: run.wh,
                owner: run.owner,
            });
            cur_w += if pending_space && !at_line_start { space_w + run.ww } else { run.ww };
            at_line_start = false;
            pending_space = false;
            continue;
        }
        // scanner ws/palavra preservando a fronteira original.
        let mut rest = run.text.as_str();
        while !rest.is_empty() {
            if rest.starts_with(char::is_whitespace) {
                pending_space = true;
                rest = rest.trim_start();
                continue;
            }
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let word = &rest[..end];
            rest = &rest[end..];

            let ww = m.text_width(word, font_size, mono, run.bold);
            let with_space = pending_space && !at_line_start;
            let need = if with_space { space_w + ww } else { ww };
            if !at_line_start && cur_w + need > max_w {
                // não cabe: fecha a linha.
                lines.push(std::mem::take(&mut cur));
                cur_w = 0.0;
                at_line_start = true;
            }
            let sep = pending_space && !at_line_start;
            let piece = if sep { format!(" {word}") } else { word.to_string() };
            // junta no último segmento se mesma cor E peso (e não-widget), senão novo.
            if let Some(last) = cur.last_mut() {
                if last.widget.is_none()
                    && last.color == run.color
                    && last.bold == run.bold
                    && last.deco == run.deco
                    // owner IGUAL tambem: fundir o texto de um <a> com o texto
                    // vizinho fora dele daria ao link um rect que invade o que
                    // nao e link — e o clique ao lado navegaria.
                    && last.owner == run.owner
                {
                    last.text.push_str(&piece);
                } else {
                    cur.push(Segment {
                        text: piece,
                        color: run.color,
                        bold: run.bold,
                        deco: run.deco,
                        widget: None,
                        ww: 0.0,
                        wh: 0.0,
                        owner: run.owner,
                    });
                }
            } else {
                cur.push(Segment {
                    text: piece,
                    color: run.color,
                    bold: run.bold,
                    deco: run.deco,
                    widget: None,
                    ww: 0.0,
                    wh: 0.0,
                    owner: run.owner,
                });
            }
            cur_w += if sep { space_w + ww } else { ww };
            at_line_start = false;
            pending_space = false;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.is_empty() {
        lines.push(vec![Segment {
            text: String::new(),
            color: 0,
            bold: false,
            deco: 0,
            widget: None,
            ww: 0.0,
            wh: 0.0,
            owner: None,
        }]);
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
            // `<script>`/`<style>` não são conteúdo renderável — o texto cru
            // deles não entra no texto pintado (mesmo skip do collect_runs).
            NodeKind::Element { tag } if is_non_rendered_tag(tag) => {}
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

    #[test]
    fn hit_test_escolhe_o_no_mais_profundo() {
        // Filho dentro do pai: clicar dentro do filho devolve o FILHO (menor
        // área = mais profundo); clicar no pai fora do filho devolve o pai;
        // fora de tudo devolve None.
        def_div();
        let dom = parse_html_to_dom(
            "<div id=pai style='padding:50px'><div id=filho style='height:20px'>x</div></div>",
        );
        let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let pai = dom.query("#pai").unwrap();
        let filho = dom.query("#filho").unwrap();
        let pai_idx = dom.resolve(pai).unwrap();
        let filho_idx = dom.resolve(filho).unwrap();
        let fr = list.node_rects[&filho_idx];
        let hit = list.hit_test(fr.x + fr.w / 2.0, fr.y + fr.h / 2.0);
        assert_eq!(hit, Some(filho_idx));
        // canto do pai (dentro do padding, fora do filho).
        let pr = list.node_rects[&pai_idx];
        let hit2 = list.hit_test(pr.x + 5.0, pr.y + 5.0);
        assert_eq!(hit2, Some(pai_idx));
        // fora de tudo.
        assert_eq!(list.hit_test(-10.0, -10.0), None);
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
        assert!(texts[1] >= 20.8, "segundo bloco abaixo do primeiro (y={})", texts[1]); // 16×1.3
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
        // "x" (1 char, ~8px = 16×0.5) centrado em 400 → x ≈ (400-8)/2 = 196.
        let cx = texts.iter().find(|(t, _)| t == "x").unwrap().1;
        assert!((cx - 196.0).abs() < 2.0, "center: {cx}");
        // "y" à direita → x ≈ 400-8 = 392.
        let rx = texts.iter().find(|(t, _)| t == "y").unwrap().1;
        assert!((rx - 392.0).abs() < 2.0, "right: {rx}");
    }

    #[test]
    fn svg_reserva_a_caixa() {
        // um `<svg>` reserva a caixa (width/height do atributo, ou razão do
        // viewBox) mesmo sem desenhar o vetor — o logo/ícones ocupam o espaço.
        let dom = parse_html_to_dom(
            "<div><svg id=logo width=272 height=92 viewBox='0 0 272 92'></svg></div>\
             <svg id=ico width=24 height=24></svg>",
        );
        let ctx = LayoutCtx { viewport_w: 1000.0, viewport_h: 800.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let r = |sel: &str| list.node_rects[&dom.resolve(dom.query(sel).unwrap()).unwrap()];
        let logo = r("#logo");
        assert!((logo.w - 272.0).abs() < 1.0 && (logo.h - 92.0).abs() < 1.0, "logo: {}x{}", logo.w, logo.h);
        let ico = r("#ico");
        assert!((ico.w - 24.0).abs() < 1.0 && (ico.h - 24.0).abs() < 1.0, "ico: {}x{}", ico.w, ico.h);
    }

    #[test]
    fn grid_fr_track_sizing() {
        // grid-template-columns: 200px 1fr 2fr num container 620 → 200/140/280.
        let dom = parse_html_to_dom(
            "<div style='display:grid;grid-template-columns:200px 1fr 2fr;width:620px'>\
             <div id=a>A</div><div id=b>B</div><div id=c>C</div></div>",
        );
        let ctx = LayoutCtx { viewport_w: 1000.0, viewport_h: 800.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let rect = |sel: &str| {
            let idx = dom.resolve(dom.query(sel).unwrap()).unwrap();
            list.node_rects[&idx]
        };
        let a = rect("#a"); let b = rect("#b"); let c = rect("#c");
        assert!((a.w - 200.0).abs() < 1.0, "col px: {}", a.w);
        assert!((b.w - 140.0).abs() < 1.0, "col 1fr: {}", b.w); // (620-200)/3
        assert!((c.w - 280.0).abs() < 1.0, "col 2fr: {}", c.w); // 2×140
        assert!((b.x - 200.0).abs() < 1.0 && (c.x - 340.0).abs() < 1.0, "posições");
    }

    #[test]
    fn grid_align_items_center_centraliza_na_celula() {
        // single-column grid de altura fixa + align-items:center → o item de
        // altura menor centraliza verticalmente na track (o logo do google).
        let dom = parse_html_to_dom(
            "<div style='display:grid;align-items:center;height:240px'>\
             <div id=logo style='height:92px'>x</div></div>",
        );
        let ctx = LayoutCtx { viewport_w: 1000.0, viewport_h: 800.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let idx = dom.resolve(dom.query("#logo").unwrap()).unwrap();
        let r = list.node_rects[&idx];
        assert!((r.y - 74.0).abs() < 2.0, "y centralizado: {} (esperado 74=(240-92)/2)", r.y);
        assert!((r.h - 92.0).abs() < 2.0, "altura preservada: {}", r.h);
    }

    #[test]
    fn calc_de_altura_resolve_contra_a_altura() {
        // `calc(100% - 560px)` num `height` resolve o `%` contra a ALTURA do
        // containing block (800), não a largura (1000): 800-560=240, não 440.
        let dom = parse_html_to_dom(
            "<div style='height:800px'>\
             <div id=c style='height:calc(100% - 560px);background:#eee'>x</div>\
             </div>",
        );
        let ctx = LayoutCtx { viewport_w: 1000.0, viewport_h: 800.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let c = dom.query("#c").unwrap();
        let idx = dom.resolve(c).unwrap();
        assert!((list.node_rects[&idx].h - 240.0).abs() < 2.0,
            "calc height: {} (esperado 240 = 800-560)", list.node_rects[&idx].h);
    }

    #[test]
    fn flex_grow_vertical_da_altura_a_filho_100pct() {
        // flex column 800px: navbar 60 + item flex-grow:1 (cresce p/ 740) e o
        // filho height:100% do item resolve contra os 740 (não a altura própria).
        let dom = parse_html_to_dom(
            "<div style='display:flex;flex-direction:column;height:800px'>\
             <div style='height:60px'>nav</div>\
             <div style='flex-grow:1'><div id=alvo style='height:100%;background:#00f'>x</div></div>\
             </div>",
        );
        let ctx = LayoutCtx { viewport_w: 1000.0, viewport_h: 800.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let alvo = dom.query("#alvo").unwrap();
        let idx = dom.resolve(alvo).unwrap();
        let r = list.node_rects[&idx];
        assert!((r.h - 740.0).abs() < 2.0, "altura do filho 100%: {} (esperado 740)", r.h);
        assert!((r.y - 60.0).abs() < 2.0, "y do filho: {}", r.y);
    }

    #[test]
    fn absolute_ancora_no_containing_block() {
        // `position:absolute` com right:0/top:0 ancora no canto do ANCESTRAL
        // positioned (relative), não do viewport (o padrão do google: ícone no
        // canto da caixa de busca).
        let dom = parse_html_to_dom(
            "<div style='position:relative;width:400px;height:50px;margin-left:100px'>\
             <span style='position:absolute;top:0px;right:0px;width:30px;height:30px;background:#00f'>i</span>\
             </div>",
        );
        let ctx = LayoutCtx { viewport_w: 1000.0, viewport_h: 800.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        // o span azul: canto direito da caixa (x=100, w=400 → 500) menos a largura.
        let sp = list.items.iter().find_map(|it| match it {
            DisplayItem::SolidRect { rect, color, .. } if *color == 0x0000FFFF => Some(*rect),
            _ => None,
        }).expect("span absolute");
        assert!((sp.x - 470.0).abs() < 2.0, "x do abs: {} (esperado ~470 = 100+400-30)", sp.x);
        assert!(sp.y.abs() < 2.0 || (sp.y - 0.0).abs() < 2.0, "y do abs: {}", sp.y);
    }

    #[test]
    fn link_ua_azul_sublinhado_por_run() {
        // `<a>` sem CSS de autor: cor azul default + underline (deco=1) SÓ no seu
        // texto — o texto adjacente do parágrafo fica preto e sem decoração.
        crate::block::define("p", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        let dom = parse_html_to_dom("<p>antes <a href=x>link</a> depois</p>");
        let ctx = LayoutCtx { viewport_w: 600.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
        let list = layout_document(&dom, &ctx);
        let segs: Vec<(String, u32, u8)> = list.items.iter().filter_map(|it| match it {
            DisplayItem::Text { text, color, decoration, .. } => Some((text.clone(), *color, *decoration)),
            _ => None,
        }).collect();
        let link = segs.iter().find(|(t, ..)| t.contains("link")).expect("run do link");
        assert_eq!(link.1, 0x0000EEFF, "link azul");
        assert_eq!(link.2, 1, "link sublinhado");
        // o texto ao redor (preto, sem deco) é um segmento SEPARADO.
        assert!(segs.iter().any(|(t, c, d)| t.contains("antes") && *c == 0x000000FF && *d == 0));
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
        // line-height:3 = 3×16 = 48px entre as linhas (div sem margin).
        let y_oi = texts.iter().find(|(t, _)| t == "OI").unwrap().1;
        let y_tchau = texts.iter().find(|(t, _)| t == "TCHAU").unwrap().1;
        assert!((y_tchau - y_oi - 48.0).abs() < 5.0, "line-height: {y_oi} → {y_tchau}");
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
        // flex-shrink:0 nos cards: estes testes validam JUSTIFY (inclusive em
        // overflow real, como o Chrome foi medido); sem o 0, o shrink default=1
        // (agora implementado!) encolheria os itens e não haveria overflow.
        let mut html = format!("<style>row{{display:flex;{style}}} .c{{width:100px;flex-shrink:0;background:#111}}</style><row>");
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
    fn flex_grow_distribui_o_espaco() {
        // O GRID do Bootstrap: `.col { flex: 1 0 0% }` — 3 colunas dividem o
        // container igualmente (base 0, grow distribui TUDO).
        crate::block::define("row", crate::block::BlockDef { display: 2, indent: 0.0, prefix: 0, flags: 0 });
        crate::block::define("div", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        let list = layout(
            "<style>row{display:flex} .col{flex:1 0 0%;background:#111}</style>\
             <row><div class='col'>a</div><div class='col'>b</div><div class='col'>c</div></row>",
            600.0,
        );
        let r = all_rects(&list);
        assert_eq!(r.len(), 3);
        assert!((r[0].w - 200.0).abs() < 0.5, "3 colunas iguais: {r:?}");
        assert!((r[1].x - 200.0).abs() < 0.5, "{r:?}");
        assert!((r[2].x - 400.0).abs() < 0.5, "{r:?}");
        // grow 1:2 → 200 e 400.
        let l2 = layout(
            "<style>row{display:flex} .a{flex:1 0 0%;background:#111} .b{flex:2 0 0%;background:#222}</style>\
             <row><div class='a'>a</div><div class='b'>b</div></row>",
            600.0,
        );
        let r2 = all_rects(&l2);
        assert!((r2[0].w - 200.0).abs() < 0.5, "grow 1: {r2:?}");
        assert!((r2[1].w - 400.0).abs() < 0.5, "grow 2: {r2:?}");
    }

    #[test]
    fn flex_shrink_encolhe_em_overflow() {
        // shrink DEFAULT = 1 (fiel ao CSS): 2 itens de 400 em 600 encolhem para
        // 300 cada; com shrink:0 no primeiro, ele mantém 400 e o outro cede.
        crate::block::define("row", crate::block::BlockDef { display: 2, indent: 0.0, prefix: 0, flags: 0 });
        crate::block::define("div", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        let list = layout(
            "<style>row{display:flex} .c{width:400px;background:#111}</style>\
             <row><div class='c'>a</div><div class='c'>b</div></row>",
            600.0,
        );
        let r = all_rects(&list);
        assert!((r[0].w - 300.0).abs() < 0.5, "shrink default encolhe: {r:?}");
        assert!((r[1].w - 300.0).abs() < 0.5, "{r:?}");
        let l2 = layout(
            "<style>row{display:flex} .fix{width:400px;flex-shrink:0;background:#111} .c{width:400px;background:#222}</style>\
             <row><div class='fix'>a</div><div class='c'>b</div></row>",
            600.0,
        );
        let r2 = all_rects(&l2);
        assert!((r2[0].w - 400.0).abs() < 0.5, "shrink:0 nao cede: {r2:?}");
        assert!((r2[1].w - 200.0).abs() < 0.5, "o flexivel cede tudo: {r2:?}");
    }

    #[test]
    fn flex_order_e_align_self() {
        // `order` reordena visualmente (menor primeiro); `align-self` vence o
        // align-items do container; STRETCH real estica o item sem height.
        crate::block::define("row", crate::block::BlockDef { display: 2, indent: 0.0, prefix: 0, flags: 0 });
        crate::block::define("div", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        let list = layout(
            "<style>row{display:flex;height:100px;align-items:flex-start}\
              .a{order:2;width:100px;height:20px;background:#111}\
              .b{order:1;width:100px;height:20px;align-self:center;background:#222}\
              .s{align-self:stretch;width:100px;background:#333}</style>\
             <row><div class='a'>a</div><div class='b'>b</div><div class='s'>s</div></row>",
            600.0,
        );
        // os rects saem na ordem VISUAL (pós-order): .s (0) → .b (1) → .a (2).
        let r = all_rects(&list);
        assert!((r[0].x - 0.0).abs() < 0.5, ".s primeiro (order 0): {r:?}");
        assert!((r[1].x - 100.0).abs() < 0.5, ".b no meio (order 1): {r:?}");
        assert!((r[2].x - 200.0).abs() < 0.5, ".a por ultimo (order 2): {r:?}");
        // align-self:stretch do .s (sem height): estica até a linha (100) — o
        // align-items:flex-start do container é VENCIDO pelo align-self.
        assert!((r[0].h - 100.0).abs() < 0.5, "stretch estica: {r:?}");
        // align-self:center do .b: y = (100-20)/2 = 40.
        assert!((r[1].y - 40.0).abs() < 0.5, "align-self center: {r:?}");
        // .a fica no topo (align-items:flex-start do container).
        assert!((r[2].y - 0.0).abs() < 0.5, "{r:?}");
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

    /// Coleta todos os SolidRect da lista, em ordem (container primeiro, filhos
    /// depois — o fundo do container é inserido ATRÁS dos filhos).
    fn all_rects(list: &DisplayList) -> Vec<Rect> {
        list.items
            .iter()
            .filter_map(|it| match it {
                DisplayItem::SolidRect { rect, .. } => Some(*rect),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn flex_column_empilha_com_gap() {
        // `flex-direction:column`: itens empilham na VERTICAL (main = Y) com o gap
        // entre eles; align default (stretch) → cada item ocupa a largura.
        let list = layout(
            "<div style='display:flex; flex-direction:column; gap:10; background:#111'>\
               <div style='background:#222; height:30'>a</div>\
               <div style='background:#333; height:40'>b</div>\
             </div>",
            600.0,
        );
        let r = all_rects(&list);
        assert_eq!(r.len(), 3, "container + 2 filhos: {r:?}");
        // filhos: o 1º em y=0; o 2º abaixo (30 + gap 10 = 40) — NÃO lado a lado.
        assert_eq!(r[1].y, 0.0);
        assert_eq!((r[1].h, r[2].h), (30.0, 40.0));
        assert_eq!(r[2].y, 40.0);
        assert_eq!(r[1].x, r[2].x, "mesmo X (coluna, não row)");
        // stretch (default): os itens ocupam a largura do container.
        assert_eq!(r[1].w, 600.0);
    }

    #[test]
    fn flex_column_margin_auto_empurra() {
        // O padrão do Bootstrap cover: header + main(mt-auto/mb-auto) + footer numa
        // coluna com altura — os margins auto ABSORVEM o espaço livre e centralizam
        // o main (spec flexbox §8.1; mb-auto/mt-auto).
        let list = layout(
            "<div style='display:flex; flex-direction:column; height:300; background:#111'>\
               <div style='background:#222; height:20'>h</div>\
               <div style='background:#333; height:60; margin-top:auto; margin-bottom:auto'>m</div>\
               <div style='background:#444; height:20'>f</div>\
             </div>",
            600.0,
        );
        let r = all_rects(&list);
        assert_eq!(r.len(), 4);
        // free = 300 - (20+60+20) = 200; 2 autos → 100 cada.
        assert_eq!(r[1].y, 0.0); // header no topo
        assert_eq!(r[2].y, 120.0); // main: 20 + 100 (mt-auto)
        assert_eq!(r[3].y, 280.0); // footer: 120 + 60 + 100 (mb-auto)
    }

    #[test]
    fn flex_column_justify_center() {
        // justify-content atua no eixo PRINCIPAL (Y em column) quando o container
        // tem altura: um item de 50 num container de 300 centra em y=125.
        let list = layout(
            "<div style='display:flex; flex-direction:column; height:300; justify-content:center'>\
               <div style='background:#222; height:50'>x</div>\
             </div>",
            600.0,
        );
        let r = all_rects(&list);
        assert_eq!(r[0].y, 125.0, "{r:?}");
    }

    #[test]
    fn height_percent_resolve_contra_altura_do_pai() {
        // `height:%` resolve contra a ALTURA do containing block (antes resolvia
        // errado contra a LARGURA). Pai height:200 → filho 50% = 100.
        let list = layout(
            "<div style='height:200; background:#111'>\
               <div style='height:50%; background:#222'>x</div>\
             </div>",
            600.0,
        );
        let r = all_rects(&list);
        assert_eq!(r[0].h, 200.0);
        assert_eq!(r[1].h, 100.0, "50% de 200: {r:?}");
        // pai SEM height (auto): height:% do filho vira auto (altura do conteúdo,
        // 1 linha ≈ 26) — fiel ao browser, não 50% da largura (que daria 300).
        let l2 = layout(
            "<div style='background:#111'><div style='height:50%; background:#222'>x</div></div>",
            600.0,
        );
        let r2 = all_rects(&l2);
        assert!(r2[1].h < 40.0, "height %% com pai auto = altura natural: {r2:?}");
    }

    #[test]
    fn unidades_relativas_em_padding_e_margem_negativa() {
        // padding: 1rem = 16px (root 16, o default de browser) — o `p-3` do
        // Bootstrap; margem NEGATIVA puxa (os gutters `.row` usam margin -12px).
        let list = layout("<div style='background:#111; padding:1rem'>x</div>", 600.0);
        let tx = list
            .items
            .iter()
            .find_map(|it| match it {
                DisplayItem::Text { x, y, .. } => Some((*x, *y)),
                _ => None,
            })
            .unwrap();
        assert_eq!(tx, (16.0, 16.0), "texto após o padding de 1rem = 16px");
        // margem negativa: o segundo bloco com margin-top:-10 SOBE sobre o primeiro.
        let l2 = layout(
            "<div style='background:#111; height:30'>a</div>             <div style='background:#222; height:30; margin-top:-10px'>b</div>",
            600.0,
        );
        let r2 = all_rects(&l2);
        assert_eq!(r2[1].y, 20.0, "30 - 10 (negativa nao clampa): {r2:?}");
    }

    #[test]
    fn max_width_em_bate_com_o_chrome() {
        // `max-width: 42em` com root 16 = 672px — o `.cover-container` do Bootstrap
        // cover, VALIDADO numero-a-numero no Chrome (viewport 1000: rect 164,0,672).
        let list = layout(
            "<div style='background:#111; max-width:42em; margin:0 auto; height:50'>x</div>",
            1000.0,
        );
        let r = all_rects(&list);
        assert_eq!(r[0].w, 672.0, "42em x 16: {r:?}");
        assert_eq!(r[0].x, 164.0, "(1000-672)/2, centrado como no Chrome");
    }

    #[test]
    fn inline_flow_links_fluem_no_paragrafo() {
        // P4 (o coracao): <p>texto <a>link</a>, fim</p> flui numa UNICA linha —
        // antes cada filho virava linha propria (o footer do cover saia em 5
        // linhas). A pontuacao NAO ganha espaco (fiel ao fonte: "Bootstrap, by").
        crate::block::define("p", crate::block::BlockDef { display: 0, indent: 0.0, prefix: 0, flags: 0 });
        let list = layout(
            "<p style='color:#ffffff'>Cover template for <a style='color:#ff0000'>Bootstrap</a>, by <a style='color:#00ff00'>mdo</a>.</p>",
            600.0,
        );
        let texts: Vec<(String, f32, f32, u32)> = list
            .items
            .iter()
            .filter_map(|it| match it {
                DisplayItem::Text { text, x, y, color, .. } => {
                    Some((text.clone(), *x, *y, *color))
                }
                _ => None,
            })
            .collect();
        // TODOS os segmentos na MESMA linha (y igual).
        let y0 = texts[0].2;
        assert!(texts.iter().all(|(_, _, y, _)| *y == y0), "uma linha so: {texts:?}");
        // os segmentos avancam em x (fluem lado a lado) e preservam a cor do span.
        assert!(texts.len() >= 4, "segmentos por cor: {texts:?}");
        assert!(texts.windows(2).all(|w| w[1].1 > w[0].1), "x crescente: {texts:?}");
        assert_eq!(texts[1].3, 0xFF0000FF, "cor do link preservada");
        // a virgula gruda no link (segmento seguinte comeca com ',' sem espaco).
        assert!(texts[2].0.starts_with(','), "pontuacao sem espaco: {:?}", texts[2].0);
    }

    #[test]
    fn float_left_right_dividem_a_linha() {
        // O header clássico (brand+nav do Bootstrap cover): float:left e
        // float:right consecutivos dividem a MESMA linha; o irmão não-float
        // começa abaixo do float mais alto; o pai contém os floats (BFC v1).
        let list = layout(
            "<div style='background:#111'>               <div style='float:left; background:#222; width:100; height:30'>brand</div>               <div style='float:right; background:#333; width:150; height:40'>nav</div>               <div style='background:#444; height:20'>abaixo</div>             </div>",
            600.0,
        );
        let r = all_rects(&list);
        assert_eq!(r.len(), 4);
        assert_eq!((r[1].x, r[1].y), (0.0, 0.0), "left encosta na esquerda: {r:?}");
        assert_eq!((r[2].x, r[2].y), (450.0, 0.0), "right encosta na direita (600-150)");
        assert_eq!(r[3].y, 40.0, "nao-float comeca abaixo do float mais alto");
        assert_eq!(r[0].h, 60.0, "pai contem os floats: 40 + 20");
    }

    #[test]
    fn position_fixed_sai_do_fluxo_e_posiciona_no_viewport() {
        // O caso do dropdown do Bootstrap cover: um `position:fixed` DENTRO de um
        // flex row NÃO pode empurrar os irmãos (sai do fluxo) e pinta contra o
        // viewport pelos offsets (bottom/right).
        let list = layout(
            "<div style='display:flex; background:#111'>\
               <div style='position:fixed; bottom:10; right:10; width:50; height:20; background:#900'>t</div>\
               <div style='background:#222; height:30'>conteudo</div>\
             </div>",
            600.0,
        );
        let r = all_rects(&list);
        assert_eq!(r.len(), 3);
        // o item NO FLUXO começa em x=0 (o fixed não o empurrou).
        assert_eq!(r[1].x, 0.0, "{r:?}");
        assert_eq!(r[1].h, 30.0);
        // o fixed: x = 600-50-10 = 540; y = viewport_h(600)-20-10 = 570. Pintado por
        // ÚLTIMO (por cima do fluxo).
        assert_eq!((r[2].x, r[2].y), (540.0, 570.0), "{r:?}");
        assert_eq!((r[2].w, r[2].h), (50.0, 20.0));
    }

    #[test]
    fn viewport_e_o_containing_block_da_raiz() {
        // `height:100%` num filho direto do document resolve contra a viewport_h
        // (600 no helper) — o `h-100` do html/body de páginas reais.
        let list = layout("<div style='height:100%; background:#111'>x</div>", 600.0);
        let r = all_rects(&list);
        assert_eq!(r[0].h, 600.0, "{r:?}");
    }
}
