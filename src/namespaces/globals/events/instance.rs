//! `EventEmitter` — constructor and instance method implementations.
//!
//! # Hybrid dispatch model
//!
//! Each `EventEmitter` stores a `Mutex<EmitterData>` inside an `Entry::EventEmitter`
//! handle slot. The `async_mode` flag selects dispatch strategy at emit time:
//!
//! - **sync** (`async_mode = false`): listeners called sequentially on the caller
//!   thread while holding no lock (listeners snapshot taken under lock, then released).
//! - **async** (`async_mode = true`): each listener is submitted as an independent
//!   `rayon::spawn` task — fire-and-forget, order not guaranteed.
//!
//! # Listener signature
//!
//! ```rust
//! extern "C" fn listener(arg: i64) -> i64;
//! ```
//!
//! Stored as `u64` fn-pointer. Callers materialise user functions via `func_addr`
//! (the first-class fn-pointer mechanism from codegen).

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use crate::namespaces::gc::handles::{Entry, alloc_entry, free_handle, shard_for_handle};

// ── Internal data ──────────────────────────────────────────────────────────────

#[derive(Clone)]
struct Listener {
    fn_ptr: u64,
    once: bool,
}

pub struct EmitterData {
    listeners: HashMap<String, Vec<Listener>>,
    async_mode: bool,
}

impl EmitterData {
    fn new(async_mode: bool) -> Self {
        Self {
            listeners: HashMap::new(),
            async_mode,
        }
    }
}

// ── Handle accessors ──────────────────────────────────────────────────────────

fn clone_arc(handle: u64) -> Option<Arc<Mutex<dyn Any + Send>>> {
    let guard = shard_for_handle(handle).lock().unwrap();
    if let Some(Entry::EventEmitter(arc)) = guard.get(handle) {
        Some(arc.clone())
    } else {
        None
    }
}

fn with_emitter<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&mut EmitterData) -> R,
{
    if let Some(arc) = clone_arc(handle) {
        let mut any = arc.lock().unwrap();
        if let Some(data) = any.downcast_mut::<EmitterData>() {
            return f(data);
        }
    }
    default
}

// ── Raw string helper ────────────────────────────────────────────────────────

unsafe fn event_name(ptr: i64, len: i64) -> String {
    if ptr == 0 || len <= 0 {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    std::str::from_utf8(bytes).unwrap_or("").to_owned()
}

// ── Constructors / destructor ─────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_FREE(handle: u64) -> i64 {
    if free_handle(handle) { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_NEW() -> u64 {
    let data: Arc<Mutex<dyn Any + Send>> = Arc::new(Mutex::new(EmitterData::new(false)));
    alloc_entry(Entry::EventEmitter(data))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_NEW_ASYNC(is_async: i64) -> u64 {
    let data: Arc<Mutex<dyn Any + Send>> = Arc::new(Mutex::new(EmitterData::new(is_async != 0)));
    alloc_entry(Entry::EventEmitter(data))
}

// ── on / once / off ───────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_ON(
    handle: u64,
    ev_ptr: i64,
    ev_len: i64,
    fn_ptr: u64,
) -> u64 {
    let event = unsafe { event_name(ev_ptr, ev_len) };
    with_emitter(handle, handle, |data| {
        data.listeners
            .entry(event)
            .or_default()
            .push(Listener { fn_ptr, once: false });
        handle
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_ONCE(
    handle: u64,
    ev_ptr: i64,
    ev_len: i64,
    fn_ptr: u64,
) -> u64 {
    let event = unsafe { event_name(ev_ptr, ev_len) };
    with_emitter(handle, handle, |data| {
        data.listeners
            .entry(event)
            .or_default()
            .push(Listener { fn_ptr, once: true });
        handle
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_OFF(
    handle: u64,
    ev_ptr: i64,
    ev_len: i64,
    fn_ptr: u64,
) -> u64 {
    let event = unsafe { event_name(ev_ptr, ev_len) };
    with_emitter(handle, handle, |data| {
        if let Some(list) = data.listeners.get_mut(&event) {
            list.retain(|l| l.fn_ptr != fn_ptr);
        }
        handle
    })
}

// ── emit ─────────────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_EMIT(
    handle: u64,
    ev_ptr: i64,
    ev_len: i64,
    arg: i64,
) -> i64 {
    let event = unsafe { event_name(ev_ptr, ev_len) };

    // Snapshot listeners under lock, then release before calling any callback.
    // This prevents deadlock if a listener calls back into this emitter.
    let (snapshot, async_mode) = {
        let Some(arc) = clone_arc(handle) else { return 0; };
        let mut any = arc.lock().unwrap();
        let Some(data) = any.downcast_mut::<EmitterData>() else { return 0; };
        let list = data.listeners.get(&event).cloned().unwrap_or_default();
        if let Some(vec) = data.listeners.get_mut(&event) {
            vec.retain(|l| !l.once);
        }
        (list, data.async_mode)
    };

    if snapshot.is_empty() {
        return 0;
    }

    // RTS codegen compiles `number` params as f64 (XMM register on x86-64).
    // Numeric conversion so integer values (42, -1, etc.) arrive correctly.
    let arg_f64 = arg as f64;

    if async_mode {
        // Fire-and-forget: each listener on the rayon thread pool.
        // SAFETY: fn_ptr is a valid `extern "C" fn(f64) -> f64` produced by codegen.
        for listener in snapshot {
            rayon::spawn(move || {
                let f: extern "C" fn(f64) -> f64 =
                    unsafe { std::mem::transmute(listener.fn_ptr as usize) };
                f(arg_f64);
            });
        }
    } else {
        // Synchronous: call in registration order.
        for listener in &snapshot {
            let f: extern "C" fn(f64) -> f64 =
                unsafe { std::mem::transmute(listener.fn_ptr as usize) };
            f(arg_f64);
        }
    }

    1 // true — at least one listener was called
}

/// Like `emit` but passes the handle arg as a bitcast f64 so all 64 bits are
/// preserved. Listener recovers the handle via `num.f64_to_bits(arg)`.
/// Use this variant when the payload is a handle/raw u64, not a numeric value.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_EMIT_HANDLE(
    handle: u64,
    ev_ptr: i64,
    ev_len: i64,
    arg: i64,
) -> i64 {
    let event = unsafe { event_name(ev_ptr, ev_len) };

    let (snapshot, async_mode) = {
        let Some(arc) = clone_arc(handle) else { return 0; };
        let mut any = arc.lock().unwrap();
        let Some(data) = any.downcast_mut::<EmitterData>() else { return 0; };
        let list = data.listeners.get(&event).cloned().unwrap_or_default();
        if let Some(vec) = data.listeners.get_mut(&event) {
            vec.retain(|l| !l.once);
        }
        (list, data.async_mode)
    };

    if snapshot.is_empty() {
        return 0;
    }

    // Bitcast: reinterpret i64 bits as f64 — no numeric conversion, all bits intact.
    // Listener recovers the original handle bits via num.f64_to_bits(arg).
    let arg_bits = f64::from_bits(arg as u64);

    if async_mode {
        for listener in snapshot {
            rayon::spawn(move || {
                let f: extern "C" fn(f64) -> f64 =
                    unsafe { std::mem::transmute(listener.fn_ptr as usize) };
                f(arg_bits);
            });
        }
    } else {
        for listener in &snapshot {
            let f: extern "C" fn(f64) -> f64 =
                unsafe { std::mem::transmute(listener.fn_ptr as usize) };
            f(arg_bits);
        }
    }

    1
}

// ── removeAllListeners / listenerCount / eventNames ──────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_REMOVE_ALL(
    handle: u64,
    ev_ptr: i64,
    ev_len: i64,
) -> u64 {
    let event = unsafe { event_name(ev_ptr, ev_len) };
    with_emitter(handle, handle, |data| {
        data.listeners.remove(&event);
        handle
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_LISTENER_COUNT(
    handle: u64,
    ev_ptr: i64,
    ev_len: i64,
) -> i64 {
    let event = unsafe { event_name(ev_ptr, ev_len) };
    with_emitter(handle, 0, |data| {
        data.listeners.get(&event).map(|v| v.len() as i64).unwrap_or(0)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_EVENT_NAMES(handle: u64) -> u64 {
    use crate::namespaces::gc::handles::alloc_entry;

    let names: Vec<String> = {
        let Some(arc) = clone_arc(handle) else { return 0; };
        let any = arc.lock().unwrap();
        let Some(data) = any.downcast_ref::<EmitterData>() else { return 0; };
        data.listeners
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, _)| k.clone())
            .collect()
    };

    // Build a Vec<i64> of string handles, return as a collections Vec handle.
    let vec_handle = alloc_entry(Entry::Vec(Box::new(Vec::new())));
    let shard = shard_for_handle(vec_handle);
    let mut guard = shard.lock().unwrap();
    if let Some(Entry::Vec(v)) = guard.get_mut(vec_handle) {
        for name in names {
            let str_handle = alloc_entry(Entry::String(name.into_bytes()));
            v.push(str_handle as i64);
        }
    }
    vec_handle
}
