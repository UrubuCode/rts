//! MessageChannel / MessagePort (web messaging) — minimal synchronous model.
//!
//! Both ports are plain `Entry::Map` objects (`port.onmessage = cb` is a
//! generic property store). `postMessage` entrega ao `onmessage` do port PEER
//! sincronamente. Migrado ao modelo `#[rts_class]` (stage 5) — duas classes no
//! mesmo arquivo. `MESSAGE_CHANNEL_PORT1`/`PORT2` (getters) e os helpers ficam
//! abaixo; `postMessage`/`close` sao InstanceMethods de MessagePort.

use indexmap::IndexMap;

use rts_engine::abi::ty::Handle;
use rts_macro::rts_class;

use crate::namespaces::gc::handles::{alloc_entry, with_entry, with_entry_mut, Entry};

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

/// MessageChannel — par de MessagePort entangled (entrega sincrona).
#[rts_class(
    MessageChannel,
    prefix = "MESSAGE_CHANNEL",
    spec = "MESSAGE_CHANNEL_CLASS_SPEC"
)]
impl MessageChannelClass {
    /// new MessageChannel() — entangled pair of ports + channel wrapper.
    #[rts_ctor(ts = "new MessageChannel(): MessageChannel")]
    pub fn new() -> Handle {
        let port1 = new_map();
        let port2 = new_map();
        map_set(port1, "__peer", port2 as i64);
        map_set(port2, "__peer", port1 as i64);
        let ch = new_map();
        map_set(ch, "port1", port1 as i64);
        map_set(ch, "port2", port2 as i64);
        ch
    }

    /// channel.port1
    #[rts_getter(ts = "readonly port1: MessagePort")]
    pub fn port1(ch: Handle) -> Handle {
        map_get(ch, "port1") as u64
    }

    /// channel.port2
    #[rts_getter(ts = "readonly port2: MessagePort")]
    pub fn port2(ch: Handle) -> Handle {
        map_get(ch, "port2") as u64
    }
}

/// MessagePort — uma ponta de um MessageChannel.
#[rts_class(MessagePort, prefix = "MESSAGE_PORT", spec = "MESSAGE_PORT_CLASS_SPEC")]
impl MessagePortClass {
    /// port.postMessage(data) — entrega ao `onmessage` do peer sincronamente.
    #[rts_method(
        name = "postMessage",
        symbol = "__RTS_FN_GL_MESSAGE_PORT_POST_MESSAGE",
        ts = "postMessage(data: any): void"
    )]
    pub fn post_message(port: Handle, data: Handle) {
        let peer = map_get(port, "__peer") as u64;
        if peer == 0 {
            return;
        }
        let onmsg = map_get(peer, "onmessage");
        if onmsg == 0 {
            return;
        }
        let ev = new_map();
        map_set(ev, "data", data as i64);
        call_fn(onmsg, &[ev as i64]);
    }

    /// port.close() — no-op no modelo sincrono.
    #[rts_method(ts = "close(): void")]
    pub fn close(_port: Handle) {}
}
