//! `EventTarget` — registration, removal, and one synchronous dispatch.
//!
//! # The borrow this module has to get right
//!
//! Every listener is user code, and user code's first act may be to call back
//! into the runtime. `with_runtime` holds a `RefCell` borrow for its whole body,
//! so calling from inside one is a panic in an `extern "C"` frame — which cannot
//! unwind, and therefore aborts the process rather than failing a test.
//!
//! So [`dispatch_event`] is written as five separate steps: read the records,
//! store the survivors back, stamp the event, resolve every callee, and only
//! then call. Each borrow opens and closes before the next begins, and the
//! calling loop holds none at all.
//!
//! # Why `once` listeners are dropped before any listener runs
//!
//! A listener that re-enters `dispatchEvent` for the same type must not see a
//! `once` registration that is already firing. `node:events`' `emit` made the
//! same choice for the same reason, and it is what "invoked once, not twice"
//! means when the listener itself is what triggers the second dispatch.

use rts_core::entry::{self, Context, Provided};

/// The own property holding one array of records per event type.
///
/// Deliberately not `node:events`' `__events__`; the module tree's doc states
/// why two contracts must not share one store.
const STORE: &str = "__listeners__";

/// The flag one dispatch sets on the event for the length of that dispatch.
///
/// Named as a constant for the reason `event::STOPPED` is: the set and the test
/// must agree on the spelling, and a typo in one of them is the unbounded
/// recursion this flag exists to refuse.
const IN_FLIGHT: &str = "__dispatching__";

const METHODS: &[(&str, Provided)] = &[
    ("addEventListener", add),
    ("removeEventListener", remove),
    ("dispatchEvent", dispatch),
];

/// The one `EventTarget.prototype`, made on first ask.
pub(super) fn prototype(context: &mut Context) -> u64 {
    entry::make_prototype(context, "EventTarget", METHODS)
}

/// Builds the `EventTarget` class and answers its constructor.
pub(super) fn install(context: &mut Context) -> u64 {
    let prototype = prototype(context);
    super::class_ctor(context, "EventTarget", 0, construct, prototype)
}

/// `new EventTarget()`.
extern "C" fn construct(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let prototype = prototype(context);
        let target = super::self_or_new(context, this, prototype);
        prepare(context, target);
        target
    })
}

/// Gives an object its listener store.
///
/// Exported because `AbortSignal` is an `EventTarget` whose constructor is not
/// this one — a signal is made by a controller, never by `new` — and a signal
/// without a store would accept `addEventListener` and deliver nothing.
pub(super) fn prepare(context: &mut Context, target: u64) {
    let store = entry::make_object(context);
    entry::put_member(context, target, STORE, store);
}

/// The store, made now if the object has none.
///
/// Lazy rather than assumed, because an object can reach these methods without
/// having run either constructor — `class Mine extends EventTarget` that never
/// calls `super()` is the ordinary case. The alternative was a silent drop:
/// writing an array into a store that is `undefined` is a no-op, so every
/// listener would register successfully and never be called.
fn store_of(target: u64) -> u64 {
    let held = super::get(target, STORE);
    if held != super::absent() {
        return held;
    }
    entry::with_runtime(|context| {
        let store = entry::make_object(context);
        entry::put_member(context, target, STORE, store);
        store
    })
}

/// `target.addEventListener(type, listener, options?)`.
///
/// A `(type, listener, capture)` triple already registered is a no-op, as
/// specified — which is the one thing `capture` is for here, since there is no
/// capture phase to run in Node or in this engine.
extern "C" fn add(_e: u64, this: u64, kind: u64, listener: u64, options: u64, _d: u64) -> u64 {
    let Some(name) = super::text(kind) else {
        return super::absent();
    };
    // `options.signal`, which the module doc refused by name and which needs
    // nothing new: the same list `AbortSignal.any` keeps of who depends on a
    // signal serves a registration that wants removing, so the reaction that
    // "a native callable cannot carry" is not a callable at all.
    //
    // Anything that is not a signal is a `TypeError`, `null` included — that is
    // the specification's own coercion of the `signal` entry, and a lenient
    // reading of it is worse than the error: a program passing the wrong thing
    // gets a listener that is never removed, which is the leak it was writing
    // `signal` to avoid.
    let signal = match super::options_bag(options).map(|bag| super::get(bag, "signal")) {
        Some(value) if value != super::absent() => {
            if !super::abort::is_signal(value) {
                entry::throw_type_error(
                    "addEventListener: options.signal must be an AbortSignal",
                );
                return super::absent();
            }
            // Already aborted: the registration is over before it began, and
            // the language says nothing is added rather than added-and-removed.
            if super::flag(value, "aborted") {
                return super::absent();
            }
            Some(value)
        }
        _ => None,
    };
    let once = super::option_flag(options, "once");
    let capture = capture_of(options);
    let passive = super::option_flag(options, "passive");
    let store = store_of(this);
    let mut records = super::elements(super::get(store, &name));
    if records.iter().any(|&held| matches(held, listener, capture)) {
        return super::absent();
    }
    let record = entry::with_runtime(|context| {
        let record = entry::make_object(context);
        entry::put_member(context, record, "fn", listener);
        entry::put_member(context, record, "once", entry::boolean_value(once));
        entry::put_member(context, record, "capture", entry::boolean_value(capture));
        entry::put_member(
            context,
            record,
            "passive",
            entry::boolean_value(passive),
        );
        record
    });
    records.push(record);
    super::store_elements(store, &name, records);
    if let Some(signal) = signal {
        super::abort::remove_on_abort(signal, this, &name, listener, capture);
    }
    super::absent()
}

/// Drops one registration, named the way `removeEventListener` names it.
///
/// The body `remove` used to hold, lifted so that an abort can withdraw a
/// registration without going through the JavaScript-visible method — the same
/// reason `dispatch_event` exists beside `dispatch`: the method is a property a
/// program can have replaced, and a signal's promise to remove a listener is not
/// a promise a program's monkey patch may break.
pub(super) fn drop_registration(target: u64, name: &str, listener: u64, capture: bool) {
    let store = store_of(target);
    let mut records = super::elements(super::get(store, name));
    if let Some(at) = records
        .iter()
        .position(|&held| matches(held, listener, capture))
    {
        records.remove(at);
        super::store_elements(store, name, records);
    }
}

/// `target.removeEventListener(type, listener, options?)` — removes the one
/// registration whose `capture` matches, as specified.
extern "C" fn remove(_e: u64, this: u64, kind: u64, listener: u64, options: u64, _d: u64) -> u64 {
    let Some(name) = super::text(kind) else {
        return super::absent();
    };
    drop_registration(this, &name, listener, capture_of(options));
    super::absent()
}

/// `target.dispatchEvent(event)`.
extern "C" fn dispatch(_e: u64, this: u64, event: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    // Anything that is not an `Event` is a `TypeError`, which is the
    // specification's own coercion of the argument. A plain object with a
    // `type` property used to be delivered, so a program that meant to pass an
    // event and passed its options bag got listeners called with the wrong
    // thing and no signal at all.
    if !is_event(event) {
        entry::throw_type_error("dispatchEvent: the argument must be an Event");
        return super::absent();
    }
    entry::boolean_value(dispatch_event(this, event))
}

/// Whether a value inherits from `Event.prototype`.
///
/// By the chain rather than by a `type` property, for the reason
/// `abort::is_signal` gives: a duck test accepts the very mistake the error
/// exists to report.
fn is_event(value: u64) -> bool {
    let wanted = entry::with_runtime(super::event::prototype);
    let mut held = entry::get_prototype(value);
    while held != entry::null_value() && held != entry::undefined_value() {
        if held == wanted {
            return true;
        }
        held = entry::get_prototype(held);
    }
    false
}

/// Delivers one event to one target, synchronously, in registration order.
///
/// Answers what `dispatchEvent` answers: `false` only when the event is
/// cancelable and a listener called `preventDefault()`.
///
/// Exported because `AbortController.abort()` dispatches the `'abort'` event
/// through this rather than through the JavaScript-visible method — reaching the
/// method would mean a property lookup a program could have replaced, and the
/// engine-generated event has to be delivered whatever a program did to the
/// prototype.
pub(super) fn dispatch_event(target: u64, event: u64) -> bool {
    let Some(name) = super::text(super::get(event, "type")) else {
        return true;
    };
    // An event that is already being dispatched is refused, and the refusal is
    // what stops a HANG: `t.dispatchEvent(ev)` from inside a listener for `ev`
    // re-entered this function with the same event for ever and took the process
    // with a stack overflow. Node raises here; the flag is cleared on the way
    // out, so the same object may be dispatched again afterwards, which is the
    // half a "spent event" reading would get wrong.
    if super::flag(event, IN_FLIGHT) {
        entry::throw_value(
            entry::make_named_error("Error", "dispatchEvent: the event is already being dispatched")
                .unwrap_or_else(super::absent),
        );
        return true;
    }
    super::put(event, IN_FLIGHT, entry::boolean_value(true));
    let store = store_of(target);
    let records = super::elements(super::get(store, &name));
    let remaining: Vec<u64> = records
        .iter()
        .copied()
        .filter(|&held| !super::flag(held, "once"))
        .collect();
    if remaining.len() != records.len() {
        super::store_elements(store, &name, remaining);
    }
    stamp(event, target, super::event::AT_TARGET);
    let callees = callees_of(target, &records);
    let absent = super::absent();
    for (record, (callee, receiver)) in records.iter().copied().zip(callees) {
        if super::flag(event, super::event::STOPPED) {
            break;
        }
        // Whether this registration is STILL registered, asked immediately
        // before the call rather than once at the top.
        //
        // The list is snapshotted — it has to be, or a listener that adds
        // another would have the new one run in the same dispatch, which the
        // specification forbids. But a listener REMOVED during the dispatch
        // must not run, and a snapshot alone cannot express that: the classic
        // pair is a listener whose job is to remove the next one, and it was
        // removing something that then ran anyway.
        //
        // Re-read rather than tracked with a flag on the record: `remove`
        // deletes the record from the store, so the store is the only thing
        // that knows, and a second bookkeeping place would be the one that
        // disagrees.
        //
        // A `once` record is EXEMPT, and getting that wrong is how this was
        // first written: `once` registrations are dropped from the store before
        // any listener runs — the module's second paragraph says why — so the
        // check would find every one of them missing and skip it, and a `once`
        // listener would never run at all.
        if !super::flag(record, "once") && !still_registered(store, &name, record) {
            continue;
        }
        super::put(
            event,
            super::event::PASSIVE_LISTENER,
            entry::boolean_value(super::flag(record, "passive")),
        );
        entry::call(callee, receiver, event, absent, absent, absent);
    }
    entry::with_runtime(|context| {
        let nothing = entry::null_in(context);
        entry::put_member(context, event, "currentTarget", nothing);
        let phase = entry::make_number(super::event::NONE);
        entry::put_member(context, event, "eventPhase", phase);
        entry::put_member(context, event, IN_FLIGHT, entry::boolean_value(false));
        entry::put_member(
            context,
            event,
            super::event::PASSIVE_LISTENER,
            entry::boolean_value(false),
        );
    });
    !(super::flag(event, "cancelable") && super::flag(event, "defaultPrevented"))
}

/// Whether a snapshotted record is still in the target's list for this type.
///
/// Identity and not equality: two registrations of the same function with the
/// same `capture` cannot both exist — `addEventListener` refuses the duplicate —
/// so the record object itself is the registration, and comparing the object is
/// what distinguishes "still there" from "an equal one was added back".
///
/// Never asked about a `once` record. Those are dropped from the store before
/// any listener runs, so this would find every one of them missing and skip it —
/// and a `once` listener that never runs is a worse bug than the one this
/// function exists to fix. The caller carries that exemption, where it is
/// visible beside the call it guards.
fn still_registered(store: u64, name: &str, record: u64) -> bool {
    super::elements(super::get(store, name))
        .iter()
        .any(|&held| held == record)
}

/// Marks an event as being dispatched at a target, and clears the flag
/// `stopImmediatePropagation` sets — one dispatch's decision must not carry into
/// the next one for an event a program reuses.
fn stamp(event: u64, target: u64, phase: f64) {
    entry::with_runtime(|context| {
        entry::put_member(context, event, "target", target);
        entry::put_member(context, event, "currentTarget", target);
        entry::put_member(context, event, "eventPhase", entry::make_number(phase));
        let clear = entry::boolean_value(false);
        entry::put_member(context, event, super::event::STOPPED, clear);
        entry::put_member(context, event, super::event::PROPAGATION_STOPPED, clear);
        entry::put_member(context, event, super::event::PASSIVE_LISTENER, clear);
        entry::put_member(context, event, "cancelBubble", clear);
    });
}

/// What each record resolves to: the function to call, and the receiver.
///
/// A listener may be an object with a `handleEvent` method instead of a
/// function, which the specification requires and which is why the receiver
/// varies — a `handleEvent` called with the target as `this` would read the
/// wrong object's fields.
///
/// Resolved in ONE borrow, before any call: the alternative is a lookup for the
/// next listener while the previous one is still running, which is the nested
/// borrow this module's doc opens with.
fn callees_of(target: u64, records: &[u64]) -> Vec<(u64, u64)> {
    entry::with_runtime(|context| {
        records
            .iter()
            .map(|&record| {
                let listener = entry::get_member(context, record, "fn");
                match entry::is_callable_in(context, listener) {
                    true => (listener, target),
                    false => (
                        entry::get_member(context, listener, "handleEvent"),
                        listener,
                    ),
                }
            })
            .collect()
    })
}

/// Whether a record is the registration `(listener, capture)` names.
fn matches(record: u64, listener: u64, capture: bool) -> bool {
    entry::strict_equals(super::get(record, "fn"), listener)
        && super::flag(record, "capture") == capture
}

/// The `capture` an options argument asks for.
///
/// Both spellings the specification allows: an options bag's `capture` entry,
/// and the legacy positional boolean.
fn capture_of(options: u64) -> bool {
    match super::options_bag(options) {
        Some(bag) => super::flag(bag, "capture"),
        None => entry::to_boolean(options),
    }
}

