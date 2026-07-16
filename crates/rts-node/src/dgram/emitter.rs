//! node:dgram — the `Socket`'s `EventEmitter` surface.
//!
//! `dgram.Socket extends EventEmitter`, and `rts-node` is independent of
//! `rts-std` (where the canonical `EventEmitter` class lives), so the emitter is
//! implemented here over the socket's own listener table — as
//! docs/node-implementation/dgram.md §5.6 prescribes. It is the same model
//! (ordered per-event listener lists, `once`, prepend, max-listeners), and the
//! listener handles are GC-pinned for as long as they are registered.
//!
//! Delivery itself is NOT here: an event produced by the OS (an inbound
//! datagram) is queued and drained by `pump.rs` on the event-loop thread.

use rts_engine::heap::handles::{alloc_entry, Entry};

use super::state::{self, Listener, SockEvent};
use crate::values::{val, Val};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_PIN_HANDLE(handle: u64);
    fn __RTS_FN_NS_GC_UNPIN_HANDLE(handle: u64);
}

/// Register `cb` for `event`. `once` → dropped after one delivery; `prepend` →
/// inserted at the front (Node's `prependListener`).
pub fn add(this: u64, event: &str, cb: u64, once: bool, prepend: bool) {
    let Some(st) = state::get(this) else {
        return;
    };
    // Pinned while registered — the pump invokes it long after the JS frame that
    // created it is gone.
    unsafe { __RTS_FN_NS_GC_PIN_HANDLE(cb) };
    let mut lst = st.listeners.lock().unwrap();
    let slot = lst.slot(event);
    let entry = Listener { cb, once };
    if prepend {
        slot.insert(0, entry);
    } else {
        slot.push(entry);
    }
}

/// The `(this, event, listener)` add-shaped methods, distinguished by their
/// `once`/`prepend` flags. `listener` arrives as a `PolyValue` word.
fn add_word(this: u64, ep: *const u8, el: i64, listener: u64, once: bool, prepend: bool) -> u64 {
    if let Val::Func(cb) = val(listener) {
        add(this, &crate::values::read(ep, el), cb, once, prepend);
    }
    this
}

/// `socket.on(event, listener)` / `addListener`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_ON(this: u64, ep: *const u8, el: i64, l: u64) -> u64 {
    add_word(this, ep, el, l, false, false)
}

/// `socket.once(event, listener)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_ONCE(this: u64, ep: *const u8, el: i64, l: u64) -> u64 {
    add_word(this, ep, el, l, true, false)
}

/// `socket.prependListener(event, listener)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_PREPEND(this: u64, ep: *const u8, el: i64, l: u64) -> u64 {
    add_word(this, ep, el, l, false, true)
}

/// `socket.prependOnceListener(event, listener)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_PREPEND_ONCE(this: u64, ep: *const u8, el: i64, l: u64) -> u64 {
    add_word(this, ep, el, l, true, true)
}

/// The IDENTITY of a listener value for `off`/`removeListener`. Each pass of a
/// function through the ABI reifies a FRESH `Entry::Function` slot, so comparing
/// handles never matches — the stable identity of a named fn / non-capturing
/// arrow is its underlying CODE pointer. A capture-carrying closure keeps its own
/// handle (JS semantics: two separately-created closures are distinct listeners).
fn identity(cb: u64) -> u64 {
    rts_engine::heap::handles::with_entry(cb, |e| match e {
        Some(Entry::Function(f)) if f.bound_args.is_empty() => Some(f.fn_ptr),
        _ => None,
    })
    .unwrap_or(cb)
}

/// `socket.off(event, listener)` / `removeListener`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_OFF(this: u64, ep: *const u8, el: i64, l: u64) -> u64 {
    let Some(st) = state::get(this) else {
        return this;
    };
    let Val::Func(cb) = val(l) else {
        return this;
    };
    let event = crate::values::read(ep, el);
    let key = identity(cb);
    let mut removed = Vec::new();
    {
        let mut lst = st.listeners.lock().unwrap();
        let slot = lst.slot(&event);
        slot.retain(|entry| {
            let hit = identity(entry.cb) == key;
            if hit {
                removed.push(entry.cb);
            }
            !hit
        });
    }
    for cb in removed {
        unsafe { __RTS_FN_NS_GC_UNPIN_HANDLE(cb) };
    }
    this
}

/// `socket.removeAllListeners(event)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_REMOVE_ALL(this: u64, ep: *const u8, el: i64) -> u64 {
    let Some(st) = state::get(this) else {
        return this;
    };
    let event = crate::values::read(ep, el);
    let dropped: Vec<u64> = {
        let mut lst = st.listeners.lock().unwrap();
        lst.slot(&event).drain(..).map(|l| l.cb).collect()
    };
    for cb in dropped {
        unsafe { __RTS_FN_NS_GC_UNPIN_HANDLE(cb) };
    }
    this
}

/// `socket.removeAllListeners()` — every event.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_REMOVE_ALL0(this: u64) -> u64 {
    let Some(st) = state::get(this) else {
        return this;
    };
    let dropped: Vec<u64> = {
        let mut lst = st.listeners.lock().unwrap();
        let all = lst.map.iter().flat_map(|(_, v)| v.iter().map(|l| l.cb)).collect();
        lst.map.clear();
        all
    };
    for cb in dropped {
        unsafe { __RTS_FN_NS_GC_UNPIN_HANDLE(cb) };
    }
    this
}

/// `socket.listenerCount(event)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_LISTENER_COUNT(this: u64, ep: *const u8, el: i64) -> i64 {
    let Some(st) = state::get(this) else {
        return 0;
    };
    let event = crate::values::read(ep, el);
    let n = st.listeners.lock().unwrap().get(&event).len();
    n as i64
}

/// `socket.listeners(event)` / `rawListeners(event)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_LISTENERS(this: u64, ep: *const u8, el: i64) -> u64 {
    let Some(st) = state::get(this) else {
        return alloc_entry(Entry::Vec(Box::new(Vec::new())));
    };
    let event = crate::values::read(ep, el);
    let words: Vec<i64> = {
        let lst = st.listeners.lock().unwrap();
        lst.get(&event)
            .iter()
            .map(|l| rts_engine::heap::shapes::handle_word_auto(l.cb) as i64)
            .collect()
    };
    alloc_entry(Entry::Vec(Box::new(words)))
}

/// `socket.eventNames()` — the events that currently have listeners.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_EVENT_NAMES(this: u64) -> u64 {
    let Some(st) = state::get(this) else {
        return alloc_entry(Entry::Vec(Box::new(Vec::new())));
    };
    let names: Vec<String> = {
        let lst = st.listeners.lock().unwrap();
        lst.map
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, _)| k.clone())
            .collect()
    };
    crate::values::string_array(&names)
}

/// `socket.getMaxListeners()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_GET_MAX(this: u64) -> i64 {
    state::get(this)
        .map(|st| st.listeners.lock().unwrap().max_listeners())
        .unwrap_or(state::DEFAULT_MAX_LISTENERS)
}

/// `socket.setMaxListeners(n)` — returns `this`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_SET_MAX(this: u64, n: i64) -> u64 {
    if let Some(st) = state::get(this) {
        st.listeners.lock().unwrap().max = n;
    }
    this
}

/// Pin the heap value a PolyValue word refers to (a no-op for an inline number/
/// singleton), so a queued event's arguments survive until the pump delivers
/// them — nothing on the JS stack keeps them alive in the meantime.
pub fn pin_word(word: u64) {
    if let Some(h) = rts_engine::heap::poly::poly_handle_normalize(word) {
        unsafe { __RTS_FN_NS_GC_PIN_HANDLE(h) };
    }
}

/// Release a [`pin_word`] pin.
pub fn unpin_word(word: u64) {
    if let Some(h) = rts_engine::heap::poly::poly_handle_normalize(word) {
        unsafe { __RTS_FN_NS_GC_UNPIN_HANDLE(h) };
    }
}

/// `socket.emit(event, ...args)` — the emitter's public emit. User-emitted
/// events go through the SAME queue the OS-produced ones do, so ordering with
/// `'message'`/`'listening'` is preserved.
fn emit_words(this: u64, ep: *const u8, el: i64, args: Vec<u64>) -> i64 {
    let Some(st) = state::get(this) else {
        return 0;
    };
    let event = crate::values::read(ep, el);
    let has = !st.listeners.lock().unwrap().get(&event).is_empty();
    for &a in &args {
        pin_word(a);
    }
    st.push(SockEvent::Custom(event, args));
    i64::from(has)
}

/// `socket.emit(event)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_EMIT0(this: u64, ep: *const u8, el: i64) -> i64 {
    emit_words(this, ep, el, Vec::new())
}

/// `socket.emit(event, a0)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_EMIT1(this: u64, ep: *const u8, el: i64, a0: u64) -> i64 {
    emit_words(this, ep, el, vec![a0])
}

/// `socket.emit(event, a0, a1)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_EMIT2(this: u64, ep: *const u8, el: i64, a0: u64, a1: u64) -> i64 {
    emit_words(this, ep, el, vec![a0, a1])
}

/// Take the listeners that must run for `event`, dropping the `once` ones (and
/// unpinning them). Called by the pump, off the JS-visible path.
pub fn take_for(st: &state::SocketState, event: &str) -> Vec<Listener> {
    let (snapshot, expired) = {
        let mut lst = st.listeners.lock().unwrap();
        let slot = lst.slot(event);
        let snapshot: Vec<Listener> = slot.clone();
        let expired: Vec<u64> = slot.iter().filter(|l| l.once).map(|l| l.cb).collect();
        slot.retain(|l| !l.once);
        (snapshot, expired)
    };
    for cb in expired {
        unsafe { __RTS_FN_NS_GC_UNPIN_HANDLE(cb) };
    }
    snapshot
}

/// Unpin every listener of a socket being closed.
pub fn release_all(st: &state::SocketState) {
    let dropped: Vec<u64> = {
        let mut lst = st.listeners.lock().unwrap();
        let all = lst.map.iter().flat_map(|(_, v)| v.iter().map(|l| l.cb)).collect();
        lst.map.clear();
        all
    };
    for cb in dropped {
        unsafe { __RTS_FN_NS_GC_UNPIN_HANDLE(cb) };
    }
}
