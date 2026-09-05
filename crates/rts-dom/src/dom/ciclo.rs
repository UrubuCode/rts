//! Document-level lifecycle state: the HTML "scripting flag" and
//! `Document.readyState`.
//!
//! Both are plain document state, the same shape as `target_fragment`
//! (`dom/eventos.rs`) or `scroll` (`dom/scroll.rs`): a `Cell`/`RefCell` this
//! crate never interprets, read and written by the facade in
//! `rts-dom-bridge` through the two native pairs it registers
//! (`scriptingEnabled`/`setScriptingEnabled`, `readyState`/`setReadyState`).
//! This crate does not know what a `<script>` is beyond a tag name — running
//! one needs the compiler, which lives on the other side of the boundary
//! this crate never crosses (see the crate doc in `lib.rs`).
//!
//! Neither field feeds `touch()`: unlike a mutation of the tree, flipping the
//! scripting flag or moving `readyState` forward changes nothing a layout
//! pass reads, so there is no render revision to bump.

use super::*;

impl Dom {
    /// Whether a `<script>` connected to this document is allowed to run.
    /// `false` for a document that only ever went through `parseHtml`.
    pub fn scripting_enabled(&self) -> bool {
        self.scripting_enabled.get()
    }

    /// Turns the scripting flag on or off. `loadDocument` (the facade) sets
    /// it once, right after parsing, and never turns it back off.
    pub fn set_scripting_enabled(&self, enabled: bool) {
        self.scripting_enabled.set(enabled);
    }

    /// `Document.readyState`, one of `"loading"`, `"interactive"` or
    /// `"complete"`.
    pub fn ready_state(&self) -> String {
        self.ready_state.borrow().clone()
    }

    /// Moves `readyState` to a new value. The facade is the only caller and
    /// the only place that knows the three-state sequence a navigation runs
    /// through; this method does not validate the transition.
    pub fn set_ready_state(&self, state: &str) {
        *self.ready_state.borrow_mut() = state.to_string();
    }
}
