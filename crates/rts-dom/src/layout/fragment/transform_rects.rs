//! The geometry half of `transform`: the matrix applied to the rects layout
//! records. The matrix itself is paint's (`paint/transform.rs`).
//!
//! Moved from `layout/transformacao.rs` on 2026-09-25 (PQ-A1); nothing in it changed.

use crate::paint::transform::Mat2d;

/// Applies `mat` to the rect of `id` in `list.box_rects` (if it has one) and,
/// RECURSIVELY, to every descendant's — descendants INHERIT the parent's
/// transform (CSS Transforms 1: the "transform target" includes the
/// subtree). Same pattern as `relative.rs::shift_box_rects` (walk the box
/// tree from `id`, per invariant I2 in `docs/ui/html-engine/box-tree.md` §7,
/// rather than the DOM), except the operation is the whole matrix (bounding
/// box of the 4 corners) instead of a sum.
///
/// `id` is already a `BoxId` — the caller resolves it through
/// `list.tree.boxes_of(node)` before calling in.
///
/// Subtrees served by a cached fragment have no entry in `list.box_rects` and
/// this walk does not find them; they are already handled elsewhere through
/// the fragment's own offset. This is a second source of truth for the same
/// answer, reconciled by hand — the lot that removes it is later than this
/// one.
pub(in crate::layout) fn transform_box_rects(
    tree: &crate::boxes::BoxTree,
    id: crate::boxes::BoxId,
    mat: &Mat2d,
    list: &mut super::DisplayList,
) {
    list.box_rects.map_fragments(id, |r| mat.transform_rect_bbox(r));
    for &child in tree.children(id) {
        transform_box_rects(tree, child, mat, list);
    }
}
