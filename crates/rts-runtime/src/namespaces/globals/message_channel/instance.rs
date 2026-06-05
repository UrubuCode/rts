//! MessageChannel / MessagePort runtime.
//!
//! Ports are `Entry::Map` objects. Keys: `__peer` (the other port's handle) and
//! `onmessage` (set by user assignment, a generic property store). `postMessage`
//! / `close` are InstanceMethods (receiver = the port). `postMessage` reads the
//! PEER's `onmessage` and invokes it synchronously with a `{ data }`
//! MessageEvent-shaped object.

use crate::namespaces::gc::handles::{alloc_entry, with_entry, with_entry_mut, Entry};
use indexmap::IndexMap;

fn map_get(h: u64, key: &str) -> i64 {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get(key).copied().unwrap_or(0),
        _ => 0,
    })
}

fn map_set(h: u64, key: &str, val: i64) {
    with_entry_mut(h, |e| {
        if let Some(Entry::Map(m)) = e {
            m.insert(key.to_string(), val);
        }
    });
}

fn new_map() -> u64 {
    alloc_entry(Entry::Map(Box::new(IndexMap::new())))
}

/// Resolves a callback value to its raw `extern "C"` fn pointer (+ bound args).
/// A stored callback may be an `Entry::Function` handle or a raw `func_addr` i64.
fn fn_ptr_of(handle: i64) -> Option<(u64, Vec<i64>)> {
    if handle == 0 {
        return None;
    }
    let as_fn = with_entry(handle as u64, |e| match e {
        Some(Entry::Function(fd)) => Some((fd.fn_ptr, fd.bound_args.clone())),
        _ => None,
    });
    Some(as_fn.unwrap_or((handle as u64, Vec::new())))
}

unsafe fn invoke(fn_ptr: u64, args: &[i64]) -> i64 {
    use std::mem::transmute;
    unsafe {
        match args.len() {
            0 => transmute::<u64, extern "C" fn() -> i64>(fn_ptr)(),
            1 => transmute::<u64, extern "C" fn(i64) -> i64>(fn_ptr)(args[0]),
            2 => transmute::<u64, extern "C" fn(i64, i64) -> i64>(fn_ptr)(args[0], args[1]),
            _ => transmute::<u64, extern "C" fn(i64, i64, i64) -> i64>(fn_ptr)(
                args[0], args[1], args[2],
            ),
        }
    }
}

/// Calls `fn_handle(extra...)` (prepending bound args). No-op if not callable.
fn call_fn(fn_handle: i64, extra: &[i64]) {
    if let Some((ptr, bound)) = fn_ptr_of(fn_handle) {
        if ptr == 0 {
            return;
        }
        let mut all = bound;
        all.extend_from_slice(extra);
        unsafe {
            invoke(ptr, &all);
        }
    }
}

/// `new MessageChannel()` — entangled pair of port objects + channel wrapper.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_MESSAGE_CHANNEL_NEW() -> u64 {
    let port1 = new_map();
    let port2 = new_map();
    map_set(port1, "__peer", port2 as i64);
    map_set(port2, "__peer", port1 as i64);
    let ch = new_map();
    map_set(ch, "port1", port1 as i64);
    map_set(ch, "port2", port2 as i64);
    ch
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_MESSAGE_CHANNEL_PORT1(ch: u64) -> u64 {
    map_get(ch, "port1") as u64
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_MESSAGE_CHANNEL_PORT2(ch: u64) -> u64 {
    map_get(ch, "port2") as u64
}

/// `port.postMessage(data)` — InstanceMethod (receiver `port`, payload `data`).
/// Delivers to the PEER port's `onmessage` synchronously, wrapped in a
/// `{ data }` MessageEvent object.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_MESSAGE_PORT_POST_MESSAGE(port: u64, data: i64) -> i64 {
    let peer = map_get(port, "__peer") as u64;
    if peer == 0 {
        return 0;
    }
    let onmsg = map_get(peer, "onmessage");
    if onmsg == 0 {
        return 0;
    }
    let ev = new_map();
    map_set(ev, "data", data);
    call_fn(onmsg, &[ev as i64]);
    0
}

/// `port.close()` — no-op in the synchronous model (no pending queue to flush).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_MESSAGE_PORT_CLOSE(_port: u64) -> i64 {
    0
}
