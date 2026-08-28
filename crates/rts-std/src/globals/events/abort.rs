//! `AbortController` and `AbortSignal`.
//!
//! # `throwIfAborted()` raises the reason, and what changed
//!
//! This section said the method ends the program, and gave a reason that was
//! true when it was written: a native could not raise a JavaScript exception,
//! and `rts-codegen` refused to compile a `try` whose body contained a call, so
//! no program could have caught the throw anyway. Both of those stopped being
//! true. `entry::throw_value` raises a VALUE a `catch` sees, which is the shape
//! this method needs and one a message-only raise could not have given it: the
//! reason is whatever `controller.abort(x)` was handed, and `abort(42)` throws
//! `42`.
//!
//! Ending the process was never the lesser half of the same answer. The whole
//! point of the method is a cancellation the caller catches.
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

/// The composites [`static_any`] built over a signal, which abort with it.
///
/// An ordinary own property holding an array, which is what makes
/// `AbortSignal.any` possible here at all: the alternative the module doc used to
/// refuse it for was a per-registration native closure, and
/// `entry::make_callable` hands back a fixed function pointer with nowhere to put
/// one. Nothing has to be captured if the SIGNAL remembers who depends on it and
/// the one abort path in this module reads the list.
const DEPENDENTS: &str = "__dependents__";

/// The registrations an abort of this signal withdraws.
///
/// The `signal` entry of `AddEventListenerOptions`, kept exactly the way
/// [`DEPENDENTS`] keeps a composite: the signal remembers what depends on it,
/// so nothing has to be captured in a callable that has no environment slot.
/// Each element is a record of `(target, type, fn, capture)` — the four
/// `removeEventListener` is defined over, and no fewer: two listeners of one
/// function on one target differ only in `capture`.
const REMOVALS: &str = "__removals__";

/// The last engine-generated abort event, used by Node's helper when a listener
/// is added after the signal has already aborted.
const NODE_ABORT_EVENT: &str = "__nodeAbortEvent__";

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
    // Named and back-linked like any other class, even though `new` on it
    // throws: `signal.constructor.name` is what a program reads to find out
    // what it is holding, and it answered nothing.
    entry::declare_host_class(context, signal, signal_prototype, "AbortSignal", 0);
    // Each static carries its own `name` and `length`: they are ordinary
    // functions, and a program reading `AbortSignal.any.length` read nothing.
    let statics: [(&str, u32, Provided); 3] = [
        ("abort", 0, static_abort),
        ("timeout", 1, static_timeout),
        ("any", 1, static_any),
    ];
    for (name, arity, code) in statics {
        let made = entry::make_callable(context, code);
        entry::describe_callable(context, made, name, arity);
        entry::put_member(context, signal, name, made);
    }
    // Registered by this module at install time, never by the host: a host that
    // names its sources is a host that forgets one, which is the defect
    // `entry::loops` was written to end.
    entry::declare_loop_source(context, "rts-std:abort-timeout", pump);
    let controller_prototype =
        entry::make_prototype(context, "AbortController", CONTROLLER_METHODS);
    let controller = super::class_ctor(context, "AbortController", 0, construct, controller_prototype);
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

/// `AbortSignal.any(signals)` — one signal that aborts when any input does,
/// with that input's reason.
///
/// # How this is done without a closure, and why the refusal is lifted
///
/// The module doc refused this by name, for a reason that was true about the
/// mechanism it assumed: a composite would have to register a reaction on every
/// input, and `entry::make_callable` answers a fixed function pointer with no
/// environment slot to name the composite from. What that reading missed is that
/// nothing has to be captured. [`signal_abort`] is the ONE path in this module
/// that aborts anything, so the input can simply record who depends on it
/// ([`DEPENDENTS`]) and that one path can read the list.
///
/// The cost of it, named: only aborts that go through this module propagate. A
/// program writing `signal.aborted = true` by hand — which this engine allows,
/// since these are data properties — moves no composite, exactly as
/// `signal.dispatchEvent(new Event('abort'))` already reaches no `onabort`.
///
/// An input that is already aborted wins immediately and in argument order,
/// which is what makes `AbortSignal.any([AbortSignal.abort("a"), other])` answer
/// a signal whose reason is `"a"` before anything is registered at all.
extern "C" fn static_any(_e: u64, _this: u64, signals: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    // The argument is iterated and every member must be a signal, both of which
    // the specification checks before anything is built. Neither was checked:
    // `AbortSignal.any(1)` answered a composite that nothing could ever abort,
    // and `AbortSignal.any([{}])` answered one that had registered a dependency
    // on a plain object.
    if super::options_bag(signals).is_none() {
        entry::throw_type_error("AbortSignal.any: the argument must be iterable");
        return super::absent();
    }
    let inputs = super::elements(signals);
    if !inputs.iter().all(|&input| is_signal(input)) {
        entry::throw_type_error("AbortSignal.any: every member must be an AbortSignal");
        return super::absent();
    }
    let composite = entry::with_runtime(fresh_signal);
    if let Some(aborted) = inputs.iter().find(|&&input| super::flag(input, "aborted")) {
        signal_abort(composite, super::get(*aborted, "reason"), ABORT_ERROR);
        return composite;
    }
    for input in inputs {
        let mut dependents = super::elements(super::get(input, DEPENDENTS));
        dependents.push(composite);
        super::store_elements(input, DEPENDENTS, dependents);
    }
    composite
}

/// `new AbortSignal()` — the specification makes this a `TypeError`, which
/// nothing here can raise where a handler could catch it. `undefined` rather
/// than an instance, because an instance would be a signal with no controller
/// that can never abort, and a program holding one would wait forever on
/// something that looks correct.
extern "C" fn illegal_constructor(_e: u64, _t: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    super::absent()
}

/// `signal.throwIfAborted()` — raises the reason, whatever the reason is.
///
/// This printed a message and called `std::process::exit(1)`, and the module doc
/// called that faithful because nothing in this engine could raise a value a
/// program's `catch` would see. `entry::throw_value` is exactly that, and it
/// takes the VALUE rather than a message — which is what this method needs and
/// what a `TypeError`-only raise could not have served: the reason is whatever
/// `controller.abort(x)` was handed, and `abort(42)` must throw `42`.
///
/// The whole point of the method is a cancellation a caller catches, so ending
/// the process was not a lesser answer to the same question — it was the
/// opposite one.
extern "C" fn throw_if_aborted(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    if !super::flag(this, "aborted") {
        return super::absent();
    }
    entry::throw_value(super::get(this, "reason"));
    super::absent()
}


/// Aborts a signal once: records the reason, fires `'abort'`, calls `onabort`.
///
/// Idempotent, as specified — a second `controller.abort()` is a no-op, and so
/// is a timeout that expires after the controller already aborted.
///
/// # Why the reason is recorded on every dependent BEFORE any listener runs
///
/// Because that is the order the specification states, and it is observable: a
/// listener on the source signal that reads `composite.reason` must find the
/// reason there rather than `undefined`. So this is two passes over the affected
/// signals rather than a recursive call per level — mark them all, then fire
/// them all, in the order [`affected`] found them.
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
    let aborting = affected(signal);
    let events: Vec<(u64, u64)> = entry::with_runtime(|context| {
        aborting
            .iter()
            .map(|&target| {
                entry::put_member(context, target, "aborted", entry::boolean_value(true));
                entry::put_member(context, target, "reason", held);
                let prototype = super::event::prototype(context);
                let event = entry::make_instance(context, prototype);
                super::event::init(context, event, "abort", &super::event::Flags::INERT);
                // The one event this engine generates itself, which is the only
                // case the reference marks as trusted.
                entry::put_member(context, event, "isTrusted", entry::boolean_value(true));
                entry::put_member(context, target, NODE_ABORT_EVENT, event);
                (target, event)
            })
            .collect()
    });
    for (target, _) in &events {
        withdraw(*target);
    }
    for (target, event) in events {
        super::target::dispatch_event(target, event);
        let handler = super::get(target, "onabort");
        if entry::with_runtime(|context| entry::is_callable_in(context, handler)) {
            let absent = super::absent();
            entry::call(handler, target, event, absent, absent, absent);
        }
    }
}

/// Whether a value is one of this module's signals.
///
/// By the prototype chain rather than by a property, so that a plain object
/// carrying `aborted: false` is refused: `addEventListener`'s `signal` entry is
/// a `TypeError` for anything that is not a signal, and a duck test would accept
/// the very mistake the error exists to report.
pub(super) fn is_signal(value: u64) -> bool {
    let wanted = entry::with_runtime(signal_prototype);
    let mut held = entry::get_prototype(value);
    while held != entry::null_value() && held != entry::undefined_value() {
        if held == wanted {
            return true;
        }
        held = entry::get_prototype(held);
    }
    false
}

/// Remembers a registration to withdraw when this signal aborts.
pub(super) fn remove_on_abort(
    signal: u64,
    target: u64,
    kind: &str,
    listener: u64,
    capture: bool,
) {
    let text = super::string(kind);
    let record = entry::with_runtime(|context| {
        let record = entry::make_object(context);
        entry::put_member(context, record, "target", target);
        entry::put_member(context, record, "type", text);
        entry::put_member(context, record, "fn", listener);
        entry::put_member(context, record, "capture", entry::boolean_value(capture));
        record
    });
    let mut held = super::elements(super::get(signal, REMOVALS));
    held.push(record);
    super::store_elements(signal, REMOVALS, held);
}

/// Withdraws every registration tied to a signal that has just aborted.
///
/// Before the `'abort'` event is delivered, which is observable: a listener
/// registered with the same signal for the same type must not run for the abort
/// that removed it.
fn withdraw(signal: u64) {
    let held = super::elements(super::get(signal, REMOVALS));
    if held.is_empty() {
        return;
    }
    super::store_elements(signal, REMOVALS, Vec::new());
    for record in held {
        let Some(kind) = super::text(super::get(record, "type")) else {
            continue;
        };
        super::target::drop_registration(
            super::get(record, "target"),
            &kind,
            super::get(record, "fn"),
            super::flag(record, "capture"),
        );
    }
}

/// The signal and everything [`static_any`] made depend on it, in the order the
/// specification aborts them: the source first, then its dependents, then
/// theirs.
///
/// Already-aborted signals are skipped, which is also what stops a cycle — a
/// program can build one by feeding a composite back into a later
/// `AbortSignal.any`, and this must answer rather than recurse forever.
fn affected(signal: u64) -> Vec<u64> {
    let mut found = vec![signal];
    let mut at = 0;
    while at < found.len() {
        let current = found[at];
        at += 1;
        for dependent in super::elements(super::get(current, DEPENDENTS)) {
            let seen = found.iter().any(|&held| held == dependent);
            if !seen && !super::flag(dependent, "aborted") {
                found.push(dependent);
            }
        }
    }
    found
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
