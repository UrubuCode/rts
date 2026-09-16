//! The document's box tree, memoised.
//!
//! The tree itself lives in `crate::boxes`; this file is only the memo, and it
//! is here because the memo fields are private to `crate::dom`.
//!
//! **Why on the `Dom` and not in `LayoutCtx`.** 111 sites construct a
//! `LayoutCtx`, and a new field there would be 111 mechanical edits in code
//! with nothing to do with boxes — most of them tests. On the `Dom` it follows
//! the shape `document_counters` and the style memo already use, and inherits
//! the same invalidation for free.

use super::Dom;
use crate::boxes::{build_mirror, BoxTree};
use std::rc::Rc;

impl Dom {
    /// The box tree for this document, keyed by `(revision, style_epoch)`.
    ///
    /// **The `Rc` is what makes it safe to hold across a layout pass.** A caller
    /// keeps its clone alive even if the memo is replaced underneath, so it
    /// never observes half of one tree and half of another — which is the
    /// failure mode a `&BoxTree` borrowed from a `RefCell` would invite.
    ///
    /// The key is the same pair the other document-wide memos use. It does NOT
    /// include `anim_epoch`: an animation frame changes computed values, not
    /// which boxes exist, and rebuilding the tree every frame of every
    /// transition would undo the reason the style memo splits those two apart.
    /// When a box's existence starts depending on an animated property, this
    /// comment is where to come back to.
    pub fn box_tree(&self) -> Rc<BoxTree> {
        let key = (self.revision, crate::style::props::style_epoch());
        if self.box_tree_memo_revision.get() == key {
            if let Some(t) = self.box_tree_memo.borrow().as_ref() {
                return Rc::clone(t);
            }
        }
        let tree = Rc::new(build_mirror(self));
        *self.box_tree_memo.borrow_mut() = Some(Rc::clone(&tree));
        self.box_tree_memo_revision.set(key);
        tree
    }
}
