//! `EventEmitter` as a GLOBAL — reachable with no import line.
//!
//! # Reuse-check: what this is and is not reusing
//!
//! `crates/rts-node-rwk/src/events.rs` already has a complete `EventEmitter`,
//! installed as `node:events`' named export, read in full before writing this.
//! Its **storage shape** — one own property (`__events__` there) holding an
//! array of `{ fn, once }` records per event name, `once` records dropped
//! before any of them runs so a re-entrant `emit` cannot see one twice, and
//! `'error'` with zero listeners printing and exiting the process because
//! nothing here can raise a catchable error a handler could ever see — is
//! reused **as a design**, because it is right and this module needs nothing
//! different from it.
//!
//! What is NOT reused is the code itself: that module lives in `rts-node-rwk`,
//! which `rts-std-rwk` does not and must not depend on (`Cargo.toml` names
//! `rts-core-rwk` only — the crate boundary this workspace's layering draws).
//! `events::mod`'s own reuse-check, for `EventTarget`, already states the twin
//! of this situation and the reason a shared implementation was rejected there:
//! two call sites are two contracts, and a real second dependency edge between
//! two sibling crates is worse than one written-out copy of an already-small
//! module. This is not a second copy of a *different* contract the way
//! `EventTarget` is — it is the same `EventEmitter` contract, reachable by two
//! paths (`new EventEmitter()` with no import, and `require("node:events")`).
//! Unifying them is future work for whoever next touches either file; it is
//! named here rather than done silently, per this task's instructions not to
//! step into `rts-node-rwk`.
//!
//! Every test this module was written against
//! (`tests/eventemitter_global.test.ts`, `tests/node_events_full.test.ts`) uses
//! the emitter with **no import at all** — `new EventEmitter()` reaching the
//! global directly — so no import-resolution path exercises the duplication.

use rts_core_rwk::entry::{self, Context, Provided};

/// The own property holding one array per event name — see the module doc for
/// why this is `__events__` (matching `node:events`) rather than the sibling
/// `EventTarget` module's `__listeners__`: this module IS that contract.
const STORE: &str = "__events__";

/// `EventEmitter.defaultMaxListeners`, process-wide.
static DEFAULT_MAX_LISTENERS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(10);

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

/// Installs `EventEmitter` as a global.
pub fn install(context: &mut Context) {
    let prototype = entry::make_prototype(context, "GlobalEventEmitter", METHODS);
    let ctor = entry::make_callable(context, construct);
    entry::put_member(context, ctor, "prototype", prototype);
    entry::declare_global(context, "EventEmitter", ctor);
}

/// `new EventEmitter(async?)` — the `async` argument is accepted (Node's own
/// `EventEmitterAsyncResource` shape leaks one in some call sites) and ignored:
/// nothing here schedules anything, so there is no synchronous/asynchronous
/// distinction to keep.
extern "C" fn construct(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "GlobalEventEmitter", METHODS);
        // `new` hands over an object already linked to `prototype`; a plain call
        // hands over `undefined`, and answering an instance anyway is ordinary
        // JS semantics (a constructor returning an object wins over the
        // implicit `this`), which is what makes `EventEmitter()` work too.
        let emitter = match entry::is_object(context, this) {
            true => this,
            false => entry::make_instance(context, prototype),
        };
        let events = entry::make_object(context);
        entry::put_member(context, emitter, STORE, events);
        emitter
    })
}

extern "C" fn on(_e: u64, this: u64, event: u64, listener: u64, _c: u64, _d: u64) -> u64 {
    add_listener(this, event, listener, false, false);
    this
}

extern "C" fn once(_e: u64, this: u64, event: u64, listener: u64, _c: u64, _d: u64) -> u64 {
    add_listener(this, event, listener, true, false);
    this
}

extern "C" fn prepend_listener(_e: u64, this: u64, event: u64, listener: u64, _c: u64, _d: u64) -> u64 {
    add_listener(this, event, listener, false, true);
    this
}

extern "C" fn prepend_once_listener(_e: u64, this: u64, event: u64, listener: u64, _c: u64, _d: u64) -> u64 {
    add_listener(this, event, listener, true, true);
    this
}

extern "C" fn remove_listener(_e: u64, this: u64, event: u64, listener: u64, _c: u64, _d: u64) -> u64 {
    let Some(name) = entry::text_of(event) else { return this };
    let events = events_object(this);
    let array = get(events, &name);
    let mut wrappers = collect_array(array);
    if let Some(at) = wrappers.iter().position(|&w| entry::strict_equals(wrapper_fn(w), listener)) {
        wrappers.remove(at);
        store_array(events, &name, wrappers);
        if name != "removeListener" {
            emit_meta(this, "removeListener", event, listener);
        }
    }
    this
}

extern "C" fn remove_all_listeners(_e: u64, this: u64, event: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    if event == absent() {
        let names: Vec<u64> = collect_array(entry::own_keys(events_object(this)));
        for key in names {
            remove_all_for(this, key);
        }
    } else {
        remove_all_for(this, event);
    }
    this
}

fn remove_all_for(this: u64, event: u64) {
    let Some(name) = entry::text_of(event) else { return };
    let events = events_object(this);
    let wrappers = collect_array(get(events, &name));
    store_array(events, &name, Vec::new());
    if name != "removeListener" {
        for wrapper in wrappers {
            emit_meta(this, "removeListener", event, wrapper_fn(wrapper));
        }
    }
}

/// `emitter.emit(eventName, ...args)` — up to three args, this module's four
/// call slots minus the receiver and the event name.
extern "C" fn emit(_e: u64, this: u64, event: u64, a0: u64, a1: u64, a2: u64) -> u64 {
    let events = events_object(this);
    let Some(name) = entry::text_of(event) else {
        return entry::boolean_value(false);
    };
    let wrappers = collect_array(get(events, &name));
    if wrappers.is_empty() {
        if name == "error" {
            crash_on_unhandled_error(a0);
        }
        return entry::boolean_value(false);
    }
    let remaining: Vec<u64> = wrappers.iter().copied().filter(|&w| !wrapper_once(w)).collect();
    if remaining.len() != wrappers.len() {
        store_array(events, &name, remaining);
    }
    let absent = absent();
    for wrapper in wrappers {
        entry::call(wrapper_fn(wrapper), this, a0, a1, a2, absent);
    }
    entry::boolean_value(true)
}

/// The same diagnostic-then-exit `node:events`' `emit` gives; see that
/// module's doc for why a native ends the process rather than throwing.
fn crash_on_unhandled_error(error: u64) -> ! {
    match entry::described(error) {
        Some(text) => eprintln!("rts: uncaught 'error' event: {text}"),
        None => eprintln!("rts: uncaught 'error' event: an object"),
    }
    std::process::exit(1)
}

extern "C" fn listener_count(_e: u64, this: u64, event: u64, listener: u64, _c: u64, _d: u64) -> u64 {
    let Some(name) = entry::text_of(event) else { return entry::make_number(0.0) };
    let wrappers = collect_array(get(events_object(this), &name));
    let count = if listener == absent() {
        wrappers.len()
    } else {
        wrappers.iter().filter(|&&w| entry::strict_equals(wrapper_fn(w), listener)).count()
    };
    entry::make_number(count as f64)
}

extern "C" fn event_names(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let events = events_object(this);
    let names: Vec<u64> = collect_array(entry::own_keys(events))
        .into_iter()
        .filter(|&key| {
            let Some(name) = entry::text_of(key) else { return false };
            !collect_array(get(events, &name)).is_empty()
        })
        .collect();
    entry::make_array(names)
}

extern "C" fn listeners(_e: u64, this: u64, event: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(name) = entry::text_of(event) else { return entry::make_array(Vec::new()) };
    let functions: Vec<u64> = collect_array(get(events_object(this), &name))
        .into_iter()
        .map(wrapper_fn)
        .collect();
    entry::make_array(functions)
}

extern "C" fn get_max_listeners(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let stored = get(this, "__maxListeners__");
    if stored == absent() {
        entry::make_number(DEFAULT_MAX_LISTENERS.load(std::sync::atomic::Ordering::Relaxed) as f64)
    } else {
        stored
    }
}

extern "C" fn set_max_listeners(_e: u64, this: u64, n: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    put(this, "__maxListeners__", n);
    this
}

/// Appends a `{ fn, once }` wrapper, firing `'newListener'` first — before the
/// listener is reachable, matching real Node.
fn add_listener(this: u64, event: u64, listener: u64, once: bool, prepend: bool) {
    let Some(name) = entry::text_of(event) else { return };
    if name != "newListener" {
        emit_meta(this, "newListener", event, listener);
    }
    let events = events_object(this);
    let mut wrappers = collect_array(get(events, &name));
    let wrapper = entry::with_runtime(|context| {
        let object = entry::make_object(context);
        entry::put_member(context, object, "fn", listener);
        entry::put_member(context, object, "once", entry::boolean_value(once));
        object
    });
    if prepend {
        wrappers.insert(0, wrapper);
    } else {
        wrappers.push(wrapper);
    }
    store_array(events, &name, wrappers);
}

fn emit_meta(this: u64, meta: &str, event: u64, listener: u64) {
    let events = events_object(this);
    let wrappers = collect_array(get(events, meta));
    if wrappers.is_empty() {
        return;
    }
    let absent = absent();
    for wrapper in wrappers {
        entry::call(wrapper_fn(wrapper), this, event, listener, absent, absent);
    }
}

fn events_object(this: u64) -> u64 {
    get(this, STORE)
}

fn wrapper_fn(wrapper: u64) -> u64 {
    get(wrapper, "fn")
}

fn wrapper_once(wrapper: u64) -> bool {
    flag(wrapper, "once")
}

fn store_array(events: u64, name: &str, wrappers: Vec<u64>) {
    entry::with_runtime(|context| {
        let array = entry::make_array_in(context, wrappers);
        entry::put_member(context, events, name, array);
    });
}

/// A JS array's elements, read through `.length` and indexed access.
fn collect_array(array: u64) -> Vec<u64> {
    if array == absent() {
        return Vec::new();
    }
    let length = entry::number_of(get(array, "length")).map(|n| n as usize).unwrap_or(0);
    (0..length)
        .map(|index| entry::get_indexed(array, entry::make_number(index as f64)))
        .collect()
}

/// `undefined`.
fn absent() -> u64 {
    entry::undefined_value()
}

/// One property of an object, by name, from outside a borrow.
fn get(object: u64, name: &str) -> u64 {
    entry::get_indexed(object, string(name))
}

/// Writes one property of an object, by name, from outside a borrow.
fn put(object: u64, name: &str, value: u64) {
    entry::with_runtime(|context| entry::put_member(context, object, name, value));
}

/// `ToBoolean` of one property of an object.
fn flag(object: u64, name: &str) -> bool {
    entry::to_boolean(get(object, name))
}

/// An interned string, from outside a borrow.
fn string(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}
