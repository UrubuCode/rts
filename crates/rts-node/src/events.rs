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
//! this crate. `rawListeners` returns the same functions [`listeners`] does
//! rather than real Node's invocable once-wrapper with a `.listener`
//! back-reference — building a per-listener closure needs a native callable
//! that carries captured state, and [`rts_core::entry::make_callable`] hands
//! back a fixed function pointer with no environment slot this crate can fill;
//! a wrapper that is itself callable waits on that. `captureRejections`,
//! `errorMonitor`, `captureRejectionSymbol`, and `MaxListenersExceededWarning`
//! remain absent — the listener-limit setters record values but do not warn.
//! The full unhandled-`'error'` process-hook path remains a larger semantic gap.
//! `addAbortListener` and Symbol-keyed event names are implemented through the
//! shared EventTarget and property-key machinery.

use rts_core::entry::{Context, Provided};
use std::sync::atomic::{AtomicU64, Ordering};

/// `EventEmitter.defaultMaxListeners` — read by [`get_max_listeners`] when an
/// instance never called `.setMaxListeners()`, written by
/// [`static_set_max_listeners`] when called with no target.
static DEFAULT_MAX_LISTENERS: AtomicU64 = AtomicU64::new(10.0f64.to_bits());

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
        ("getMaxListeners", static_get_max_listeners),
        ("setMaxListeners", static_set_max_listeners),
        ("addAbortListener", add_abort_listener),
    ];
    let namespace = rts_core::entry::make_namespace(context, members);
    // `defaultMaxListeners` is a mutable module property in Node, not a copy of
    // the current value. Keep it backed by the same atomic used by every
    // emitter's fallback so reads and writes observe one setting.
    rts_core::entry::define_accessor_in(
        context,
        namespace,
        "defaultMaxListeners",
        default_max_listeners_get,
        Some(default_max_listeners_set),
    );
    // The constructor carries `prototype`, which is what makes `new` link the
    // object it allocates to it. Returning an instance from the constructor is
    // NOT enough: `new` over a native keeps the object it made, so every
    // emitter came back with no prototype and no `on` at all — which the
    // agent's own tests did not reach and a fixture calling `emit` did.
    let prototype = emitter_prototype(context);
    let constructor = rts_core::entry::get_member(context, namespace, "EventEmitter");
    rts_core::entry::put_member(context, constructor, "prototype", prototype);
    // Node's CommonJS export is the constructor itself and also exposes that
    // constructor as `.EventEmitter`, which named destructuring reads.
    rts_core::entry::put_member(context, constructor, "EventEmitter", constructor);
    // CommonJS returns the constructor itself (`require("events")`), whose
    // static helpers mirror the module namespace in Node. Keep both views on
    // the same callable cells rather than rebuilding any function.
    for name in ["getEventListeners", "getMaxListeners", "setMaxListeners", "addAbortListener"] {
        let member = rts_core::entry::get_member(context, namespace, name);
        rts_core::entry::put_member(context, constructor, name, member);
    }
    rts_core::entry::define_accessor_in(
        context,
        constructor,
        "defaultMaxListeners",
        default_max_listeners_get,
        Some(default_max_listeners_set),
    );
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
        let event_names = rts_core::entry::make_array_in(context, Vec::new());
        rts_core::entry::put_member(context, emitter, "__eventNames__", event_names);
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
    if !valid_event_name(event) {
        return this;
    }
    let events = events_object(this);
    let array = rts_core::entry::get_indexed(events, event);
    let mut wrappers = collect_array(array);
    if let Some(at) = wrappers.iter().position(|&w| rts_core::entry::strict_equals(wrapper_fn(w), listener)) {
        wrappers.remove(at);
        store_array(events, event, wrappers);
        if !is_remove_listener_event(event) {
            emit_meta(this, "removeListener", event, listener);
        }
        if collect_array(rts_core::entry::get_indexed(events, event)).is_empty() {
            forget_event_name(this, event);
        }
    }
    this
}

/// `emitter.removeAllListeners(eventName?)` — emits `'removeListener'` once
/// per listener actually removed, for the event(s) named.
extern "C" fn remove_all_listeners(_e: u64, this: u64, event: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = rts_core::entry::undefined_value();
    if event == absent {
        let names = collect_array(event_name_list(this));
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
    if !valid_event_name(event) {
        return;
    }
    let events = events_object(this);
    let array = rts_core::entry::get_indexed(events, event);
    let wrappers = collect_array(array);
    store_array(events, event, Vec::new());
    forget_event_name(this, event);
    if !is_remove_listener_event(event) {
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
    if remaining.len() != wrappers.len() {
        store_array(events, event, remaining);
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
    let names: Vec<u64> = collect_array(event_name_list(this))
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

/// `events.addAbortListener(signal, listener)`.
///
/// The callback is registered on the engine's global `AbortSignal` through its
/// ordinary EventTarget API, but carries an internal protected flag. EventTarget
/// delivers that registration after `stopImmediatePropagation`, which is the
/// security property Node's helper promises. A fixed native callable stores the
/// user's listener as its closure state, so disposal needs no runtime cache.
extern "C" fn add_abort_listener(
    _e: u64,
    _this: u64,
    signal: u64,
    listener: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    let absent = rts_core::entry::undefined_value();
    if !is_abort_signal(signal) {
        rts_core::entry::invalid_arg_instance("signal", "AbortSignal", signal);
        return absent;
    }
    let callable = rts_core::entry::with_runtime(|context| {
        rts_core::entry::is_callable_in(context, listener)
    });
    if !callable {
        rts_core::entry::invalid_arg_type("listener", "function", listener);
        return absent;
    }

    let callback = rts_core::entry::closure_new(
        abort_listener_callback as *const () as usize as i64,
        listener,
    );
    let options = rts_core::entry::with_runtime(|context| {
        let options = rts_core::entry::make_object(context);
        rts_core::entry::put_member(context, options, "once", rts_core::entry::boolean_value(true));
        rts_core::entry::put_member(
            context,
            options,
            "__nodeProtectedAbortListener__",
            rts_core::entry::boolean_value(true),
        );
        options
    });
    let add = rts_core::entry::get_indexed(signal, string_key("addEventListener"));
    rts_core::entry::call(add, signal, string_key("abort"), callback, options, absent);
    if rts_core::entry::thrown() != 0 {
        return absent;
    }

    let aborted = rts_core::entry::to_boolean(rts_core::entry::get_indexed(signal, string_key("aborted")));
    if aborted {
        let event = rts_core::entry::get_indexed(signal, string_key("__nodeAbortEvent__"));
        rts_core::entry::call(listener, signal, event, absent, absent, absent);
    }
    disposable(signal, callback)
}

/// The fixed callable stored by a protected EventTarget registration.
extern "C" fn abort_listener_callback(
    listener: u64,
    this: u64,
    event: u64,
    _b: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    let absent = rts_core::entry::undefined_value();
    rts_core::entry::call(listener, this, event, absent, absent, absent)
}

/// Build the object returned by `addAbortListener`.
fn disposable(signal: u64, callback: u64) -> u64 {
    let (object, dispose_key, dispose_fn) = rts_core::entry::with_runtime(|context| {
        let object = rts_core::entry::make_object(context);
        rts_core::entry::put_member(context, object, "__abortSignal__", signal);
        rts_core::entry::put_member(context, object, "__abortCallback__", callback);
        let dispose_key = rts_core::entry::well_known_symbol(context, "dispose");
        let dispose_fn = rts_core::entry::make_callable(context, dispose_abort_listener);
        (object, dispose_key, dispose_fn)
    });
    let object_root = rts_core::entry::hold_current(object);
    let dispose_root = rts_core::entry::hold_current(dispose_fn);
    rts_core::entry::set_indexed(object, dispose_key, dispose_fn, 0 /* strict: quem escreve a partir do host reporta a recusa */);
    rts_core::entry::release_current(dispose_root);
    rts_core::entry::release_current(object_root);
    object
}

/// `[Symbol.dispose]()` removes the protected registration. Repeated disposal
/// is idempotent because EventTarget removal is idempotent for a missing record.
extern "C" fn dispose_abort_listener(
    _e: u64,
    this: u64,
    _a0: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let absent = rts_core::entry::undefined_value();
    let signal = rts_core::entry::get_indexed(this, string_key("__abortSignal__"));
    let callback = rts_core::entry::get_indexed(this, string_key("__abortCallback__"));
    if signal == absent || callback == absent {
        return absent;
    }
    let remove = rts_core::entry::get_indexed(signal, string_key("removeEventListener"));
    rts_core::entry::call(remove, signal, string_key("abort"), callback, absent, absent);
    rts_core::entry::set_indexed(this, string_key("__abortSignal__"), absent, 0 /* strict: quem escreve a partir do host reporta a recusa */);
    rts_core::entry::set_indexed(this, string_key("__abortCallback__"), absent, 0 /* strict: quem escreve a partir do host reporta a recusa */);
    absent
}

fn is_abort_signal(value: u64) -> bool {
    let constructor = rts_core::entry::with_runtime(|context| {
        let global = rts_core::entry::global_object(context);
        rts_core::entry::get_member(context, global, "AbortSignal")
    });
    constructor != rts_core::entry::undefined_value()
        && rts_core::entry::instance_of(value, constructor)
}

/// `events.getMaxListeners(emitter)` — the module-level spelling of the
/// instance query. EventTarget is a separate ambient surface and remains
/// outside this module; EventEmitter-shaped objects use the shared query.
extern "C" fn static_get_max_listeners(
    _e: u64,
    _this: u64,
    emitter: u64,
    _b: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    get_max_listeners(0, emitter, 0, 0, 0, 0)
}

/// `emitter.getMaxListeners()` — the explicit `setMaxListeners()` value if
/// one was set, else [`DEFAULT_MAX_LISTENERS`].
extern "C" fn get_max_listeners(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = rts_core::entry::undefined_value();
    let stored = rts_core::entry::with_runtime(|context| {
        rts_core::entry::get_member(context, this, "__maxListeners__")
    });
    if stored == absent {
        rts_core::entry::make_number(f64::from_bits(DEFAULT_MAX_LISTENERS.load(Ordering::Relaxed)))
    } else {
        stored
    }
}

/// `emitter.setMaxListeners(n)` — records the validated limit. Warning
/// emission is still outside this module's current process-hook surface.
extern "C" fn set_max_listeners(_e: u64, this: u64, n: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(number) = valid_max_listeners(n) else {
        return rts_core::entry::undefined_value();
    };
    let value = rts_core::entry::make_number(number);
    rts_core::entry::with_runtime(|context| {
        rts_core::entry::put_member(context, this, "__maxListeners__", value);
    });
    this
}

/// Getter for the mutable module-level `defaultMaxListeners` property.
extern "C" fn default_max_listeners_get(
    _e: u64,
    _this: u64,
    _a0: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    rts_core::entry::make_number(f64::from_bits(DEFAULT_MAX_LISTENERS.load(Ordering::Relaxed)))
}

/// Setter for `events.defaultMaxListeners`, with the same non-negative numeric
/// boundary used by Node's listener-limit API.
extern "C" fn default_max_listeners_set(
    _e: u64,
    _this: u64,
    value: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let Some(number) = rts_core::entry::number_of(value) else {
        rts_core::entry::invalid_arg_type("defaultMaxListeners", "number", value);
        return rts_core::entry::undefined_value();
    };
    if number.is_nan() || number < 0.0 {
        rts_core::entry::out_of_range("defaultMaxListeners", ">= 0", value);
        return rts_core::entry::undefined_value();
    }
    DEFAULT_MAX_LISTENERS.store(number.to_bits(), Ordering::Relaxed);
    rts_core::entry::undefined_value()
}

/// Validate the numeric listener limit once for both instance and static
/// setters. `Infinity` is a valid Node value; only NaN and negative numbers
/// are rejected.
fn valid_max_listeners(value: u64) -> Option<f64> {
    let Some(number) = rts_core::entry::number_of(value) else {
        rts_core::entry::invalid_arg_type("n", "number", value);
        return None;
    };
    if number.is_nan() || number < 0.0 {
        rts_core::entry::out_of_range("n", ">= 0", value);
        return None;
    }
    Some(number)
}

/// `events.setMaxListeners(n, target?)` — with no target, changes
/// [`DEFAULT_MAX_LISTENERS`]; the native ABI exposes three target slots, so the
/// implementation accepts the corresponding variadic prefix and validates all
/// targets before writing any of them.
extern "C" fn static_set_max_listeners(
    _e: u64,
    _this: u64,
    n: u64,
    target: u64,
    target_b: u64,
    target_c: u64,
) -> u64 {
    let absent = rts_core::entry::undefined_value();
    let Some(number) = valid_max_listeners(n) else {
        return absent;
    };
    let targets = [target, target_b, target_c];
    if targets.iter().all(|&one| one == absent) {
        DEFAULT_MAX_LISTENERS.store(number.to_bits(), Ordering::Relaxed);
        return absent;
    }
    let mut valid_targets = Vec::new();
    for one in targets {
        if one == absent {
            continue;
        }
        let is_object = rts_core::entry::with_runtime(|context| {
            rts_core::entry::is_object(context, one)
        });
        if !is_object {
            rts_core::entry::invalid_arg_type(
                "eventTargets",
                "EventEmitter or EventTarget",
                one,
            );
            return absent;
        }
        valid_targets.push(one);
    }
    let value = rts_core::entry::make_number(number);
    for one in valid_targets {
        set_max_listeners(0, one, value, 0, 0, 0);
    }
    absent
}

/// Appends a `{ fn, once }` wrapper to an event's listener array, at the
/// front when `prepend` is set, and fires `'newListener'` first — before the
/// listener is actually reachable, matching real Node (a `'newListener'`
/// handler that itself emits the same event synchronously will not see the
/// not-yet-added listener).
fn add_listener(this: u64, event: u64, listener: u64, once: bool, prepend: bool) {
    if !valid_event_name(event) {
        return;
    }
    if !is_new_listener_event(event) {
        emit_meta(this, "newListener", event, listener);
    }
    let events = events_object(this);
    let array = rts_core::entry::get_indexed(events, event);
    let mut wrappers = collect_array(array);
    let was_empty = wrappers.is_empty();
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
    store_array(events, event, wrappers);
    if was_empty {
        remember_event_name(this, event);
    }
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

/// Replaces one event's listener array under its string or Symbol key.
fn store_array(events: u64, key: u64, wrappers: Vec<u64>) {
    let array = rts_core::entry::with_runtime(|context| {
        rts_core::entry::make_array_in(context, wrappers)
    });
    rts_core::entry::set_indexed(events, key, array, 0 /* strict: quem escreve a partir do host reporta a recusa */);
}

/// The ordered event names, including Symbols, that have ever had a listener.
fn event_name_list(this: u64) -> u64 {
    rts_core::entry::get_indexed(this, string_key("__eventNames__"))
}

/// Add an event name once, when its first listener is registered.
fn remember_event_name(this: u64, event: u64) {
    rts_core::entry::array_append(event_name_list(this), event);
}

/// Remove an event name after its listener array becomes empty.
fn forget_event_name(this: u64, event: u64) {
    let list = collect_array(event_name_list(this));
    let kept: Vec<u64> = list
        .into_iter()
        .filter(|&name| !rts_core::entry::strict_equals(name, event))
        .collect();
    rts_core::entry::with_runtime(|context| {
        let replacement = rts_core::entry::make_array_in(context, kept);
        rts_core::entry::put_member(context, this, "__eventNames__", replacement);
    });
}

/// Event names are strings or Symbols; objects are not coerced inside this native.
fn valid_event_name(event: u64) -> bool {
    rts_core::entry::text_of(event).is_some()
        || rts_core::entry::with_runtime(|context| rts_core::entry::is_symbol_in(context, event))
}

fn is_new_listener_event(event: u64) -> bool {
    rts_core::entry::text_of(event).as_deref() == Some("newListener")
}

fn is_remove_listener_event(event: u64) -> bool {
    rts_core::entry::text_of(event).as_deref() == Some("removeListener")
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
