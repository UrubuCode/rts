//! Implementacao runtime de AbortController / AbortSignal.
//!
//! Usa Entry::Map (IndexMap<String,i64>) como storage. Signal tem fields:
//!   "aborted" -> 0/1
//!   "reason"  -> handle (0 = undefined)
//!   "listeners" -> handle de Vec<i64> (fn ptrs)

use crate::namespaces::gc::handles::{alloc_entry, with_entry, with_entry_mut, Entry};
use indexmap::IndexMap;

fn str_from_parts<'a>(ptr: i64, len: i64) -> &'a str {
    if ptr == 0 || len <= 0 {
        return "";
    }
    unsafe {
        let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
        std::str::from_utf8_unchecked(slice)
    }
}

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

/// new AbortController() — cria controller com signal vazio.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ABORT_CONTROLLER_NEW() -> u64 {
    let sig = new_signal();
    let mut m: IndexMap<String, i64> = IndexMap::new();
    m.insert("signal".to_string(), sig as i64);
    m.insert("__rts_class".to_string(), {
        alloc_entry(Entry::String(b"AbortController".to_vec())) as i64
    });
    alloc_entry(Entry::Map(Box::new(m)))
}

/// controller.signal — retorna o signal armazenado.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ABORT_CONTROLLER_SIGNAL(h: u64) -> u64 {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("signal").copied().unwrap_or(0) as u64,
        _ => 0,
    })
}

/// controller.abort(reason) — marca signal como aborted, dispara listeners.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ABORT_CONTROLLER_ABORT(h: u64, reason: u64) -> u64 {
    let signal_h: u64 = with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("signal").copied().unwrap_or(0) as u64,
        _ => 0,
    });
    if signal_h == 0 {
        return 0;
    }
    // Marca aborted=1 e reason.
    let listeners_h: u64 = with_entry_mut(signal_h, |e| {
        if let Some(Entry::Map(m)) = e {
            m.insert("aborted".to_string(), 1);
            m.insert("reason".to_string(), reason as i64);
            m.get("listeners").copied().unwrap_or(0) as u64
        } else {
            0
        }
    });
    // Dispara listeners.
    invoke_listeners(listeners_h);
    0
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
        // INVOKE_AUTO(fn_h, this=0, args=empty_vec).
        let empty_args = alloc_entry(Entry::Vec(Box::new(Vec::new())));
        unsafe extern "C" {
            fn __RTS_FN_RT_INVOKE_AUTO(callee: i64, this_arg: i64, args_handle: u64) -> i64;
        }
        let _ = unsafe { __RTS_FN_RT_INVOKE_AUTO(fp as i64, 0, empty_args) };
    }
}

/// signal.aborted — bool.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ABORT_SIGNAL_ABORTED(h: u64) -> i64 {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("aborted").copied().unwrap_or(0),
        _ => 0,
    })
}

/// signal.reason — handle (0 = undefined).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ABORT_SIGNAL_REASON(h: u64) -> u64 {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("reason").copied().unwrap_or(0) as u64,
        _ => 0,
    })
}

/// signal.addEventListener(type, fn) — armazena fn no Vec listeners se
/// type == "abort". Outros types sao no-op nesta fase.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ABORT_SIGNAL_ADD_LISTENER(
    h: u64,
    type_ptr: i64,
    type_len: i64,
    fn_h: u64,
) {
    let ty = str_from_parts(type_ptr, type_len);
    if ty != "abort" {
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

/// signal.removeEventListener — remove fn do Vec listeners (so type=abort).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ABORT_SIGNAL_REMOVE_LISTENER(
    h: u64,
    type_ptr: i64,
    type_len: i64,
    fn_h: u64,
) {
    let ty = str_from_parts(type_ptr, type_len);
    if ty != "abort" {
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

/// signal.throwIfAborted() — se aborted, lanca exception via slot de erro.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ABORT_SIGNAL_THROW_IF_ABORTED(h: u64) {
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

// ── AbortSignal static methods ────────────────────────────────────────────────

/// AbortSignal.abort(reason) — cria signal ja aborted.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ABORT_SIGNAL_STATIC_ABORT(reason: u64) -> u64 {
    let sig = new_signal();
    with_entry_mut(sig, |e| {
        if let Some(Entry::Map(m)) = e {
            m.insert("aborted".to_string(), 1);
            m.insert("reason".to_string(), reason as i64);
        }
    });
    sig
}

/// AbortSignal.any(signals) — cria signal que aborta quando qualquer
/// signal da lista abortar. Se algum ja' esta aborted, o resultado nasce
/// aborted com a mesma reason. Caso contrario, adiciona listener em cada
/// signal pra abortar o composto.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ABORT_SIGNAL_ANY(signals_arr_h: u64) -> u64 {
    let signals: Vec<u64> = with_entry(signals_arr_h, |e| match e {
        Some(Entry::Vec(v)) => v.iter().map(|&x| x as u64).collect(),
        _ => Vec::new(),
    });
    let result = new_signal();
    // Checa se algum ja' esta aborted.
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
    // Senao, adiciona um "listener bridge" em cada signal: spawn thread
    // que polla o aborted (simples mas funciona pra fixtures sync).
    // V2 ideal: forwardar via callback no addEventListener.
    let result_clone = result;
    let signals_clone = signals.clone();
    std::thread::spawn(move || {
        // Polling rapido — 500us, ate 5s.
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

/// AbortSignal.timeout(ms) — cria signal que aborta apos ms.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ABORT_SIGNAL_TIMEOUT(ms: i64) -> u64 {
    let sig = new_signal();
    let sig_clone = sig;
    let delay = if ms > 0 { ms as u64 } else { 0 };
    std::thread::spawn(move || {
        if delay > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay));
        }
        // (cross-runtime #84) Reason precisa ter `.name == "TimeoutError"`
        // (JS spec usa DOMException; Entry::ErrorObj eh suficiente para
        // satisfazer `.name`/`.message` que sao o que o spec testa).
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
