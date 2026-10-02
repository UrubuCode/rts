//! Where a socket a NATIVE owns sends its `'error'`.
//!
//! `http.request` and `https.request` each open a connection of their own —
//! `new net.Socket()` in `http::client::build_request`, `tls.connect` in
//! `https::client::build_request` — and the object a program holds is the
//! `ClientRequest`, never that socket. So nothing a program can write puts a
//! listener on it. Since an `'error'` with no listener became a throw
//! (`rts_core::entry::unhandled`), a refused or reset connection killed the
//! process from INSIDE `http.request(...)`, uncatchably: the ordinary Node
//! idiom
//!
//! ```js
//! const req = http.request({ host, port });
//! req.on("error", onError);
//! ```
//!
//! never reached its second line, and neither did a `try`/`catch` around the
//! first. Measured against Node v22.23.2 on 2026-10-02:
//! `new (require("ws"))("ws://127.0.0.1:1/x")` hands its `'error'` listener
//! `connect ECONNREFUSED 127.0.0.1:1` there, and here it ended the process
//! with `Error [ERR_UNHANDLED_ERROR]` raised inside `initAsClient` — which is
//! the wall `@whiskeysockets/baileys` hit, first on a refused connection and
//! then on WhatsApp's own reset (`os error 10054`) mid-handshake.
//!
//! # The rule, in one place rather than two
//!
//! A socket a native owns gets a listener the native installs, and that
//! listener RECORDS the error instead of letting it escape. The request then
//! reports it on itself — on a later turn, through `emit_error_later`, which
//! is where a program's own `req.on('error', …)` has already run.
//!
//! This is `tls::socket::on_underlying_error`'s answer for the one socket
//! `tls.connect` wraps, generalised: that function relays the inner socket's
//! `'error'` onto the `TLSSocket` a program holds, for exactly this reason,
//! and `http`/`https` had no equivalent. It lives at the crate root rather
//! than in `http::common` and `https::common` because those two hold
//! one-line property helpers that cannot disagree with each other, where this
//! is a rule — and the two copies of `option_text` are what a rule written
//! twice cost this crate a day ago. Rejected: punching a `pub(crate)` hole in
//! `http::common` for `https` to reach (the crate's modules are private to
//! each other on purpose, and each `common` says so), and giving the socket a
//! listener that re-emits on the request directly (the request does not exist
//! yet when the socket is built, and emitting there would be the synchronous
//! `'error'` this module exists to stop).

use rts_core::entry;

/// Where a recorded error waits. Underscored like every other internal field
/// this crate hangs off an instance (`__body__`, `__tlsId__`), so a program
/// reading its own socket cannot collide with it.
const SLOT: &str = "__ownedError__";

/// Installs the recording `'error'` listener on a socket a native owns.
///
/// Called once, as soon as the socket exists and BEFORE anything can make it
/// fail — which for `http` is before `socket.connect(...)`, since a refused
/// connection is delivered by `net::registry::pump`, and the first thing that
/// pumps is `connect_blocking` one line later.
pub(crate) fn absorb_errors(socket: u64) {
    let absent = entry::undefined_value();
    // A plain host callable, not a closure over the socket: `events::emit`
    // calls a listener with the emitter as `this` (`entry::call(listener, this,
    // …)`), so the socket is already in hand, and a closure here is one more
    // object for the collector to reach for no answer it gives.
    let listener = entry::with_runtime(|context| entry::make_callable(context, record));
    let on_fn = entry::with_runtime(|context| entry::get_member(context, socket, "on"));
    if on_fn == absent {
        return;
    }
    let event = entry::with_runtime(|context| entry::make_string(context, "error"));
    entry::call(on_fn, socket, event, listener, absent, absent);
}

/// Keeps the FIRST error and drops the rest.
///
/// First and not last because a failed connection is followed by a reset and
/// then by a close, and the one that says what happened is the first —
/// `node:net` emits `ECONNREFUSED` and then `'close'`, and reporting the last
/// would answer the consequence.
extern "C" fn record(_e: u64, socket: u64, error: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    entry::with_runtime(|context| {
        if entry::get_member(context, socket, SLOT) == absent {
            entry::put_member(context, socket, SLOT, error);
        }
    });
    absent
}

/// The error [`absorb_errors`]' listener recorded, if the socket failed.
///
/// A property of the socket, so the collector already knows about it — rule 10
/// of `rts-core`'s README is why this is not a table on the side keyed by the
/// socket's handle.
pub(crate) fn recorded_error(socket: u64) -> Option<u64> {
    let absent = entry::undefined_value();
    let held = entry::with_runtime(|context| entry::get_member(context, socket, SLOT));
    match held == absent {
        true => None,
        false => Some(held),
    }
}
