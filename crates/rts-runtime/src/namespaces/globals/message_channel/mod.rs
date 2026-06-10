//! MessageChannel / MessagePort (web messaging) — minimal synchronous model.
//!
//! Both ports are plain `Entry::Map` objects (`port.onmessage = cb` is a
//! generic property store). `postMessage` entrega ao `onmessage` do port PEER
//! sincronamente. `MESSAGE_CHANNEL_PORT1`/`PORT2` (getters) e os helpers ficam
//! abaixo; `postMessage`/`close` sao InstanceMethods de MessagePort.
//!
//! Migrado do `#[rts_class]` (macro) pro modelo builder hand-written do
//! `rts-engine` (rumo à remoção da `rts-macro`). Os externs
//! `__RTS_FN_GL_MESSAGE_CHANNEL_*` / `__RTS_FN_GL_MESSAGE_PORT_*` +
//! `register_message_channel_class_spec()` / `register_message_port_class_spec()`
//! são escritos à mão.

use indexmap::IndexMap;

use rts_engine::abi::ty::Handle;
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

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

// ── MessageChannel externs (hand-written, sem macro). ────────────────────────

/// new MessageChannel() — entangled pair of ports + channel wrapper.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_MESSAGE_CHANNEL_NEW() -> Handle {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_MESSAGE_CHANNEL_PORT1(ch: Handle) -> Handle {
    map_get(ch, "port1") as u64
}

/// channel.port2
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_MESSAGE_CHANNEL_PORT2(ch: Handle) -> Handle {
    map_get(ch, "port2") as u64
}

// ── MessagePort externs (hand-written, sem macro). ───────────────────────────

/// port.postMessage(data) — entrega ao `onmessage` do peer sincronamente.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_MESSAGE_PORT_POST_MESSAGE(port: Handle, data: Handle) {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_MESSAGE_PORT_CLOSE(_port: Handle) {}

/// Membro de classe global (helper hand-written, espelha `leak_class` da macro).
#[allow(clippy::too_many_arguments)]
fn m(
    name: &str,
    kind: MemberKind,
    sig: Sig,
    symbol: &str,
    ts: &str,
    doc: &str,
    fp: *const u8,
    pure: bool,
) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure,
        intrinsic: None,
    }
}

/// Registra a classe global `MessageChannel` no motor (hand-written, sem macro).
pub fn register_message_channel_class_spec(e: &mut Engine) {
    e.class("MessageChannel")
        .doc("MessageChannel — par de MessagePort entangled (entrega sincrona).")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(Vec::new(), AbiType::Handle),
            "__RTS_FN_GL_MESSAGE_CHANNEL_NEW",
            "new MessageChannel(): MessageChannel",
            "new MessageChannel() — entangled pair of ports + channel wrapper.",
            __RTS_FN_GL_MESSAGE_CHANNEL_NEW as *const u8,
            false,
        ))
        .member(m(
            "port1",
            MemberKind::InstanceGetter,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_MESSAGE_CHANNEL_PORT1",
            "readonly port1: MessagePort",
            "channel.port1",
            __RTS_FN_GL_MESSAGE_CHANNEL_PORT1 as *const u8,
            false,
        ))
        .member(m(
            "port2",
            MemberKind::InstanceGetter,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_MESSAGE_CHANNEL_PORT2",
            "readonly port2: MessagePort",
            "channel.port2",
            __RTS_FN_GL_MESSAGE_CHANNEL_PORT2 as *const u8,
            false,
        ))
        .done();
}

/// Registra a classe global `MessagePort` no motor (hand-written, sem macro).
pub fn register_message_port_class_spec(e: &mut Engine) {
    e.class("MessagePort")
        .doc("MessagePort — uma ponta de um MessageChannel.")
        .member(m(
            "postMessage",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Void),
            "__RTS_FN_GL_MESSAGE_PORT_POST_MESSAGE",
            "postMessage(data: any): void",
            "port.postMessage(data) — entrega ao `onmessage` do peer sincronamente.",
            __RTS_FN_GL_MESSAGE_PORT_POST_MESSAGE as *const u8,
            false,
        ))
        .member(m(
            "close",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Void),
            "__RTS_FN_GL_MESSAGE_PORT_CLOSE",
            "close(): void",
            "port.close() — no-op no modelo sincrono.",
            __RTS_FN_GL_MESSAGE_PORT_CLOSE as *const u8,
            false,
        ))
        .done();
}
