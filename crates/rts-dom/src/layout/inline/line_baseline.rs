//! The vertical placement of a LINE THAT HOLDS AN INLINE-BLOCK (CSS 2.1
//! §10.8.1): every item — the text's strut and each atom — has an extent
//! above and below one shared baseline, and the line box is the largest of
//! each side.
//!
//! The text flow (`line.rs`) did not ask this. It took the line's height as
//! the tallest atom and treated every inline-block as sitting on its bottom
//! edge, with a special case for "taller than the strut and next to text".
//! That gave an empty 20px inline-block a 20px line where Blink gives 25 (the
//! text's descent hangs below the box, whose bottom is on the baseline), a
//! padded one with text 37 where Blink gives 32 (its baseline is its TEXT's,
//! not its bottom), and ignored `vertical-align` entirely
//! (`claude-inline-block-baseline`).
//!
//! The model already existed — [`Envelope`] and
//! [`item_top_with_baseline`] in `vertical_align.rs`, used by the
//! run of sibling inline-blocks (`line_inline_block.rs`) — and its module doc named
//! this migration as the cut still open. This module only feeds it the atoms
//! of a text line. A line with NO inline-block keeps the old path untouched:
//! text and images alone were already right, and moving them is a separate,
//! separately measured change.

use super::vertical_align::{Envelope, envelope_with_baseline, item_top_with_baseline};
use super::*;
use crate::style::VerticalAlign;

/// One atom of the line as the envelope sees it: its outer height, the
/// distance from its top to ITS baseline, and its `vertical-align`.
struct Atom {
    height: f32,
    ascent: f32,
    valign: VerticalAlign,
}

/// The envelope of `line` when it holds an inline-block (a real one or a
/// generated one), `None` otherwise — the caller then keeps its old path.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn line_envelope(
    dom: &Dom,
    line: &[Segment],
    font_size: f32,
    line_height: f32,
    family: Option<&str>,
    content_w: f32,
    ctx: &LayoutCtx,
) -> Option<Envelope> {
    // A text segment whose font is not the container's is an inline box of
    // its own height around the same baseline (CSS 2.1 §10.8): a `<code>` in a
    // serif line, a bigger `<span>` — the line box has to hold it.
    let own_fonts: Vec<(f32, f32, VerticalAlign)> = line
        .iter()
        .filter(|s| s.atomic.is_none() && !s.text.is_empty())
        .filter_map(|s| super::run_font::of_segment(dom, &s.owners, family, font_size, ctx.measurer))
        .map(|f| (f.box_height, f.box_ascent, VerticalAlign::Baseline))
        .collect();
    if own_fonts.is_empty() && !line.iter().any(|s| is_inline_block_atom(s)) {
        return None;
    }
    let items: Vec<(f32, f32, VerticalAlign)> = line
        .iter()
        .filter_map(|s| atom(dom, s, content_w, ctx))
        .map(|a| (a.height, a.ascent, a.valign))
        .chain(own_fonts)
        .collect();
    Some(envelope_with_baseline(&items, font_size, line_height, family, ctx.measurer))
}

/// The top of an atom of a line whose envelope is `env` and whose top is `cy`.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn atom_top(
    dom: &Dom,
    seg: &Segment,
    cy: f32,
    env: &Envelope,
    font_size: f32,
    family: Option<&str>,
    content_w: f32,
    ctx: &LayoutCtx,
) -> f32 {
    match atom(dom, seg, content_w, ctx) {
        Some(a) => item_top_with_baseline(a.valign, a.height, a.ascent, cy, env, font_size, family, ctx.measurer),
        None => cy,
    }
}

/// Sem TEXTO mas com um `<img>` mais alto do que a linha normal: ele senta
/// na BASELINE (linha 344, `topo = text_top + ascent - wh`) mas — ao
/// contrário de texto — não tem DESCIDA nenhuma (CSS 2.1 §10.8): toda a
/// sua altura fica ACIMA da baseline. A meia-entrelinha simétrica reparte
/// o excesso de `line_h` (que É a própria altura da imagem, pelo `fold`
/// acima) igualmente acima/abaixo da content-area do texto — e sobe a
/// caixa da imagem bem acima de `cy`, mesmo ELA sendo o motivo do
/// `line_h` ter crescido. `<p>…</p><img/>` (o idioma-padrão de quadrado
/// de referência do WPT) saía com o quadrado a começar 43px ANTES do fim
/// do parágrafo — deslocando a REFERÊNCIA de qualquer reftest que o use.
/// Só toca `text_top`, não `line_advance`/`text_owner_anchor`.
pub(in crate::layout) fn tall_image_without_text(line: &[Segment], line_h: f32, lh: f32, has_text: bool) -> bool {
    line_h > lh + 0.001 && !has_text && line.iter().any(|s| matches!(s.atomic, Some((_, _, AtomicKind::Replaced))))
}

fn is_inline_block_atom(seg: &Segment) -> bool {
    matches!(
        seg.atomic,
        Some((_, _, AtomicKind::Block | AtomicKind::Gerada(_, crate::inline_box::ParteGerada::Atomo)))
    )
}

/// The atom behind a segment, for the atoms that have a body on the line:
/// inline-blocks (real and generated), replaced elements and form widgets.
fn atom(dom: &Dom, seg: &Segment, content_w: f32, ctx: &LayoutCtx) -> Option<Atom> {
    let (id, box_id, kind) = seg.atomic?;
    let height = seg.wh;
    match kind {
        AtomicKind::Block => {
            let css = dom.computed_style_idx(id)?;
            Some(Atom {
                height,
                ascent: block_ascent(dom, id, box_id, &css, height, seg.ww, content_w, ctx),
                valign: css.vertical_align.unwrap_or(VerticalAlign::Baseline),
            })
        }
        AtomicKind::Gerada(pe, crate::inline_box::ParteGerada::Atomo) => {
            let pseudo = dom.pseudo_box(id, pe)?;
            Some(Atom {
                height,
                ascent: generated_ascent(&pseudo, height, content_w, ctx),
                valign: pseudo.css.vertical_align.unwrap_or(VerticalAlign::Baseline),
            })
        }
        // A replaced element has no baseline of its own: its bottom margin edge
        // sits on the line's (CSS 2.1 §10.8.1), which is where `line.rs`
        // already puts it.
        AtomicKind::Replaced => Some(Atom { height, ascent: height, valign: VerticalAlign::Baseline }),
        AtomicKind::Widget => Some(Atom {
            height,
            ascent: super::line_inline_block::ascent_do_item(dom, id, height, content_w, ctx),
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
fn block_ascent(
    dom: &Dom,
    id: NodeIdx,
    box_id: crate::boxes::BoxId,
    css: &ComputedStyle,
    height: f32,
    width: f32,
    content_w: f32,
    ctx: &LayoutCtx,
) -> f32 {
    if clips_overflow(css) {
        return height;
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
    let font = font_px(css, DEFAULT_FONT_SIZE);
    let r = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let (mt, mb) = (css.margin.top.resolve(&r).unwrap_or(0.0), css.margin.bottom.resolve(&r).unwrap_or(0.0));
    if flex && crate::layout::flex::baseline::tem_itens_elemento(dom, id) {
        // A flex container's baseline is its first line's item's (Flexbox
        // §8.5), read where that item really sits once the container is laid out.
        let start = marca();
        let mut scratch = DisplayList::for_dom(dom);
        layout_block(dom, id, box_id, 0.0, 0.0, content_w, None, Some(width), None, false, true, &BlockFormattingContext::new(), ctx, &mut scratch);
        descarta(start);
        if let Some(b) = crate::layout::flex::baseline::baseline_no_layout(dom, id, &scratch, content_w, ctx) {
            return b.clamp(0.0, height);
        }
    }
    if flex {
        let border_h = (height - mt - mb).max(0.0);
        let inner = super::line_inline_block::ascent_do_item(dom, id, border_h, content_w, ctx);
        // `ascent_do_item` answers the whole border box when the box is empty:
        // then the baseline is the bottom MARGIN edge, which is `height`.
        return if inner >= border_h { height } else { mt + inner };
    }
    match last_line_baseline(dom, id, box_id, width, content_w, ctx) {
        Some(b) => b.min(height),
        None => height,
    }
}

/// One recorded line: the flow's owner, the baseline of its last line box in
/// the coordinates of the list it was laid into, and whether a cached
/// FRAGMENT re-announced it (its subtree's answer) or a flow pushed it live.
#[derive(Clone, Copy)]
struct LineRecord {
    owner: NodeIdx,
    baseline: f32,
    from_fragment: bool,
}

thread_local! {
    /// The last line box of every inline flow laid out, in layout order —
    /// pushed by `layout_inline_flow` and the inline-block run
    /// ([`register_last_line`]) and by every emitted fragment
    /// ([`regista_do_fragmento`]), read by whoever [`collect_baselines`]s.
    ///
    /// A thread-local and not a field of `DisplayList`, whose file is past the
    /// ceiling. Used as a STACK: a reader takes a [`marca`] before laying out,
    /// reads only what was pushed after it and truncates on the way out, so a
    /// nested reader leaves the outer's view intact. A fragment build collapses
    /// its whole subtree to one record, and `layout_document` clears the stack,
    /// so it never holds more than one pass's top-level records.
    static LINES: std::cell::RefCell<Vec<LineRecord>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Records the baseline of the last line of the inline flow owned by `owner`.
pub(in crate::layout) fn register_last_line(owner: NodeIdx, baseline: f32) {
    LINES.with(|v| v.borrow_mut().push(LineRecord { owner, baseline, from_fragment: false }));
}

/// Re-announces a cached fragment's last own line where it is emitted: a
/// fragment served from the cache runs no flow, and without this an atom
/// holding a cached block would find no line in it and sit on its bottom edge —
/// silently, and only on the second layout pass.
pub(in crate::layout) fn regista_do_fragmento(owner: NodeIdx, baseline: f32) {
    LINES.with(|v| v.borrow_mut().push(LineRecord { owner, baseline, from_fragment: true }));
}

/// Where the stack stands now — what a reader passes back to [`collect_baselines`].
pub(in crate::layout) fn marca() -> usize {
    LINES.with(|v| v.borrow().len())
}

/// Drops what was recorded since `marca` unread — a throwaway layout's lines
/// are in its own coordinates and must not reach the reader around it.
pub(in crate::layout) fn descarta(marca: usize) {
    LINES.with(|v| v.borrow_mut().truncate(marca));
}

/// Empties the stack. Called where a document layout starts.
pub(in crate::layout) fn limpa() {
    LINES.with(|v| v.borrow_mut().clear());
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
/// "Own" is [`own_flow`]: a line of `id` itself or of a block inside it,
/// never of an atom nested in it (whose lines are its own, CSS 2.1 §10.8.1).
pub(in crate::layout) fn collect_baselines(dom: &Dom, id: NodeIdx, marca: usize) -> (Option<f32>, Option<f32>) {
    LINES.with(|v| {
        let mut v = v.borrow_mut();
        let max_of = |acc: Option<f32>, b: f32| Some(acc.map_or(b, |m: f32| m.max(b)));
        let (mut direct, mut total) = (None, None);
        for l in v[marca.min(v.len())..].iter().filter(|l| own_flow(dom, id, l.owner, l.from_fragment)) {
            total = max_of(total, l.baseline);
            if !l.from_fragment {
                direct = max_of(direct, l.baseline);
            }
        }
        v.truncate(marca);
        (direct, total)
    })
}

/// `overflow` other than `visible` on either axis.
fn clips_overflow(css: &ComputedStyle) -> bool {
    [css.overflow_x, css.overflow_y].iter().flatten().any(|o| o.clips())
}

/// [`collect_baselines`] for the fragment of the BLOCK `id`, laid out at `y` with outer
/// height `height`: a block that clips answers its bottom margin edge for both
/// values instead of its lines (see [`own_flow`]).
pub(in crate::layout) fn colhe_do_bloco(dom: &Dom, id: NodeIdx, marca: usize, y: f32, height: f32) -> (Option<f32>, Option<f32>) {
    let lines = collect_baselines(dom, id, marca);
    if dom.computed_style_idx(id).is_some_and(|c| clips_overflow(&c)) {
        return (Some(y + height), Some(y + height));
    }
    lines
}

/// The `ultima_linha` of a STITCHED fragment of `id`: its direct lines, which
/// a stitch never touches, against each child fragment's own answer where the
/// child now sits (`dy` is the child's offset from where it was computed).
pub(in crate::layout) fn total_da_costura(
    dom: &Dom,
    id: NodeIdx,
    previous: &Fragment,
    pieces: &[super::Piece],
    tree: &crate::boxes::BoxTree,
) -> Option<f32> {
    // A clipping block stands for its bottom edge, and a stitch keeps its size.
    if dom.computed_style_idx(id).is_some_and(|c| clips_overflow(&c)) {
        return previous.ultima_linha;
    }
    let direct = previous.linha_directa;
    crate::paint::pieces::children(pieces)
        .filter(|c| tree.node_of(c.caixa).is_none_or(|n| own_flow(dom, id, n, true)))
        .filter_map(|c| c.fragment.ultima_linha.map(|b| b + c.dy))
        .chain(direct)
        .fold(None, |acc: Option<f32>, b| Some(acc.map_or(b, |m| m.max(b))))
}

/// The baseline of an atom's LAST line box, from the top of its outer box, or
/// `None` when it has none: the atom is laid out in a throwaway list and its
/// lowest own line is the answer. "The lowest text painted" was tried first
/// and picked a nested `vertical-align: top` inline-block's text
/// (`flexbox-baseline-multi-line-horiz-001`).
fn last_line_baseline(
    dom: &Dom,
    id: NodeIdx,
    box_id: crate::boxes::BoxId,
    width: f32,
    content_w: f32,
    ctx: &LayoutCtx,
) -> Option<f32> {
    let start = marca();
    let mut scratch = DisplayList::for_dom(dom);
    layout_block(
        dom,
        id,
        box_id,
        0.0,
        0.0,
        content_w,
        None,
        Some(width),
        None,
        false,
        true,
        &BlockFormattingContext::new(),
        ctx,
        &mut scratch,
    );
    collect_baselines(dom, id, start).1
}

/// Is the line recorded by `owner` one of the atom `id`'s OWN line boxes — `owner`
/// is `id`, or a block reached from it without crossing another atom, an
/// out-of-flow box or a box that CLIPS?
///
/// A block whose `overflow` is not `visible` hides its lines from the atom
/// around it and stands for its bottom margin edge instead (what Blink does,
/// and what WPT `CSS2/linebox/baseline-block-with-overflow-001` pins): its
/// lines are refused here, and its fragment announces the edge
/// ([`colhe_do_bloco`]) — a record `from_fragment`, whose own `owner` is
/// therefore allowed to clip.
fn own_flow(dom: &Dom, id: NodeIdx, owner: NodeIdx, from_fragment: bool) -> bool {
    let mut cur = owner;
    while cur != id {
        let Some(css) = dom.computed_style_idx(cur) else { return false };
        use crate::style::DisplayKind as D;
        let atomic = matches!(
            css.effective_display(),
            Some(D::InlineBlock | D::InlineFlex | D::InlineFlexWrap | D::InlineGrid | D::InlineTable | D::Flex | D::FlexWrap | D::Grid | D::Table)
        );
        let outside = css.float_side.is_some_and(|f| f != crate::style::FloatSide::None) || css.position.is_some_and(|p| p.out_of_flow());
        if atomic || outside || (clips_overflow(&css) && !(cur == owner && from_fragment)) {
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
fn generated_ascent(pseudo: &crate::pseudo::PseudoBox, height: f32, content_w: f32, ctx: &LayoutCtx) -> f32 {
    let css = &pseudo.css;
    if clips_overflow(css) || pseudo.texto.trim().is_empty() {
        return height;
    }
    let font = font_px(css, DEFAULT_FONT_SIZE);
    let r = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let edges = crate::layout::block::pseudo_box::resolve_arestas(css, &r);
    let family = css.font_family.as_deref();
    let lh = crate::inline_box::altura_da_linha(css, font, ctx.measurer);
    let content = crate::inline_box::altura_do_conteudo(font, family, ctx.measurer);
    (edges.mt + edges.valores[0] + crate::inline_box::meia_entrelinha(lh, content) + ctx.measurer.font_ascent_family(font, family)).min(height)
}
