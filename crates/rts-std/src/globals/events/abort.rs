//! `AbortController` and `AbortSignal`.
//!
//! # Why `throwIfAborted()` ends the program
//!
//! Because in this engine there is no third answer, and the two that exist are
//! not equally honest. A native cannot raise a JavaScript exception —
//! `rts_core::entry::throw` ends the process rather than transferring to a
//! handler, and `rts-codegen` refuses at compile time to emit a `try` whose body
//! contains a call. So **no program this engine compiles could have caught the
//! throw anyway**: every use of `throwIfAborted()` on an aborted signal is, by
//! construction, an uncaught exception, and ending the program with the reason
//! reported is exactly what an uncaught exception does.
//!
//! Returning `undefined` instead was the alternative, and it is what
//! `node:assert` chose for its own unraisable failures. It loses here for a
//! reason that does not apply there: `assert.ok(false)` is a diagnostic whose
//! whole value is the message, while `throwIfAborted()` exists to *stop the
//! caller* — a program that continues past it operates on a cancelled request,
//! which is a wrong answer that runs.
//!
//! The exit is written here rather than through `entry::throw` because that
//! entry point takes a tag, and the tag numbering belongs to `rts-codegen`'s
//! `JS_THROW`, which this crate cannot name. Writing the number by hand is the
//! second-source-of-one-number mistake the reuse rule exists to prevent, and it
//! would buy nothing: on the escaping path the tag is printed and not
//! interpreted. `node:events` ends the process the same way for an unhandled
//! `'error'`.
//!
//! # `onabort`, and where it fires
//!
//! The specification registers `onabort` as a listener at the moment it is
//! assigned, so its position among the `addEventListener` listeners depends on
//! assignment order. Reproducing that needs a property setter, which is
//! `#[rtse::class]`'s and not reachable from a host crate — so `onabort` is a
//! plain property read by [`signal_abort`] **after** the registered listeners.
//! Two divergences follow and are stated rather than discovered: the ordering,
//! and that `signal.dispatchEvent(new Event('abort'))` written by a program does
//! not reach it, because only this module's own abort path looks.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use rts_core::entry::{self, Context, Pending, Provided};

/// What `controller.abort()` records when the caller named no reason.
const ABORT_ERROR: (&str, &str) = ("AbortError", "This operation was aborted");

/// What an expired [`static_timeout`] records.
const TIMEOUT_ERROR: (&str, &str) = ("TimeoutError", "The operation timed out.");

thread_local! {
    /// Signals waiting on a deadline, and when each is due.
    ///
    /// Per thread and not one table, for `node:timers`' measured reason: a
    /// signal is a cell in the region of the thread that made it, so a shared
    /// table lets one thread abort another thread's signal with the wrong
    /// context installed — which is a handle naming a cell in a region this
    /// thread does not have, not merely an ordering surprise.
    static DEADLINES: RefCell<Vec<(Instant, u64)>> = const { RefCell::new(Vec::new()) };
}

const CONTROLLER_METHODS: &[(&str, Provided)] = &[("abort", abort)];

const SIGNAL_METHODS: &[(&str, Provided)] = &[("throwIfAborted", throw_if_aborted)];

/// Builds both classes and answers `(AbortController, AbortSignal)`.
pub(super) fn install(context: &mut Context) -> (u64, u64) {
    let signal_prototype = signal_prototype(context);
    // `AbortSignal` has no usable constructor and still has to be a callable:
    // `instanceof` reads the `prototype` property off the value, and a plain
    // object cannot carry one that `new` respects.
    let signal = entry::make_callable(context, illegal_constructor);
    entry::put_member(context, signal, "prototype", signal_prototype);
    let statics: [(&str, Provided); 2] = [("abort", static_abort), ("timeout", static_timeout)];
    for (name, code) in statics {
        let made = entry::make_callable(context, code);
        entry::put_member(context, signal, name, made);
    }
    // Registered by this module at install time, never by the host: a host that
    // names its sources is a host that forgets one, which is the defect
    // `entry::loops` was written to end.
    entry::declare_loop_source(context, "rts-std:abort-timeout", pump);
    let controller_prototype =
        entry::make_prototype(context, "AbortController", CONTROLLER_METHODS);
    let controller = super::class_ctor(context, construct, controller_prototype);
    (controller, signal)
}

/// `AbortSignal.prototype`, linked to `EventTarget.prototype`.
///
/// The link is what makes `addEventListener` work on a signal: it is found by
/// the ordinary chain walk, so there is one `EventTarget` implementation rather
/// than a copy of three methods on a second prototype.
fn signal_prototype(context: &mut Context) -> u64 {
    let base = super::target::prototype(context);
    let made = entry::make_prototype(context, "AbortSignal", SIGNAL_METHODS);
    entry::set_prototype_in(context, made, base);
    made
}

/// A signal that has not been aborted.
fn fresh_signal(context: &mut Context) -> u64 {
    let prototype = signal_prototype(context);
    let signal = entry::make_instance(context, prototype);
    super::target::prepare(context, signal);
    entry::put_member(context, signal, "aborted", entry::boolean_value(false));
    let nothing = entry::undefined_in(context);
    entry::put_member(context, signal, "reason", nothing);
    // `null` rather than `undefined`, which is what the specification says a
    // never-assigned event handler reads as, and what `signal.onabort === null`
    // tests for.
    let null = entry::null_in(context);
    entry::put_member(context, signal, "onabort", null);
    signal
}

/// `new AbortController()` — its signal is created here and never replaced, so
/// `controller.signal === controller.signal` holds.
extern "C" fn construct(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "AbortController", CONTROLLER_METHODS);
        let controller = super::self_or_new(context, this, prototype);
        let signal = fresh_signal(context);
        entry::put_member(context, controller, "signal", signal);
        controller
    })
}

/// `controller.abort(reason?)`.
extern "C" fn abort(_e: u64, this: u64, reason: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    signal_abort(super::get(this, "signal"), reason, ABORT_ERROR);
    super::absent()
}

/// `AbortSignal.abort(reason?)` — a signal that is already aborted, with no
/// controller and therefore no way for it ever to fire a listener.
extern "C" fn static_abort(_e: u64, _this: u64, reason: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let signal = entry::with_runtime(fresh_signal);
    signal_abort(signal, reason, ABORT_ERROR);
    signal
}

/// `AbortSignal.timeout(delay)`.
///
/// A delay that is not a number reads as `0` rather than being coerced: reading
/// it would mean `ToNumber` on an object, whose `valueOf` is user code no entry
/// point can run. `AbortSignal.timeout("10")` therefore aborts on the next pass
/// instead of after ten milliseconds — stated rather than silently rounded to
/// something plausible.
extern "C" fn static_timeout(_e: u64, _this: u64, delay: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let millis = entry::number_of(delay)
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(0.0);
    let signal = entry::with_runtime(fresh_signal);
    let due = Instant::now() + Duration::from_secs_f64(millis / 1000.0);
    DEADLINES.with(|table| table.borrow_mut().push((due, signal)));
    signal
}

/// `new AbortSignal()` — the specification makes this a `TypeError`, which
/// nothing here can raise where a handler could catch it. `undefined` rather
/// than an instance, because an instance would be a signal with no controller
/// that can never abort, and a program holding one would wait forever on
/// something that looks correct.
extern "C" fn illegal_constructor(_e: u64, _t: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    super::absent()
}

/// `signal.throwIfAborted()` — see the module doc for why this ends the program
/// rather than returning.
extern "C" fn throw_if_aborted(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    if !super::flag(this, "aborted") {
        return super::absent();
    }
    let reason = super::get(this, "reason");
    // `described` answers the text of a primitive and nothing for an object,
    // whose `toString` is user code. A reason built by this module is an object,
    // so its two data properties are read directly — the same read
    // `entry::throw` performs on an `Error` for the same reason.
    let text = entry::described(reason).unwrap_or_else(|| {
        let name = super::text(super::get(reason, "name")).unwrap_or_else(|| "Error".to_owned());
        let message = super::text(super::get(reason, "message")).unwrap_or_default();
        format!("{name}: {message}")
    });
    eprintln!("rts: uncaught exception from AbortSignal.throwIfAborted(): {text}");
    std::process::exit(1)
}

/// Aborts a signal once: records the reason, fires `'abort'`, calls `onabort`.
///
/// Idempotent, as specified — a second `controller.abort()` is a no-op, and so
/// is a timeout that expires after the controller already aborted.
fn signal_abort(signal: u64, reason: u64, default: (&str, &str)) {
    if super::flag(signal, "aborted") {
        return;
    }
    // Built BEFORE the borrow below, and that is not tidiness: a `DOMException`
    // runs the program's own `Error` constructor, which takes its own borrow of
    // the context — a second one inside an `extern "C"` frame is a panic that
    // cannot unwind, so it aborts the process.
    let held = match reason == entry::undefined_value() {
        true => crate::globals::dom_exception::make(default.1, default.0),
        false => reason,
    };
    let event = entry::with_runtime(|context| {
        entry::put_member(context, signal, "aborted", entry::boolean_value(true));
        entry::put_member(context, signal, "reason", held);
        let prototype = super::event::prototype(context);
        let event = entry::make_instance(context, prototype);
        super::event::init(context, event, "abort", &super::event::Flags::INERT);
        // The one event this engine generates itself, which is the only case the
        // reference marks as trusted.
        entry::put_member(context, event, "isTrusted", entry::boolean_value(true));
        event
    });
    super::target::dispatch_event(signal, event);
    let handler = super::get(signal, "onabort");
    if entry::with_runtime(|context| entry::is_callable_in(context, handler)) {
        let absent = super::absent();
        entry::call(handler, signal, event, absent, absent, absent);
    }
}

/// Aborts whatever is due, and says whether anything is still waiting.
///
/// [`Pending::Blocked`] rather than [`Pending::In`] while something waits: a
/// pending timeout does **not** hold the program open. Node unrefs the timer
/// behind `AbortSignal.timeout`, so a signal does not keep its loop alive there
/// either, and `entry::loops` made the same call for this engine — an interval
/// does not hold a program open. The consequence, named: a program whose only
/// outstanding work is the timeout exits before it fires.
///
/// The table's borrow is released before any signal is aborted, because
/// aborting calls listeners, and a listener calling `AbortSignal.timeout()`
/// again would re-enter this table.
fn pump() -> Pending {
    let now = Instant::now();
    let due: Vec<u64> = DEADLINES.with(|table| {
        let mut table = table.borrow_mut();
        let (due, waiting): (Vec<_>, Vec<_>) = table.drain(..).partition(|(at, _)| *at <= now);
        *table = waiting;
        due.into_iter().map(|(_, signal)| signal).collect()
    });
    for signal in due {
        signal_abort(signal, entry::undefined_value(), TIMEOUT_ERROR);
    }
    match DEADLINES.with(|table| table.borrow().is_empty()) {
        true => Pending::Idle,
        false => Pending::Blocked,
    }
}
