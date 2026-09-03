//! `node:events` — `EventEmitter`, built on one real prototype, plus the
//! promise/iterator surface that waits on it.
//!
//! # Why this is a folder and not one file
//!
//! It was one file of 769 lines, which is already over this workspace's
//! 500-line ceiling outside the two engine crates — so the two additions this
//! change makes (`events.once`, `events.on`) could not land as more of the
//! same file. Split into cohesive pieces instead, each named by what it owns:
//! [`listener`] (registration and the `.once()` wrapper), [`emit`] (firing),
//! [`abort`] (`addAbortListener`), [`max_listeners`] (the listener-count
//! ceiling), [`once_promise`] (`events.once`) and [`on_iterator`]
//! (`events.on`). This file keeps only what every one of them needs: the
//! shared `EventEmitter` shape and the storage helpers over it.
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
//! `new EventEmitter()` and plain `EventEmitter()` still both work: this is
//! ordinary JS semantics (a constructor returning an object wins over the
//! implicit `this`), not a special case this module adds.
//!
//! Listener storage: an ordinary own property, `__events__`, holding one array
//! per event name; each array element is a small `{ fn, once, raw }` object —
//! `fn` is always the original listener a program passed in, `once` is the
//! flag `.once()` set, and `raw` is what [`listener::raw_listeners`] hands
//! back (see [`add_listener`] for why it is built eagerly rather than on
//! demand).
//!
//! # Chaining onto `"EventEmitter"` is not enough — TWO real defect classes
//! found across this crate, both fixed
//!
//! `Domain` (`crate::domain`) and the `cluster` module object/its worker
//! instances (`crate::cluster`) each chained onto a same-named
//! `"EventEmitter"` prototype for the METHODS but never ran [`make_emitter`]
//! — the real `new EventEmitter()` constructor, which is the ONLY thing that
//! ever built the instance's OWN `__events__`/`__eventNames__` before this
//! pass. Every method here reads or writes those as an OWN property with no
//! fallback, so `.on(...)` on either wrote into `get_indexed(undefined, …)`
//! and `.emit(...)` found zero listeners for EVERY event, `'error'` included
//! — silent, and for `Domain` specifically, its entire reason to exist.
//! `repl.REPLServer`'s own `server` object (`crate::repl`) had the identical
//! gap for the same reason. All three now build both properties inline at
//! construction; see each module's own doc.
//!
//! A second, narrower class: several OTHER construction sites built
//! `__events__` but not `__eventNames__` — [`listener::event_names`]
//! and no-argument `removeAllListeners` read `__eventNames__` specifically,
//! so `.on`/`.emit`/`.listenerCount` all worked while `.eventNames()`
//! answered `[]` forever, silently. Found (and fixed) in the shared
//! `init_emitter` helpers `net`/`http`/`https`/`stream`/`tls::common` each
//! duplicate, in `http2::js::instance_of` (which `http2::delivery` now calls
//! instead of its own second, independently-incomplete copy), and at
//! individual construction sites in `process`, `readline`, `worker_threads`,
//! `inspector`, `ws`, `fs::watch`, `fs::utf8stream` and
//! `child_process::stdio`. `child_process::spawn_async`'s `ChildProcess` has
//! the same gap, named but left unfixed in `child_process`'s own module doc:
//! that file was already at this workspace's 500-line ceiling.
//!
//! # The borrow every module here has to get right
//!
//! [`emit::emit`] stores functions and later calls them. Every read here —
//! [`collect_array`], [`rts_core::entry::get_indexed`] — finishes and drops
//! its borrow before returning, because each is one call to an entry point
//! that opens and closes the runtime borrow itself. Calling a listener from
//! inside a held borrow is not a bug this module produces slowly; it is an
//! abort on the first call. [`once_promise`] and [`on_iterator`] carry the
//! same rule one level further: [`rts_core::entry::closure_new`] and
//! [`rts_core::entry::promise_new`] each take the borrow themselves, so both
//! are minted OUTSIDE any `with_runtime` block already open — see
//! [`add_listener`] for where that already mattered before this change.
//!
//! **This claim was false for one function until 2026-09**, and the fixture
//! that found it (`tests/claude-node-events-once-crash.test.ts`) is the
//! reason it is stated this carefully now rather than just asserted:
//! [`once_promise`]'s `on_event` called [`packed_args`] — which calls the
//! ambient `entry::undefined_value` — from INSIDE the `with_runtime` reading
//! `state.promise`, so `events.once(e, 'x')` aborted the process on every
//! SUCCESSFUL resolution (rejection settles through a different listener and
//! was never affected). [`on_iterator`]'s own `on_event` already called
//! `packed_args` outside any borrow, which is what made the two comparable
//! side by side and is why the fix moved the call rather than rewriting
//! `packed_args` itself.
//!
//! # `'error'` with no listener
//!
//! Real Node throws the error value, prints it, and exits the process when
//! `'error'` is emitted against zero `'error'` listeners. A native entry
//! point here cannot throw a value the compiled caller could catch and
//! recover a program from — [`crate::assert`] documents the same limit: no
//! protected region exists for it to unwind into. So [`emit::emit`] does not
//! throw either, but it also does not swallow the error silently, which would
//! be worse than either real behavior: it prints the same diagnostic real
//! Node would and ends the process the same way, via `std::process::exit`,
//! which needs no unwind and so is safe to call from a native. `events.once`
//! sidesteps this entirely for the event it names, and NOT for any other
//! `'error'` a program never awaited — see [`once_promise`]'s own doc.
//!
//! # Not implemented, by name
//!
//! `EventEmitterAsyncResource`, `NodeEventTarget` — narrower Node-internal
//! shapes over the same two ideas this module already has (`EventEmitter` and
//! the DOM `EventTarget` re-exported below); nothing in this crate's P0/P1
//! target set imports either by name.
//!
//! `captureRejections`, `errorMonitor`, `captureRejectionSymbol`, and
//! `MaxListenersExceededWarning` remain absent — the listener-limit setters
//! record values but do not warn, and a listener's rejected promise is not
//! observed. The full unhandled-`'error'` process-hook path (`errorMonitor`
//! running first, `captureRejectionSymbol` as an override) remains a larger
//! semantic gap than this change closes.
//!
//! `options.close`, `options.highWaterMark`, `options.lowWaterMark` on
//! `events.on` — see [`on_iterator`]'s own doc for why backpressure is not
//! wired.

mod abort;
mod emit;
mod listener;
mod max_listeners;
mod on_iterator;
mod once_promise;

use rts_core::entry::{self, Context, Provided};

/// The instance methods every `EventEmitter` shares through one prototype.
const METHODS: &[(&str, Provided)] = &[
    ("on", listener::on),
    ("addListener", listener::on),
    ("once", listener::once),
    ("prependListener", listener::prepend_listener),
    ("prependOnceListener", listener::prepend_once_listener),
    ("off", listener::remove_listener),
    ("removeListener", listener::remove_listener),
    ("removeAllListeners", listener::remove_all_listeners),
    ("emit", emit::emit),
    ("listenerCount", listener::listener_count),
    ("eventNames", listener::event_names),
    ("listeners", listener::listeners),
    ("rawListeners", listener::raw_listeners),
    ("getMaxListeners", max_listeners::get_max_listeners),
    ("setMaxListeners", max_listeners::set_max_listeners),
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
/// installs natives on it every time it is called. So the surface was built
/// three times on every startup and two of them were thrown away after one
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
    entry::make_prototype(context, "EventEmitter", METHODS)
}

/// The namespace `node:events` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("EventEmitter", make_emitter),
        ("getEventListeners", listener::get_event_listeners),
        ("getMaxListeners", max_listeners::static_get_max_listeners),
        ("setMaxListeners", max_listeners::static_set_max_listeners),
        ("addAbortListener", abort::add_abort_listener),
        ("once", once_promise::once),
        ("on", on_iterator::on),
    ];
    let namespace = entry::make_namespace(context, members);
    // `defaultMaxListeners` is a mutable module property in Node, not a copy of
    // the current value. Keep it backed by the same atomic used by every
    // emitter's fallback so reads and writes observe one setting.
    entry::define_accessor_in(
        context,
        namespace,
        "defaultMaxListeners",
        max_listeners::default_get,
        Some(max_listeners::default_set),
    );
    // The constructor carries `prototype`, which is what makes `new` link the
    // object it allocates to it. Returning an instance from the constructor is
    // NOT enough: `new` over a native keeps the object it made, so every
    // emitter came back with no prototype and no `on` at all — which the
    // agent's own tests did not reach and a fixture calling `emit` did.
    let prototype = emitter_prototype(context);
    let constructor = entry::get_member(context, namespace, "EventEmitter");
    entry::put_member(context, constructor, "prototype", prototype);
    // The `prototype.constructor` back-link — see `crate::stream::class_ctor`'s
    // doc for why a hand-built native constructor needs this said explicitly
    // (`new EventEmitter().constructor.name` read `"Object"` without it, and
    // every class chained onto this prototype — `Readable`, `Duplex`, … —
    // inherited that same wrong answer until each got its OWN back-link).
    entry::declare_host_class(context, constructor, prototype, "EventEmitter", 0);
    // Node's CommonJS export is the constructor itself and also exposes that
    // constructor as `.EventEmitter`, which named destructuring reads.
    entry::put_member(context, constructor, "EventEmitter", constructor);
    // `EventTarget`, `Event`, `CustomEvent` — Node re-exports the SAME WHATWG
    // globals `rts-std` already installs (`globals/events/mod.rs`), not a
    // second set. Reached by name off `globalThis` rather than built here:
    // building would make `new (require('node:events').EventTarget)()
    // instanceof EventTarget` false, which real Node keeps true because it is
    // one class either way.
    let global = entry::global_object(context);
    for name in ["EventTarget", "Event", "CustomEvent"] {
        let class = entry::get_member(context, global, name);
        entry::put_member(context, namespace, name, class);
        entry::put_member(context, constructor, name, class);
    }
    // CommonJS returns the constructor itself (`require("events")`), whose
    // static helpers mirror the module namespace in Node. Keep both views on
    // the same callable cells rather than rebuilding any function.
    for name in [
        "getEventListeners",
        "getMaxListeners",
        "setMaxListeners",
        "addAbortListener",
        "once",
        "on",
    ] {
        let member = entry::get_member(context, namespace, name);
        entry::put_member(context, constructor, name, member);
    }
    entry::define_accessor_in(
        context,
        constructor,
        "defaultMaxListeners",
        max_listeners::default_get,
        Some(max_listeners::default_set),
    );
    namespace
}

/// `new EventEmitter()` — see the module doc for why calling it plainly also
/// works, and for what sharing one prototype across every instance changes.
extern "C" fn make_emitter(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "EventEmitter", METHODS);
        // `new` hands over an object already linked to `prototype`; a plain
        // call hands over `undefined` and this makes one. Both answer something
        // with the methods on it, which is what lets `EventEmitter()` work.
        let emitter = match entry::is_object(context, this) {
            true => this,
            false => entry::make_instance(context, prototype),
        };
        let events = entry::make_object(context);
        entry::put_member(context, emitter, "__events__", events);
        let event_names = entry::make_array_in(context, Vec::new());
        entry::put_member(context, emitter, "__eventNames__", event_names);
        emitter
    })
}

/// Appends a `{ fn, once, raw }` wrapper to an event's listener array, at the
/// front when `prepend` is set, and fires `'newListener'` first — before the
/// listener is actually reachable, matching real Node (a `'newListener'`
/// handler that itself emits the same event synchronously will not see the
/// not-yet-added listener).
///
/// # Why `raw` is built here, eagerly, rather than by `rawListeners()` on
/// demand
///
/// Node's `rawListeners()` returns the SAME wrapper object across repeated
/// calls — it is not a fresh view, it is what is actually stored. A wrapper
/// built lazily inside `rawListeners()` would answer a *different* callable
/// each call, so `emitter.rawListeners('x')[0] ===
/// emitter.rawListeners('x')[0]` would be `false` where Node answers `true`.
/// Building it once, here, at registration time, is what keeps that identity
/// stable — the same reason [`once_promise`] and [`on_iterator`] mint their
/// promise before doing anything else that could observe it.
///
/// For a plain (non-`once`) registration `raw` is just `listener` again: real
/// Node's internal array holds the bare function for those, so `rawListeners`
/// and `listeners` already agree without a wrapper.
pub(super) fn add_listener(this: u64, event: u64, listener: u64, once: bool, prepend: bool) {
    if !valid_event_name(event) {
        return;
    }
    if !is_new_listener_event(event) {
        emit_meta(this, "newListener", event, listener);
    }
    // Built OUTSIDE any borrow: `closure_new` takes the runtime borrow itself,
    // and building it while the `with_runtime` below still holds one aborts
    // the process rather than failing — the rule this file's own doc states.
    let raw = match once {
        true => listener::once_wrapper(listener),
        false => listener,
    };
    let events = events_object(this);
    let array = entry::get_indexed(events, event);
    let mut wrappers = collect_array(array);
    let was_empty = wrappers.is_empty();
    let wrapper = entry::with_runtime(|context| {
        let object = entry::make_object(context);
        entry::put_member(context, object, "fn", listener);
        entry::put_member(context, object, "once", entry::boolean_value(once));
        entry::put_member(context, object, "raw", raw);
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
/// listener storage, bypassing [`emit::emit`]'s `'error'`-crash special case —
/// neither meta-event is `'error'`, so that branch never applies, but going
/// through `emit` itself would also re-derive `events_object` for no reason.
pub(super) fn emit_meta(this: u64, meta: &str, event: u64, listener: u64) {
    let events = events_object(this);
    let key = string_key(meta);
    let array = entry::get_indexed(events, key);
    let wrappers = collect_array(array);
    if wrappers.is_empty() {
        return;
    }
    let absent = entry::undefined_value();
    for wrapper in wrappers {
        entry::call(wrapper_fn(wrapper), this, event, listener, absent, absent);
    }
}

/// The `__events__` object an emitter carries.
pub(super) fn events_object(this: u64) -> u64 {
    entry::get_indexed(this, string_key("__events__"))
}

/// The original listener a `{ fn, once, raw }` wrapper holds — what
/// `.listeners()` answers, and what `.off()` matches against.
pub(super) fn wrapper_fn(wrapper: u64) -> u64 {
    entry::get_indexed(wrapper, string_key("fn"))
}

/// What `.rawListeners()` answers: the original for a plain registration, the
/// invocable once-wrapper (carrying `.listener`) for a `.once()` one.
pub(super) fn wrapper_raw(wrapper: u64) -> u64 {
    entry::get_indexed(wrapper, string_key("raw"))
}

/// Whether a wrapper is a `.once()` registration.
pub(super) fn wrapper_once(wrapper: u64) -> bool {
    entry::to_boolean(entry::get_indexed(wrapper, string_key("once")))
}

/// Replaces one event's listener array under its string or Symbol key.
pub(super) fn store_array(events: u64, key: u64, wrappers: Vec<u64>) {
    let array = entry::with_runtime(|context| entry::make_array_in(context, wrappers));
    entry::set_indexed(events, key, array, 0 /* strict: quem escreve a partir do host reporta a recusa */);
}

/// The ordered event names, including Symbols, that have ever had a listener.
pub(super) fn event_name_list(this: u64) -> u64 {
    entry::get_indexed(this, string_key("__eventNames__"))
}

/// Add an event name once, when its first listener is registered.
pub(super) fn remember_event_name(this: u64, event: u64) {
    entry::array_append(event_name_list(this), event);
}

/// Remove an event name after its listener array becomes empty.
pub(super) fn forget_event_name(this: u64, event: u64) {
    let list = collect_array(event_name_list(this));
    let kept: Vec<u64> = list.into_iter().filter(|&name| !entry::strict_equals(name, event)).collect();
    entry::with_runtime(|context| {
        let replacement = entry::make_array_in(context, kept);
        entry::put_member(context, this, "__eventNames__", replacement);
    });
}

/// Event names are strings or Symbols; objects are not coerced inside this
/// native.
pub(super) fn valid_event_name(event: u64) -> bool {
    entry::text_of(event).is_some() || entry::with_runtime(|context| entry::is_symbol_in(context, event))
}

pub(super) fn is_new_listener_event(event: u64) -> bool {
    entry::text_of(event).as_deref() == Some("newListener")
}

pub(super) fn is_remove_listener_event(event: u64) -> bool {
    entry::text_of(event).as_deref() == Some("removeListener")
}

/// A JS array's elements, read out through `.length` and indexed access.
pub(super) fn collect_array(array: u64) -> Vec<u64> {
    let absent = entry::undefined_value();
    if array == absent {
        return Vec::new();
    }
    let length_value = entry::get_indexed(array, string_key("length"));
    // Asked of the value, not of its text: reading a number by parsing its
    // decimal back is lossy where a double's shortest decimal is not the double.
    let length = entry::number_of(length_value).map(|value| value as usize).unwrap_or(0);
    (0..length)
        .map(|index| entry::get_indexed(array, entry::make_number(index as f64)))
        .collect()
}

/// An interned string, for use as a property key from outside a borrow.
pub(super) fn string_key(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}

/// `[a0, a1, a2]` with any trailing `undefined` dropped — what
/// [`once_promise`] and [`on_iterator`] both resolve a listener's arguments
/// into. This module's calling convention pads a shorter argument list with
/// `undefined`, so a REAL trailing `undefined` a program passed to `emit` is
/// indistinguishable from one the ABI added — the same limit [`emit::emit`]'s
/// own doc states for the three-argument cap itself.
pub(super) fn packed_args(a0: u64, a1: u64, a2: u64) -> Vec<u64> {
    let absent = entry::undefined_value();
    let mut args = vec![a0, a1, a2];
    while args.last() == Some(&absent) {
        args.pop();
    }
    args
}

/// Whether `value` is a callable EventEmitter — has a callable `.on`. Used by
/// [`once_promise::once`] and [`on_iterator::on`] to refuse anything else
/// loudly (`ERR_INVALID_ARG_TYPE`) rather than registering a listener onto
/// storage nothing reads — see [`once_promise`]'s own doc for why a real
/// `EventTarget` is one such case and is refused rather than silently inert.
pub(super) fn is_emitter_like(emitter: u64) -> bool {
    entry::with_runtime(|context| {
        if !entry::is_object(context, emitter) {
            return false;
        }
        let on = entry::get_member(context, emitter, "on");
        entry::is_callable_in(context, on)
    })
}
