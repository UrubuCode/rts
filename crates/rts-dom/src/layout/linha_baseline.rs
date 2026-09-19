//! The vertical placement of a LINE THAT HOLDS AN INLINE-BLOCK (CSS 2.1
//! §10.8.1): every item — the text's strut and each atom — has an extent
//! above and below one shared baseline, and the line box is the largest of
//! each side.
//!
//! The text flow (`linha.rs`) did not ask this. It took the line's height as
//! the tallest atom and treated every inline-block as sitting on its bottom
//! edge, with a special case for "taller than the strut and next to text".
//! That gave an empty 20px inline-block a 20px line where Blink gives 25 (the
//! text's descent hangs below the box, whose bottom is on the baseline), a
//! padded one with text 37 where Blink gives 32 (its baseline is its TEXT's,
//! not its bottom), and ignored `vertical-align` entirely
//! (`claude-inline-block-baseline`).
//!
//! The model already existed — [`Envelope`] and
//! [`topo_do_item_com_baseline`] in `alinhamento_vertical.rs`, used by the
//! run of sibling inline-blocks (`linha_ib.rs`) — and its module doc named
//! this migration as the cut still open. This module only feeds it the atoms
//! of a text line. A line with NO inline-block keeps the old path untouched:
//! text and images alone were already right, and moving them is a separate,
//! separately measured change.

use super::alinhamento_vertical::{Envelope, envelope_com_baseline, topo_do_item_com_baseline};
use super::*;
use crate::style::VerticalAlign;

/// One atom of the line as the envelope sees it: its outer height, the
/// distance from its top to ITS baseline, and its `vertical-align`.
struct Atomo {
    altura: f32,
    ascent: f32,
    valign: VerticalAlign,
}

/// The envelope of `line` when it holds an inline-block (a real one or a
/// generated one), `None` otherwise — the caller then keeps its old path.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn envelope_da_linha(
    dom: &Dom,
    line: &[Segment],
    font_size: f32,
    line_height: f32,
    family: Option<&str>,
    content_w: f32,
    ctx: &LayoutCtx,
) -> Option<Envelope> {
    if !line.iter().any(|s| e_inline_block(s)) {
        return None;
    }
    let itens: Vec<(f32, f32, VerticalAlign)> = line
        .iter()
        .filter_map(|s| atomo(dom, s, content_w, ctx))
        .map(|a| (a.altura, a.ascent, a.valign))
        .collect();
    Some(envelope_com_baseline(&itens, font_size, line_height, family, ctx.measurer))
}

/// The top of an atom of a line whose envelope is `env` and whose top is `cy`.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn topo_do_atomo(
    dom: &Dom,
    seg: &Segment,
    cy: f32,
    env: &Envelope,
    font_size: f32,
    family: Option<&str>,
    content_w: f32,
    ctx: &LayoutCtx,
) -> f32 {
    match atomo(dom, seg, content_w, ctx) {
        Some(a) => topo_do_item_com_baseline(a.valign, a.altura, a.ascent, cy, env, font_size, family, ctx.measurer),
        None => cy,
    }
}

fn e_inline_block(seg: &Segment) -> bool {
    matches!(
        seg.atomic,
        Some((_, _, AtomicKind::Block | AtomicKind::Gerada(_, crate::inline_box::ParteGerada::Atomo)))
    )
}

/// The atom behind a segment, for the atoms that have a body on the line:
/// inline-blocks (real and generated), replaced elements and form widgets.
fn atomo(dom: &Dom, seg: &Segment, content_w: f32, ctx: &LayoutCtx) -> Option<Atomo> {
    let (id, caixa, kind) = seg.atomic?;
    let altura = seg.wh;
    match kind {
        AtomicKind::Block => {
            let css = dom.computed_style_idx(id)?;
            Some(Atomo {
                altura,
                ascent: ascent_do_bloco(dom, id, caixa, &css, altura, seg.ww, content_w, ctx),
                valign: css.vertical_align.unwrap_or(VerticalAlign::Baseline),
            })
        }
        AtomicKind::Gerada(pe, crate::inline_box::ParteGerada::Atomo) => {
            let caixa = dom.pseudo_box(id, pe)?;
            Some(Atomo {
                altura,
                ascent: ascent_do_gerado(&caixa, altura, content_w, ctx),
                valign: caixa.css.vertical_align.unwrap_or(VerticalAlign::Baseline),
            })
        }
        // A replaced element has no baseline of its own: its bottom margin edge
        // sits on the line's (CSS 2.1 §10.8.1), which is where `linha.rs`
        // already puts it.
        AtomicKind::Replaced => Some(Atomo { altura, ascent: altura, valign: VerticalAlign::Baseline }),
        AtomicKind::Widget => Some(Atomo {
            altura,
            ascent: super::linha_ib::ascent_do_item(dom, id, altura, content_w, ctx),
            valign: dom
                .computed_style_idx(id)
                .and_then(|c| c.vertical_align)
                .unwrap_or(VerticalAlign::Baseline),
        }),
        _ => None,
    }
}

/// An inline-block's baseline, measured from the top of its OUTER box: its
/// LAST line box's baseline, or its bottom margin edge when it has no line
/// box or its `overflow` is not `visible` (CSS 2.1 §10.8.1).
///
/// The last line box is found by LAYING THE ATOM OUT in a throwaway list, the
/// way `measure_block` measures a height, and taking the lowest text it
/// painted. `ascent_do_item` answered with a formula over the box's OWN font,
/// which is right only when the box holds one line of its own text: a 14px
/// box holding a 26px line put its baseline 6px high, and the WPT
/// `flexbox-baseline-*` references (inline-blocks) disagreed with their tests
/// (inline-flexes). A flex container keeps `ascent_do_item`: its baseline is
/// its first item's (Flexbox §8.5), not its last line's. The cost is one extra
/// layout of each inline-block on a line that holds one — stated, not measured.
#[allow(clippy::too_many_arguments)]
fn ascent_do_bloco(
    dom: &Dom,
    id: NodeIdx,
    caixa: Option<crate::boxes::BoxId>,
    css: &ComputedStyle,
    altura: f32,
    largura: f32,
    content_w: f32,
    ctx: &LayoutCtx,
) -> f32 {
    if recorta(css) {
        return altura;
    }
    let flex = matches!(
        css.effective_display(),
        Some(
            crate::style::DisplayKind::Flex
                | crate::style::DisplayKind::FlexWrap
                | crate::style::DisplayKind::InlineFlex
                | crate::style::DisplayKind::InlineFlexWrap
        )
    );
    let fonte = font_px(css, DEFAULT_FONT_SIZE);
    let r = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: fonte,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let (mt, mb) = (css.margin.top.resolve(&r).unwrap_or(0.0), css.margin.bottom.resolve(&r).unwrap_or(0.0));
    if let (true, Some(caixa)) = (flex && super::flex_baseline::tem_itens_elemento(dom, id), caixa) {
        // A flex container's baseline is its first line's item's (Flexbox
        // §8.5), read where that item really sits once the container is laid out.
        let inicio = marca();
        let mut scratch = DisplayList::for_dom(dom);
        layout_block(dom, id, Some(caixa), 0.0, 0.0, content_w, None, Some(largura), None, false, true, &BlockFormattingContext::new(), ctx, &mut scratch);
        descarta(inicio);
        if let Some(b) = super::flex_baseline::baseline_no_layout(dom, id, &scratch, content_w, ctx) {
            return b.clamp(0.0, altura);
        }
    }
    if flex || caixa.is_none() {
        let borda = (altura - mt - mb).max(0.0);
        let dentro = super::linha_ib::ascent_do_item(dom, id, borda, content_w, ctx);
        // `ascent_do_item` answers the whole border box when the box is empty:
        // then the baseline is the bottom MARGIN edge, which is `altura`.
        return if dentro >= borda { altura } else { mt + dentro };
    }
    match baseline_da_ultima_linha(dom, id, caixa.expect("checked above"), largura, content_w, ctx) {
        Some(b) => b.min(altura),
        None => altura,
    }
}

/// One recorded line: the flow's owner, the baseline of its last line box in
/// the coordinates of the list it was laid into, and whether a cached
/// FRAGMENT re-announced it (its subtree's answer) or a flow pushed it live.
#[derive(Clone, Copy)]
struct Linha {
    dono: NodeIdx,
    baseline: f32,
    de_fragmento: bool,
}

thread_local! {
    /// The last line box of every inline flow laid out, in layout order —
    /// pushed by `layout_inline_flow` and the inline-block run
    /// ([`regista_ultima_linha`]) and by every emitted fragment
    /// ([`regista_do_fragmento`]), read by whoever [`colhe`]s.
    ///
    /// A thread-local and not a field of `DisplayList`, whose file is past the
    /// ceiling. Used as a STACK: a reader takes a [`marca`] before laying out,
    /// reads only what was pushed after it and truncates on the way out, so a
    /// nested reader leaves the outer's view intact. A fragment build collapses
    /// its whole subtree to one record, and `layout_document` clears the stack,
    /// so it never holds more than one pass's top-level records.
    static LINHAS: std::cell::RefCell<Vec<Linha>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Records the baseline of the last line of the inline flow owned by `dono`.
pub(in crate::layout) fn regista_ultima_linha(dono: NodeIdx, baseline: f32) {
    LINHAS.with(|v| v.borrow_mut().push(Linha { dono, baseline, de_fragmento: false }));
}

/// Re-announces a cached fragment's last own line where it is emitted: a
/// fragment served from the cache runs no flow, and without this an atom
/// holding a cached block would find no line in it and sit on its bottom edge —
/// silently, and only on the second layout pass.
pub(in crate::layout) fn regista_do_fragmento(dono: NodeIdx, baseline: f32) {
    LINHAS.with(|v| v.borrow_mut().push(Linha { dono, baseline, de_fragmento: true }));
}

/// Where the stack stands now — what a reader passes back to [`colhe`].
pub(in crate::layout) fn marca() -> usize {
    LINHAS.with(|v| v.borrow().len())
}

/// Drops what was recorded since `marca` unread — a throwaway layout's lines
/// are in its own coordinates and must not reach the reader around it.
pub(in crate::layout) fn descarta(marca: usize) {
    LINHAS.with(|v| v.borrow_mut().truncate(marca));
}

/// Empties the stack. Called where a document layout starts.
pub(in crate::layout) fn limpa() {
    LINHAS.with(|v| v.borrow_mut().clear());
}

/// The LOWEST own line box of `id` among what was recorded since `marca`, as
/// `(direct, total)`: `direct` counts only lines pushed live by a flow —
/// what a stitched fragment keeps when one of its child fragments is replaced
/// — and `total` counts the re-announced fragments too. Truncates to `marca`.
///
/// "Lowest" and not "last pushed": a fragment re-announces its subtree after
/// its own flows ran, so push order is not document order. In normal flow the
/// last line box is the lowest one; a negative margin that lifts a later line
/// above an earlier one is the case this gets wrong.
///
/// "Own" is [`fluxo_proprio`]: a line of `id` itself or of a block inside it,
/// never of an atom nested in it (whose lines are its own, CSS 2.1 §10.8.1).
pub(in crate::layout) fn colhe(dom: &Dom, id: NodeIdx, marca: usize) -> (Option<f32>, Option<f32>) {
    LINHAS.with(|v| {
        let mut v = v.borrow_mut();
        let maior = |acc: Option<f32>, b: f32| Some(acc.map_or(b, |m: f32| m.max(b)));
        let (mut directa, mut total) = (None, None);
        for l in v[marca.min(v.len())..].iter().filter(|l| fluxo_proprio(dom, id, l.dono, l.de_fragmento)) {
            total = maior(total, l.baseline);
            if !l.de_fragmento {
                directa = maior(directa, l.baseline);
            }
        }
        v.truncate(marca);
        (directa, total)
    })
}

/// `overflow` other than `visible` on either axis.
fn recorta(css: &ComputedStyle) -> bool {
    [css.overflow_x, css.overflow_y].iter().flatten().any(|o| o.clips())
}

/// [`colhe`] for the fragment of the BLOCK `id`, laid out at `y` with outer
/// height `altura`: a block that clips answers its bottom margin edge for both
/// values instead of its lines (see [`fluxo_proprio`]).
pub(in crate::layout) fn colhe_do_bloco(dom: &Dom, id: NodeIdx, marca: usize, y: f32, altura: f32) -> (Option<f32>, Option<f32>) {
    let linhas = colhe(dom, id, marca);
    if dom.computed_style_idx(id).is_some_and(|c| recorta(&c)) {
        return (Some(y + altura), Some(y + altura));
    }
    linhas
}

/// The `ultima_linha` of a STITCHED fragment of `id`: its direct lines, which
/// a stitch never touches, against each child fragment's own answer where the
/// child now sits (`dy` is the child's offset from where it was computed).
pub(in crate::layout) fn total_da_costura(
    dom: &Dom,
    id: NodeIdx,
    anterior: &Fragment,
    children: &[ChildRef],
    tree: &crate::boxes::BoxTree,
) -> Option<f32> {
    // A clipping block stands for its bottom edge, and a stitch keeps its size.
    if dom.computed_style_idx(id).is_some_and(|c| recorta(&c)) {
        return anterior.ultima_linha;
    }
    let directa = anterior.linha_directa;
    children
        .iter()
        .filter(|c| tree.node_of(c.caixa).is_none_or(|n| fluxo_proprio(dom, id, n, true)))
        .filter_map(|c| c.fragment.ultima_linha.map(|b| b + c.dy))
        .chain(directa)
        .fold(None, |acc: Option<f32>, b| Some(acc.map_or(b, |m| m.max(b))))
}

/// The baseline of an atom's LAST line box, from the top of its outer box, or
/// `None` when it has none: the atom is laid out in a throwaway list and its
/// lowest own line is the answer. "The lowest text painted" was tried first
/// and picked a nested `vertical-align: top` inline-block's text
/// (`flexbox-baseline-multi-line-horiz-001`).
fn baseline_da_ultima_linha(
    dom: &Dom,
    id: NodeIdx,
    caixa: crate::boxes::BoxId,
    largura: f32,
    content_w: f32,
    ctx: &LayoutCtx,
) -> Option<f32> {
    let inicio = marca();
    let mut scratch = DisplayList::for_dom(dom);
    layout_block(
        dom,
        id,
        Some(caixa),
        0.0,
        0.0,
        content_w,
        None,
        Some(largura),
        None,
        false,
        true,
        &BlockFormattingContext::new(),
        ctx,
        &mut scratch,
    );
    colhe(dom, id, inicio).1
}

/// Is the line recorded by `dono` one of the atom `id`'s OWN line boxes — `dono`
/// is `id`, or a block reached from it without crossing another atom, an
/// out-of-flow box or a box that CLIPS?
///
/// A block whose `overflow` is not `visible` hides its lines from the atom
/// around it and stands for its bottom margin edge instead (what Blink does,
/// and what WPT `CSS2/linebox/baseline-block-with-overflow-001` pins): its
/// lines are refused here, and its fragment announces the edge
/// ([`colhe_do_bloco`]) — a record `de_fragmento`, whose own `dono` is
/// therefore allowed to clip.
fn fluxo_proprio(dom: &Dom, id: NodeIdx, dono: NodeIdx, de_fragmento: bool) -> bool {
    let mut cur = dono;
    while cur != id {
        let Some(css) = dom.computed_style_idx(cur) else { return false };
        use crate::style::DisplayKind as D;
        let atomico = matches!(
            css.effective_display(),
            Some(D::InlineBlock | D::InlineFlex | D::InlineFlexWrap | D::InlineGrid | D::InlineTable | D::Flex | D::FlexWrap | D::Grid | D::Table)
        );
        let fora = css.float_side.is_some_and(|f| f != crate::style::FloatSide::None) || css.position.is_some_and(|p| p.out_of_flow());
        if atomico || fora || (recorta(&css) && !(cur == dono && de_fragmento)) {
            return false;
        }
        match dom.node(cur).parent {
            Some(p) => cur = p,
            None => return false,
        }
    }
    true
}

/// The same for a generated `inline-block` (`::before`/`::after`): its text's
/// first line baseline, or its bottom margin edge when it has no text.
fn ascent_do_gerado(caixa: &crate::pseudo::PseudoBox, altura: f32, content_w: f32, ctx: &LayoutCtx) -> f32 {
    let css = &caixa.css;
    if recorta(css) || caixa.texto.trim().is_empty() {
        return altura;
    }
    let fonte = font_px(css, DEFAULT_FONT_SIZE);
    let r = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: fonte,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let arestas = super::pseudo_caixa::resolve_arestas(css, &r);
    let familia = css.font_family.as_deref();
    let lh = crate::inline_box::altura_da_linha(css, fonte, ctx.measurer);
    let conteudo = crate::inline_box::altura_do_conteudo(fonte, familia, ctx.measurer);
    (arestas.mt + arestas.valores[0] + crate::inline_box::meia_entrelinha(lh, conteudo) + ctx.measurer.font_ascent_family(fonte, familia)).min(altura)
}
