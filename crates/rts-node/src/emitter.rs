//! The crate-shared `EventEmitter` surface for `rts-node`'s classes.
//!
//! Several Node classes ARE EventEmitters (`dgram.Socket`, `net.Server`,
//! `net.Socket`, …) and `rts-node` is independent of `rts-std`, where the
//! canonical `EventEmitter` class lives — so the emitter is implemented here,
//! once, exactly as docs/node-implementation/dgram.md §5.6 and net.md §5.7
//! prescribe (each recommends a local base; §7 of dgram.md flags "decide it once
//! a second consumer exists" — this is that decision).
//!
//! Why ONE implementation serves every class: the externs take the receiver
//! handle (`this`) like any instance method, and the listener table is keyed by
//! that handle. Nothing here knows which class the receiver is, so a class opts
//! in purely as DATA — it registers [`members`] on its own `ClassBuilder`.
//!
//! What is NOT here: `emit`. A backend class queues its events so a user
//! `emit()` keeps its ordering against the OS-produced ones, and that queue
//! belongs to the class (dgram's `pump`), not to the listener table.
//!
//! Listener handles are GC-pinned while registered — a pump invokes them long
//! after the JS frame that created them is gone.

use std::sync::{Mutex, MutexGuard, OnceLock};

use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};
use rts_engine::AbiType::{self, Handle, I64, PolyValue, StrPtr};
use rts_engine::{ClassBuilder, FnPtr, Member, MemberFlags, MemberKind, Sig};

use crate::values::{read, string_array, val, Val};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_PIN_HANDLE(handle: u64);
    fn __RTS_FN_NS_GC_UNPIN_HANDLE(handle: u64);
}

/// Node's `EventEmitter.defaultMaxListeners`.
pub const DEFAULT_MAX_LISTENERS: i64 = 10;

/// A registered listener: the Function HANDLE (normalized off the PolyValue word
/// and pinned while registered) plus `once` semantics.
#[derive(Clone, Copy)]
pub struct Listener {
    pub cb: u64,
    pub once: bool,
}

/// One receiver's listeners: ordered per-event lists + its max-listeners setting.
#[derive(Default)]
struct Listeners {
    map: Vec<(String, Vec<Listener>)>,
    max: i64,
}

impl Listeners {
    fn slot(&mut self, event: &str) -> &mut Vec<Listener> {
        if let Some(i) = self.map.iter().position(|(k, _)| k == event) {
            return &mut self.map[i].1;
        }
        self.map.push((event.to_string(), Vec::new()));
        let last = self.map.len() - 1;
        &mut self.map[last].1
    }

    fn get(&self, event: &str) -> &[Listener] {
        self.map
            .iter()
            .find(|(k, _)| k == event)
            .map(|(_, v)| v.as_slice())
            .unwrap_or(&[])
    }
}

type Table = indexmap::IndexMap<u64, Listeners>;

fn table() -> MutexGuard<'static, Table> {
    static T: OnceLock<Mutex<Table>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(Table::new())).lock().unwrap()
}

/// Register `cb` for `event` on `this`. `once` → dropped after one delivery;
/// `prepend` → inserted at the front (Node's `prependListener`).
pub fn add(this: u64, event: &str, cb: u64, once: bool, prepend: bool) {
    unsafe { __RTS_FN_NS_GC_PIN_HANDLE(cb) };
    let mut t = table();
    let slot = t.entry(this).or_default().slot(event);
    let entry = Listener { cb, once };
    if prepend {
        slot.insert(0, entry);
    } else {
        slot.push(entry);
    }
}

/// Whether `this` has any listener for `event` — how a pump decides whether an
/// `'error'` has a handler (Node: an unhandled one is fatal).
pub fn has(this: u64, event: &str) -> bool {
    !table().get(&this).map(|l| l.get(event).is_empty()).unwrap_or(true)
}

/// Take the listeners that must run for `event`, dropping (and unpinning) the
/// `once` ones. Called by a class's pump, off the JS-visible path.
pub fn take_for(this: u64, event: &str) -> Vec<Listener> {
    let (snapshot, expired) = {
        let mut t = table();
        let Some(listeners) = t.get_mut(&this) else {
            return Vec::new();
        };
        let slot = listeners.slot(event);
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

/// Drop every listener of `this` and unpin them — a class calls this when its
/// instance is finalized.
pub fn release_all(this: u64) {
    let dropped: Vec<u64> = {
        let mut t = table();
        match t.shift_remove(&this) {
            Some(l) => l.map.iter().flat_map(|(_, v)| v.iter().map(|x| x.cb)).collect(),
            None => Vec::new(),
        }
    };
    for cb in dropped {
        unsafe { __RTS_FN_NS_GC_UNPIN_HANDLE(cb) };
    }
}

/// Pin the heap value a PolyValue word refers to (a no-op for an inline number/
/// singleton), so a queued event's arguments survive until its pump delivers
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

/// The IDENTITY of a listener value for `off`/`removeListener`. Each pass of a
/// function through the ABI reifies a FRESH `Entry::Function` slot, so comparing
/// handles never matches — the stable identity of a named fn / non-capturing
/// arrow is its underlying CODE pointer. A capture-carrying closure keeps its own
/// handle (JS semantics: two separately-created closures are distinct listeners).
fn identity(cb: u64) -> u64 {
    with_entry(cb, |e| match e {
        Some(Entry::Function(f)) if f.bound_args.is_empty() => Some(f.fn_ptr),
        _ => None,
    })
    .unwrap_or(cb)
}

/// The `(this, event, listener)` add-shaped methods, by their flags.
fn add_word(this: u64, ep: *const u8, el: i64, listener: u64, once: bool, prepend: bool) -> u64 {
    if let Val::Func(cb) = val(listener) {
        add(this, &read(ep, el), cb, once, prepend);
    }
    this
}

/// `emitter.on(event, listener)` / `addListener`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_EE_ON(this: u64, ep: *const u8, el: i64, l: u64) -> u64 {
    add_word(this, ep, el, l, false, false)
}

/// `emitter.once(event, listener)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_EE_ONCE(this: u64, ep: *const u8, el: i64, l: u64) -> u64 {
    add_word(this, ep, el, l, true, false)
}

/// `emitter.prependListener(event, listener)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_EE_PREPEND(this: u64, ep: *const u8, el: i64, l: u64) -> u64 {
    add_word(this, ep, el, l, false, true)
}

/// `emitter.prependOnceListener(event, listener)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_EE_PREPEND_ONCE(this: u64, ep: *const u8, el: i64, l: u64) -> u64 {
    add_word(this, ep, el, l, true, true)
}

/// `emitter.off(event, listener)` / `removeListener`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_EE_OFF(this: u64, ep: *const u8, el: i64, l: u64) -> u64 {
    let Val::Func(cb) = val(l) else { return this };
    let event = read(ep, el);
    let key = identity(cb);
    let mut removed = Vec::new();
    {
        let mut t = table();
        if let Some(listeners) = t.get_mut(&this) {
            listeners.slot(&event).retain(|entry| {
                let hit = identity(entry.cb) == key;
                if hit {
                    removed.push(entry.cb);
                }
                !hit
            });
        }
    }
    for cb in removed {
        unsafe { __RTS_FN_NS_GC_UNPIN_HANDLE(cb) };
    }
    this
}

/// `emitter.removeAllListeners(event)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_EE_REMOVE_ALL(this: u64, ep: *const u8, el: i64) -> u64 {
    let event = read(ep, el);
    let dropped: Vec<u64> = {
        let mut t = table();
        match t.get_mut(&this) {
            Some(listeners) => listeners.slot(&event).drain(..).map(|l| l.cb).collect(),
            None => Vec::new(),
        }
    };
    for cb in dropped {
        unsafe { __RTS_FN_NS_GC_UNPIN_HANDLE(cb) };
    }
    this
}

/// `emitter.removeAllListeners()` — every event.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_EE_REMOVE_ALL0(this: u64) -> u64 {
    release_all(this);
    this
}

/// `emitter.listenerCount(event)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_EE_LISTENER_COUNT(this: u64, ep: *const u8, el: i64) -> i64 {
    let event = read(ep, el);
    table().get(&this).map(|l| l.get(&event).len() as i64).unwrap_or(0)
}

/// `emitter.listeners(event)` / `rawListeners(event)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_EE_LISTENERS(this: u64, ep: *const u8, el: i64) -> u64 {
    let event = read(ep, el);
    let words: Vec<i64> = table()
        .get(&this)
        .map(|l| {
            l.get(&event)
                .iter()
                .map(|x| rts_engine::heap::shapes::handle_word_auto(x.cb) as i64)
                .collect()
        })
        .unwrap_or_default();
    alloc_entry(Entry::Vec(Box::new(words)))
}

/// `emitter.eventNames()` — the events that currently have listeners.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_EE_EVENT_NAMES(this: u64) -> u64 {
    let names: Vec<String> = table()
        .get(&this)
        .map(|l| {
            l.map
                .iter()
                .filter(|(_, v)| !v.is_empty())
                .map(|(k, _)| k.clone())
                .collect()
        })
        .unwrap_or_default();
    string_array(&names)
}

/// `emitter.getMaxListeners()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_EE_GET_MAX(this: u64) -> i64 {
    table()
        .get(&this)
        .map(|l| if l.max == 0 { DEFAULT_MAX_LISTENERS } else { l.max })
        .unwrap_or(DEFAULT_MAX_LISTENERS)
}

/// `emitter.setMaxListeners(n)` — returns `this`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_EE_SET_MAX(this: u64, n: i64) -> u64 {
    table().entry(this).or_default().max = n;
    this
}

#[allow(clippy::too_many_arguments)]
fn member(name: &str, args: Vec<AbiType>, ret: AbiType, symbol: &str, ts: &str, fp: *const u8) -> Member {
    let mut full = vec![Handle];
    full.extend(args);
    Member {
        name: name.to_string(),
        kind: MemberKind::InstanceMethod,
        sig: Sig::new(full, ret),
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::THROWS,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: String::new(),
        ret_class: None,
        pure: false,
        emit: None,
    }
}

/// Register the `EventEmitter` surface a Node class inherits onto `class`. The
/// `emit` members are NOT here — a class queues its own events (see the module
/// doc) — so it registers those itself.
///
/// `ret_class` is the class's own name: `on`/`once`/… return `this`, and naming
/// the class in the ts-signature is what lets a chained `s.on(..).on(..)`
/// dispatch statically.
pub fn install<'a>(class: ClassBuilder<'a>, ret_class: &str) -> ClassBuilder<'a> {
    let this_ret = |name: &str| format!("{name}(event: string, listener: object): {ret_class}");
    class
        .member(member("on", vec![StrPtr, PolyValue], Handle, "__RTS_FN_NODE_EE_ON", &this_ret("on"), __RTS_FN_NODE_EE_ON as *const u8))
        .member(member("addListener", vec![StrPtr, PolyValue], Handle, "__RTS_FN_NODE_EE_ON", &this_ret("addListener"), __RTS_FN_NODE_EE_ON as *const u8))
        .member(member("once", vec![StrPtr, PolyValue], Handle, "__RTS_FN_NODE_EE_ONCE", &this_ret("once"), __RTS_FN_NODE_EE_ONCE as *const u8))
        .member(member("off", vec![StrPtr, PolyValue], Handle, "__RTS_FN_NODE_EE_OFF", &this_ret("off"), __RTS_FN_NODE_EE_OFF as *const u8))
        .member(member("removeListener", vec![StrPtr, PolyValue], Handle, "__RTS_FN_NODE_EE_OFF", &this_ret("removeListener"), __RTS_FN_NODE_EE_OFF as *const u8))
        .member(member("prependListener", vec![StrPtr, PolyValue], Handle, "__RTS_FN_NODE_EE_PREPEND", &this_ret("prependListener"), __RTS_FN_NODE_EE_PREPEND as *const u8))
        .member(member("prependOnceListener", vec![StrPtr, PolyValue], Handle, "__RTS_FN_NODE_EE_PREPEND_ONCE", &this_ret("prependOnceListener"), __RTS_FN_NODE_EE_PREPEND_ONCE as *const u8))
        .member(member("removeAllListeners", vec![], Handle, "__RTS_FN_NODE_EE_REMOVE_ALL0", &format!("removeAllListeners(): {ret_class}"), __RTS_FN_NODE_EE_REMOVE_ALL0 as *const u8))
        .member(member("removeAllListeners", vec![StrPtr], Handle, "__RTS_FN_NODE_EE_REMOVE_ALL", &format!("removeAllListeners(event: string): {ret_class}"), __RTS_FN_NODE_EE_REMOVE_ALL as *const u8))
        .member(member("listenerCount", vec![StrPtr], I64, "__RTS_FN_NODE_EE_LISTENER_COUNT", "listenerCount(event: string): number", __RTS_FN_NODE_EE_LISTENER_COUNT as *const u8))
        .member(member("listeners", vec![StrPtr], Handle, "__RTS_FN_NODE_EE_LISTENERS", "listeners(event: string): object[]", __RTS_FN_NODE_EE_LISTENERS as *const u8))
        .member(member("rawListeners", vec![StrPtr], Handle, "__RTS_FN_NODE_EE_LISTENERS", "rawListeners(event: string): object[]", __RTS_FN_NODE_EE_LISTENERS as *const u8))
        .member(member("eventNames", vec![], Handle, "__RTS_FN_NODE_EE_EVENT_NAMES", "eventNames(): string[]", __RTS_FN_NODE_EE_EVENT_NAMES as *const u8))
        .member(member("getMaxListeners", vec![], I64, "__RTS_FN_NODE_EE_GET_MAX", "getMaxListeners(): number", __RTS_FN_NODE_EE_GET_MAX as *const u8))
        .member(member("setMaxListeners", vec![I64], Handle, "__RTS_FN_NODE_EE_SET_MAX", &format!("setMaxListeners(n: number): {ret_class}"), __RTS_FN_NODE_EE_SET_MAX as *const u8))
}
