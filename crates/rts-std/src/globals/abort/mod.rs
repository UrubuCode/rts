//! `AbortController` / `AbortSignal` global classes (#62).
//!
//! AbortController.signal retorna signal persistente; abort(reason) marca
//! aborted + dispara listeners. Duas classes no mesmo arquivo. Helpers
//! (new_signal/invoke_listeners) abaixo.
//!
//! Migrado do `#[rts_class]` (macro) pro modelo builder hand-written do
//! `rts-engine` (rumo à remoção da `rts-macro`). Os externs
//! `__RTS_FN_GL_ABORT_CONTROLLER_*` / `__RTS_FN_GL_ABORT_SIGNAL_*` +
//! `register_abort_controller_class_spec()` / `register_abort_signal_class_spec()`
//! são escritos à mão.

use indexmap::IndexMap;

use rts_engine::abi::ty::{Bool, Handle, I64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use rts_engine::heap::handles::{alloc_entry, with_entry, with_entry_mut, Entry};

fn new_signal() -> u64 {
    let listeners = alloc_entry(Entry::Vec(Box::new(Vec::new())));
    let mut m: IndexMap<String, i64> = IndexMap::new();
    m.insert("aborted".to_string(), 0);
    m.insert("reason".to_string(), 0);
    m.insert("listeners".to_string(), listeners as i64);
    m.insert("__rts_class".to_string(), {
        alloc_entry(Entry::String(b"AbortSignal".to_vec())) as i64
    });
    alloc_entry(Entry::Map(Box::new(m)))
}

fn invoke_listeners(listeners_h: u64) {
    let fps: Vec<u64> = with_entry(listeners_h, |e| match e {
        Some(Entry::Vec(v)) => v.iter().map(|&x| x as u64).collect(),
        _ => Vec::new(),
    });
    for fp in fps {
        if fp == 0 {
            continue;
        }
        let empty_args = alloc_entry(Entry::Vec(Box::new(Vec::new())));
        unsafe extern "C" {
            fn __RTS_FN_RT_INVOKE_AUTO(callee: i64, this_arg: i64, args_handle: u64) -> i64;
        }
        let _ = unsafe { __RTS_FN_RT_INVOKE_AUTO(fp as i64, 0, empty_args) };
    }
}

// ── AbortController externs (hand-written, sem macro). ───────────────────────

/// new AbortController() — cria controller com signal vazio.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ABORT_CONTROLLER_NEW() -> Handle {
    let sig = new_signal();
    let mut m: IndexMap<String, i64> = IndexMap::new();
    m.insert("signal".to_string(), sig as i64);
    m.insert("__rts_class".to_string(), {
        alloc_entry(Entry::String(b"AbortController".to_vec())) as i64
    });
    alloc_entry(Entry::Map(Box::new(m)))
}

/// controller.signal — AbortSignal associado.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ABORT_CONTROLLER_SIGNAL(h: Handle) -> Handle {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("signal").copied().unwrap_or(0) as u64,
        _ => 0,
    })
}

/// controller.abort(reason?) — aborta signal, dispara listeners.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ABORT_CONTROLLER_ABORT(h: Handle, reason: Handle) -> Handle {
    let signal_h: u64 = with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("signal").copied().unwrap_or(0) as u64,
        _ => 0,
    });
    if signal_h == 0 {
        return 0;
    }
    let listeners_h: u64 = with_entry_mut(signal_h, |e| {
        if let Some(Entry::Map(m)) = e {
            m.insert("aborted".to_string(), 1);
            m.insert("reason".to_string(), reason as i64);
            m.get("listeners").copied().unwrap_or(0) as u64
        } else {
            0
        }
    });
    invoke_listeners(listeners_h);
    0
}

// ── AbortSignal externs (hand-written, sem macro). ──────────────────────────

/// signal.aborted — true se ja' foi abortado.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ABORT_SIGNAL_ABORTED(h: Handle) -> Bool {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("aborted").copied().unwrap_or(0),
        _ => 0,
    })
}

/// signal.reason — handle do reason passado em abort().
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ABORT_SIGNAL_REASON(h: Handle) -> Handle {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("reason").copied().unwrap_or(0) as u64,
        _ => 0,
    })
}

/// signal.addEventListener(type, fn) — so type='abort' efetivo.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ABORT_SIGNAL_ADD_LISTENER(
    h: Handle,
    ty_ptr: *const u8,
    ty_len: i64,
    fn_h: Handle,
) {
    let ty = unsafe { rts_engine::abi::str_abi::from_abi(ty_ptr, ty_len) };
    if ty.unwrap_or("") != "abort" {
        return;
    }
    let listeners_h: u64 = with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("listeners").copied().unwrap_or(0) as u64,
        _ => 0,
    });
    if listeners_h == 0 {
        return;
    }
    with_entry_mut(listeners_h, |e| {
        if let Some(Entry::Vec(v)) = e {
            v.push(fn_h as i64);
        }
    });
}

/// signal.removeEventListener(type, fn)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ABORT_SIGNAL_REMOVE_LISTENER(
    h: Handle,
    ty_ptr: *const u8,
    ty_len: i64,
    fn_h: Handle,
) {
    let ty = unsafe { rts_engine::abi::str_abi::from_abi(ty_ptr, ty_len) };
    if ty.unwrap_or("") != "abort" {
        return;
    }
    let listeners_h: u64 = with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("listeners").copied().unwrap_or(0) as u64,
        _ => 0,
    });
    if listeners_h == 0 {
        return;
    }
    with_entry_mut(listeners_h, |e| {
        if let Some(Entry::Vec(v)) = e {
            v.retain(|&x| x != fn_h as i64);
        }
    });
}

/// signal.throwIfAborted() — set runtime error se aborted.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ABORT_SIGNAL_THROW_IF_ABORTED(h: Handle) {
    let (is_ab, reason): (i64, u64) = with_entry(h, |e| match e {
        Some(Entry::Map(m)) => (
            m.get("aborted").copied().unwrap_or(0),
            m.get("reason").copied().unwrap_or(0) as u64,
        ),
        _ => (0, 0),
    });
    if is_ab != 0 {
        // gc::error::__RTS_FN_RT_ERROR_SET fica no collector do rts-runtime
        // (no_mangle extern); resolve por link. Declaração canônica (`safe fn`)
        // em `crate::gc_surface` — usá-la em vez de re-declarar localmente evita
        // o warning `clashing_extern_declarations` (safe vs unsafe) sem mudar o
        // símbolo nem o link.
        crate::gc_surface::__RTS_FN_RT_ERROR_SET(reason);
    }
}

/// AbortSignal.abort(reason?) — cria signal ja' aborted.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ABORT_SIGNAL_STATIC_ABORT(reason: Handle) -> Handle {
    let sig = new_signal();
    with_entry_mut(sig, |e| {
        if let Some(Entry::Map(m)) = e {
            m.insert("aborted".to_string(), 1);
            m.insert("reason".to_string(), reason as i64);
        }
    });
    sig
}

/// AbortSignal.timeout(ms) — aborta apos ms.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ABORT_SIGNAL_TIMEOUT(ms: I64) -> Handle {
    let sig = new_signal();
    let sig_clone = sig;
    let delay = if ms > 0 { ms as u64 } else { 0 };
    std::thread::spawn(move || {
        if delay > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay));
        }
        let reason = alloc_entry(Entry::ErrorObj {
            message: "The operation timed out.".to_string(),
            name: "TimeoutError".to_string(),
            cause: 0,
        });
        let listeners_h: u64 = with_entry_mut(sig_clone, |e| {
            if let Some(Entry::Map(m)) = e {
                m.insert("aborted".to_string(), 1);
                m.insert("reason".to_string(), reason as i64);
                m.get("listeners").copied().unwrap_or(0) as u64
            } else {
                0
            }
        });
        invoke_listeners(listeners_h);
    });
    sig
}

/// AbortSignal.any(signals) — aborta quando qualquer signal abortar.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ABORT_SIGNAL_ANY(signals_arr_h: Handle) -> Handle {
    let signals: Vec<u64> = with_entry(signals_arr_h, |e| match e {
        Some(Entry::Vec(v)) => v.iter().map(|&x| x as u64).collect(),
        _ => Vec::new(),
    });
    let result = new_signal();
    for &sig_h in &signals {
        let (is_ab, reason): (i64, u64) = with_entry(sig_h, |e| match e {
            Some(Entry::Map(m)) => (
                m.get("aborted").copied().unwrap_or(0),
                m.get("reason").copied().unwrap_or(0) as u64,
            ),
            _ => (0, 0),
        });
        if is_ab != 0 {
            with_entry_mut(result, |e| {
                if let Some(Entry::Map(m)) = e {
                    m.insert("aborted".to_string(), 1);
                    m.insert("reason".to_string(), reason as i64);
                }
            });
            return result;
        }
    }
    let result_clone = result;
    let signals_clone = signals.clone();
    std::thread::spawn(move || {
        for _ in 0..10_000 {
            for &sig_h in &signals_clone {
                let (is_ab, reason): (i64, u64) = with_entry(sig_h, |e| match e {
                    Some(Entry::Map(m)) => (
                        m.get("aborted").copied().unwrap_or(0),
                        m.get("reason").copied().unwrap_or(0) as u64,
                    ),
                    _ => (0, 0),
                });
                if is_ab != 0 {
                    let listeners_h: u64 = with_entry_mut(result_clone, |e| {
                        if let Some(Entry::Map(m)) = e {
                            m.insert("aborted".to_string(), 1);
                            m.insert("reason".to_string(), reason as i64);
                            m.get("listeners").copied().unwrap_or(0) as u64
                        } else {
                            0
                        }
                    });
                    invoke_listeners(listeners_h);
                    return;
                }
            }
            std::thread::sleep(std::time::Duration::from_micros(500));
        }
    });
    result
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

/// Registra a classe global `AbortController` no motor (hand-written, sem macro).
pub fn register_abort_controller_class_spec(e: &mut Engine) {
    e.class("AbortController")
        .doc("AbortController — sinal abortavel para cancelar operacoes.")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(Vec::new(), AbiType::Handle),
            "__RTS_FN_GL_ABORT_CONTROLLER_NEW",
            "new AbortController()",
            "new AbortController() — cria controller com signal vazio.",
            __RTS_FN_GL_ABORT_CONTROLLER_NEW as *const u8,
            true,
        ))
        .member(m(
            "signal",
            MemberKind::InstanceGetter,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ABORT_CONTROLLER_SIGNAL",
            "readonly signal: AbortSignal",
            "controller.signal — AbortSignal associado.",
            __RTS_FN_GL_ABORT_CONTROLLER_SIGNAL as *const u8,
            true,
        ))
        .member(m(
            "abort",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ABORT_CONTROLLER_ABORT",
            "abort(reason?: any): void",
            "controller.abort(reason?) — aborta signal, dispara listeners.",
            __RTS_FN_GL_ABORT_CONTROLLER_ABORT as *const u8,
            false,
        ))
        .done();
}

/// Registra a classe global `AbortSignal` no motor (hand-written, sem macro).
pub fn register_abort_signal_class_spec(e: &mut Engine) {
    e.class("AbortSignal")
        .doc("AbortSignal — sinal abortavel observavel.")
        .member(m(
            "aborted",
            MemberKind::InstanceGetter,
            Sig::new(vec![AbiType::Handle], AbiType::Bool),
            "__RTS_FN_GL_ABORT_SIGNAL_ABORTED",
            "readonly aborted: boolean",
            "signal.aborted — true se ja' foi abortado.",
            __RTS_FN_GL_ABORT_SIGNAL_ABORTED as *const u8,
            true,
        ))
        .member(m(
            "reason",
            MemberKind::InstanceGetter,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ABORT_SIGNAL_REASON",
            "readonly reason: any",
            "signal.reason — handle do reason passado em abort().",
            __RTS_FN_GL_ABORT_SIGNAL_REASON as *const u8,
            true,
        ))
        .member(m(
            "addEventListener",
            MemberKind::InstanceMethod,
            Sig::new(
                vec![AbiType::Handle, AbiType::StrPtr, AbiType::Handle],
                AbiType::Void,
            ),
            "__RTS_FN_GL_ABORT_SIGNAL_ADD_LISTENER",
            "addEventListener(type: string, listener: () => void): void",
            "signal.addEventListener(type, fn) — so type='abort' efetivo.",
            __RTS_FN_GL_ABORT_SIGNAL_ADD_LISTENER as *const u8,
            false,
        ))
        .member(m(
            "removeEventListener",
            MemberKind::InstanceMethod,
            Sig::new(
                vec![AbiType::Handle, AbiType::StrPtr, AbiType::Handle],
                AbiType::Void,
            ),
            "__RTS_FN_GL_ABORT_SIGNAL_REMOVE_LISTENER",
            "removeEventListener(type: string, listener: () => void): void",
            "signal.removeEventListener(type, fn)",
            __RTS_FN_GL_ABORT_SIGNAL_REMOVE_LISTENER as *const u8,
            false,
        ))
        .member(m(
            "throwIfAborted",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Void),
            "__RTS_FN_GL_ABORT_SIGNAL_THROW_IF_ABORTED",
            "throwIfAborted(): void",
            "signal.throwIfAborted() — set runtime error se aborted.",
            __RTS_FN_GL_ABORT_SIGNAL_THROW_IF_ABORTED as *const u8,
            false,
        ))
        .member(m(
            "abort",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ABORT_SIGNAL_STATIC_ABORT",
            "static abort(reason?: any): AbortSignal",
            "AbortSignal.abort(reason?) — cria signal ja' aborted.",
            __RTS_FN_GL_ABORT_SIGNAL_STATIC_ABORT as *const u8,
            true,
        ))
        .member(m(
            "timeout",
            MemberKind::Function,
            Sig::new(vec![AbiType::I64], AbiType::Handle),
            "__RTS_FN_GL_ABORT_SIGNAL_TIMEOUT",
            "static timeout(ms: number): AbortSignal",
            "AbortSignal.timeout(ms) — aborta apos ms.",
            __RTS_FN_GL_ABORT_SIGNAL_TIMEOUT as *const u8,
            false,
        ))
        .member(m(
            "any",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ABORT_SIGNAL_ANY",
            "static any(signals: AbortSignal[]): AbortSignal",
            "AbortSignal.any(signals) — aborta quando qualquer signal abortar.",
            __RTS_FN_GL_ABORT_SIGNAL_ANY as *const u8,
            false,
        ))
        .done();
}
