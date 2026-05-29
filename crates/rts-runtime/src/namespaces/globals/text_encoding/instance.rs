use crate::namespaces::gc::handles::{alloc_entry, with_entry, Entry};

fn str_from_parts(ptr: i64, len: i64) -> &'static str {
    if ptr == 0 || len == 0 {
        return "";
    }
    unsafe {
        let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
        std::str::from_utf8_unchecked(slice)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TEXTENC_ENCODE(ptr: i64, len: i64) -> u64 {
    let s = str_from_parts(ptr, len);
    alloc_entry(Entry::Buffer(s.as_bytes().to_vec()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TEXTENC_DECODE(buf_handle: u64) -> u64 {
    let bytes = with_entry(buf_handle, |entry| match entry {
        Some(Entry::Buffer(v)) | Some(Entry::String(v)) => Some(v.clone()),
        _ => None,
    });
    match bytes {
        Some(b) => alloc_entry(Entry::String(b)),
        None => 0,
    }
}

const B64_ALPHA: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn b64_encode(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity((bytes.len() + 2) / 3 * 4);
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let (b0, b1, b2) = (bytes[i], bytes[i + 1], bytes[i + 2]);
        out.push(B64_ALPHA[(b0 >> 2) as usize]);
        out.push(B64_ALPHA[(((b0 & 3) << 4) | (b1 >> 4)) as usize]);
        out.push(B64_ALPHA[(((b1 & 15) << 2) | (b2 >> 6)) as usize]);
        out.push(B64_ALPHA[(b2 & 63) as usize]);
        i += 3;
    }
    match bytes.len() - i {
        1 => {
            let b0 = bytes[i];
            out.push(B64_ALPHA[(b0 >> 2) as usize]);
            out.push(B64_ALPHA[((b0 & 3) << 4) as usize]);
            out.push(b'=');
            out.push(b'=');
        }
        2 => {
            let (b0, b1) = (bytes[i], bytes[i + 1]);
            out.push(B64_ALPHA[(b0 >> 2) as usize]);
            out.push(B64_ALPHA[(((b0 & 3) << 4) | (b1 >> 4)) as usize]);
            out.push(B64_ALPHA[((b1 & 15) << 2) as usize]);
            out.push(b'=');
        }
        _ => {}
    }
    out
}

fn b64_decode(s: &[u8]) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            b'=' => Some(0),
            _ => None,
        }
    }
    let s: Vec<u8> = s.iter().copied().filter(|&c| c != b'\n' && c != b'\r').collect();
    if s.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let mut i = 0;
    while i < s.len() {
        let a = val(s[i])?;
        let b = val(s[i + 1])?;
        let c = val(s[i + 2])?;
        let d = val(s[i + 3])?;
        out.push((a << 2) | (b >> 4));
        if s[i + 2] != b'=' {
            out.push((b << 4) | (c >> 2));
        }
        if s[i + 3] != b'=' {
            out.push((c << 6) | d);
        }
        i += 4;
    }
    Some(out)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TEXTENC_BTOA(ptr: i64, len: i64) -> u64 {
    let s = str_from_parts(ptr, len);
    let encoded = b64_encode(s.as_bytes());
    alloc_entry(Entry::String(encoded))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TEXTENC_ATOB(ptr: i64, len: i64) -> u64 {
    let s = str_from_parts(ptr, len);
    match b64_decode(s.as_bytes()) {
        Some(decoded) => alloc_entry(Entry::String(decoded)),
        None => 0,
    }
}

/// (#316) Helper recursivo: clona handle preservando set_kind/map_kind
/// flags. Slots que parecem handles validos sao clonados recursivamente.
/// `visited` mapeia handle_original -> handle_clone para suportar
/// self-references (JS spec do structuredClone preserva ciclos).
fn clone_handle_deep(
    handle: u64,
    visited: &mut std::collections::HashMap<u64, u64>,
) -> u64 {
    if let Some(&existing) = visited.get(&handle) {
        return existing;
    }
    let entry_clone = with_entry(handle, |entry| match entry {
        Some(Entry::String(v)) => Some(Entry::String(v.clone())),
        Some(Entry::Buffer(v)) => Some(Entry::Buffer(v.clone())),
        Some(Entry::Vec(v)) => Some(Entry::Vec(v.clone())),
        Some(Entry::Map(m)) => Some(Entry::Map(m.clone())),
        Some(Entry::Json(j)) => Some(Entry::Json(j.clone())),
        Some(Entry::DateMs(ms)) => Some(Entry::DateMs(*ms)),
        // Regex nao tem Clone — passa handle original (shared, imutavel).
        _ => None,
    });
    let Some(entry) = entry_clone else { return handle; };
    let new_h = alloc_entry(entry);
    visited.insert(handle, new_h);
    // Preserva kind flags
    if crate::namespaces::collections::map::handle_is_set_kind(handle) {
        crate::namespaces::collections::map::mark_set_kind(new_h);
    }
    // Deep clone de slots que sao handles a estruturas clonaveis.
    use crate::namespaces::gc::handles::with_entry_mut;
    let _ = with_entry_mut(new_h, |entry| match entry {
        Some(Entry::Map(m)) => {
            let pairs: Vec<(String, i64)> = m.iter().map(|(k, v)| (k.clone(), *v)).collect();
            for (k, v) in pairs {
                let v_u = v as u64;
                if v_u > 0xFFFF_FFFF {
                    let v_kind = with_entry(v_u, |e| matches!(
                        e,
                        Some(Entry::Map(_)) | Some(Entry::Vec(_)) | Some(Entry::String(_))
                        | Some(Entry::Buffer(_)) | Some(Entry::Json(_))
                        | Some(Entry::DateMs(_)) | Some(Entry::Regex(_))
                    ));
                    if v_kind {
                        let cloned = clone_handle_deep(v_u, visited);
                        m.insert(k, cloned as i64);
                    }
                }
            }
        }
        Some(Entry::Vec(v)) => {
            for slot in v.iter_mut() {
                let s_u = *slot as u64;
                if s_u > 0xFFFF_FFFF {
                    let v_kind = with_entry(s_u, |e| matches!(
                        e,
                        Some(Entry::Map(_)) | Some(Entry::Vec(_)) | Some(Entry::String(_))
                        | Some(Entry::Buffer(_)) | Some(Entry::Json(_))
                        | Some(Entry::DateMs(_)) | Some(Entry::Regex(_))
                    ));
                    if v_kind {
                        *slot = clone_handle_deep(s_u, visited) as i64;
                    }
                }
            }
        }
        _ => {}
    });
    new_h
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TEXTENC_STRUCTURED_CLONE(handle: u64) -> u64 {
    let mut visited = std::collections::HashMap::new();
    clone_handle_deep(handle, &mut visited)
}

type CallbackFn = unsafe extern "C" fn(i64) -> i64;

/// (cross-runtime #56) Microtask queue thread-local. queueMicrotask
/// enfileira o callback; ele eh drenado no fim do task corrente (top-level
/// __RTS_MAIN ou apos um await).
///
/// Suporta dois tipos:
/// - `Bare(fp)`: queueMicrotask(cb) — invoca fp() sem args.
/// - `SettledThen { ... }`: Promise.then com promise ja' settled — invoca
///   fp(value), settle o result slot com retorno (ou propaga rejection).
use std::cell::RefCell;
pub(crate) enum Microtask {
    Bare(u64),
    SettledThen {
        fn_ptr: u64,
        bound: Vec<i64>,
        value: i64,
        fulfilled: bool,
        result_slot: std::sync::Arc<crate::namespaces::gc::handles::PromiseSlot>,
    },
    /// promise.finally(fp) fast-path: invoca fp() sem args, preserva state/value
    /// original (resolve com value se fulfilled, reject se rejected).
    SettledFinally {
        fn_ptr: u64,
        value: i64,
        fulfilled: bool,
        result_slot: std::sync::Arc<crate::namespaces::gc::handles::PromiseSlot>,
    },
    /// (#207) Promise.then sobre promise PENDING — em vez de spawn_blocking
    /// (thread nao-deterministica), faz polling na microtask queue: a cada
    /// drain, se a source ainda esta pending re-enfileira; quando settle,
    /// invoca o callback e settle o result. Preserva ordem FIFO determinista
    /// (JS spec) entre chains de Promise no mesmo task sync.
    PendingThen {
        source: std::sync::Arc<crate::namespaces::gc::handles::PromiseSlot>,
        fn_ptr: u64,
        bound: Vec<i64>,
        is_catch: bool,
        result_slot: std::sync::Arc<crate::namespaces::gc::handles::PromiseSlot>,
    },
    /// (#207) promise.finally sobre source PENDING — polling determinista.
    /// Quando settle, invoca fp() sem args e PRESERVA state/value original.
    PendingFinally {
        source: std::sync::Arc<crate::namespaces::gc::handles::PromiseSlot>,
        fn_ptr: u64,
        result_slot: std::sync::Arc<crate::namespaces::gc::handles::PromiseSlot>,
    },
}
thread_local! {
    static MICROTASK_QUEUE: RefCell<Vec<Microtask>> = const { RefCell::new(Vec::new()) };
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TEXTENC_QUEUE_MICROTASK(fp: u64) {
    // (cross-runtime #56) JS spec: queueMicrotask enfileira; drena no fim
    // do script sync. Antes executava inline para "preservar FIFO com
    // Promise.then fast-path", mas isso quebrava ordem com sync:end
    // (callback rodava antes de continuar o top-level).
    if fp != 0 {
        MICROTASK_QUEUE.with(|q| q.borrow_mut().push(Microtask::Bare(fp)));
    }
}

/// (cross-runtime #56/#285) Enfileira microtask de Promise.then com promise
/// ja' settled. Quando drenado, invoca o callback (se fp != 0) e settle
/// result_slot com o retorno; se promise rejeitada, propaga reject.
pub fn enqueue_microtask_settled(
    fn_ptr: u64,
    bound: Vec<i64>,
    value: i64,
    fulfilled: bool,
    result_slot: std::sync::Arc<crate::namespaces::gc::handles::PromiseSlot>,
) {
    MICROTASK_QUEUE.with(|q| {
        q.borrow_mut().push(Microtask::SettledThen {
            fn_ptr,
            bound,
            value,
            fulfilled,
            result_slot,
        })
    });
}

/// Enfileira finally(fp) com promise ja settled. Quando drenado, invoca
/// fp() sem args e propaga state/value original ao result_slot.
pub fn enqueue_microtask_finally(
    fn_ptr: u64,
    value: i64,
    fulfilled: bool,
    result_slot: std::sync::Arc<crate::namespaces::gc::handles::PromiseSlot>,
) {
    MICROTASK_QUEUE.with(|q| {
        q.borrow_mut().push(Microtask::SettledFinally {
            fn_ptr,
            value,
            fulfilled,
            result_slot,
        })
    });
}

/// (#207) Enfileira `.then`/`.catch` sobre promise PENDING como PendingThen
/// (polling determinista no drain). Evita spawn_blocking nao-deterministico.
pub fn enqueue_microtask_pending_then(
    source: std::sync::Arc<crate::namespaces::gc::handles::PromiseSlot>,
    fn_ptr: u64,
    bound: Vec<i64>,
    is_catch: bool,
    result_slot: std::sync::Arc<crate::namespaces::gc::handles::PromiseSlot>,
) {
    MICROTASK_QUEUE.with(|q| {
        q.borrow_mut().push(Microtask::PendingThen {
            source, fn_ptr, bound, is_catch, result_slot,
        })
    });
}

/// (#207) Enfileira `.finally` sobre promise PENDING (polling determinista).
pub fn enqueue_microtask_pending_finally(
    source: std::sync::Arc<crate::namespaces::gc::handles::PromiseSlot>,
    fn_ptr: u64,
    result_slot: std::sync::Arc<crate::namespaces::gc::handles::PromiseSlot>,
) {
    MICROTASK_QUEUE.with(|q| {
        q.borrow_mut().push(Microtask::PendingFinally { source, fn_ptr, result_slot })
    });
}

/// Drena microtasks pendentes. Chamada pelo pipeline pos-main e tambem
/// pode ser chamada pelo codegen no fim de cada task (futuro).
pub fn drain_microtasks() {
    use crate::namespaces::gc::promise_slot;
    // (#207) Guard contra loop infinito: se varios ciclos so' contem
    // PendingThen cuja source nunca settla (Promise pending por I/O real que
    // resolveria numa thread tokio), faz fallback p/ spawn_blocking nesses
    // restantes e encerra o polling. Caso sync (chains resolviveis no drain)
    // nunca atinge o limite.
    let mut stall = 0u32;
    const STALL_LIMIT: u32 = 10_000;
    loop {
        let queue: Vec<Microtask> =
            MICROTASK_QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()));
        if queue.is_empty() {
            break;
        }
        // Detecta ciclo sem progresso: todos PendingThen ainda pending.
        let all_stalled_pending = queue.iter().all(|t| match t {
            Microtask::PendingThen { source, .. }
            | Microtask::PendingFinally { source, .. } => {
                promise_slot::current_state(source) == promise_slot::STATE_PENDING
            }
            _ => false,
        });
        if all_stalled_pending {
            stall += 1;
            if stall >= STALL_LIMIT {
                // Fallback: resolve os PendingThen restantes via spawn_blocking
                // (caminho thread original) p/ Promises que dependem de I/O.
                for task in queue {
                    if let Microtask::PendingThen { source, fn_ptr, bound, is_catch, result_slot } = task {
                        let rt = crate::runtime::async_rt::handle();
                        rt.spawn_blocking(move || {
                            let (st, value) = promise_slot::wait_blocking(&source);
                            let fulfilled = st == promise_slot::STATE_FULFILLED;
                            let runs = if is_catch { !fulfilled } else { fulfilled };
                            if runs && fn_ptr != 0 {
                                let r = unsafe { invoke_microtask_callback(fn_ptr, &bound, Some(value)) };
                                promise_slot::resolve(&result_slot, r);
                            } else if fulfilled {
                                promise_slot::resolve(&result_slot, value);
                            } else {
                                promise_slot::reject(&result_slot, value);
                            }
                        });
                    } else if let Microtask::PendingFinally { source, fn_ptr, result_slot } = task {
                        let rt = crate::runtime::async_rt::handle();
                        rt.spawn_blocking(move || {
                            let (st, value) = promise_slot::wait_blocking(&source);
                            if fn_ptr != 0 {
                                let _ = unsafe { invoke_microtask_callback(fn_ptr, &[], None) };
                            }
                            if st == promise_slot::STATE_FULFILLED {
                                promise_slot::resolve(&result_slot, value);
                            } else {
                                promise_slot::reject(&result_slot, value);
                            }
                        });
                    }
                }
                break;
            }
        } else {
            stall = 0;
        }
        for task in queue {
            match task {
                Microtask::Bare(fp) => {
                    if fp != 0 {
                        unsafe {
                            (std::mem::transmute::<u64, CallbackFn>(fp))(0);
                        }
                    }
                }
                Microtask::SettledThen {
                    fn_ptr,
                    bound,
                    value,
                    fulfilled,
                    result_slot,
                } => {
                    if fulfilled {
                        if fn_ptr == 0 {
                            promise_slot::resolve(&result_slot, value);
                        } else {
                            let r = unsafe {
                                invoke_microtask_callback(fn_ptr, &bound, Some(value))
                            };
                            promise_slot::resolve(&result_slot, r);
                        }
                    } else {
                        // catch path nao usa este enqueue ainda; reject direto.
                        promise_slot::reject(&result_slot, value);
                    }
                }
                Microtask::SettledFinally {
                    fn_ptr,
                    value,
                    fulfilled,
                    result_slot,
                } => {
                    if fn_ptr != 0 {
                        let _ = unsafe {
                            invoke_microtask_callback(fn_ptr, &[], None)
                        };
                    }
                    if fulfilled {
                        promise_slot::resolve(&result_slot, value);
                    } else {
                        promise_slot::reject(&result_slot, value);
                    }
                }
                // (#207) Polling determinista de .then sobre source pending.
                Microtask::PendingThen {
                    source,
                    fn_ptr,
                    bound,
                    is_catch,
                    result_slot,
                } => {
                    let st = promise_slot::current_state(&source);
                    if st == promise_slot::STATE_PENDING {
                        // Ainda nao settled — re-enfileira no FIM da fila atual
                        // p/ re-checar no proximo ciclo do drain (apos outras
                        // microtasks avancarem o estado das chains).
                        MICROTASK_QUEUE.with(|q| {
                            q.borrow_mut().push(Microtask::PendingThen {
                                source, fn_ptr, bound, is_catch, result_slot,
                            })
                        });
                    } else {
                        let value = promise_slot::current_value(&source);
                        let fulfilled = st == promise_slot::STATE_FULFILLED;
                        // .then: roda no fulfilled; .catch: roda no rejected.
                        let runs = if is_catch { !fulfilled } else { fulfilled };
                        if runs && fn_ptr != 0 {
                            let r = unsafe {
                                invoke_microtask_callback(fn_ptr, &bound, Some(value))
                            };
                            promise_slot::resolve(&result_slot, r);
                        } else if fulfilled {
                            promise_slot::resolve(&result_slot, value);
                        } else {
                            promise_slot::reject(&result_slot, value);
                        }
                    }
                }
                Microtask::PendingFinally { source, fn_ptr, result_slot } => {
                    let st = promise_slot::current_state(&source);
                    if st == promise_slot::STATE_PENDING {
                        MICROTASK_QUEUE.with(|q| {
                            q.borrow_mut().push(Microtask::PendingFinally {
                                source, fn_ptr, result_slot,
                            })
                        });
                    } else {
                        if fn_ptr != 0 {
                            let _ = unsafe { invoke_microtask_callback(fn_ptr, &[], None) };
                        }
                        let value = promise_slot::current_value(&source);
                        if st == promise_slot::STATE_FULFILLED {
                            promise_slot::resolve(&result_slot, value);
                        } else {
                            promise_slot::reject(&result_slot, value);
                        }
                    }
                }
            }
        }
    }
}

/// Cópia local de invoke_callback (promise/ops.rs) para evitar dep
/// circular do módulo. Aridade ate 8 com bound_args prepended.
unsafe fn invoke_microtask_callback(
    fn_ptr: u64,
    bound: &[i64],
    extra: Option<i64>,
) -> i64 {
    use std::mem::transmute;
    let mut args: Vec<i64> = bound.to_vec();
    if let Some(v) = extra {
        args.push(v);
    }
    unsafe {
        match args.len() {
            0 => transmute::<u64, extern "C" fn() -> i64>(fn_ptr)(),
            1 => transmute::<u64, extern "C" fn(i64) -> i64>(fn_ptr)(args[0]),
            2 => transmute::<u64, extern "C" fn(i64, i64) -> i64>(fn_ptr)(args[0], args[1]),
            3 => transmute::<u64, extern "C" fn(i64, i64, i64) -> i64>(fn_ptr)(
                args[0], args[1], args[2],
            ),
            _ => 0,
        }
    }
}

// TextEncoder / TextDecoder constructors — stateless, token handle.
// encode/decode são chamados com (self_handle, str_ptr, str_len) no instance path
// mas o self é ignorado; a impl real está em ENCODE/DECODE acima.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TEXTENC_NEW() -> u64 {
    alloc_entry(Entry::Env(vec![1])) // token "TextEncoder"
}

/// (cross-runtime #874) Aceita label opcional como (ptr, len). Em RTS so'
/// UTF-8 e' suportado; o label e' aceito mas ignorado (Bun/Node aceitam
/// `new TextDecoder("utf-8")` sem erro).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TEXTDEC_NEW(_label_ptr: i64, _label_len: i64) -> u64 {
    alloc_entry(Entry::Env(vec![2])) // token "TextDecoder"
}

// Instance method variants: (self_handle, ptr, len) — self ignored.
// Usados pelos GlobalClassSpec (encode/decode em receiver this).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TEXTENC_ENCODE_INSTANCE(
    _self_h: u64,
    ptr: i64,
    len: i64,
) -> u64 {
    __RTS_FN_GL_TEXTENC_ENCODE(ptr, len)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TEXTDEC_DECODE_INSTANCE(_self_h: u64, buf_h: u64) -> u64 {
    __RTS_FN_GL_TEXTENC_DECODE(buf_h)
}
