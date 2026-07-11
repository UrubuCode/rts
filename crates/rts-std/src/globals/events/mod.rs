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
    max_listeners: i64,
}

impl EmitterData {
    fn new(async_mode: bool) -> Self {
        Self {
            listeners: HashMap::new(),
            async_mode,
            max_listeners: 10,
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
    let key = listener_identity(fn_ptr);
    with_emitter(handle, handle, |data| {
        if let Some(list) = data.listeners.get_mut(&event) {
            list.retain(|l| listener_identity(l.fn_ptr) != key);
        }
        handle
    })
}

/// emitter.prependListener(event, listener) — unshift to the front.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_PREPEND(
    handle: Handle,
    event_ptr: *const u8,
    event_len: i64,
    fn_ptr: U64,
) -> Handle {
    let event = unsafe { rts_engine::abi::str_abi::from_abi(event_ptr, event_len) }
        .unwrap_or("")
        .to_string();
    with_emitter(handle, handle, |data| {
        data.listeners
            .entry(event)
            .or_default()
            .insert(0, Listener { fn_ptr, once: false });
        handle
    })
}

/// emitter.prependOnceListener(event, listener).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_PREPEND_ONCE(
    handle: Handle,
    event_ptr: *const u8,
    event_len: i64,
    fn_ptr: U64,
) -> Handle {
    let event = unsafe { rts_engine::abi::str_abi::from_abi(event_ptr, event_len) }
        .unwrap_or("")
        .to_string();
    with_emitter(handle, handle, |data| {
        data.listeners
            .entry(event)
            .or_default()
            .insert(0, Listener { fn_ptr, once: true });
        handle
    })
}

/// emitter.listeners(event) / rawListeners(event) — array of listener fn words.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_LISTENERS(
    handle: Handle,
    event_ptr: *const u8,
    event_len: i64,
) -> Handle {
    let event = unsafe { rts_engine::abi::str_abi::from_abi(event_ptr, event_len) }
        .unwrap_or("")
        .to_string();
    let words: Vec<i64> = with_emitter(handle, Vec::new(), |data| {
        data.listeners
            .get(&event)
            .map(|list| list.iter().map(|l| l.fn_ptr as i64).collect())
            .unwrap_or_default()
    });
    alloc_entry(Entry::Vec(Box::new(words)))
}

/// emitter.getMaxListeners().
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_GET_MAX(handle: Handle) -> I64 {
    with_emitter(handle, 10, |data| data.max_listeners)
}

/// emitter.setMaxListeners(n) — returns `this`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_SET_MAX(handle: Handle, n: I64) -> Handle {
    with_emitter(handle, handle, |data| {
        data.max_listeners = n;
        handle
    })
}

/// The IDENTITY key of a listener value for `off`. Each `ee.on(ev, f)` /
/// `ee.off(ev, f)` reifies `f` to a FRESH `Entry::Function` slot, so comparing
/// the PolyValue words (or handles) directly never matches — the stable identity
/// of a named fn / non-capturing arrow is its underlying CODE POINTER. A
/// capture-carrying closure keeps the word itself (per-reify env — JS semantics:
/// two separately-created closures are distinct listeners anyway).
fn listener_identity(fn_ptr: u64) -> u64 {
    use rts_engine::heap::poly::{POLY_BOX_BASE, POLY_TAG_FUNCTION, POLY_TAG_SHIFT};
    let is_poly_fn = (fn_ptr & POLY_BOX_BASE) == POLY_BOX_BASE
        && ((fn_ptr >> POLY_TAG_SHIFT) & 0x7) == POLY_TAG_FUNCTION;
    let handle = if is_poly_fn {
        // 48-bit payload → full (gen|slot) handle.
        rts_engine::heap::handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(fn_ptr & 0xFFFF_FFFF_FFFF)
    } else {
        fn_ptr
    };
    rts_engine::heap::handles::with_entry(handle, |e| match e {
        Some(rts_engine::heap::handles::Entry::Function(f)) if f.bound_args.is_empty() => {
            Some(f.fn_ptr)
        }
        _ => None,
    })
    .unwrap_or(fn_ptr)
}

/// emitter.emit(event, arg) — listeners recebem `arg` como number (f64).
#[unsafe(no_mangle)]
/// Invoca um listener com 1 arg f64. Se `fn_ptr` for um Function handle (closure
/// CAPTURANTE — arrow `(x) => { ...captura... }` reificada com env), despacha
/// pelo invoker da registry (bound env + kinds). Antes o EMIT transmutava
/// SEMPRE pra `extern "C" fn(f64)` cru → handle (0x...) saltava como código →
/// ACCESS_VIOLATION. Fn nomeada / arrow sem captura = raw ptr → fast path.
/// Invoke a listener forwarding up to 4 PolyValue argument WORDS unchanged (the
/// callback bridge — a string/object/number arg keeps its tag, unlike the old
/// f64-only path). A listener is a PolyValue FUNCTION word (new engine),
/// dispatched through the codegen fn-invoke thunk.
#[inline]
fn invoke_listener(fn_ptr: u64, args: [u64; 4]) {
    use rts_engine::heap::poly::{POLY_BOX_BASE, POLY_TAG_FUNCTION, POLY_TAG_SHIFT, POLY_UNDEFINED};
    let is_poly_fn = (fn_ptr & POLY_BOX_BASE) == POLY_BOX_BASE
        && ((fn_ptr >> POLY_TAG_SHIFT) & 0x7) == POLY_TAG_FUNCTION;
    if is_poly_fn {
        crate::gc_surface::__rtsadp_fn_invoke(
            fn_ptr,
            args[0],
            args[1],
            args[2],
            args[3],
            POLY_UNDEFINED,
        );
        return;
    }
    let is_handle = rts_engine::heap::handles::with_entry(fn_ptr, |e| {
        matches!(e, Some(rts_engine::heap::handles::Entry::Function(_)))
    });
    if is_handle {
        rts_primitives::function::ops::invoke_fn_ptr_with_registry(
            fn_ptr,
            &[args[0] as i64, args[1] as i64, args[2] as i64, args[3] as i64],
        );
    }
}

pub extern "C" fn __RTS_FN_GL_EE_EMIT(
    handle: Handle,
    event_ptr: *const u8,
    event_len: i64,
    arg: U64,
) -> Bool {
    emit_impl(handle, event_ptr, event_len, [arg, undef(), undef(), undef()])
}

fn undef() -> u64 {
    rts_engine::heap::poly::POLY_UNDEFINED
}

/// Shared emit: snapshot listeners, drop `once`s, invoke each forwarding the
/// argument WORDS. `'error'` with no listener throws (Node's special case).
fn emit_impl(handle: Handle, event_ptr: *const u8, event_len: i64, args: [u64; 4]) -> Bool {
    let event = unsafe { rts_engine::abi::str_abi::from_abi(event_ptr, event_len) }
        .unwrap_or("")
        .to_string();
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
        if event == "error" {
            unsafe extern "C" {
                fn __rtsadp_throw_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64);
            }
            let msg = "Unhandled 'error' event";
            unsafe {
                __rtsadp_throw_js_error(b"Error".as_ptr(), 5, msg.as_ptr(), msg.len() as i64);
            }
        }
        return 0;
    }
    if async_mode {
        for listener in snapshot {
            rayon::spawn(move || invoke_listener(listener.fn_ptr, args));
        }
    } else {
        for listener in &snapshot {
            invoke_listener(listener.fn_ptr, args);
        }
    }
    1
}

/// emitter.emit(event) — no args.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_EMIT0(handle: Handle, ep: *const u8, el: i64) -> Bool {
    emit_impl(handle, ep, el, [undef(), undef(), undef(), undef()])
}

/// emitter.emit(event, a0, a1).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_EMIT2(handle: Handle, ep: *const u8, el: i64, a0: U64, a1: U64) -> Bool {
    emit_impl(handle, ep, el, [a0, a1, undef(), undef()])
}

/// emitter.emit(event, a0, a1, a2).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_EMIT3(handle: Handle, ep: *const u8, el: i64, a0: U64, a1: U64, a2: U64) -> Bool {
    emit_impl(handle, ep, el, [a0, a1, a2, undef()])
}

/// emitter.emitHandle(event, handle) — legacy alias forwarding the raw word.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EE_EMIT_HANDLE(handle: Handle, ep: *const u8, el: i64, arg: U64) -> Bool {
    emit_impl(handle, ep, el, [arg, undef(), undef(), undef()])
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
                vec![AbiType::Handle, AbiType::StrPtr, AbiType::PolyValue],
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
                vec![AbiType::Handle, AbiType::StrPtr, AbiType::PolyValue],
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
                vec![AbiType::Handle, AbiType::StrPtr, AbiType::PolyValue],
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
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Bool),
            "__RTS_FN_GL_EE_EMIT0",
            "emit(event: string): boolean",
            "emitter.emit(event)",
            __RTS_FN_GL_EE_EMIT0 as *const u8,
            false,
        ))
        .member(m(
            "emit",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr, AbiType::PolyValue], AbiType::Bool),
            "__RTS_FN_GL_EE_EMIT",
            "emit(event: string, a0: object): boolean",
            "emitter.emit(event, a0) — the arg is forwarded to listeners unchanged.",
            __RTS_FN_GL_EE_EMIT as *const u8,
            false,
        ))
        .member(m(
            "emit",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr, AbiType::PolyValue, AbiType::PolyValue], AbiType::Bool),
            "__RTS_FN_GL_EE_EMIT2",
            "emit(event: string, a0: object, a1: object): boolean",
            "emitter.emit(event, a0, a1)",
            __RTS_FN_GL_EE_EMIT2 as *const u8,
            false,
        ))
        .member(m(
            "emit",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr, AbiType::PolyValue, AbiType::PolyValue, AbiType::PolyValue], AbiType::Bool),
            "__RTS_FN_GL_EE_EMIT3",
            "emit(event: string, a0: object, a1: object, a2: object): boolean",
            "emitter.emit(event, a0, a1, a2)",
            __RTS_FN_GL_EE_EMIT3 as *const u8,
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
        // --- aliases + the previously-missing surface (Node parity) ---------
        .member(add_style("addListener", "__RTS_FN_GL_EE_ON", __RTS_FN_GL_EE_ON as *const u8))
        .member(add_style("removeListener", "__RTS_FN_GL_EE_OFF", __RTS_FN_GL_EE_OFF as *const u8))
        .member(add_style("prependListener", "__RTS_FN_GL_EE_PREPEND", __RTS_FN_GL_EE_PREPEND as *const u8))
        .member(add_style("prependOnceListener", "__RTS_FN_GL_EE_PREPEND_ONCE", __RTS_FN_GL_EE_PREPEND_ONCE as *const u8))
        .member(m(
            "listeners",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_GL_EE_LISTENERS",
            "listeners(event: string): Function[]",
            "emitter.listeners(event)",
            __RTS_FN_GL_EE_LISTENERS as *const u8,
            false,
        ))
        .member(m(
            "rawListeners",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_GL_EE_LISTENERS",
            "rawListeners(event: string): Function[]",
            "emitter.rawListeners(event)",
            __RTS_FN_GL_EE_LISTENERS as *const u8,
            false,
        ))
        .member(m(
            "getMaxListeners",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_EE_GET_MAX",
            "getMaxListeners(): number",
            "emitter.getMaxListeners()",
            __RTS_FN_GL_EE_GET_MAX as *const u8,
            false,
        ))
        .member(m(
            "setMaxListeners",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::Handle),
            "__RTS_FN_GL_EE_SET_MAX",
            "setMaxListeners(n: number): this",
            "emitter.setMaxListeners(n)",
            __RTS_FN_GL_EE_SET_MAX as *const u8,
            false,
        ))
        .done();
}

/// An `(event, listener)` add-style member (`this, StrPtr, PolyValue) -> this`).
fn add_style(name: &str, symbol: &str, fp: *const u8) -> Member {
    m(
        name,
        MemberKind::InstanceMethod,
        Sig::new(
            vec![AbiType::Handle, AbiType::StrPtr, AbiType::PolyValue],
            AbiType::Handle,
        ),
        symbol,
        &format!("{name}(event: string, listener: Function): this"),
        "",
        fp,
        false,
    )
}
