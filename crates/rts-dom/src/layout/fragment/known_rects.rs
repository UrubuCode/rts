//! Every node rect a list has placed so far, the subtrees it reused included —
//! the table of containing blocks the out-of-flow pass reads mid-layout
//! (`layout.rs`), and the `rects` half of `query::Geometry`.
//!
//! PQ-C3 (F5 of `docs/superpowers/plans/2026-09-26-paint-query-phase-c.md`):
//! the out-of-flow pass used to build a whole `Geometry` for this, reaching
//! from layout into query in the middle of a layout. The fold over reused
//! subtrees lived only in `query/geometry.rs::add_children`; it lives here now,
//! because walking `Piece` is layout reading paint — the allowed direction —
//! and query calls it for its own `rects`. One fold, two callers.

use crate::boxes::BoxTree;
use crate::dom::NodeIdx;
use crate::fasthash::FastMap;
use crate::paint::list::{DisplayList, Rect};
use crate::paint::pieces::Piece;

/// `box_rects` aggregated by node through `tree.node_of` (several boxes of one
/// node unite, which is what `getBoundingClientRect` asks), then the rects of
/// every reused subtree, offset, in the pre-order `geometry_now` always
/// inserted them in.
pub(crate) fn known_rects(list: &DisplayList) -> FastMap<NodeIdx, Rect> {
    // Agrega `box_rects` (por CAIXA) em `rects` (por NÓ) — o limite de
    // agregação que o §6 do desenho da árvore de caixas pede.
    let mut rects: FastMap<NodeIdx, Rect> = FastMap::default();
    for (box_id, rect) in list.box_rects.unions() {
        if let Some(node) = list.tree.node_of(box_id) {
            match rects.get_mut(&node) {
                Some(existing) => *existing = existing.union(rect),
                None => {
                    rects.insert(node, rect);
                }
            }
        }
    }
    add_children(&list.tree, &list.pieces, 0.0, 0.0, &mut rects);
    rects
}

/// The rects of the subtrees `pieces` reuses, offset, into `out`: each
/// subtree's own rects, then its subtrees'.
fn add_children(tree: &BoxTree, pieces: &[Piece], dx: f32, dy: f32, out: &mut FastMap<NodeIdx, Rect>) {
    for c in crate::paint::pieces::children(pieces) {
        let (dx, dy) = (dx + c.dx, dy + c.dy);
        let moved = dx != 0.0 || dy != 0.0;
        for (box_id, rect) in c.fragment.rects.iter() {
            let mut rect = *rect;
            if moved {
                rect.x += dx;
                rect.y += dy;
            }
            let Some(node) = tree.node_of(*box_id) else { continue };
            out.entry(node).and_modify(|r| *r = r.union(rect)).or_insert(rect);
        }
        add_children(tree, &c.fragment.pieces, dx, dy, out);
    }
}
