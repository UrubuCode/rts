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
//! Both halves of that sentence live here: [`absorb_errors`] records, and
//! [`relay_errors`] is the reporting side. The reporting side used to be a
//! private pair in `http::client` while `https::client` had none at all,
//! because `https` blocked on its handshake and read the recorded error out
//! of the socket by hand instead. When that block was removed (the deadlock
//! `http::response_reader`'s doc measures, reached a second time through
//! `tls`), `https` needed the same relay — and a rule written twice is the
//! thing this module's own doc already names as the day's expense. So the
//! relay moved here beside the recorder it pairs with, and `http::client`
//! calls it rather than keeping its copy.
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
/// connection is delivered by `net::registry::pump` and the first `write` is
/// what pumps.
///
/// `https` cannot manage "before": `tls.connect` connects AND writes the
/// ClientHello inside one call, so a refusal can already be in hand by the
/// time this runs on the `TLSSocket` it answers with. That is what
/// [`relay_errors`] reporting an already-recorded error exists for.
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

/// The property the relay listener reads its `ClientRequest` back out of.
///
/// A property of the socket and not a closure over the request: the closure
/// form was tried in `http::client` first and never fired, and a property is
/// a root the collector already walks (rule 10 of `rts-core`'s README).
const REQUEST: &str = "__clientRequest__";

/// Marks that the request has already reported a failure, so it reports one
/// and not three.
const REPORTED: &str = "__errored__";

/// Relays `socket`'s failure onto the `ClientRequest` a program holds, on a
/// later turn — and reports one already recorded.
///
/// Two deliveries, because the socket can fail on either side of this call.
/// Afterwards: [`absorb_errors`]' recorder keeps absorbing (so nothing
/// escapes to `entry::unhandled`) and this listener reports. Before: for
/// `https` the connection attempt and the ClientHello both happen inside
/// `tls.connect`, so `ECONNREFUSED` is already sitting in the socket's slot
/// when the request is built — relying on the listener alone read as a
/// request that never answers anything, which is the silent half of the
/// failure this crate keeps paying for.
pub(crate) fn relay_errors(request: u64, socket: u64) {
    let absent = entry::undefined_value();
    let on_fn = entry::with_runtime(|context| entry::get_member(context, socket, "on"));
    if on_fn == absent {
        return;
    }
    entry::with_runtime(|context| entry::put_member(context, socket, REQUEST, request));
    let listener = entry::with_runtime(|context| entry::make_callable(context, relay));
    let event = entry::with_runtime(|context| entry::make_string(context, "error"));
    entry::call(on_fn, socket, event, listener, absent, absent);
    if let Some(error) = recorded_error(socket) {
        report_once(request, error);
    }
}

/// The socket's `'error'`, reported on the `ClientRequest` a program holds.
extern "C" fn relay(_e: u64, socket: u64, error: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let request = entry::with_runtime(|context| entry::get_member(context, socket, REQUEST));
    if request != absent {
        report_once(request, error);
    }
    absent
}

/// Once. A refused connection is followed by a reset and then a close, and
/// `node:net` emits `'error'` for more than one of them; Node reports the
/// first on the request and nothing after it.
fn report_once(request: u64, error: u64) {
    if entry::with_runtime(|context| entry::get_member(context, request, REPORTED)) == entry::boolean_value(true) {
        return;
    }
    entry::with_runtime(|context| {
        let held = entry::boolean_value(true);
        entry::put_member(context, request, REPORTED, held);
    });
    report_error_later(request, error);
}

/// Emits `'error'` on `request` from a `setTimeout(fn, 0)` turn.
///
/// Later and not now: emitting from inside `http(s).request(...)` itself —
/// before the value it is building has even been returned — makes
/// `req.on('error', cb)` on the caller's next line impossible to run in
/// time, and an `'error'` with no listener ends the process
/// (`http::common::emit`'s own doc), uncatchably even from a `try`/`catch`
/// around the whole call. Node never emits it synchronously either, for the
/// same reason: a connection attempt there is always asynchronous.
///
/// `node:timers`' zero-delay `setTimeout` already IS "a later turn", so this
/// reuses it rather than building a second queue-and-pump beside
/// `net::registry`'s — there is nothing to poll for here, the outcome is
/// already in hand.
pub(crate) fn report_error_later(request: u64, error: u64) {
    let state = entry::with_runtime(|context| {
        let state = entry::make_object(context);
        entry::put_member(context, state, "request", request);
        entry::put_member(context, state, "error", error);
        state
    });
    // Minted OUTSIDE the borrow above — `entry::closure_new` takes the
    // runtime borrow itself.
    let closure = entry::closure_new(deliver as *const () as usize as i64, state);
    let (timers_ns, absent) = entry::with_runtime(|context| (entry::module_at_name(context, "node:timers"), entry::undefined_in(context)));
    let set_timeout = entry::with_runtime(|context| entry::get_member(context, timers_ns, "setTimeout"));
    let delay = entry::make_number(0.0);
    entry::call(set_timeout, absent, closure, delay, absent, absent);
}

/// The `setTimeout` callback [`report_error_later`] schedules.
extern "C" fn deliver(state: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let (request, error) =
        entry::with_runtime(|context| (entry::get_member(context, state, "request"), entry::get_member(context, state, "error")));
    let absent = entry::undefined_value();
    let emit_fn = entry::with_runtime(|context| entry::get_member(context, request, "emit"));
    if emit_fn != absent {
        let event = entry::with_runtime(|context| entry::make_string(context, "error"));
        entry::call(emit_fn, request, event, error, absent, absent);
    }
    absent
}
