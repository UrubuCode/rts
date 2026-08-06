//! `node:events` — `EventEmitter`, built out of plain objects and instance
//! methods that read and write their own properties.
//!
//! # What shape `EventEmitter` actually has
//!
//! Not a class. Nothing in the value API this crate was given builds a
//! prototype chain from Rust, so there is no `EventEmitter.prototype` a
//! subclass could extend. What this exports under `EventEmitter` is a
//! **factory function** — [`make_emitter`] — that builds a fresh plain object
//! and hangs the instance methods directly on it, one callable per method,
//! every time it runs.
//!
//! That is not as different from `new EventEmitter()` as it sounds: when a
//! constructor function returns an object, `new` uses *that* object instead
//! of the implicit `this` — ordinary JS semantics, not a special case this
//! module adds. So both `events.EventEmitter()` and `new
//! events.EventEmitter()` produce a working emitter. What does not work is
//! `class Foo extends EventEmitter` — there is no prototype to extend, and a
//! program that writes one gets `undefined` methods rather than a clear
//! error, which is the honest cost of this shape.
//!
//! Listener storage is an ordinary property, `__events__`, holding one array
//! per event name; each array element is a small `{ fn, once }` object rather
//! than the bare function, so `once()` has somewhere to keep its flag.
//!
//! # The borrow this module has to get right
//!
//! [`emit`] stores functions and later calls them. Every read here —
//! [`collect_array`], [`get_indexed`](rts_core_rwk::entry::get_indexed) —
//! finishes and drops its borrow before returning, because each is one call
//! to an entry point that opens and closes the runtime borrow itself. `emit`
//! collects every listener into a `Vec` *first*, and only calls
//! [`rts_core_rwk::entry::call`] afterward, on that already-detached `Vec` —
//! never while a lookup for the next one is still open. Calling a listener
//! from inside a held borrow is not a bug this module produces slowly; it is
//! an abort on the first `emit`.
//!
//! # Not implemented, by name
//!
//! `EventTarget`, `Event`, `CustomEvent`, `EventEmitterAsyncResource` — the
//! WHATWG globals this module re-exports in real Node are a separate ambient
//! surface this crate was not given a way to reach or construct. Module-level
//! `events.on`/`events.once`/`events.getEventListeners`/`events.setMaxListeners`
//! (the static async-iterator/promise helpers — not to be confused with the
//! instance methods of the same short names, which *are* implemented below).
//! `events.addAbortListener`. `prependListener`, `prependOnceListener`,
//! `listeners`, `rawListeners`, `getMaxListeners`. The `'newListener'` and
//! `'removeListener'` meta-events. The special crash-if-unhandled behavior of
//! emitting `'error'` with no listener — it emits like any other event here,
//! silently, which is the one divergence a program is likeliest to notice.
//! `captureRejections`. `errorMonitor`/`captureRejectionSymbol`/
//! `defaultMaxListeners` statics, and `MaxListenersExceededWarning` — nothing
//! is warned on; [`set_max_listeners`] records a number nobody reads back.
//! Symbol-keyed event names — every event name here goes through
//! [`text_of`](rts_core_rwk::entry::text_of), so only strings work; a symbol
//! argument reads as an absent event and silently does nothing.

use rts_core_rwk::entry::{Context, Provided};

/// The namespace `node:events` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[("EventEmitter", make_emitter)];
    rts_core_rwk::entry::make_namespace(context, members)
}

/// `new EventEmitter()` — see the module doc for why calling it plainly also
/// works.
extern "C" fn make_emitter(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    rts_core_rwk::entry::with_runtime(|context| {
        let emitter = rts_core_rwk::entry::make_object(context);
        let events = rts_core_rwk::entry::make_object(context);
        rts_core_rwk::entry::put_member(context, emitter, "__events__", events);
        let methods: &[(&str, Provided)] = &[
            ("on", on),
            ("addListener", on),
            ("once", once),
            ("off", remove_listener),
            ("removeListener", remove_listener),
            ("removeAllListeners", remove_all_listeners),
            ("emit", emit),
            ("listenerCount", listener_count),
            ("eventNames", event_names),
            ("setMaxListeners", set_max_listeners),
        ];
        for (name, code) in methods {
            let method = rts_core_rwk::entry::make_callable(context, *code);
            rts_core_rwk::entry::put_member(context, emitter, name, method);
        }
        emitter
    })
}

/// `emitter.on(eventName, listener)` / `.addListener(...)`.
extern "C" fn on(_e: u64, this: u64, event: u64, listener: u64, _c: u64, _d: u64) -> u64 {
    add_listener(this, event, listener, false);
    this
}

/// `emitter.once(eventName, listener)`.
extern "C" fn once(_e: u64, this: u64, event: u64, listener: u64, _c: u64, _d: u64) -> u64 {
    add_listener(this, event, listener, true);
    this
}

/// `emitter.off(eventName, listener)` / `.removeListener(...)` — removes at
/// most one matching registration, like real Node.
extern "C" fn remove_listener(_e: u64, this: u64, event: u64, listener: u64, _c: u64, _d: u64) -> u64 {
    let Some(name) = rts_core_rwk::entry::text_of(event) else {
        return this;
    };
    let events = events_object(this);
    let array = rts_core_rwk::entry::get_indexed(events, event);
    let mut wrappers = collect_array(array);
    if let Some(at) = wrappers.iter().position(|&w| rts_core_rwk::entry::strict_equals(wrapper_fn(w), listener)) {
        wrappers.remove(at);
        store_array(events, &name, wrappers);
    }
    this
}

/// `emitter.removeAllListeners(eventName?)`.
extern "C" fn remove_all_listeners(_e: u64, this: u64, event: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = rts_core_rwk::entry::undefined_value();
    rts_core_rwk::entry::with_runtime(|context| {
        if event == absent {
            let fresh = rts_core_rwk::entry::make_object(context);
            rts_core_rwk::entry::put_member(context, this, "__events__", fresh);
        } else if let Some(name) = rts_core_rwk::entry::text_of(event) {
            let events = rts_core_rwk::entry::get_member(context, this, "__events__");
            let empty = rts_core_rwk::entry::make_array_in(context, Vec::new());
            rts_core_rwk::entry::put_member(context, events, &name, empty);
        }
    });
    this
}

/// `emitter.emit(eventName, ...args)` — up to three args, the most this
/// module's four call slots leave room for once the receiver and event name
/// each take one.
extern "C" fn emit(_e: u64, this: u64, event: u64, a0: u64, a1: u64, a2: u64) -> u64 {
    let events = events_object(this);
    let array = rts_core_rwk::entry::get_indexed(events, event);
    let wrappers = collect_array(array);
    if wrappers.is_empty() {
        return rts_core_rwk::entry::boolean_value(false);
    }
    // `once` listeners are dropped from storage before any of them runs, so a
    // listener re-entering `emit` for the same event does not see them twice.
    let remaining: Vec<u64> = wrappers.iter().copied().filter(|&w| !wrapper_once(w)).collect();
    if remaining.len() != wrappers.len()
        && let Some(name) = rts_core_rwk::entry::text_of(event)
    {
        store_array(events, &name, remaining);
    }
    let absent = rts_core_rwk::entry::undefined_value();
    for wrapper in wrappers {
        let listener = wrapper_fn(wrapper);
        rts_core_rwk::entry::call(listener, this, a0, a1, a2, absent);
    }
    rts_core_rwk::entry::boolean_value(true)
}

/// `emitter.listenerCount(eventName, listener?)`.
extern "C" fn listener_count(_e: u64, this: u64, event: u64, listener: u64, _c: u64, _d: u64) -> u64 {
    let events = events_object(this);
    let array = rts_core_rwk::entry::get_indexed(events, event);
    let wrappers = collect_array(array);
    let absent = rts_core_rwk::entry::undefined_value();
    let count = if listener == absent {
        wrappers.len()
    } else {
        wrappers.iter().filter(|&&w| rts_core_rwk::entry::strict_equals(wrapper_fn(w), listener)).count()
    };
    rts_core_rwk::entry::make_number(count as f64)
}

/// `emitter.eventNames()` — the names holding at least one listener.
extern "C" fn event_names(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let events = events_object(this);
    let names: Vec<u64> = collect_array(rts_core_rwk::entry::own_keys(events))
        .into_iter()
        .filter(|&key| {
            let array = rts_core_rwk::entry::get_indexed(events, key);
            !collect_array(array).is_empty()
        })
        .collect();
    rts_core_rwk::entry::make_array(names)
}

/// `emitter.setMaxListeners(n)` — recorded, never enforced or warned on; see
/// the module doc.
extern "C" fn set_max_listeners(_e: u64, this: u64, n: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    rts_core_rwk::entry::with_runtime(|context| {
        rts_core_rwk::entry::put_member(context, this, "__maxListeners__", n);
    });
    this
}

/// Appends a `{ fn, once }` wrapper to an event's listener array.
fn add_listener(this: u64, event: u64, listener: u64, once: bool) {
    let Some(name) = rts_core_rwk::entry::text_of(event) else {
        return;
    };
    let events = events_object(this);
    let array = rts_core_rwk::entry::get_indexed(events, event);
    let mut wrappers = collect_array(array);
    let wrapper = rts_core_rwk::entry::with_runtime(|context| {
        let object = rts_core_rwk::entry::make_object(context);
        rts_core_rwk::entry::put_member(context, object, "fn", listener);
        let flag = rts_core_rwk::entry::boolean_value(once);
        rts_core_rwk::entry::put_member(context, object, "once", flag);
        object
    });
    wrappers.push(wrapper);
    store_array(events, &name, wrappers);
}

/// The `__events__` object an emitter carries.
fn events_object(this: u64) -> u64 {
    rts_core_rwk::entry::get_indexed(this, string_key("__events__"))
}

/// The original listener a `{ fn, once }` wrapper holds.
fn wrapper_fn(wrapper: u64) -> u64 {
    rts_core_rwk::entry::get_indexed(wrapper, string_key("fn"))
}

/// Whether a wrapper is a `.once()` registration.
fn wrapper_once(wrapper: u64) -> bool {
    rts_core_rwk::entry::to_boolean(rts_core_rwk::entry::get_indexed(wrapper, string_key("once")))
}

/// Replaces one event's listener array under `events.<name>`.
fn store_array(events: u64, name: &str, wrappers: Vec<u64>) {
    rts_core_rwk::entry::with_runtime(|context| {
        let array = rts_core_rwk::entry::make_array_in(context, wrappers);
        rts_core_rwk::entry::put_member(context, events, name, array);
    });
}

/// A JS array's elements, read out through `.length` and indexed access.
fn collect_array(array: u64) -> Vec<u64> {
    let absent = rts_core_rwk::entry::undefined_value();
    if array == absent {
        return Vec::new();
    }
    let length_value = rts_core_rwk::entry::get_indexed(array, string_key("length"));
    // Asked of the value, not of its text: reading a number by parsing its
    // decimal back is lossy where a double's shortest decimal is not the double.
    let length = rts_core_rwk::entry::number_of(length_value)
        .map(|value| value as usize)
        .unwrap_or(0);
    (0..length)
        .map(|index| rts_core_rwk::entry::get_indexed(array, rts_core_rwk::entry::make_number(index as f64)))
        .collect()
}

/// An interned string, for use as a property key from outside a borrow.
fn string_key(text: &str) -> u64 {
    rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_string(context, text))
}
