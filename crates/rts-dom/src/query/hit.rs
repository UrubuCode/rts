//! HIT-TEST: which node is under a point of a laid-out paint list.
//!
//! It reads the fragment tree directly: the pieces in REVERSE paint order, a
//! reused subtree entered where its `Piece::Child` stands with its offset, and
//! a `Piece::Rect(box)` tested against that box's OWN line fragments. The first
//! hit is the top-most box. There is no table in between — the per-node
//! `hit_order` this replaced united a box's fragments into one rectangle, so a
//! link wrapped across two lines was also hit in the empty gap after the first
//! line's end (invariant I4, `docs/ui/html-engine/box-tree.md` §7).
//!
//! **A clip bounds what it encloses.** Between a `BeginClip` and its `EndClip`
//! nothing is hit at a point outside the clip rect — the child of an
//! `overflow:hidden` box that sticks out is not clickable where it is not
//! painted. The table ignored clips; this is the one behaviour change of
//! Phase B (`docs/superpowers/plans/2026-09-25-paint-and-query.md`). The clip's
//! scroll offset is not applied, as the geometry never applied it: the backend
//! converts screen to content coordinates before calling.

use crate::boxes::{BoxId, BoxTree};
use crate::dom::{Dom, NodeIdx};
use crate::paint::item::DisplayItem;
use crate::paint::list::{DisplayList, Rect};
use crate::paint::pieces::Piece;

impl DisplayList {
    /// The node under `(x, y)` in CONTENT coordinates (the backend adds the
    /// scroll offset first). Paint order already carries ancestors before
    /// descendants, siblings and `z-index`, so the last box painted over the
    /// point is the one on top.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<NodeIdx> {
        self.hit_where(x, y, &|_| true)
    }

    /// The hit-test with a filter on the candidate node: a node the filter
    /// refuses is transparent and the walk goes on beneath it. One traversal
    /// for both public answers, so they cannot disagree about anything but
    /// the filter.
    pub(crate) fn hit_where(&self, x: f32, y: f32, accept: &dyn Fn(NodeIdx) -> bool) -> Option<NodeIdx> {
        let own = |b: BoxId| self.box_rects.fragments(b).map(<[Rect]>::to_vec).unwrap_or_default();
        let walk = Walk { tree: &self.tree, x, y, accept };
        walk.range(&self.pieces, 0.0, 0.0, &own)
    }
}

impl Dom {
    /// [`DisplayList::hit_test`] with `pointer-events: none` TRANSPARENT to the
    /// click: a node whose computed value is `none` is passed over and the walk
    /// answers what lies beneath. The computed style already carries the
    /// inheritance the spec asks for, so no ancestor is climbed here.
    ///
    /// Lives on `Dom` because only the `Dom` has the cascade; the list has
    /// boxes and nodes and nothing else.
    pub fn hit_test_clickable(&self, list: &DisplayList, x: f32, y: f32) -> Option<NodeIdx> {
        list.hit_where(x, y, &|idx| {
            !matches!(
                self.computed_style_idx(idx).and_then(|s| s.pointer_events),
                Some(crate::style::vocab::PointerEvents::None)
            )
        })
    }
}

struct Walk<'a> {
    tree: &'a BoxTree,
    x: f32,
    y: f32,
    accept: &'a dyn Fn(NodeIdx) -> bool,
}

fn contains(r: Rect, dx: f32, dy: f32, x: f32, y: f32) -> bool {
    let (rx, ry) = (r.x + dx, r.y + dy);
    x >= rx && x < rx + r.w && y >= ry && y < ry + r.h
}

fn clip_rect(piece: &Piece) -> Option<Rect> {
    match piece {
        Piece::Item(DisplayItem::BeginClip { rect, .. }) => Some(*rect),
        _ => None,
    }
}

fn is_end_clip(piece: &Piece) -> bool {
    matches!(piece, Piece::Item(DisplayItem::EndClip))
}

/// The outermost `BeginClip` of `pieces` whose `EndClip` is not in `pieces`:
/// everything after it in this sequence is inside it.
fn unclosed_begin(pieces: &[Piece]) -> Option<usize> {
    let mut open: Vec<usize> = Vec::new();
    for (i, p) in pieces.iter().enumerate() {
        if clip_rect(p).is_some() {
            open.push(i);
        } else if is_end_clip(p) {
            open.pop();
        }
    }
    open.first().copied()
}

/// The `BeginClip` that the `EndClip` at `end` closes, searched backwards.
fn matching_begin(pieces: &[Piece], end: usize) -> Option<usize> {
    let mut depth = 0usize;
    for i in (0..end).rev() {
        if is_end_clip(&pieces[i]) {
            depth += 1;
        } else if clip_rect(&pieces[i]).is_some() {
            if depth == 0 {
                return Some(i);
            }
            depth -= 1;
        }
    }
    None
}

impl Walk<'_> {
    /// The top-most accepted hit in `pieces`, which sit at `(dx, dy)`; `rects`
    /// answers a box's fragments in the coordinates of `pieces`.
    fn range(&self, pieces: &[Piece], dx: f32, dy: f32, rects: &dyn Fn(BoxId) -> Vec<Rect>) -> Option<NodeIdx> {
        // A clip opened here and closed by an ancestor's sequence encloses the
        // whole tail: test that first (it paints last), and only inside it.
        if let Some(k) = unclosed_begin(pieces) {
            let rect = clip_rect(&pieces[k]).expect("unclosed_begin answers a BeginClip");
            if contains(rect, dx, dy, self.x, self.y) {
                if let Some(hit) = self.range(&pieces[k + 1..], dx, dy, rects) {
                    return Some(hit);
                }
            }
            return self.range(&pieces[..k], dx, dy, rects);
        }
        let mut j = pieces.len();
        while j > 0 {
            j -= 1;
            match &pieces[j] {
                Piece::Item(DisplayItem::EndClip) => {
                    // An `EndClip` without its opener here closes a clip an
                    // ancestor already tested; it bounds nothing in this range.
                    let Some(i) = matching_begin(pieces, j) else { continue };
                    let rect = clip_rect(&pieces[i]).expect("matching_begin answers a BeginClip");
                    if contains(rect, dx, dy, self.x, self.y) {
                        if let Some(hit) = self.range(&pieces[i + 1..j], dx, dy, rects) {
                            return Some(hit);
                        }
                    }
                    j = i;
                }
                Piece::Item(_) => {}
                Piece::Rect(box_id) => {
                    // An anonymous box has no node to answer: the bridge
                    // promises nodes, so it is skipped rather than climbed.
                    let Some(node) = self.tree.node_of(*box_id) else { continue };
                    if (self.accept)(node) && rects(*box_id).iter().any(|r| contains(*r, dx, dy, self.x, self.y)) {
                        return Some(node);
                    }
                }
                Piece::Child(c) => {
                    let frag = &c.fragment;
                    // Grouped once per descent: filtering the flat pairs per
                    // `Piece::Rect` would be quadratic in the fragment's boxes.
                    let mut by_box: crate::fasthash::FastMap<BoxId, Vec<Rect>> = Default::default();
                    for (id, r) in frag.rects.iter() {
                        by_box.entry(*id).or_default().push(*r);
                    }
                    let own = |b: BoxId| by_box.get(&b).cloned().unwrap_or_default();
                    if let Some(hit) = self.range(&frag.pieces, dx + c.dx, dy + c.dy, &own) {
                        return Some(hit);
                    }
                }
            }
        }
        None
    }
}

