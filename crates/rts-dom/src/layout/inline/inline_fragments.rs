//! O inline como FRAGMENTOS de linha (CSS 2.1 §9.2.2): a caixa de um
//! `<span>` que quebra é a união dos pedaços que ficam em cada linha, e o
//! fundo/borda/padding pintam-se por pedaço — a borda esquerda só no primeiro,
//! a direita só no último, o topo e o fundo em todos. Extraído de `line.rs`
//! (no teto de 500 linhas): o fragmento de cada dono, e as superfícies de uma
//! linha.
//!
//! Antes deste módulo um inline com superfície era promovido a caixa atómica
//! (`AtomicKind::Block`) e NEM QUEBRAVA — `claude-inline-fragmentos`: o
//! contentor de 130px ficava com 22px de altura onde o Blink dá 60 (três
//! linhas), e a borda engrossava a linha. Os três remendos revertidos na vaga 3
//! tentavam corrigir isso sem separar "quem pinta" de "quem flui".

use super::*;

/// Um grupo inline sem nenhum átomo com corpo — só `Marker`s (e whitespace
/// entre eles) — não cria linha, mas cada `Marker` ainda é um elemento do
/// documento e o Blink dá-lhe um retângulo 0×0 na posição onde a linha teria
/// começado (`claude-sel-has.html`). Todos caem no MESMO ponto: um marker não
/// tem largura, nenhum avança o cursor.
pub(in crate::layout) fn register_markers_without_line(
    list: &mut DisplayList,
    x: f32,
    y: f32,
    runs: &[InlineRun],
    group: &[(NodeIdx, crate::boxes::BoxId)],
) {
    let line_scope = super::LineScope::fresh(group);
    for r in runs {
        if let Some((idx, _, AtomicKind::Marker)) = r.atomic {
            crate::inline_box::union_rect(list, idx, Rect::new(x, y, 0.0, 0.0), &line_scope);
        }
    }
}

/// O fragmento que ESTE dono recebe desta fatia de linha.
///
/// A altura é a content area da fonte DELE, não a do bloco que conduz o fluxo:
/// um `<span>` de 14px dentro de um título de 17,5px mede 15,75 e não 19,7. Sem
/// isto, 1 172 dos 1 257 `<span>` da Wikipédia com altura errada tinham
/// exatamente `1.125 x a fonte de um ANCESTRAL`. Fica CENTRADO na content area
/// da linha (a mesma aproximação da meia-entrelinha). Um inline por fragmentos
/// leva ainda o padding e a borda verticais — a border box, que é o que o
/// `getBoundingClientRect` do Blink responde (y=-2 com `border:2px` numa linha
/// que começa em 0).
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn owner_fragment(
    dom: &Dom,
    owner: NodeIdx,
    x: f32,
    y: f32,
    w: f32,
    line_content: f32,
    ctx: &LayoutCtx,
    align_to_baseline: bool,
) -> Rect {
    let css = dom.computed_style_idx(owner);
    let with_edges = css.as_deref().is_some_and(crate::inline_box::inline_por_fragmentos);
    // A relative inline — or one inside a relative inline — is shifted HERE, the
    // one place its fragment is made: client rects and painted surface move together.
    let (dx, dy) = crate::layout::positioned::relative::offset_do_inline(dom, Some(owner), ctx);
    styled_fragment(css.as_deref(), with_edges, x + dx, y + dy, w, line_content, ctx, align_to_baseline)
}

/// [`owner_fragment`] for a style that is not a node's — a generated
/// box's. `with_edges`: the rect is the border box (vertical padding and
/// border added), which a node asks through `inline_por_fragmentos` and a
/// generated inline with a surface always is.
#[allow(clippy::too_many_arguments)]
fn styled_fragment(
    css: Option<&ComputedStyle>,
    with_edges: bool,
    x: f32,
    y: f32,
    w: f32,
    line_content: f32,
    ctx: &LayoutCtx,
    align_to_baseline: bool,
) -> Rect {
    let Some(css) = css else {
        return Rect::new(x, y, w, line_content);
    };
    let Some(crate::style::Dimension::Px(font)) = css.font_size else {
        return Rect::new(x, y, w, line_content);
    };
    let content = crate::inline_box::altura_do_conteudo(font, css.font_family.as_deref(), ctx.measurer);
    let top = if align_to_baseline {
        y - ctx.measurer.font_ascent_family(font, css.font_family.as_deref())
    } else {
        y + (line_content - content) / 2.0
    };
    if with_edges {
        let [_, _, top_edge, bottom_edge] =
            crate::inline_box::arestas_do_inline(css, font, ctx.viewport_w, ctx);
        return Rect::new(x, top - top_edge, w, content + top_edge + bottom_edge);
    }
    Rect::new(x, top, w, content)
}

/// As superfícies (fundo e borda) dos inlines por fragmentos de UMA linha,
/// acumuladas ao longo dos segmentos e pintadas atrás deles no fim.
#[derive(Default)]
pub(in crate::layout) struct Surfaces {
    // Por ordem de primeira aparição — um ancestral aparece antes do
    // descendente, que é a ordem de pintura (o fundo do `<a>` por cima do do
    // `<span>` que o contém).
    surfaces: Vec<Surface>,
}

/// Whose surface it is. A generated inline (`::before`/`::after`) has no
/// node, so it is named by its originating element and the pseudo-element —
/// the same pair `pseudo/mod.rs` keys counters by.
#[derive(Clone, Copy, PartialEq)]
enum Owner {
    Node(NodeIdx),
    Generated(NodeIdx, crate::style::PseudoElement),
}

struct Surface {
    owner: Owner,
    x0: f32,
    x1: f32,
    // este fragmento contém a aresta inicial/final do inline? (é o que decide
    // se a borda esquerda/direita se pinta aqui)
    start: bool,
    end: bool,
    // A generated inline whose end edge has not been seen yet: every segment
    // until then is its content. A node's surface does not need this — each
    // segment names its node owners — but a segment cannot name a generated
    // box (it has no node), and the pseudo's text is by construction exactly
    // what lies between its two edges in the run order.
    open: bool,
}

impl Surfaces {
    /// Um segmento de `x0` a `x1` pertence a estes donos — and to every
    /// generated inline still open.
    pub(in crate::layout) fn cover(&mut self, dom: &Dom, owners: &[NodeIdx], x0: f32, x1: f32) {
        for s in self.surfaces.iter_mut().filter(|s| s.open) {
            s.x0 = s.x0.min(x0);
            s.x1 = s.x1.max(x1);
        }
        for &o in owners {
            let flows = dom
                .computed_style_idx(o)
                .is_some_and(|c| crate::inline_box::inline_por_fragmentos(&c));
            if !flows {
                continue;
            }
            match self.surfaces.iter_mut().find(|s| s.owner == Owner::Node(o)) {
                Some(s) => {
                    s.x0 = s.x0.min(x0);
                    s.x1 = s.x1.max(x1);
                }
                None => self.surfaces.push(Surface { owner: Owner::Node(o), x0, x1, start: false, end: false, open: false }),
            }
        }
    }

    /// A aresta inicial (`start == true`) ou final do inline `owner` está nesta linha.
    pub(in crate::layout) fn mark(&mut self, owner: NodeIdx, start: bool) {
        if let Some(s) = self.surfaces.iter_mut().find(|s| s.owner == Owner::Node(owner)) {
            if start {
                s.start = true;
            } else {
                s.end = true;
            }
        }
    }

    /// The start edge of the generated inline `pe` of `id` is the segment at
    /// `x` of width `ww`: its surface opens after its left margin. `base_w`
    /// is the line's width, the base the edge was sized against.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::layout) fn open_generated(&mut self, dom: &Dom, id: NodeIdx, pe: crate::style::PseudoElement, x: f32, ww: f32, base_w: f32, ctx: &LayoutCtx) {
        let (ml, _) = generated_margins(dom, id, pe, base_w, ctx);
        let owner = Owner::Generated(id, pe);
        self.surfaces.push(Surface { owner, x0: x + ml, x1: x + ww, start: true, end: false, open: true });
    }

    /// Its end edge was just seen (and [`Self::cover`] already took it in): the
    /// surface closes before its right margin.
    pub(in crate::layout) fn close_generated(&mut self, dom: &Dom, id: NodeIdx, pe: crate::style::PseudoElement, base_w: f32, ctx: &LayoutCtx) {
        let (_, mr) = generated_margins(dom, id, pe, base_w, ctx);
        if let Some(s) = self.surfaces.iter_mut().find(|s| s.open && s.owner == Owner::Generated(id, pe)) {
            s.x1 -= mr;
            s.end = true;
            s.open = false;
        }
    }

    /// Insere o fundo e as barras de borda de cada dono em `at` (the piece
    /// position where the line began), atrás do texto. CORTE dito: as cores saem cruas — sem
    /// `opacity`/`filter` do elemento, que o caminho de bloco aplica por
    /// `cor()` — e sem `border-radius`.
    ///
    /// Returns what the NEXT line starts with: a generated inline still open
    /// here goes on there, with no start edge and no extent yet.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::layout) fn paint(
        self,
        dom: &Dom,
        list: &mut DisplayList,
        at: usize,
        line_scope: super::LineId,
        y: f32,
        line_content: f32,
        align_to_baseline: bool,
        ctx: &LayoutCtx,
    ) -> Surfaces {
        let mut at = at;
        let mut put = |list: &mut DisplayList, rect: Rect, color: u32| {
            list.pieces.insert(at, Piece::Item(DisplayItem::SolidRect { rect, color, radius: Corners::ZERO }));
            at += 1;
        };
        let mut next_surfaces = Surfaces::default();
        for s in self.surfaces {
            if s.open {
                let (x0, x1) = (f32::INFINITY, f32::NEG_INFINITY);
                next_surfaces.surfaces.push(Surface { x0, x1, start: false, end: false, ..s });
            }
            if s.x1 < s.x0 {
                continue;
            }
            let (css, r) = match s.owner {
                Owner::Node(n) => {
                    let Some(css) = dom.computed_style_idx(n) else { continue };
                    let r = owner_fragment(dom, n, s.x0, y, s.x1 - s.x0, line_content, ctx, align_to_baseline);
                    (css, r)
                }
                Owner::Generated(n, pe) => {
                    let Some(pseudo) = dom.pseudo_box(n, pe) else { continue };
                    let r = styled_fragment(Some(&pseudo.css), true, s.x0, y, s.x1 - s.x0, line_content, ctx, align_to_baseline);
                    // Each line's piece of the generated inline is a fragment
                    // of its box, as a real inline's are (`union_rect`). Found
                    // by node because a surface is named by `(node, pseudo)`;
                    // only the fragment that holds the box emits it (`runs.rs`).
                    if let Some(generated) = list.tree.generated_of(n, pe) {
                        crate::layout::fragment::items::add_line_fragment(list, generated, r, line_scope);
                    }
                    (std::rc::Rc::new(pseudo.css), r)
                }
            };
            if let Some(bg) = css.bg.filter(|_| !deve_suprimir_fundo(&css)) {
                put(list, r, bg);
            }
            let sides = crate::style::borders::resolved_sides(&css);
            let [t, rt, b, l] = crate::style::borders::used_widths(&css);
            if sides[0].paints() {
                put(list, Rect::new(r.x, r.y, r.w, t), sides[0].color);
            }
            if sides[2].paints() {
                put(list, Rect::new(r.x, r.y + r.h - b, r.w, b), sides[2].color);
            }
            if s.start && sides[3].paints() {
                put(list, Rect::new(r.x, r.y, l, r.h), sides[3].color);
            }
            if s.end && sides[1].paints() {
                put(list, Rect::new(r.x + r.w - rt, r.y, rt, r.h), sides[1].color);
            }
        }
        next_surfaces
    }
}

/// The horizontal margins of the generated box `pe` of `id`, resolved as
/// `pseudo_inline.rs` resolved them when it sized the edges.
fn generated_margins(dom: &Dom, id: NodeIdx, pe: crate::style::PseudoElement, base_w: f32, ctx: &LayoutCtx) -> (f32, f32) {
    dom.pseudo_box(id, pe).map_or((0.0, 0.0), |pseudo| {
        let font = font_px(&pseudo.css, DEFAULT_FONT_SIZE);
        super::pseudo_inline::horizontal_margins(&pseudo.css, font, base_w, ctx)
    })
}

/// Este grupo é TODO o conteúdo do dono?
///
/// A caixa gerada do DONO envolve todo o conteúdo dele — e só existe como run
/// aqui quando este grupo É todo o conteúdo. Com filhos de bloco pelo meio, o
/// conteúdo do dono parte-se em vários grupos e a caixa gerada teria de virar
/// um bloco anónimo, que é maquinaria de árvore de caixas que este layout não
/// tem; nesse caso não se gera nada, que é o estado anterior, em vez de a pôr
/// num pedaço arbitrário do conteúdo.
///
/// Contado sobre os filhos que geram conteúdo. Os nós de texto só com espaços não contam: um HTML
/// indentado põe um antes e outro depois de cada elemento, e compará-los
/// fazia um `<div>` com o `<span>` numa linha indentada parecer conteúdo
/// partido, e perdia a caixa gerada em quase toda a página real.
pub(in crate::layout) fn group_is_whole_owner(
    dom: &Dom,
    owner: NodeIdx,
    group: &[(NodeIdx, crate::boxes::BoxId)],
) -> bool {
    // A comment counts on NEITHER side. It has no box, so it never enters the
    // group (`FlowStep::NoBox` only opens one); before BT-2a it was in
    // the group with no box and counted on both sides — the same equality.
    let counts = |n: NodeIdx| match &dom.node(n).kind {
        NodeKind::Comment(_) => false,
        NodeKind::Text(t) => !t.trim().is_empty(),
        _ => true,
    };
    let children_with_content = dom.node(owner).children.iter().filter(|&&c| counts(c)).count();
    group.iter().filter(|&&(c, _)| counts(c)).count() == children_with_content
}
