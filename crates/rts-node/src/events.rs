//! `node:events` — `EventEmitter`, built on one real prototype.
//!
//! # What shape `EventEmitter` has
//!
//! One object, made once, holds every instance method:
//! [`rts_core::entry::make_prototype`] builds it under the name
//! `"EventEmitter"` and remembers it, so a second call inside the same run
//! answers the *same* object rather than building a second one.
//! [`rts_core::entry::make_instance`] then gives each `new EventEmitter()`
//! a fresh plain object whose `[[Prototype]]` link points at that one shared
//! object, which is what makes a method found by the ordinary chain walk
//! rather than by anything here knowing what an `EventEmitter` is.
//!
//! This module's predecessor built a **factory**: every call to `on()`,
//! `emit()` and so on hung a fresh callable directly on the new instance.
//! Three things a program can observe were wrong under that shape, and are
//! not wrong under this one:
//!
//! - `Object.keys(emitter)` no longer lists `on`, `emit`, `off`, … — those
//!   live on the prototype, not as own properties, so only `__events__` (and
//!   `__maxListeners__`, once set) show up, matching real Node.
//! - `Object.getPrototypeOf(a) === Object.getPrototypeOf(b)` is now `true`
//!   for two emitters — under the factory shape every instance had its own
//!   freshly-built methods, so this was always `false`.
//! - A program that reaches the prototype (`Object.getPrototypeOf(emitter)`)
//!   and adds a method to it now reaches every emitter that shares it —
//!   under the factory shape there was no shared object to add to.
//!
//! `new EventEmitter()` and plain `EventEmitter()` still both work: this is
//! ordinary JS semantics (a constructor returning an object wins over the
//! implicit `this`), not a special case this module adds.
//!
//! Listener storage is unchanged from the factory version and is reused as-is:
//! an ordinary property, `__events__`, holding one array per event name; each
//! array element is a small `{ fn, once }` object rather than the bare
//! function, so `.once()` has somewhere to keep its flag.
//!
//! # The borrow this module has to get right
//!
//! [`emit`] stores functions and later calls them. Every read here —
//! [`collect_array`], [`get_indexed`](rts_core::entry::get_indexed) —
//! finishes and drops its borrow before returning, because each is one call
//! to an entry point that opens and closes the runtime borrow itself. `emit`
//! collects every listener into a `Vec` *first*, and only calls
//! [`rts_core::entry::call`] afterward, on that already-detached `Vec` —
//! never while a lookup for the next one is still open. Calling a listener
//! from inside a held borrow is not a bug this module produces slowly; it is
//! an abort on the first `emit`.
//!
//! # `'error'` with no listener
//!
//! Real Node throws the error value, prints it, and exits the process when
//! `'error'` is emitted against zero `'error'` listeners. A native entry
//! point here cannot throw — [`crate::assert`] documents the same limit: no
//! protected region exists for it to unwind into. So [`emit`] does not throw
//! either, but it also does not swallow the error silently, which is worse
//! than either real behavior: it prints the same diagnostic real Node would
//! **What this costs an embedder, named rather than discovered.** A host
//! running many programs in one process — which is what a test runner over a
//! corpus is — loses every result when one program emits an unhandled
//! `'error'`. That is faithful to Node, where the process IS the program, and
//! it is not faithful to this engine, where it is not. The day a runner here
//! spans files, this needs a way for the host to say "report it and stop THIS
//! program" — which is the same missing mechanism `assert` waits on, and is why
//! that module chose to print and count rather than exit.
//!
//! and ends the process the same way, via `std::process::exit`, which needs
//! no unwind and so is safe to call from a native. What is not reproduced is
//! `EventEmitter.errorMonitor` running first for observability — that symbol
//! is not implemented here (see below).
//!
//! # Not implemented, by name
//!
//! `EventTarget`, `Event`, `CustomEvent`, `EventEmitterAsyncResource` — the
//! WHATWG globals this module re-exports in real Node are a separate ambient
//! surface this crate was not given a way to reach or construct.
//! `events.on`/`events.once` — real Node's async-iterator and promise
//! variants. Building either needs constructing a `Promise` and driving it
//! from a native, which is not an operation this crate's entry surface
//! exposes (`rts_core::entry` has `settled`/`drain_microtasks` for
//! reading a promise that already exists, nothing to make one from Rust) —
//! this is exactly the gap `docs/reference/node/events.md` §5.1 already
//! names as belonging to a `.ts` shim over the primordial `Promise`, not to
//! this crate. `events.addAbortListener`. `rawListeners` returns the same
//! functions [`listeners`] does rather than real Node's invocable
//! once-wrapper with a `.listener` back-reference — building a per-listener
//! closure needs a native callable that carries captured state, and
//! [`rts_core::entry::make_callable`] hands back a fixed function
//! pointer with no environment slot this crate can fill; a wrapper that is
//! itself callable waits on that. `captureRejections`. `errorMonitor`,
//! `EventEmitter.defaultMaxListeners`/`captureRejectionSymbol` as *statics on
//! the `EventEmitter` value itself*, and `MaxListenersExceededWarning` —
//! nothing is warned on; [`set_max_listeners`] and
//! [`static_set_max_listeners`] record numbers, never compare them against a
//! count. Symbol-keyed event names — every event name here goes through
//! [`text_of`](rts_core::entry::text_of), so only strings work; a symbol
//! argument reads as an absent event and silently does nothing.

use rts_core::entry::{Context, Provided};
use std::sync::atomic::{AtomicU64, Ordering};

/// `EventEmitter.defaultMaxListeners` — read by [`get_max_listeners`] when an
/// instance never called `.setMaxListeners()`, written by
/// [`static_set_max_listeners`] when called with no target.
static DEFAULT_MAX_LISTENERS: AtomicU64 = AtomicU64::new(10);

/// The instance methods every `EventEmitter` shares through one prototype.
const METHODS: &[(&str, Provided)] = &[
    ("on", on),
    ("addListener", on),
    ("once", once),
    ("prependListener", prepend_listener),
    ("prependOnceListener", prepend_once_listener),
    ("off", remove_listener),
    ("removeListener", remove_listener),
    ("removeAllListeners", remove_all_listeners),
    ("emit", emit),
    ("listenerCount", listener_count),
    ("eventNames", event_names),
    ("listeners", listeners),
    ("rawListeners", listeners),
    ("getMaxListeners", get_max_listeners),
    ("setMaxListeners", set_max_listeners),
];

/// What every emitter in the program inherits from, without building a
/// namespace to reach it.
///
/// # Why this exists rather than `namespace(context)` at three call sites
///
/// `node:process` and `node:readline` both want exactly one thing from
/// `node:events`: the prototype `EventEmitter` carries. Both used to ask for the
/// whole namespace and walk `EventEmitter` → `prototype` to get it, and
/// [`namespace`] is **not** memoized — it builds a fresh namespace object and
/// installs three natives on it every time it is called. So the surface was
/// built three times on every startup and two of them were thrown away after one
/// property read.
///
/// This is the memoized half. `make_prototype` records by name and returns what
/// it recorded (`rts-core`'s `modules.rs`), so the second and third caller pay a
/// table lookup instead of a namespace.
///
/// # Why it is safe where `make_prototype(context, "EventEmitter", &[])` is not
///
/// `node:process`'s own comment records the hazard: a call with an **empty**
/// member list wins the name and registers a prototype with no methods on it, so
/// `node:events` finds it later and never installs `on`. This passes the real
/// [`METHODS`] table, which is the same argument [`namespace`] passes — so
/// whichever of the two runs first installs the same surface, and the other gets
/// it back. That is what makes the callers independent of install order rather
/// than merely ordered correctly today.
pub fn emitter_prototype(context: &mut Context) -> u64 {
    rts_core::entry::make_prototype(context, "EventEmitter", METHODS)
}

/// The namespace `node:events` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("EventEmitter", make_emitter),
        ("getEventListeners", get_event_listeners),
        ("setMaxListeners", static_set_max_listeners),
    ];
    let namespace = rts_core::entry::make_namespace(context, members);
    // The constructor carries `prototype`, which is what makes `new` link the
    // object it allocates to it. Returning an instance from the constructor is
    // NOT enough: `new` over a native keeps the object it made, so every
    // emitter came back with no prototype and no `on` at all — which the
    // agent's own tests did not reach and a fixture calling `emit` did.
    let prototype = emitter_prototype(context);
    let constructor = rts_core::entry::get_member(context, namespace, "EventEmitter");
    rts_core::entry::put_member(context, constructor, "prototype", prototype);
    namespace
}

/// `new EventEmitter()` — see the module doc for why calling it plainly also
/// works, and for what sharing one prototype across every instance changes.
extern "C" fn make_emitter(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    rts_core::entry::with_runtime(|context| {
        let prototype = rts_core::entry::make_prototype(context, "EventEmitter", METHODS);
        // `new` hands over an object already linked to `prototype`; a plain
        // call hands over `undefined` and this makes one. Both answer something
        // with the methods on it, which is what lets `EventEmitter()` work.
        let emitter = match rts_core::entry::is_object(context, this) {
            true => this,
            false => rts_core::entry::make_instance(context, prototype),
        };
        let events = rts_core::entry::make_object(context);
        rts_core::entry::put_member(context, emitter, "__events__", events);
        emitter
    })
}

/// `emitter.on(eventName, listener)` / `.addListener(...)`.
extern "C" fn on(_e: u64, this: u64, event: u64, listener: u64, _c: u64, _d: u64) -> u64 {
    add_listener(this, event, listener, false, false);
    this
}

/// `emitter.once(eventName, listener)`.
extern "C" fn once(_e: u64, this: u64, event: u64, listener: u64, _c: u64, _d: u64) -> u64 {
    add_listener(this, event, listener, true, false);
    this
}

/// `emitter.prependListener(eventName, listener)`.
extern "C" fn prepend_listener(_e: u64, this: u64, event: u64, listener: u64, _c: u64, _d: u64) -> u64 {
    add_listener(this, event, listener, false, true);
    this
}

/// `emitter.prependOnceListener(eventName, listener)`.
extern "C" fn prepend_once_listener(_e: u64, this: u64, event: u64, listener: u64, _c: u64, _d: u64) -> u64 {
    add_listener(this, event, listener, true, true);
    this
}

/// `emitter.off(eventName, listener)` / `.removeListener(...)` — removes at
/// most one matching registration, like real Node, and emits
/// `'removeListener'` with the original (unwrapped) function afterward.
extern "C" fn remove_listener(_e: u64, this: u64, event: u64, listener: u64, _c: u64, _d: u64) -> u64 {
    let Some(name) = rts_core::entry::text_of(event) else {
        return this;
    };
    let events = events_object(this);
    let array = rts_core::entry::get_indexed(events, event);
    let mut wrappers = collect_array(array);
    if let Some(at) = wrappers.iter().position(|&w| rts_core::entry::strict_equals(wrapper_fn(w), listener)) {
        wrappers.remove(at);
        store_array(events, &name, wrappers);
        if name != "removeListener" {
            emit_meta(this, "removeListener", event, listener);
        }
    }
    this
}

/// `emitter.removeAllListeners(eventName?)` — emits `'removeListener'` once
/// per listener actually removed, for the event(s) named.
extern "C" fn remove_all_listeners(_e: u64, this: u64, event: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = rts_core::entry::undefined_value();
    if event == absent {
        let names: Vec<u64> = collect_array(rts_core::entry::own_keys(events_object(this)));
        for key in names {
            remove_all_for(this, key);
        }
    } else {
        remove_all_for(this, event);
    }
    this
}

/// Clears one event's listener array, firing `'removeListener'` for each.
fn remove_all_for(this: u64, event: u64) {
    let Some(name) = rts_core::entry::text_of(event) else {
        return;
    };
    let events = events_object(this);
    let array = rts_core::entry::get_indexed(events, event);
    let wrappers = collect_array(array);
    store_array(events, &name, Vec::new());
    if name != "removeListener" {
        for wrapper in wrappers {
            emit_meta(this, "removeListener", event, wrapper_fn(wrapper));
        }
    }
}

/// `emitter.emit(eventName, ...args)` — up to three args, the most this
/// module's four call slots leave room for once the receiver and event name
/// each take one. `'error'` with zero listeners ends the process; see the
/// module doc.
extern "C" fn emit(_e: u64, this: u64, event: u64, a0: u64, a1: u64, a2: u64) -> u64 {
    let events = events_object(this);
    let array = rts_core::entry::get_indexed(events, event);
    let wrappers = collect_array(array);
    if wrappers.is_empty() {
        if rts_core::entry::text_of(event).as_deref() == Some("error") {
            crash_on_unhandled_error(a0);
        }
        return rts_core::entry::boolean_value(false);
    }
    // `once` listeners are dropped from storage before any of them runs, so a
    // listener re-entering `emit` for the same event does not see them twice.
    let remaining: Vec<u64> = wrappers.iter().copied().filter(|&w| !wrapper_once(w)).collect();
    if remaining.len() != wrappers.len()
        && let Some(name) = rts_core::entry::text_of(event)
    {
        store_array(events, &name, remaining);
    }
    let absent = rts_core::entry::undefined_value();
    for wrapper in wrappers {
        let listener = wrapper_fn(wrapper);
        rts_core::entry::call(listener, this, a0, a1, a2, absent);
    }
    rts_core::entry::boolean_value(true)
}

/// The same diagnostic-then-exit real Node gives an unhandled `'error'`
/// event — see the module doc for why a native ends the process instead of
/// throwing.
fn crash_on_unhandled_error(error: u64) -> ! {
    match rts_core::entry::described(error) {
        Some(text) => eprintln!("rts: uncaught 'error' event: {text}"),
        None => eprintln!("rts: uncaught 'error' event: an object"),
    }
    std::process::exit(1)
}

/// `emitter.listenerCount(eventName, listener?)`.
extern "C" fn listener_count(_e: u64, this: u64, event: u64, listener: u64, _c: u64, _d: u64) -> u64 {
    let events = events_object(this);
    let array = rts_core::entry::get_indexed(events, event);
    let wrappers = collect_array(array);
    let absent = rts_core::entry::undefined_value();
    let count = if listener == absent {
        wrappers.len()
    } else {
        wrappers.iter().filter(|&&w| rts_core::entry::strict_equals(wrapper_fn(w), listener)).count()
    };
    rts_core::entry::make_number(count as f64)
}

/// `emitter.eventNames()` — the names holding at least one listener.
extern "C" fn event_names(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let events = events_object(this);
    let names: Vec<u64> = collect_array(rts_core::entry::own_keys(events))
        .into_iter()
        .filter(|&key| {
            let array = rts_core::entry::get_indexed(events, key);
            !collect_array(array).is_empty()
        })
        .collect();
    rts_core::entry::make_array(names)
}

/// `emitter.listeners(eventName)` and `emitter.rawListeners(eventName)` — the
/// same array under both names; see the module doc for the divergence.
extern "C" fn listeners(_e: u64, this: u64, event: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let events = events_object(this);
    let array = rts_core::entry::get_indexed(events, event);
    let functions: Vec<u64> = collect_array(array).into_iter().map(wrapper_fn).collect();
    rts_core::entry::make_array(functions)
}

/// `events.getEventListeners(emitterOrTarget, eventName)` — the module-level
/// static, same answer as the instance method.
extern "C" fn get_event_listeners(e: u64, _this: u64, emitter: u64, event: u64, _c: u64, _d: u64) -> u64 {
    listeners(e, emitter, event, 0, 0, 0)
}

/// `emitter.getMaxListeners()` — the explicit `setMaxListeners()` value if
/// one was set, else [`DEFAULT_MAX_LISTENERS`].
extern "C" fn get_max_listeners(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = rts_core::entry::undefined_value();
    let stored = rts_core::entry::with_runtime(|context| {
        rts_core::entry::get_member(context, this, "__maxListeners__")
    });
    if stored == absent {
        rts_core::entry::make_number(DEFAULT_MAX_LISTENERS.load(Ordering::Relaxed) as f64)
    } else {
        stored
    }
}

/// `emitter.setMaxListeners(n)` — recorded, never enforced or warned on; see
/// the module doc.
extern "C" fn set_max_listeners(_e: u64, this: u64, n: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    rts_core::entry::with_runtime(|context| {
        rts_core::entry::put_member(context, this, "__maxListeners__", n);
    });
    this
}

/// `events.setMaxListeners(n, target?)` — with no target, changes
/// [`DEFAULT_MAX_LISTENERS`]; real Node also accepts a variadic list of
/// targets, which this module's four call slots have no room left for once
/// `n` and one target are read, so only the single-target form is honest
/// here.
extern "C" fn static_set_max_listeners(_e: u64, _this: u64, n: u64, target: u64, _c: u64, _d: u64) -> u64 {
    let absent = rts_core::entry::undefined_value();
    if target == absent {
        if let Some(value) = rts_core::entry::number_of(n) {
            DEFAULT_MAX_LISTENERS.store(value as u64, Ordering::Relaxed);
        }
    } else {
        set_max_listeners(0, target, n, 0, 0, 0);
    }
    absent
}

/// Appends a `{ fn, once }` wrapper to an event's listener array, at the
/// front when `prepend` is set, and fires `'newListener'` first — before the
/// listener is actually reachable, matching real Node (a `'newListener'`
/// handler that itself emits the same event synchronously will not see the
/// not-yet-added listener).
fn add_listener(this: u64, event: u64, listener: u64, once: bool, prepend: bool) {
    let Some(name) = rts_core::entry::text_of(event) else {
        return;
    };
    if name != "newListener" {
        emit_meta(this, "newListener", event, listener);
    }
    let events = events_object(this);
    let array = rts_core::entry::get_indexed(events, event);
    let mut wrappers = collect_array(array);
    let wrapper = rts_core::entry::with_runtime(|context| {
        let object = rts_core::entry::make_object(context);
        rts_core::entry::put_member(context, object, "fn", listener);
        let flag = rts_core::entry::boolean_value(once);
        rts_core::entry::put_member(context, object, "once", flag);
        object
    });
    if prepend {
        wrappers.insert(0, wrapper);
    } else {
        wrappers.push(wrapper);
    }
    store_array(events, &name, wrappers);
}

/// Emits `'newListener'`/`'removeListener'` directly against `this`'s own
/// listener storage, bypassing [`emit`]'s `'error'`-crash special case —
/// neither meta-event is `'error'`, so that branch never applies, but going
/// through `emit` itself would also re-derive `events_object` for no reason.
fn emit_meta(this: u64, meta: &str, event: u64, listener: u64) {
    let events = events_object(this);
    let key = string_key(meta);
    let array = rts_core::entry::get_indexed(events, key);
    let wrappers = collect_array(array);
    if wrappers.is_empty() {
        return;
    }
    let absent = rts_core::entry::undefined_value();
    for wrapper in wrappers {
        rts_core::entry::call(wrapper_fn(wrapper), this, event, listener, absent, absent);
    }
}

/// The `__events__` object an emitter carries.
fn events_object(this: u64) -> u64 {
    rts_core::entry::get_indexed(this, string_key("__events__"))
}

/// The original listener a `{ fn, once }` wrapper holds.
fn wrapper_fn(wrapper: u64) -> u64 {
    rts_core::entry::get_indexed(wrapper, string_key("fn"))
}

/// Whether a wrapper is a `.once()` registration.
fn wrapper_once(wrapper: u64) -> bool {
    rts_core::entry::to_boolean(rts_core::entry::get_indexed(wrapper, string_key("once")))
}

/// Replaces one event's listener array under `events.<name>`.
fn store_array(events: u64, name: &str, wrappers: Vec<u64>) {
    rts_core::entry::with_runtime(|context| {
        let array = rts_core::entry::make_array_in(context, wrappers);
        rts_core::entry::put_member(context, events, name, array);
    });
}

/// A JS array's elements, read out through `.length` and indexed access.
fn collect_array(array: u64) -> Vec<u64> {
    let absent = rts_core::entry::undefined_value();
    if array == absent {
        return Vec::new();
    }
    let length_value = rts_core::entry::get_indexed(array, string_key("length"));
    // Asked of the value, not of its text: reading a number by parsing its
    // decimal back is lossy where a double's shortest decimal is not the double.
    let length = rts_core::entry::number_of(length_value)
        .map(|value| value as usize)
        .unwrap_or(0);
    (0..length)
        .map(|index| rts_core::entry::get_indexed(array, rts_core::entry::make_number(index as f64)))
        .collect()
}

/// An interned string, for use as a property key from outside a borrow.
fn string_key(text: &str) -> u64 {
    rts_core::entry::with_runtime(|context| rts_core::entry::make_string(context, text))
}
