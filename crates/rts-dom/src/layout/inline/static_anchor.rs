//! The STATIC POSITION of an absolutely positioned box that appears in the
//! middle of a LINE (CSS 2.1 §10.3.7 / §10.6.4: where the box would be had it
//! stayed in flow).
//!
//! Measured in Blink (`claude-absoluto-posicao-estatica-na-linha`): a box that
//! was BLOCK-level before `position` blockified it goes below the line it
//! appears in, at the flow's start edge; one that was INLINE-level stays where
//! it appears on the line, at the line's top. `static_position.rs` could
//! answer neither: it looks at the box's DOM siblings, and inside a line the
//! "sibling" is text, which has no rectangle — every such box landed at the
//! top of the enclosing element.
//!
//! So the inline flow says where the box WOULD be. The walk leaves a
//! zero-width ANCHOR for it (`AtomicKind::StaticAnchor`, as a float leaves one),
//! and when the line places the anchor, the position is recorded in
//! `DisplayList::static_anchors`.
//!
//! **Not in `box_rects`, and the alternative was measured against the code,
//! not guessed:** `geometry_now` UNIONS every rect recorded for a node, so a
//! zero-size rect at the static position would stretch the element's real
//! rectangle whenever an inset moved it away from there. The anchors are a
//! table of their own, and — because a block served from the FRAGMENT CACHE
//! runs no flow — a fragment carries the anchors recorded while it was built
//! and [`all`] finds them through the tree of reused fragments, the way
//! `rect_of_box` finds a rectangle. Without that the second layout pass would
//! silently fall back to the old position: the class `lost-roots.md` names.

use super::*;
use crate::boxes::BoxId;

/// The anchor of `id` in the inline flow when `id` is absolutely positioned;
/// `None` when it is not. Zero width, no owners: the box takes no room on the
/// line and the inlines around it do not count it as content. Returning early
/// here is also what keeps the walk from descending into the box and leaking
/// its text into the line.
pub(in crate::layout) fn anchor(dom: &Dom, id: NodeIdx, box_id: BoxId, color: u32) -> Option<InlineRun> {
    is_out_of_flow(dom, id).then(|| InlineRun {
        text: String::new(),
        color,
        bold: false,
        italic: false,
        deco: 0,
        owners: Vec::new(),
        atomic: Some((id, box_id, AtomicKind::StaticAnchor)),
        ww: 0.0,
        wh: 0.0,
    })
}

/// Handles the atoms that have NOTHING on the line — a float's anchor and a
/// static-position anchor — and answers `true` for them, so the caller skips
/// the segment. For the latter it records where the box would have been:
/// `(seg_x, line_top)` if it was inline-level, `(flow_x, line_bottom)` if it
/// was block-level — and `(flow_x, line_top)` if it was block-level with
/// NOTHING before it on the line. In flow it would have split the line, and
/// the part before it would be a line box with no text and no atom: zero
/// height (CSS 2.1 §9.4.2), so the box sits where that line starts. Measured in
/// Blink (`claude-absoluto-posicao-estatica-linha-vazia`), which does not count
/// a START BORDER of the enclosing inline as content either, whatever §9.4.2
/// says of it.
///
/// `line_start` is `list.pieces.len()` as it stood when the line began:
/// "nothing before it" is asked as "the line has PAINTED nothing yet" — a
/// geometry mark alone (a marker's rect) does not count, as it did not when
/// this compared the item and subtree counts — since the surfaces of the
/// inlines are inserted only after the line's last segment. Cut, stated: text that paints nothing
/// (`visibility: hidden`) reads as an empty line here.
pub(in crate::layout) fn outside_line(
    dom: &Dom,
    atomic: (NodeIdx, BoxId, AtomicKind),
    seg_x: f32,
    flow_x: f32,
    line_top: f32,
    line_bottom: f32,
    line_start: usize,
    list: &mut DisplayList,
) -> bool {
    match atomic {
        // The float was laid out by `float_in_line`, and neither its box nor
        // the boxes of the inlines around it pass through the line — Blink
        // leaves it out of the client rects of the inline that contains it.
        (_, _, AtomicKind::Float) => true,
        (id, box_id, AtomicKind::StaticAnchor) => {
            let empty = !crate::paint::pieces::paints(&list.pieces[line_start..]);
            let (x, y) = match (was_block(dom, id), empty) {
                (true, true) => (flow_x, line_top),
                (true, false) => (flow_x, line_bottom),
                (false, _) => (seg_x, line_top),
            };
            list.static_anchors.push((box_id, x, y));
            true
        }
        _ => false,
    }
}

/// Is `line` made of anchors alone — floats' and static positions'? Then it is
/// no line box and the caller skips it; but the static positions still have to
/// be said, and with no line they are all the same place: where the line would
/// have started (Blink, `claude-absoluto-posicao-estatica-linha-vazia` case 5).
pub(in crate::layout) fn anchors_only_line(dom: &Dom, line: &[Segment], flow_x: f32, cy: f32, list: &mut DisplayList) -> bool {
    let anchors_only = line.iter().all(|s| matches!(s.atomic, Some((_, _, AtomicKind::Float | AtomicKind::StaticAnchor))));
    if anchors_only {
        let start = list.pieces.len();
        for atomic in line.iter().filter_map(|s| s.atomic) {
            outside_line(dom, atomic, flow_x, flow_x, cy, cy, start, list);
        }
    }
    anchors_only
}

/// Was this box block-level BEFORE `position: absolute` blockified it? The
/// declared `display` decides, and the tag's default when none is declared —
/// `effective_display` cannot be asked, since it answers after blockification.
fn was_block(dom: &Dom, id: NodeIdx) -> bool {
    match dom.computed_style_idx(id).and_then(|c| c.display) {
        Some(d) => !d.is_inline_level(),
        None => matches!(&dom.node(id).kind, NodeKind::Element { tag } if crate::block::lookup(tag).is_some()),
    }
}

/// Every static-position anchor of `list`, by NODE and in the list's own
/// coordinates, including those inside the fragments it reuses.
pub(in crate::layout) fn all(list: &DisplayList) -> Vec<(NodeIdx, Rect)> {
    let mut out = Vec::new();
    let mut add = |tree: &crate::boxes::BoxTree, a: &[(BoxId, f32, f32)], dx: f32, dy: f32| {
        out.extend(a.iter().filter_map(|&(b, x, y)| Some((tree.node_of(b)?, Rect::new(x + dx, y + dy, 0.0, 0.0)))));
    };
    add(&list.tree, &list.static_anchors, 0.0, 0.0);
    let mut stack: Vec<(&ChildRef, f32, f32)> = crate::paint::pieces::children(&list.pieces).map(|c| (c, c.dx, c.dy)).collect();
    while let Some((c, dx, dy)) = stack.pop() {
        add(&c.fragment.tree, &c.fragment.static_anchors, dx, dy);
        stack.extend(crate::paint::pieces::children(&c.fragment.pieces).map(|n| (n, dx + n.dx, dy + n.dy)));
    }
    out
}
