//! The rectangles of each box, one per FRAGMENT — invariant I4 of
//! `docs/ui/html-engine/box-tree.md` §7 (lot BT-2c).
//!
//! An inline that wraps across two lines is one box with two line fragments.
//! Until BT-2c the list kept ONE rect per box and grew it line by line, so the
//! two fragments existed only inside a union and `getClientRects` could never
//! be answered. Here a box holds `Vec<Rect>`, one per line it appears on, and
//! the union is a VIEW taken at the boundary that asks for four numbers per
//! element: [`BoxRects::union`], which `rect_of_box`, `rect_of` and the
//! hit-test geometry all read.
//!
//! **Why a `Vec<Rect>` per box and not repeated `(BoxId, Rect)` entries with
//! a fragment index on `Piece::Rect`.** The hit-test order is derived from the
//! `Piece::Rect` marks (`pieces.rs`), one per box, and the hit rect must stay the
//! per-box union (`rect_cliente.rs` says why it must not grow further). With a
//! mark per fragment, `pieces::collect` would push the same node once per line
//! and every consumer of `hit_order` would have to deduplicate — a second rule
//! for one answer. Keeping the multiplicity INSIDE the entry leaves the marks,
//! the traversal and the stitch exactly as BT-2b left them. `Fragment::rects`
//! is the flattened form and does hold a box several times (form point 5):
//! it is only ever read by unioning per node or by searching for one box.
//!
//! **What a "line" is, and why it is a token from a counter.** Several segments
//! of one owner share a line (`<span>a <b>b</b> c</span>` gives the span three
//! segments) and must make ONE fragment; the next line must make a new one. The
//! line loop takes a fresh [`LineId`] per line box and every write passes it.
//! A process-wide counter and not the index in the loop: an inline-block laid
//! out in the MIDDLE of a line runs its own lines before the outer line's later
//! segments are recorded, and an index restarted per flow would make the outer
//! span's second segment look like it belonged to the inner flow's line of the
//! same number. A counter never repeats, so no nesting can collide.

use super::Rect;
use crate::boxes::BoxId;

/// One line box, as far as fragment bookkeeping is concerned: two writes with
/// the same `LineId` to the same box grow one fragment.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct LineId(u64);

impl LineId {
    /// A line never seen before. See the module header for why it is global.
    pub(crate) fn fresh() -> LineId {
        thread_local! { static NEXT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) }; }
        NEXT.with(|n| {
            let id = n.get();
            n.set(id + 1);
            LineId(id)
        })
    }
}

/// One line box of one inline formatting context: the line, and the members of
/// the flow it belongs to.
///
/// The members are what make a write by NODE land in the right box. A split
/// inline (CSS 2.1 §9.2.1.1) is one node with a box per fragment, each in a
/// different flow; the segments only name the owner's node, and writing to
/// every box of it — which is what the per-node union did, invisibly — gave the
/// fragment before the block the text after it as well. The owner's box in
/// THIS flow is the one at or under one of the flow's members.
pub(crate) struct LineScope<'a> {
    pub(crate) id: LineId,
    group: &'a [(crate::dom::NodeIdx, BoxId)],
}

impl<'a> LineScope<'a> {
    pub(crate) fn fresh(group: &'a [(crate::dom::NodeIdx, BoxId)]) -> Self {
        LineScope { id: LineId::fresh(), group }
    }

    /// The boxes of `node` this flow lays out: its only box, or — for a node
    /// with several — those whose ancestry reaches a member of the flow.
    pub(crate) fn boxes_of(&self, tree: &crate::boxes::BoxTree, node: crate::dom::NodeIdx) -> Vec<BoxId> {
        let all = tree.boxes_of(node);
        if all.len() <= 1 {
            return all.to_vec();
        }
        let in_flow = |b: BoxId| {
            std::iter::successors(Some(b), |&c| tree.parent(c)).any(|a| self.group.iter().any(|&(_, m)| m == a))
        };
        all.iter().copied().filter(|&b| in_flow(b)).collect()
    }
}

#[derive(Clone, Debug)]
struct Entry {
    /// Never empty. A reserved entry holds the `0,0,0,0` placeholder until the
    /// first real write REPLACES it (`reserved`) — flagged rather than
    /// recognised by value, so a genuine 0×0 fragment at the origin is kept.
    fragments: Vec<Rect>,
    reserved: bool,
    /// The line the LAST fragment came from; `None` for a box laid out once
    /// (a block), which never grows.
    line: Option<LineId>,
}

/// Every box's fragments. Equal when the rectangles are: the line tokens are
/// bookkeeping of the pass that wrote them, and two layouts of one document
/// draw different tokens for the same lines.
#[derive(Clone, Debug, Default)]
pub struct BoxRects {
    map: crate::fasthash::FastMap<BoxId, Entry>,
}

impl PartialEq for BoxRects {
    fn eq(&self, other: &Self) -> bool {
        self.map.len() == other.map.len()
            && self.map.iter().all(|(id, e)| other.map.get(id).is_some_and(|o| o.fragments == e.fragments))
    }
}

impl BoxRects {
    /// Boxes with geometry — read by the `node_rects` counter alone.
    #[cfg(feature = "metrics")]
    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Holds the box's place with a placeholder; true when the box is new.
    pub(crate) fn reserve(&mut self, box_id: BoxId) -> bool {
        if self.map.contains_key(&box_id) {
            return false;
        }
        let placeholder = Rect::new(0.0, 0.0, 0.0, 0.0);
        self.map.insert(box_id, Entry { fragments: vec![placeholder], reserved: true, line: None });
        true
    }

    /// The box's ONE rect, replacing whatever it had — a box laid out once.
    /// True when the box is new.
    pub(crate) fn insert(&mut self, box_id: BoxId, rect: Rect) -> bool {
        self.map
            .insert(box_id, Entry { fragments: vec![rect], reserved: false, line: None })
            .is_none()
    }

    /// One more piece of the box on `line`: it grows the last fragment when that
    /// came from the same line, and starts a new fragment otherwise. True when
    /// the box is new.
    pub(crate) fn add_on_line(&mut self, box_id: BoxId, rect: Rect, line: LineId) -> bool {
        match self.map.get_mut(&box_id) {
            Some(e) if e.reserved => {
                *e = Entry { fragments: vec![rect], reserved: false, line: Some(line) };
                false
            }
            Some(e) if e.line == Some(line) => {
                let last = e.fragments.last_mut().expect("an entry is never empty");
                *last = last.union(rect);
                false
            }
            Some(e) => {
                e.fragments.push(rect);
                e.line = Some(line);
                false
            }
            None => {
                self.map.insert(box_id, Entry { fragments: vec![rect], reserved: false, line: Some(line) });
                true
            }
        }
    }

    /// The box's fragments in line order.
    pub(crate) fn fragments(&self, box_id: BoxId) -> Option<&[Rect]> {
        self.map.get(&box_id).map(|e| e.fragments.as_slice())
    }

    /// The bounding rect of the box's fragments — what the boundary by node and
    /// the hit test read.
    pub(crate) fn union(&self, box_id: BoxId) -> Option<Rect> {
        self.fragments(box_id).map(union_all)
    }

    /// Every box with its union.
    pub(crate) fn unions(&self) -> impl Iterator<Item = (BoxId, Rect)> + '_ {
        self.map.iter().map(|(&id, e)| (id, union_all(&e.fragments)))
    }

    /// Applies `f` to each fragment of the box (a relative shift, a transform):
    /// per fragment, so the union stays a view of what was moved.
    pub(crate) fn map_fragments(&mut self, box_id: BoxId, mut f: impl FnMut(Rect) -> Rect) {
        if let Some(e) = self.map.get_mut(&box_id) {
            for r in e.fragments.iter_mut() {
                *r = f(*r);
            }
        }
    }

    /// Takes `other`'s boxes; a box in both keeps `other`'s, as the map's
    /// `extend` always did.
    pub(crate) fn extend(&mut self, other: BoxRects) {
        self.map.extend(other.map);
    }

    /// The flat form a cached `Fragment` keeps: a box appears once per fragment.
    pub(crate) fn into_pairs(self) -> Vec<(BoxId, Rect)> {
        self.map
            .into_iter()
            .flat_map(|(id, e)| e.fragments.into_iter().map(move |r| (id, r)))
            .collect()
    }
}

fn union_all(rects: &[Rect]) -> Rect {
    rects[1..].iter().fold(rects[0], |acc, r| acc.union(*r))
}

impl super::DisplayList {
    /// The fragments of ONE box — a line fragment each for an inline that
    /// wraps — including a box emitted inside a reused subtree. The per-box
    /// answer `getClientRects` would be built from. Not exposed by node, so
    /// until that API exists its only readers are the tests pinning I4.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn rects_of_box(&self, box_id: BoxId) -> Vec<Rect> {
        match self.box_rects.fragments(box_id) {
            Some(own) => own.to_vec(),
            None => crate::paint::pieces::rects_in_children(&self.pieces, box_id, 0.0, 0.0).unwrap_or_default(),
        }
    }
}
