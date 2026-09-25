//! The paint output as ONE ordered sequence of pieces (BT-2b; invariant I5 of
//! `docs/ui/html-engine/box-tree.md` §7).
//!
//! Before this module a list was `items: Vec<DisplayItem>` beside
//! `children: Vec<ChildRef>`, each child saying "paint me before item `at`",
//! plus `hit_order: Vec<BoxId>` with each child saying "my hit order enters
//! before entry `hit_at`". Two indices into vectors that grow for different
//! reasons, fixed by hand wherever something was inserted: `insert_item` (+1 to
//! every later `at`, but only for children created after the insertion point
//! was reserved), `BeginClip { filhos_antes }`/`EndClip { filhos_dentro }` (which
//! children a clip contains), `merge_before` (every index shifted by the
//! prepended list), the relative and transform shifts (`c.at >= desde`). The
//! comments at those sites recorded three defects of that arithmetic: a
//! `position:fixed` painted BEHIND the page, an `overflow:hidden` whose children
//! were drawn before the clip opened, and a whole Wikipedia page clipped to a
//! 1px accessibility box.
//!
//! Now the order IS the vector order. A subtree reused from the fragment cache
//! is one [`Piece::Child`] at the place it paints; what lies between a
//! `BeginClip` and its `EndClip` is inside the clip; painting a background
//! behind children already emitted is an insert at the position remembered
//! before them. Nothing points INTO the vector, so an insert has nothing to fix.
//!
//! [`Piece::Rect`] marks where a box recorded its geometry, and the hit-test
//! order is DERIVED from those marks by the same traversal that paints
//! ([`collect`]). Storing it beside the paint order was the rejected
//! alternative: that second sequence is exactly what `hit_at` existed to keep
//! aligned, and what cost the `z-index` defect its comment named.

use super::*;
use crate::boxes::{BoxId, BoxTree};

/// One step of a list's output, in paint order.
#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    /// A paint instruction of this list's own.
    Item(DisplayItem),
    /// A subtree painted by REFERENCE — a cached fragment, shifted by the
    /// `ChildRef`'s offset — exactly here in the order.
    Child(ChildRef),
    /// A box recorded its geometry here: its place in the hit-test order.
    /// Carries no rectangle — that lives in `box_rects`/`Fragment::rects`,
    /// keyed by box, where the geometry queries read it.
    Rect(BoxId),
}

/// Every item of `pieces`, in paint order, with the offset to add — the
/// subtrees reused by reference included, never copied.
pub(in crate::layout) fn walk(pieces: &[Piece], dx: f32, dy: f32, f: &mut impl FnMut(&DisplayItem, f32, f32)) {
    for piece in pieces {
        match piece {
            Piece::Item(item) => f(item, dx, dy),
            Piece::Child(c) => walk(&c.fragment.pieces, dx + c.dx, dy + c.dy, f),
            Piece::Rect(_) => {}
        }
    }
}

/// The subtrees `pieces` reuses, in paint order.
pub(in crate::layout) fn children(pieces: &[Piece]) -> impl Iterator<Item = &ChildRef> {
    pieces.iter().filter_map(|p| match p {
        Piece::Child(c) => Some(c),
        _ => None,
    })
}

/// How many items `pieces` paints, counting the subtrees it reuses.
pub(in crate::layout) fn count_items(pieces: &[Piece]) -> usize {
    pieces
        .iter()
        .map(|p| match p {
            Piece::Item(_) => 1,
            Piece::Child(c) => count_items(&c.fragment.pieces),
            Piece::Rect(_) => 0,
        })
        .sum()
}

/// Does `pieces` paint anything — an item or a subtree? A geometry mark alone
/// paints nothing, and the two callers that ask ("has this line emitted
/// anything yet", "is there a negative layer to prepend") asked it of the old
/// `items` and `children` and never of `hit_order`.
pub(in crate::layout) fn paints(pieces: &[Piece]) -> bool {
    pieces.iter().any(|p| !matches!(p, Piece::Rect(_)))
}

/// Shifts by `(dx, dy)` what was painted from `mark` on: own items translated
/// in place, reused subtrees through their `ChildRef` offset (their items are
/// shared with the cache and must never be mutated).
///
/// **The start is [`legacy_tie_start`], not `mark`, on purpose.** The index
/// filter this replaces, `c.at >= mark`, also caught the subtrees emitted just
/// BEFORE `mark` with no own item after them: they shared the index `mark`. So
/// a `position:relative` float, inline-block or table cell laid out straight
/// into its parent's list dragged the preceding sibling's cached subtree along
/// (`<div><div>A</div><div style="float:left;position:relative;left:20px">
/// </div></div>` moves "A" by 20px). BT-2b was a zero-change lot, and keeping
/// that answer costs one line here where the fix is replacing the call by
/// `mark`, so it stays until a lot of its own measures the fix.
pub(in crate::layout) fn shift_from(pieces: &mut [Piece], mark: usize, dx: f32, dy: f32) {
    if dx == 0.0 && dy == 0.0 {
        return;
    }
    let start = legacy_tie_start(pieces, mark);
    for piece in &mut pieces[start..] {
        match piece {
            Piece::Item(item) => translate_item(item, dx, dy),
            Piece::Child(c) => {
                c.dx += dx;
                c.dy += dy;
            }
            Piece::Rect(_) => {}
        }
    }
}

/// Just after the last own item before `mark` — where the old `at` index of
/// `mark` began. See [`shift_from`].
fn legacy_tie_start(pieces: &[Piece], mark: usize) -> usize {
    pieces[..mark]
        .iter()
        .rposition(|p| matches!(p, Piece::Item(_)))
        .map_or(0, |i| i + 1)
}

/// Replaces every reused subtree from `start` on by copies of its items,
/// translated by the subtree's offset — for the one caller that has to wrap a
/// range in a `PushTransform` whose matrix a shared item must never see
/// applied twice. The flattened subtrees' geometry is dropped, as the whole-list
/// `materialize` this replaces always did. Only the TAIL: flattening what came
/// before `start` would move pieces an ancestor still has to insert behind.
pub(in crate::layout) fn flatten_from(pieces: &mut Vec<Piece>, start: usize) {
    if !pieces[start..].iter().any(|p| matches!(p, Piece::Child(_))) {
        return;
    }
    for piece in pieces.split_off(start) {
        match piece {
            Piece::Child(c) => walk(&c.fragment.pieces, c.dx, c.dy, &mut |item, dx, dy| {
                let mut item = item.clone();
                if dx != 0.0 || dy != 0.0 {
                    translate_item(&mut item, dx, dy);
                }
                pieces.push(Piece::Item(item));
            }),
            own => pieces.push(own),
        }
    }
}

/// The hit order and the geometry of the subtrees `pieces` reuses, into `out`.
///
/// ONE walk in paint order: a box's hit-test entry is its [`Piece::Rect`], and a
/// subtree's entries come in where its `Child` stands — which is what `hit_at`
/// reconstructed by counting. The `Geometry` answers by NODE, so a box is
/// translated here, and an anonymous box (no node) does not enter: the bridge
/// promises element boxes, and a box the document does not have is not
/// reachable by any `NodeId`.
///
/// **Each entry carries its BOX's rect** (`rect_of`, already offset), not the
/// node's. A split inline's fragment after the block marks its box after the
/// block's; with the node's union — which spans the block — it won a click on
/// the block that Blink gives the block (`rect_cliente.rs`). That only held
/// while every fragment box was written at the first line, marking them all
/// before the block — true of the per-node union, false once each fragment
/// records its own rects (BT-2c).
pub(in crate::layout) fn collect(
    tree: &BoxTree,
    pieces: &[Piece],
    dx: f32,
    dy: f32,
    rect_of: &dyn Fn(BoxId) -> Option<Rect>,
    out: &mut Geometry,
) {
    for piece in pieces {
        match piece {
            Piece::Rect(box_id) => out.hit_order.extend(tree.node_of(*box_id).zip(rect_of(*box_id))),
            Piece::Child(c) => collect_fragment(tree, &c.fragment, dx + c.dx, dy + c.dy, out),
            Piece::Item(_) => {}
        }
    }
}

/// A reused fragment's rects (several boxes of one node unite, which is what
/// `getBoundingClientRect` asks), then its pieces, then its scroll regions —
/// the order `geometry_now` always produced them in.
fn collect_fragment(tree: &BoxTree, fragment: &Fragment, dx: f32, dy: f32, out: &mut Geometry) {
    let moved = dx != 0.0 || dy != 0.0;
    let mut by_box: crate::fasthash::FastMap<BoxId, Rect> = crate::fasthash::FastMap::default();
    for (box_id, rect) in fragment.rects.iter() {
        let mut rect = *rect;
        if moved {
            rect.x += dx;
            rect.y += dy;
        }
        by_box.entry(*box_id).and_modify(|r| *r = r.union(rect)).or_insert(rect);
        let Some(node) = tree.node_of(*box_id) else { continue };
        out.rects.entry(node).and_modify(|r| *r = r.union(rect)).or_insert(rect);
    }
    collect(tree, &fragment.pieces, dx, dy, &|b| by_box.get(&b).copied(), out);
    for region in fragment.scroll_regions.iter() {
        let mut region = *region;
        if moved {
            region.visible.x += dx;
            region.visible.y += dy;
        }
        out.scroll_regions.push(region);
    }
}

/// The rect of one box inside the subtrees `pieces` reuses, with their offsets
/// added: the union of its fragments. Unlike the public geometry by node, this
/// reaches anonymous boxes too.
pub(in crate::layout) fn rect_in_children(pieces: &[Piece], box_id: BoxId, dx: f32, dy: f32) -> Option<Rect> {
    let rects = rects_in_children(pieces, box_id, dx, dy)?;
    Some(rects[1..].iter().fold(rects[0], |acc, r| acc.union(*r)))
}

/// Every fragment of one box inside the subtrees `pieces` reuses, offsets
/// added. A box's line fragments all come from the ONE inline formatting
/// context that laid it out, so they sit in one `Fragment` and the first
/// fragment naming the box has them all.
pub(in crate::layout) fn rects_in_children(pieces: &[Piece], box_id: BoxId, dx: f32, dy: f32) -> Option<Vec<Rect>> {
    children(pieces).find_map(|c| {
        let (dx, dy) = (dx + c.dx, dy + c.dy);
        let own: Vec<Rect> = c
            .fragment
            .rects
            .iter()
            .filter(|(id, _)| *id == box_id)
            .map(|(_, r)| Rect::new(r.x + dx, r.y + dy, r.w, r.h))
            .collect();
        if own.is_empty() {
            rects_in_children(&c.fragment.pieces, box_id, dx, dy)
        } else {
            Some(own)
        }
    })
}
