//! What the `ws` package EXPORTS, as the npm package exports it.
//!
//! A module of its own and not three more lines in [`super::api`], because that
//! file was at 494 lines and this crate's ceiling is 500 — a focused module is
//! what the ceiling exists to produce, and appending here is what it exists to
//! prevent.
//!
//! The npm `ws` does `module.exports = WebSocket` and hangs the rest off the
//! class, so both questions a program can ask of this package — what does
//! `require("ws")` answer, and what carries the `readyState` numbers — are one
//! question about one object. They are answered here together for that reason.

use rts_core::entry::{self, Context};

/// `readyState`, with the numbers the web API defines and `ws` copies.
///
/// Here rather than in [`super::api`], which holds the three it USES: this is
/// the full set a program compares against, and `CLOSING` is one this
/// implementation never writes — a close completes before it answers — but which
/// the npm package exports, so `WebSocket.CLOSING` must not read `undefined`.
const READY_STATES: [(&str, f64); 4] =
    [("CONNECTING", 0.0), ("OPEN", 1.0), ("CLOSING", 2.0), ("CLOSED", 3.0)];

/// Hangs the four `readyState` numbers on the `WebSocket` class.
///
/// They were on neither the class nor the namespace, so
/// `socket.readyState === WebSocket.OPEN` compared a number against `undefined`
/// and was false for an open socket.
pub(super) fn declare_ready_states(context: &mut Context, class: u64) {
    for (name, number) in READY_STATES {
        let held = entry::make_number(number);
        entry::put_member(context, class, name, held);
    }
}

/// What `require("ws")` answers, for [`super::install`] to declare.
///
/// Read back off the namespace rather than returned beside it, so there is one
/// statement of which member is the package's `module.exports`.
pub(super) fn common(context: &mut Context, namespace: u64) -> u64 {
    entry::get_member(context, namespace, "WebSocket")
}
