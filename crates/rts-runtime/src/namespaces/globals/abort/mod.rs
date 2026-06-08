//! `AbortController` / `AbortSignal` global classes (#62).
//!
//! AbortController.signal retorna signal persistente; abort(reason) marca
//! aborted + dispara listeners. Migrado ao modelo `#[rts_class]` (stage 5) —
//! duas classes no mesmo arquivo. Helpers (new_signal/invoke_listeners) abaixo.

use indexmap::IndexMap;

use rts_abi::ty::{Bool, Handle, I64};
use rts_macro::rts_class;

use crate::namespaces::gc::handles::{alloc_entry, with_entry, with_entry_mut, Entry};

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

/// AbortController — sinal abortavel para cancelar operacoes.
#[rts_class(
    AbortController,
    prefix = "ABORT_CONTROLLER",
    spec = "ABORT_CONTROLLER_CLASS_SPEC"
)]
impl AbortControllerClass {
    /// new AbortController() — cria controller com signal vazio.
    #[rts_ctor(ts = "new AbortController()", pure)]
    pub fn new() -> Handle {
        let sig = new_signal();
        let mut m: IndexMap<String, i64> = IndexMap::new();
        m.insert("signal".to_string(), sig as i64);
        m.insert("__rts_class".to_string(), {
            alloc_entry(Entry::String(b"AbortController".to_vec())) as i64
        });
        alloc_entry(Entry::Map(Box::new(m)))
    }

    /// controller.signal — AbortSignal associado.
    #[rts_getter(ts = "readonly signal: AbortSignal", pure)]
    pub fn signal(h: Handle) -> Handle {
        with_entry(h, |e| match e {
            Some(Entry::Map(m)) => m.get("signal").copied().unwrap_or(0) as u64,
            _ => 0,
        })
    }

    /// controller.abort(reason?) — aborta signal, dispara listeners.
    #[rts_method(ts = "abort(reason?: any): void")]
    pub fn abort(h: Handle, reason: Handle) -> Handle {
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
}

/// AbortSignal — sinal abortavel observavel.
#[rts_class(AbortSignal, prefix = "ABORT_SIGNAL", spec = "ABORT_SIGNAL_CLASS_SPEC")]
impl AbortSignalClass {
    /// signal.aborted — true se ja' foi abortado.
    #[rts_getter(ts = "readonly aborted: boolean", pure)]
    pub fn aborted(h: Handle) -> Bool {
        with_entry(h, |e| match e {
            Some(Entry::Map(m)) => m.get("aborted").copied().unwrap_or(0),
            _ => 0,
        })
    }

    /// signal.reason — handle do reason passado em abort().
    #[rts_getter(ts = "readonly reason: any", pure)]
    pub fn reason(h: Handle) -> Handle {
        with_entry(h, |e| match e {
            Some(Entry::Map(m)) => m.get("reason").copied().unwrap_or(0) as u64,
            _ => 0,
        })
    }

    /// signal.addEventListener(type, fn) — so type='abort' efetivo.
    #[rts_method(
        name = "addEventListener",
        symbol = "__RTS_FN_GL_ABORT_SIGNAL_ADD_LISTENER",
        ts = "addEventListener(type: string, listener: () => void): void",
        opt_str
    )]
    pub fn add_event_listener(h: Handle, ty: Str, fn_h: Handle) {
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
    #[rts_method(
        name = "removeEventListener",
        symbol = "__RTS_FN_GL_ABORT_SIGNAL_REMOVE_LISTENER",
        ts = "removeEventListener(type: string, listener: () => void): void",
        opt_str
    )]
    pub fn remove_event_listener(h: Handle, ty: Str, fn_h: Handle) {
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
    #[rts_method(name = "throwIfAborted", ts = "throwIfAborted(): void")]
    pub fn throw_if_aborted(h: Handle) {
        let (is_ab, reason): (i64, u64) = with_entry(h, |e| match e {
            Some(Entry::Map(m)) => (
                m.get("aborted").copied().unwrap_or(0),
                m.get("reason").copied().unwrap_or(0) as u64,
            ),
            _ => (0, 0),
        });
        if is_ab != 0 {
            crate::namespaces::gc::error::__RTS_FN_RT_ERROR_SET(reason);
        }
    }

    /// AbortSignal.abort(reason?) — cria signal ja' aborted.
    #[rts_fn(
        name = "abort",
        symbol = "__RTS_FN_GL_ABORT_SIGNAL_STATIC_ABORT",
        ts = "static abort(reason?: any): AbortSignal",
        pure
    )]
    pub fn static_abort(reason: Handle) -> Handle {
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
    #[rts_fn(name = "timeout", ts = "static timeout(ms: number): AbortSignal")]
    pub fn timeout(ms: I64) -> Handle {
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
    #[rts_fn(name = "any", ts = "static any(signals: AbortSignal[]): AbortSignal")]
    pub fn any(signals_arr_h: Handle) -> Handle {
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
}
