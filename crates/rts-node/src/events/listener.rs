//! `on`/`once`/`off` and everything that reads the listener array back:
//! `listeners`, `rawListeners`, `listenerCount`, `eventNames`.
//!
//! # The once-wrapper, and what it is for
//!
//! [`once_wrapper`] is the piece this file adds that the module did not have:
//! a real invocable callable, minted with [`rts_core::entry::closure_new`],
//! that carries a `.listener` back-reference to the original function —
//! exactly the shape real Node's `rawListeners()` returns for a `.once()`
//! registration. `emit::emit` never calls it: `emit` already tracks
//! once-removal itself, against the `{ fn, once }` pair, before this file
//! existed. This wrapper exists purely so a program that reads
//! `rawListeners()` and calls what it finds there reaches the original
//! listener, and so `wrapper.listener === originalFn` holds the way Node's
//! own does.
//!
//! What calling the raw wrapper directly does NOT do: deregister itself from
//! the emitter. `emit()`'s removal happens against the internal `{ fn, once }`
//! record, which a direct call to the wrapper never touches — so
//! `rawListeners('x')[0]()` called by hand fires the listener again on a
//! later real `emit('x')`, where Node's own wrapper (which IS the stored
//! record) would not. Named here rather than silently diverging: a program
//! that grabs `rawListeners` to invoke by hand rather than to inspect is rare
//! enough that this crate's own reuse-check found no caller of the pattern in
//! this repository's corpus, and closing it needs the wrapper to also know
//! its own emitter and event name, which a fixed function pointer plus one
//! captured value cannot without a second allocation this module does not yet
//! have a use for.

use rts_core::entry;

/// `emitter.on(eventName, listener)` / `.addListener(...)`.
pub(super) extern "C" fn on(_e: u64, this: u64, event: u64, listener: u64, _c: u64, _d: u64) -> u64 {
    super::add_listener(this, event, listener, false, false);
    this
}

/// `emitter.once(eventName, listener)`.
pub(super) extern "C" fn once(_e: u64, this: u64, event: u64, listener: u64, _c: u64, _d: u64) -> u64 {
    super::add_listener(this, event, listener, true, false);
    this
}

/// `emitter.prependListener(eventName, listener)`.
pub(super) extern "C" fn prepend_listener(_e: u64, this: u64, event: u64, listener: u64, _c: u64, _d: u64) -> u64 {
    super::add_listener(this, event, listener, false, true);
    this
}

/// `emitter.prependOnceListener(eventName, listener)`.
pub(super) extern "C" fn prepend_once_listener(_e: u64, this: u64, event: u64, listener: u64, _c: u64, _d: u64) -> u64 {
    super::add_listener(this, event, listener, true, true);
    this
}

/// `emitter.off(eventName, listener)` / `.removeListener(...)` — removes at
/// most one matching registration, like real Node, and emits
/// `'removeListener'` with the original (unwrapped) function afterward.
///
/// Matches against EITHER the original function or the raw once-wrapper, so
/// `off(x)` works whether `x` is the function a program passed to `.once()`
/// or the wrapper `.rawListeners()` handed back for it — both name the same
/// registration now that [`once_wrapper`] makes the wrapper a real, distinct
/// value from the original.
pub(super) extern "C" fn remove_listener(_e: u64, this: u64, event: u64, listener: u64, _c: u64, _d: u64) -> u64 {
    if !super::valid_event_name(event) {
        return this;
    }
    let events = super::events_object(this);
    let array = entry::get_indexed(events, event);
    let mut wrappers = super::collect_array(array);
    let matches = |&w: &u64| {
        entry::strict_equals(super::wrapper_fn(w), listener) || entry::strict_equals(super::wrapper_raw(w), listener)
    };
    if let Some(at) = wrappers.iter().position(matches) {
        let original = super::wrapper_fn(wrappers[at]);
        wrappers.remove(at);
        super::store_array(events, event, wrappers);
        if !super::is_remove_listener_event(event) {
            super::emit_meta(this, "removeListener", event, original);
        }
        if super::collect_array(entry::get_indexed(events, event)).is_empty() {
            super::forget_event_name(this, event);
        }
    }
    this
}

/// `emitter.removeAllListeners(eventName?)` — emits `'removeListener'` once
/// per listener actually removed, for the event(s) named.
pub(super) extern "C" fn remove_all_listeners(_e: u64, this: u64, event: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    if event == absent {
        let names = super::collect_array(super::event_name_list(this));
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
    if !super::valid_event_name(event) {
        return;
    }
    let events = super::events_object(this);
    let array = entry::get_indexed(events, event);
    let wrappers = super::collect_array(array);
    super::store_array(events, event, Vec::new());
    super::forget_event_name(this, event);
    if !super::is_remove_listener_event(event) {
        for wrapper in wrappers {
            super::emit_meta(this, "removeListener", event, super::wrapper_fn(wrapper));
        }
    }
}

/// `emitter.listenerCount(eventName, listener?)`.
pub(super) extern "C" fn listener_count(_e: u64, this: u64, event: u64, listener: u64, _c: u64, _d: u64) -> u64 {
    let events = super::events_object(this);
    let array = entry::get_indexed(events, event);
    let wrappers = super::collect_array(array);
    let absent = entry::undefined_value();
    let count = if listener == absent {
        wrappers.len()
    } else {
        wrappers.iter().filter(|&&w| entry::strict_equals(super::wrapper_fn(w), listener)).count()
    };
    entry::make_number(count as f64)
}

/// `emitter.eventNames()` — the names holding at least one listener.
pub(super) extern "C" fn event_names(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let events = super::events_object(this);
    let live: Vec<u64> = super::collect_array(super::event_name_list(this))
        .into_iter()
        .filter(|&key| !super::collect_array(entry::get_indexed(events, key)).is_empty())
        .collect();
    // STRINGS first, then symbols, each group keeping its registration order.
    //
    // Node's `eventNames` is `Reflect.ownKeys(this._events)`, and that is a
    // property of the OBJECT the listeners hang on rather than of the order they
    // were registered in: own keys are integer indices in numeric order, then
    // strings by insertion, then symbols by insertion. This answered the raw
    // insertion list, so a symbol registered before a string came out in the
    // wrong place — `on(s1); on("b"); on(s2); on("a")` gave
    // `[s1, "b", s2, "a"]` where node gives `["b", "a", s1, s2]`.
    //
    // An index-like name goes in the first group, because `Reflect.ownKeys`
    // puts it there: `e.on("0", fn)` is a string key that IS a canonical array
    // index, and node lists it before the other strings.
    let (mut indices, mut strings, mut symbols) = (Vec::new(), Vec::new(), Vec::new());
    for key in live {
        match entry::with_runtime(|context| entry::string_in(context, key)) {
            Some(text) => match canonical_index(&text) {
                Some(at) => indices.push((at, key)),
                None => strings.push(key),
            },
            // Not a primitive string: a symbol, which is the only other thing a
            // registration can be keyed by.
            None => symbols.push(key),
        }
    }
    indices.sort_by_key(|(at, _)| *at);
    let names: Vec<u64> = indices
        .into_iter()
        .map(|(_, key)| key)
        .chain(strings)
        .chain(symbols)
        .collect();
    entry::make_array(names)
}

/// The array index a key spells, if it spells one exactly.
///
/// `"0"` is an index and `"01"`, `"1.0"` and `" 1"` are not — the round trip
/// through `to_string` is what says so, and it is the same test the language's
/// own canonical-index rule makes.
fn canonical_index(text: &str) -> Option<u32> {
    let at: u32 = text.parse().ok()?;
    (at.to_string() == text).then_some(at)
}

/// `events.listenerCount(emitter, eventName)` — the module-level form.
///
/// Deprecated in Node and still exported, and a program that calls it wants the
/// number rather than a lecture. The receiver moves from `this` to the first
/// argument and the rest is [`listener_count`]: counting listeners twice would
/// be two answers to one question, and the deprecated spelling is exactly the
/// one nobody would remember to keep in step.
pub(super) extern "C" fn static_listener_count(
    e: u64,
    _this: u64,
    emitter: u64,
    event: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    listener_count(e, emitter, event, 0, 0, 0)
}

/// `emitter.listeners(eventName)` — the original function for every entry,
/// `.once()`-registered ones included; see the module doc for why
/// [`raw_listeners`] differs.
pub(super) extern "C" fn listeners(_e: u64, this: u64, event: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let events = super::events_object(this);
    let array = entry::get_indexed(events, event);
    let functions: Vec<u64> = super::collect_array(array).into_iter().map(super::wrapper_fn).collect();
    entry::make_array(functions)
}

/// `emitter.rawListeners(eventName)` — the internal record: the original
/// function for a plain registration, the invocable [`once_wrapper`] for a
/// `.once()` one.
pub(super) extern "C" fn raw_listeners(_e: u64, this: u64, event: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let events = super::events_object(this);
    let array = entry::get_indexed(events, event);
    let functions: Vec<u64> = super::collect_array(array).into_iter().map(super::wrapper_raw).collect();
    entry::make_array(functions)
}

/// `events.getEventListeners(emitterOrTarget, eventName)` — the module-level
/// static, same answer as the instance method.
pub(super) extern "C" fn get_event_listeners(e: u64, _this: u64, emitter: u64, event: u64, _c: u64, _d: u64) -> u64 {
    listeners(e, emitter, event, 0, 0, 0)
}

/// Mints the real once-wrapper [`super::add_listener`] stores as `raw` for a
/// `.once()` registration — see the module doc for what calling it does and
/// does not do.
pub(super) fn once_wrapper(listener: u64) -> u64 {
    let wrapper = entry::closure_new(once_wrapper_call as *const () as usize as i64, listener);
    entry::with_runtime(|context| {
        entry::put_member(context, wrapper, "listener", listener);
    });
    wrapper
}

/// The code [`once_wrapper`]'s closure runs: forwards every argument to the
/// original listener it was minted over. `Provided`'s shape — `(environment,
/// this, a0, a1, a2, a3)` — is exactly what [`entry::closure_new`] arranges to
/// call this with, `environment` being the original listener.
extern "C" fn once_wrapper_call(listener: u64, this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    entry::call(listener, this, a0, a1, a2, a3)
}
