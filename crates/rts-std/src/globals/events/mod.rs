//! `EventEmitter` global class — hybrid sync/async listener dispatch.
//!
//! Migrado do `#[rts_class]` (macro) pro modelo builder hand-written do
//! `rts-engine` (rumo à remoção da `rts-macro`). Os externs
//! `__RTS_FN_GL_EE_*` + `register_class_spec()` são escritos à mão.
//! `EmitterData`/`Listener` + os helpers `clone_arc`/`with_emitter` ficam como
//! itens de modulo. Listener signature: `extern "C" fn(f64) -> f64`.

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rts_engine::abi::ty::{Bool, Handle, I64, U64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use rts_engine::heap::handles::{alloc_entry, free_handle, with_entry, Entry};

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

fn clone_arc(handle: u64) -> Option<Arc<Mutex<dyn Any + Send>>> {
    with_entry(handle, |entry| match entry {
        Some(Entry::EventEmitter(arc)) => Some(arc.clone()),
        _ => None,
    })
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

/// emitter.free() — libera o handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_FREE(handle: Handle) -> I64 {
    if free_handle(handle) {
        1
    } else {
        0
    }
}

/// new EventEmitter()
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_NEW() -> Handle {
    let data: Arc<Mutex<dyn Any + Send>> = Arc::new(Mutex::new(EmitterData::new(false)));
    alloc_entry(Entry::EventEmitter(data))
}

/// new EventEmitter(async)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_NEW_ASYNC(is_async: Bool) -> Handle {
    let data: Arc<Mutex<dyn Any + Send>> =
        Arc::new(Mutex::new(EmitterData::new(is_async != 0)));
    alloc_entry(Entry::EventEmitter(data))
}

/// emitter.on(event, listener)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_ON(
    handle: Handle,
    event_ptr: *const u8,
    event_len: i64,
    fn_ptr: U64,
) -> Handle {
    let event = unsafe { rts_engine::abi::str_abi::from_abi(event_ptr, event_len) };
    let event = event.unwrap_or("").to_string();
    with_emitter(handle, handle, |data| {
        data.listeners.entry(event).or_default().push(Listener {
            fn_ptr,
            once: false,
        });
        handle
    })
}

/// emitter.once(event, listener)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_ONCE(
    handle: Handle,
    event_ptr: *const u8,
    event_len: i64,
    fn_ptr: U64,
) -> Handle {
    let event = unsafe { rts_engine::abi::str_abi::from_abi(event_ptr, event_len) };
    let event = event.unwrap_or("").to_string();
    with_emitter(handle, handle, |data| {
        data.listeners
            .entry(event)
            .or_default()
            .push(Listener { fn_ptr, once: true });
        handle
    })
}

/// emitter.off(event, listener)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_OFF(
    handle: Handle,
    event_ptr: *const u8,
    event_len: i64,
    fn_ptr: U64,
) -> Handle {
    let event = unsafe { rts_engine::abi::str_abi::from_abi(event_ptr, event_len) };
    let event = event.unwrap_or("").to_string();
    with_emitter(handle, handle, |data| {
        if let Some(list) = data.listeners.get_mut(&event) {
            list.retain(|l| l.fn_ptr != fn_ptr);
        }
        handle
    })
}

/// emitter.emit(event, arg) — listeners recebem `arg` como number (f64).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_EMIT(
    handle: Handle,
    event_ptr: *const u8,
    event_len: i64,
    arg: I64,
) -> Bool {
    let event = unsafe { rts_engine::abi::str_abi::from_abi(event_ptr, event_len) };
    let event = event.unwrap_or("").to_string();
    let (snapshot, async_mode) = {
        let Some(arc) = clone_arc(handle) else {
            return 0;
        };
        let mut any = arc.lock().unwrap();
        let Some(data) = any.downcast_mut::<EmitterData>() else {
            return 0;
        };
        let list = data.listeners.get(&event).cloned().unwrap_or_default();
        if let Some(vec) = data.listeners.get_mut(&event) {
            vec.retain(|l| !l.once);
        }
        (list, data.async_mode)
    };
    if snapshot.is_empty() {
        return 0;
    }
    let arg_f64 = arg as f64;
    if async_mode {
        for listener in snapshot {
            rayon::spawn(move || {
                let f: extern "C" fn(f64) -> f64 =
                    unsafe { std::mem::transmute(listener.fn_ptr as usize) };
                f(arg_f64);
            });
        }
    } else {
        for listener in &snapshot {
            let f: extern "C" fn(f64) -> f64 =
                unsafe { std::mem::transmute(listener.fn_ptr as usize) };
            f(arg_f64);
        }
    }
    1
}

/// emitter.emitHandle(event, handle) — passa o arg como bits f64 (handle raw).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_EMIT_HANDLE(
    handle: Handle,
    event_ptr: *const u8,
    event_len: i64,
    arg: I64,
) -> Bool {
    let event = unsafe { rts_engine::abi::str_abi::from_abi(event_ptr, event_len) };
    let event = event.unwrap_or("").to_string();
    let (snapshot, async_mode) = {
        let Some(arc) = clone_arc(handle) else {
            return 0;
        };
        let mut any = arc.lock().unwrap();
        let Some(data) = any.downcast_mut::<EmitterData>() else {
            return 0;
        };
        let list = data.listeners.get(&event).cloned().unwrap_or_default();
        if let Some(vec) = data.listeners.get_mut(&event) {
            vec.retain(|l| !l.once);
        }
        (list, data.async_mode)
    };
    if snapshot.is_empty() {
        return 0;
    }
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

/// emitter.removeAllListeners(event)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_REMOVE_ALL(
    handle: Handle,
    event_ptr: *const u8,
    event_len: i64,
) -> Handle {
    let event = unsafe { rts_engine::abi::str_abi::from_abi(event_ptr, event_len) };
    let event = event.unwrap_or("").to_string();
    with_emitter(handle, handle, |data| {
        data.listeners.remove(&event);
        handle
    })
}

/// emitter.listenerCount(event)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_LISTENER_COUNT(
    handle: Handle,
    event_ptr: *const u8,
    event_len: i64,
) -> I64 {
    let event = unsafe { rts_engine::abi::str_abi::from_abi(event_ptr, event_len) };
    let event = event.unwrap_or("").to_string();
    with_emitter(handle, 0, |data| {
        data.listeners
            .get(&event)
            .map(|v| v.len() as i64)
            .unwrap_or(0)
    })
}

/// emitter.eventNames()
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_EVENT_NAMES(handle: Handle) -> Handle {
    let names: Vec<String> = {
        let Some(arc) = clone_arc(handle) else {
            return 0;
        };
        let any = arc.lock().unwrap();
        let Some(data) = any.downcast_ref::<EmitterData>() else {
            return 0;
        };
        data.listeners
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, _)| k.clone())
            .collect()
    };
    let str_handles: Vec<i64> = names
        .into_iter()
        .map(|name| alloc_entry(Entry::String(name.into_bytes())) as i64)
        .collect();
    alloc_entry(Entry::Vec(Box::new(str_handles)))
}

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

/// Registra a classe global `EventEmitter` no motor (Fase 2 — hand-written, sem macro).
pub fn register_class_spec(e: &mut Engine) {
    e.class("EventEmitter")
        .doc("EventEmitter — on/once/off + emit (sync ou async fire-and-forget).")
        .member(m(
            "free",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_EE_FREE",
            "free(): number",
            "emitter.free() — libera o handle.",
            __RTS_FN_GL_EE_FREE as *const u8,
            false,
        ))
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(Vec::new(), AbiType::Handle),
            "__RTS_FN_GL_EE_NEW",
            "new EventEmitter(): EventEmitter",
            "new EventEmitter()",
            __RTS_FN_GL_EE_NEW as *const u8,
            false,
        ))
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::Bool], AbiType::Handle),
            "__RTS_FN_GL_EE_NEW_ASYNC",
            "new EventEmitter(async: boolean): EventEmitter",
            "new EventEmitter(async)",
            __RTS_FN_GL_EE_NEW_ASYNC as *const u8,
            false,
        ))
        .member(m(
            "on",
            MemberKind::InstanceMethod,
            Sig::new(
                vec![AbiType::Handle, AbiType::StrPtr, AbiType::U64],
                AbiType::Handle,
            ),
            "__RTS_FN_GL_EE_ON",
            "on(event: string, listener: (arg: number) => void): this",
            "emitter.on(event, listener)",
            __RTS_FN_GL_EE_ON as *const u8,
            false,
        ))
        .member(m(
            "once",
            MemberKind::InstanceMethod,
            Sig::new(
                vec![AbiType::Handle, AbiType::StrPtr, AbiType::U64],
                AbiType::Handle,
            ),
            "__RTS_FN_GL_EE_ONCE",
            "once(event: string, listener: (arg: number) => void): this",
            "emitter.once(event, listener)",
            __RTS_FN_GL_EE_ONCE as *const u8,
            false,
        ))
        .member(m(
            "off",
            MemberKind::InstanceMethod,
            Sig::new(
                vec![AbiType::Handle, AbiType::StrPtr, AbiType::U64],
                AbiType::Handle,
            ),
            "__RTS_FN_GL_EE_OFF",
            "off(event: string, listener: (arg: number) => void): this",
            "emitter.off(event, listener)",
            __RTS_FN_GL_EE_OFF as *const u8,
            false,
        ))
        .member(m(
            "emit",
            MemberKind::InstanceMethod,
            Sig::new(
                vec![AbiType::Handle, AbiType::StrPtr, AbiType::I64],
                AbiType::Bool,
            ),
            "__RTS_FN_GL_EE_EMIT",
            "emit(event: string, arg: number): boolean",
            "emitter.emit(event, arg) — listeners recebem `arg` como number (f64).",
            __RTS_FN_GL_EE_EMIT as *const u8,
            false,
        ))
        .member(m(
            "emitHandle",
            MemberKind::InstanceMethod,
            Sig::new(
                vec![AbiType::Handle, AbiType::StrPtr, AbiType::I64],
                AbiType::Bool,
            ),
            "__RTS_FN_GL_EE_EMIT_HANDLE",
            "emitHandle(event: string, handle: number): boolean",
            "emitter.emitHandle(event, handle) — passa o arg como bits f64 (handle raw).",
            __RTS_FN_GL_EE_EMIT_HANDLE as *const u8,
            false,
        ))
        .member(m(
            "removeAllListeners",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_GL_EE_REMOVE_ALL",
            "removeAllListeners(event: string): this",
            "emitter.removeAllListeners(event)",
            __RTS_FN_GL_EE_REMOVE_ALL as *const u8,
            false,
        ))
        .member(m(
            "listenerCount",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::I64),
            "__RTS_FN_GL_EE_LISTENER_COUNT",
            "listenerCount(event: string): number",
            "emitter.listenerCount(event)",
            __RTS_FN_GL_EE_LISTENER_COUNT as *const u8,
            true,
        ))
        .member(m(
            "eventNames",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_EE_EVENT_NAMES",
            "eventNames(): string[]",
            "emitter.eventNames()",
            __RTS_FN_GL_EE_EVENT_NAMES as *const u8,
            true,
        ))
        .done();
}
