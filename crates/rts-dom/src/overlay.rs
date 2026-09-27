//! The node the inspector is highlighting — state here, paint in `rts-egui`.
//!
//! One cell per thread, not a field of `Dom`: at most one node of one document
//! is highlighted at a time (it is DevTools' hover), and a field would put an
//! inspector concern into every document of every program. A program that never
//! opens the inspector never writes this; the window reads it once per frame.

use std::cell::Cell;

use crate::dom::NodeId;

thread_local! {
    static HIGHLIGHT: Cell<Option<(u64, NodeId)>> = const { Cell::new(None) };
}

/// Highlights `node` of document `handle`, or clears the highlight with `None`.
pub fn set_highlight(handle: u64, node: Option<NodeId>) {
    HIGHLIGHT.with(|cell| cell.set(node.map(|node| (handle, node))));
}

/// The highlighted node of document `handle`, if any.
pub fn highlight(handle: u64) -> Option<NodeId> {
    HIGHLIGHT.with(|cell| cell.get().filter(|(h, _)| *h == handle).map(|(_, node)| node))
}
