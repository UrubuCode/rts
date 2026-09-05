//! The two flags a navigated page's lifecycle needs: whether a `<script>`
//! that connects to the document is allowed to run, and `Document.readyState`.
//!
//! Module apart from `events.rs` for the same reason `location.rs` is: that
//! file is already at the 500-line ceiling, and this is document state, not
//! an event.

use rts_core::entry::Provided;

use crate::value::{handle, int, integer, nothing, string, text};

pub const MEMBERS: &[(&str, Provided)] = &[
    ("scriptingEnabled", scripting_enabled),
    ("setScriptingEnabled", set_scripting_enabled),
    ("readyState", ready_state),
    ("setReadyState", set_ready_state),
];

/// `scriptingEnabled(doc)` — `1` if a `<script>` connected to this document
/// runs. `0` (never `undefined`) for a handle the store does not know, the
/// same "gone document answers the falsy default" convention `location.rs`
/// and `scroll.rs` use.
extern "C" fn scripting_enabled(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let h = handle(doc);
    let enabled = rts_dom::store::with_dom(h, |d| d.scripting_enabled()).unwrap_or(false);
    int(i64::from(enabled))
}

/// `setScriptingEnabled(doc, 1|0)` — `loadDocument` (the facade) calls this
/// once, right after parsing, to turn scripting on for a navigated page.
extern "C" fn set_scripting_enabled(_e: u64, _t: u64, doc: u64, value: u64, _b: u64, _c: u64) -> u64 {
    let h = handle(doc);
    let enabled = integer(value, 0) != 0;
    rts_dom::store::with_dom(h, |d| d.set_scripting_enabled(enabled));
    nothing()
}

/// `readyState(doc)` — `"loading"`/`"interactive"`/`"complete"`. `"complete"`
/// for a handle the store does not know, matching `Dom::new`'s default for a
/// document that never navigated.
extern "C" fn ready_state(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let h = handle(doc);
    let state = rts_dom::store::with_dom(h, |d| d.ready_state()).unwrap_or_else(|| "complete".to_string());
    string(&state)
}

/// `setReadyState(doc, state)` — `loadDocument` moves the document through
/// the three states as it runs the page's scripts and fires its events.
extern "C" fn set_ready_state(_e: u64, _t: u64, doc: u64, value: u64, _b: u64, _c: u64) -> u64 {
    let h = handle(doc);
    let state = text(value);
    rts_dom::store::with_dom(h, |d| d.set_ready_state(&state));
    nothing()
}
