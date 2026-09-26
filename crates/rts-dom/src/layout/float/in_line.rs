//! A FLOAT that appears in the MIDDLE of an inline flow.
//!
//! CSS 2.1 §9.5.1, rules 6 and 8: a float's top may not be higher than the top
//! of the line box holding the content before it, and it is placed as high as
//! possible. What browsers do with that: if the float fits in what the line it
//! appears in still has free, its top is the top of THAT line and the line is
//! shortened around it; if it does not fit, it goes to the next line.
//!
//! The previous model had no such question. A direct-child float CLOSED the
//! inline flow (`vertical_flow.rs`) and went below the last line, and a float inside
//! a `<span>` split the span into three boxes (`boxes/build.rs`) to end up in
//! the same place — `<div>before<div style=float:left></div>after</div>` came
//! out as two lines where Blink gives one, with the text beside the float.
//!
//! ## How, and the declared cost
//!
//! The anchor (`AtomicKind::Float`) is zero-width and enters line breaking as a
//! `Marker` does. For each anchor, in document order: break the flow with the
//! exclusions that exist, find the line it landed on and how much of that line
//! it must leave free, decide the top, place the float (`placement.rs`,
//! the same path as a direct child) and move on — the next anchor already sees
//! this float among the exclusions. The caller breaks one last time with all of
//! them. **That is `k + 1` line breakings for `k` floats in the flow**, and not
//! one more when there are none, which is almost always.
//!
//! Each line's top is predicted from its INDEX (`y + i × lh`) — the same
//! declared approximation as the line width in `line.rs`, for the same
//! reason: a line holding a taller atom shifts the ones after it, and the float
//! then sits a fraction of a line above its real place.

use super::*;
use crate::boxes::BoxId;

/// Places, in the BFC, every float anchored in `runs`. It neither paints the
/// line nor breaks it for good: it returns once the last float is placed, and
/// the caller breaks with the exclusions that are left.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn place_anchored_floats(
    dom: &Dom,
    tree: &crate::boxes::BoxTree,
    runs: &[InlineRun],
    // Breaks the runs against these exclusions — the caller's `wrap_runs`,
    // with the container's parameters already fixed.
    break_lines: &dyn Fn(&[Exclusao]) -> Vec<Vec<Segment>>,
    (x, y, content_w, lh): (f32, f32, f32, f32),
    // The container's `white-space: nowrap`/`pre`: the whole line is one word.
    nowrap: bool,
    parent_css: &ComputedStyle,
    font_size: f32,
    bfc: &BlockFormattingContext,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) {
    let anchors: Vec<(NodeIdx, BoxId)> = runs
        .iter()
        .filter_map(|r| match r.atomic {
            Some((node, caixa, AtomicKind::Float)) => Some((node, caixa)),
            _ => None,
        })
        .collect();
    for (node, caixa) in anchors {
        let exclusions = bfc.snapshot();
        let lines = break_lines(&exclusions);
        let Some((i, occupied)) = where_it_landed(&lines, node, nowrap) else { continue };
        // A percentage size on the float resolves against an indefinite height
        // here: the inline flow does not know its container's height, and this
        // is the same `None` an atom of the line already gets.
        let size = super::placement::measure_float(dom, tree, node, caixa, content_w, None, parent_css, font_size, ctx);
        let line_top = y + i as f32 * lh;
        let (_, free) = banda_livre(&exclusions, line_top, lh, x, content_w);
        // Does it fit in what the line still has? An anchor at the START of the
        // line always fits by this question — if the float fits in no band on
        // its own, `place_float`'s search moves it down, and that search knows
        // the bottoms of the other floats and not only the next line.
        let fits = occupied <= 0.0 || occupied + size.0 <= free + 0.01;
        let top = if fits { line_top } else { line_top + lh };
        let side = dom
            .computed_style_idx(node)
            .and_then(|c| c.float_side)
            .unwrap_or(crate::style::FloatSide::Left);
        super::placement::place_float(dom, node, caixa, side, size, top, x, content_w, None, bfc, ctx, list);
    }
}

/// The line the anchor of `node` landed on, and the width the float must leave
/// free on it: what is BEFORE the anchor plus what is glued AFTER it up to the
/// next break opportunity.
///
/// The "after" is what Blink and Gecko do and what the WPT
/// `CSS2/floats/float-nowrap-*` pin: the float is handled after the word it
/// appears in, because the line cannot end in the middle of it. A `nowrap` line
/// has no break at all, so the rest of the line counts — and a line that
/// overflows pushes the float below it. Counting only what came before kept the
/// float on the line and lost those two reftests.
///
/// ⚠️ DECLARED APPROXIMATION for normal wrapping: it counts the following
/// segments with NO gap before them and no space inside. A glued segment with a
/// space in the middle is left out whole instead of counting its prefix up to
/// the space — that would need the text measurer, which this question lacks.
fn where_it_landed(lines: &[Vec<Segment>], node: NodeIdx, nowrap: bool) -> Option<(usize, f32)> {
    let width = |seg: &Segment| seg.lead_w + if seg.atomic.is_some() { seg.ww } else { seg.text_width };
    for (i, line) in lines.iter().enumerate() {
        let Some(k) = line
            .iter()
            .position(|seg| matches!(seg.atomic, Some((a, _, AtomicKind::Float)) if a == node))
        else {
            continue;
        };
        let before: f32 = line[..k].iter().map(width).sum();
        let glued: f32 = line[k + 1..]
            .iter()
            .take_while(|seg| nowrap || (seg.lead_w <= 0.0 && !seg.text.contains(char::is_whitespace)))
            .map(width)
            .sum();
        return Some((i, before + glued));
    }
    None
}

/// The anchor of `id` in the inline flow when `id` floats; `None` when it does
/// not.
///
/// Zero width and nothing else: the walk does not descend into the float (its
/// content is its own, laid out by `layout_block` when it is placed), and the
/// inlines around it do not count it as their content — empty `owners`. With no
pub(in crate::layout) fn anchor(dom: &Dom, id: NodeIdx, caixa: BoxId, color: u32) -> Option<InlineRun> {
    // `float` declared and not cancelled by `position: absolute/fixed` (CSS 2.1
    // §9.7 — an absolutely positioned box does not float).
    let floats = dom.computed_style_idx(id).is_some_and(|c| {
        c.float_side.is_some_and(|f| f != crate::style::FloatSide::None)
            && !c.position.is_some_and(|p| p.out_of_flow())
    });
    floats.then(|| InlineRun {
        text: String::new(),
        color,
        bold: false,
        italic: false,
        deco: 0,
        owners: Vec::new(),
        atomic: Some((id, caixa, AtomicKind::Float)),
        ww: 0.0,
        wh: 0.0,
    })
}
