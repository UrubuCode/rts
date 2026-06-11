use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};

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

const B64_ALPHA: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

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
    let s: Vec<u8> = s
        .iter()
        .copied()
        .filter(|&c| c != b'\n' && c != b'\r')
        .collect();
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
fn clone_handle_deep(handle: u64, visited: &mut std::collections::HashMap<u64, u64>) -> u64 {
    if let Some(&existing) = visited.get(&handle) {
        return existing;
    }
    // (#225 structuredClone) RegExp: clona recompilando source+flags num novo
    // handle (objeto DIFERENTE, `clone !== original`). RtsRegex nao deriva
    // Clone; extrai source/flags e recompila via REGEX_COMPILE.
    {
        let rx_info: Option<(String, String)> = with_entry(handle, |entry| match entry {
            Some(Entry::Regex(rx)) => Some((rx.engine.source(), rx.flags.clone())),
            _ => None,
        });
        if let Some((src, flags)) = rx_info {
            let new_h = rts_shared::regex::__RTS_FN_NS_REGEX_COMPILE(
                src.as_ptr(),
                src.len() as i64,
                flags.as_ptr(),
                flags.len() as i64,
            );
            visited.insert(handle, new_h);
            return new_h;
        }
    }
    let entry_clone = with_entry(handle, |entry| match entry {
        Some(Entry::String(v)) => Some(Entry::String(v.clone())),
        Some(Entry::Buffer(v)) => Some(Entry::Buffer(v.clone())),
        Some(Entry::Vec(v)) => Some(Entry::Vec(v.clone())),
        Some(Entry::Map(m)) => Some(Entry::Map(m.clone())),
        Some(Entry::Json(j)) => Some(Entry::Json(j.clone())),
        Some(Entry::DateMs(ms)) => Some(Entry::DateMs(*ms)),
        // (#1068) RegExp: clona para um novo handle (identidade distinta do
        // original — `structuredClone(re) === re` deve ser `false`).
        Some(Entry::Regex(r)) => Some(Entry::Regex(r.clone())),
        _ => None,
    });
    let Some(entry) = entry_clone else {
        return handle;
    };
    let new_h = alloc_entry(entry);
    visited.insert(handle, new_h);
    // Preserva kind flags
    if rts_shared::collections::map::handle_is_set_kind(handle) {
        rts_shared::collections::map::mark_set_kind(new_h);
    }
    // Deep clone de slots que sao handles a estruturas clonaveis.
    use rts_engine::heap::handles::with_entry_mut;
    let _ = with_entry_mut(new_h, |entry| match entry {
        Some(Entry::Map(m)) => {
            let pairs: Vec<(String, i64)> = m.iter().map(|(k, v)| (k.clone(), *v)).collect();
            for (k, v) in pairs {
                let v_u = v as u64;
                if v_u > 0xFFFF_FFFF {
                    let v_kind = with_entry(v_u, |e| {
                        matches!(
                            e,
                            Some(Entry::Map(_))
                                | Some(Entry::Vec(_))
                                | Some(Entry::String(_))
                                | Some(Entry::Buffer(_))
                                | Some(Entry::Json(_))
                                | Some(Entry::DateMs(_))
                                | Some(Entry::Regex(_))
                        )
                    });
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
                    let v_kind = with_entry(s_u, |e| {
                        matches!(
                            e,
                            Some(Entry::Map(_))
                                | Some(Entry::Vec(_))
                                | Some(Entry::String(_))
                                | Some(Entry::Buffer(_))
                                | Some(Entry::Json(_))
                                | Some(Entry::DateMs(_))
                                | Some(Entry::Regex(_))
                        )
                    });
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
#[derive(Clone)]
pub(crate) enum Microtask {
    Bare(u64),
    SettledThen {
        fn_ptr: u64,
        bound: Vec<i64>,
        value: i64,
        fulfilled: bool,
        result_slot: std::sync::Arc<rts_engine::heap::handles::PromiseSlot>,
    },
    /// promise.finally(fp) fast-path: invoca fp() sem args, preserva state/value
    /// original (resolve com value se fulfilled, reject se rejected).
    SettledFinally {
        fn_ptr: u64,
        value: i64,
        fulfilled: bool,
        result_slot: std::sync::Arc<rts_engine::heap::handles::PromiseSlot>,
    },
    /// (#207) Promise.then sobre promise PENDING — em vez de spawn_blocking
    /// (thread nao-deterministica), faz polling na microtask queue: a cada
    /// drain, se a source ainda esta pending re-enfileira; quando settle,
    /// invoca o callback e settle o result. Preserva ordem FIFO determinista
    /// (JS spec) entre chains de Promise no mesmo task sync.
    PendingThen {
        source: std::sync::Arc<rts_engine::heap::handles::PromiseSlot>,
        fn_ptr: u64,
        bound: Vec<i64>,
        is_catch: bool,
        result_slot: std::sync::Arc<rts_engine::heap::handles::PromiseSlot>,
    },
    /// (cross-runtime #393/#116) `.then(onFul, onRej)` instance sobre source
    /// PENDING — variante 2-callback do PendingThen. Quando a source settla:
    /// fulfilled invoca `on_ful` (ou propaga value se 0), rejected invoca
    /// `on_rej` recuperando (ou propaga reject se 0). Substitui o antigo
    /// spawn_blocking nao-deterministico do path instance, dando interleaving
    /// FIFO correto entre chains (`.then().then()`).
    PendingThen2 {
        source: std::sync::Arc<rts_engine::heap::handles::PromiseSlot>,
        on_ful: u64,
        on_rej: u64,
        result_slot: std::sync::Arc<rts_engine::heap::handles::PromiseSlot>,
    },
    /// (#207) promise.finally sobre source PENDING — polling determinista.
    /// Quando settle, invoca fp() sem args e PRESERVA state/value original.
    PendingFinally {
        source: std::sync::Arc<rts_engine::heap::handles::PromiseSlot>,
        fn_ptr: u64,
        result_slot: std::sync::Arc<rts_engine::heap::handles::PromiseSlot>,
    },
    /// (#207 async-SM) Retomada de uma `async function` suspensa em `await`.
    /// `source` eh a promise awaited; enquanto pending, re-enfileira; quando
    /// settla, injeta o valor/erro no GenState `gen_handle` e roda o proximo
    /// passo da SM. Produz o interleaving cooperativo de 393.
    AsyncResume {
        gen_handle: u64,
        source: std::sync::Arc<rts_engine::heap::handles::PromiseSlot>,
    },
}
thread_local! {
    static MICROTASK_QUEUE: RefCell<Vec<Microtask>> = const { RefCell::new(Vec::new()) };
    /// (cross-runtime #344/#393) The batch currently being processed by
    /// `drain_microtasks` — moved out of MICROTASK_QUEUE for iteration. Kept here
    /// (not just a local) so `mark_microtask_roots` marks handles held by the
    /// IN-FLIGHT microtask while its callback allocates (and may trigger a GC
    /// tick). Without this the executing task's handles (e.g. a generator being
    /// driven) get swept mid-callback → use-after-free / infinite loops.
    static MICROTASK_INFLIGHT: RefCell<Vec<Microtask>> = const { RefCell::new(Vec::new()) };
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
    result_slot: std::sync::Arc<rts_engine::heap::handles::PromiseSlot>,
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
    result_slot: std::sync::Arc<rts_engine::heap::handles::PromiseSlot>,
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
    source: std::sync::Arc<rts_engine::heap::handles::PromiseSlot>,
    fn_ptr: u64,
    bound: Vec<i64>,
    is_catch: bool,
    result_slot: std::sync::Arc<rts_engine::heap::handles::PromiseSlot>,
) {
    MICROTASK_QUEUE.with(|q| {
        q.borrow_mut().push(Microtask::PendingThen {
            source,
            fn_ptr,
            bound,
            is_catch,
            result_slot,
        })
    });
}

/// (cross-runtime #393/#116) Enfileira `.then(onFul, onRej)` instance sobre
/// source PENDING como PendingThen2 (polling determinista no drain).
pub fn enqueue_microtask_pending_then2(
    source: std::sync::Arc<rts_engine::heap::handles::PromiseSlot>,
    on_ful: u64,
    on_rej: u64,
    result_slot: std::sync::Arc<rts_engine::heap::handles::PromiseSlot>,
) {
    MICROTASK_QUEUE.with(|q| {
        q.borrow_mut().push(Microtask::PendingThen2 {
            source,
            on_ful,
            on_rej,
            result_slot,
        })
    });
}

/// (#207) Enfileira `.finally` sobre promise PENDING (polling determinista).
pub fn enqueue_microtask_pending_finally(
    source: std::sync::Arc<rts_engine::heap::handles::PromiseSlot>,
    fn_ptr: u64,
    result_slot: std::sync::Arc<rts_engine::heap::handles::PromiseSlot>,
) {
    MICROTASK_QUEUE.with(|q| {
        q.borrow_mut().push(Microtask::PendingFinally {
            source,
            fn_ptr,
            result_slot,
        })
    });
}

/// (#207 async-SM) Enfileira a retomada de uma async fn suspensa em `await`
/// sobre a promise `source`. Quando `source` settla, o drain injeta o valor no
/// GenState e roda o proximo passo da SM (interleaving cooperativo do 393).
pub fn enqueue_microtask_async_resume(
    gen_handle: u64,
    source: std::sync::Arc<rts_engine::heap::handles::PromiseSlot>,
) {
    MICROTASK_QUEUE.with(|q| {
        q.borrow_mut()
            .push(Microtask::AsyncResume { gen_handle, source })
    });
}

/// Drena microtasks pendentes. Chamada pelo pipeline pos-main e tambem
/// pode ser chamada pelo codegen no fim de cada task (futuro).
/// (cross-runtime #344/#393) GC root marking for the microtask queue. Handles
/// captured by queued microtasks (callback closures' bound args, settled values,
/// promise-slot values, the async-resume GenState) live ONLY in this heap queue
/// — not on any scanned stack — so a GC tick during synchronous code (e.g. the
/// many allocations before an `await`/`.then` drains) would sweep them, leaving
/// the async drive operating on freed handles (e.g. a generator that never
/// reports `done` → infinite microtask loop). `finish_cycle` calls this before
/// sweeping so everything reachable from a pending microtask survives.
/// `mark_handle` is transitive, so marking a closure handle covers its captures.
pub fn mark_microtask_roots() {
    use rts_engine::heap::handles::mark_handle;
    use crate::promise_slot;
    let mark_slot = |s: &std::sync::Arc<rts_engine::heap::handles::PromiseSlot>| {
        mark_handle(promise_slot::current_value(s) as u64);
    };
    let mark_task = |t: &Microtask| match t {
        Microtask::Bare(fp) => mark_handle(*fp),
        Microtask::SettledThen {
            fn_ptr,
            bound,
            value,
            result_slot,
            ..
        } => {
            mark_handle(*fn_ptr);
            for b in bound {
                mark_handle(*b as u64);
            }
            mark_handle(*value as u64);
            mark_slot(result_slot);
        }
        Microtask::SettledFinally {
            fn_ptr,
            value,
            result_slot,
            ..
        } => {
            mark_handle(*fn_ptr);
            mark_handle(*value as u64);
            mark_slot(result_slot);
        }
        Microtask::PendingThen {
            source,
            fn_ptr,
            bound,
            result_slot,
            ..
        } => {
            mark_slot(source);
            mark_handle(*fn_ptr);
            for b in bound {
                mark_handle(*b as u64);
            }
            mark_slot(result_slot);
        }
        Microtask::PendingThen2 {
            source,
            on_ful,
            on_rej,
            result_slot,
        } => {
            mark_slot(source);
            mark_handle(*on_ful);
            mark_handle(*on_rej);
            mark_slot(result_slot);
        }
        Microtask::PendingFinally {
            source,
            fn_ptr,
            result_slot,
        } => {
            mark_slot(source);
            mark_handle(*fn_ptr);
            mark_slot(result_slot);
        }
        Microtask::AsyncResume { gen_handle, source } => {
            mark_handle(*gen_handle);
            mark_slot(source);
        }
    };
    MICROTASK_QUEUE.with(|q| {
        for t in q.borrow().iter() {
            mark_task(t);
        }
    });
    MICROTASK_INFLIGHT.with(|q| {
        for t in q.borrow().iter() {
            mark_task(t);
        }
    });
}

pub fn drain_microtasks() {
    use crate::promise_slot;
    // (#207) Guard contra loop infinito: se varios ciclos so' contem
    // PendingThen cuja source nunca settla (Promise pending por I/O real que
    // resolveria numa thread tokio), faz fallback p/ spawn_blocking nesses
    // restantes e encerra o polling. Caso sync (chains resolviveis no drain)
    // nunca atinge o limite.
    let mut stall = 0u32;
    const STALL_LIMIT: u32 = 10_000;
    loop {
        let queue: Vec<Microtask> = MICROTASK_QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()));
        if queue.is_empty() {
            break;
        }
        // Detecta ciclo sem progresso: todos PendingThen ainda pending.
        let all_stalled_pending = queue.iter().all(|t| match t {
            Microtask::PendingThen { source, .. }
            | Microtask::PendingThen2 { source, .. }
            | Microtask::PendingFinally { source, .. }
            | Microtask::AsyncResume { source, .. } => {
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
                    if let Microtask::PendingThen {
                        source,
                        fn_ptr,
                        bound,
                        is_catch,
                        result_slot,
                    } = task
                    {
                        let rt = crate::runtime::async_rt::handle();
                        rt.spawn_blocking(move || {
                            let (st, value) = promise_slot::wait_blocking(&source);
                            let fulfilled = st == promise_slot::STATE_FULFILLED;
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
                        });
                    } else if let Microtask::PendingFinally {
                        source,
                        fn_ptr,
                        result_slot,
                    } = task
                    {
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
                    } else if let Microtask::PendingThen2 {
                        source,
                        on_ful,
                        on_rej,
                        result_slot,
                    } = task
                    {
                        let rt = crate::runtime::async_rt::handle();
                        rt.spawn_blocking(move || {
                            let (st, value) = promise_slot::wait_blocking(&source);
                            let fulfilled = st == promise_slot::STATE_FULFILLED;
                            let cb = if fulfilled { on_ful } else { on_rej };
                            if cb != 0 {
                                let r = unsafe { invoke_microtask_callback(cb, &[], Some(value)) };
                                promise_slot::resolve(&result_slot, r);
                            } else if fulfilled {
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
        // (cross-runtime #393/#116) Snapshot do estado das sources NO INICIO
        // do batch. JS spec: uma reaction de Promise roda no tick SEGUINTE ao
        // settle da source. Se a source settla DURANTE este batch (uma
        // microtask anterior resolveu o result_slot que e' source desta), a
        // reaction tem que esperar o proximo batch — nao colapsar no mesmo
        // tick. Sem isto, `Promise.resolve().then(a).then(b)` roda a e b no
        // mesmo tick e quebra o interleaving FIFO entre chains paralelas.
        // AsyncResume fica de fora (mantem comportamento que ja' passa).
        let settled_at_start: Vec<bool> = queue
            .iter()
            .map(|t| match t {
                Microtask::PendingThen { source, .. }
                | Microtask::PendingThen2 { source, .. }
                | Microtask::PendingFinally { source, .. } => {
                    promise_slot::current_state(source) != promise_slot::STATE_PENDING
                }
                _ => true,
            })
            .collect();
        // (cross-runtime #344/#393) Keep the in-flight batch markable while its
        // callbacks run (they allocate → may GC). Cleared after the batch.
        MICROTASK_INFLIGHT.with(|f| *f.borrow_mut() = queue.clone());
        for (idx, task) in queue.into_iter().enumerate() {
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
                            let r =
                                unsafe { invoke_microtask_callback(fn_ptr, &bound, Some(value)) };
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
                        let _ = unsafe { invoke_microtask_callback(fn_ptr, &[], None) };
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
                    if !settled_at_start[idx] {
                        // Source ainda nao settled no inicio do batch (ou
                        // settlou agora, mid-batch) — re-enfileira p/ rodar no
                        // proximo tick. Preserva ordem FIFO entre chains.
                        MICROTASK_QUEUE.with(|q| {
                            q.borrow_mut().push(Microtask::PendingThen {
                                source,
                                fn_ptr,
                                bound,
                                is_catch,
                                result_slot,
                            })
                        });
                    } else {
                        let value = promise_slot::current_value(&source);
                        let fulfilled = st == promise_slot::STATE_FULFILLED;
                        // .then: roda no fulfilled; .catch: roda no rejected.
                        let runs = if is_catch { !fulfilled } else { fulfilled };
                        if runs && fn_ptr != 0 {
                            let r =
                                unsafe { invoke_microtask_callback(fn_ptr, &bound, Some(value)) };
                            promise_slot::resolve(&result_slot, r);
                        } else if fulfilled {
                            promise_slot::resolve(&result_slot, value);
                        } else {
                            promise_slot::reject(&result_slot, value);
                        }
                    }
                }
                Microtask::PendingFinally {
                    source,
                    fn_ptr,
                    result_slot,
                } => {
                    let st = promise_slot::current_state(&source);
                    if !settled_at_start[idx] {
                        MICROTASK_QUEUE.with(|q| {
                            q.borrow_mut().push(Microtask::PendingFinally {
                                source,
                                fn_ptr,
                                result_slot,
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
                // (cross-runtime #393/#116) `.then(onFul, onRej)` instance,
                // source pending — variante 2-callback determinista.
                Microtask::PendingThen2 {
                    source,
                    on_ful,
                    on_rej,
                    result_slot,
                } => {
                    if !settled_at_start[idx] {
                        MICROTASK_QUEUE.with(|q| {
                            q.borrow_mut().push(Microtask::PendingThen2 {
                                source,
                                on_ful,
                                on_rej,
                                result_slot,
                            })
                        });
                    } else {
                        let st = promise_slot::current_state(&source);
                        let value = promise_slot::current_value(&source);
                        let fulfilled = st == promise_slot::STATE_FULFILLED;
                        let cb = if fulfilled { on_ful } else { on_rej };
                        if cb != 0 {
                            // .then(f) sucesso e .catch(g)/onRej recuperam:
                            // result resolved com o retorno do callback.
                            let r = unsafe { invoke_microtask_callback(cb, &[], Some(value)) };
                            promise_slot::resolve(&result_slot, r);
                        } else if fulfilled {
                            promise_slot::resolve(&result_slot, value);
                        } else {
                            // sem handler de rejeicao — propaga reject.
                            promise_slot::reject(&result_slot, value);
                        }
                    }
                }
                // (#207 async-SM) Retomada de async fn suspensa em await.
                Microtask::AsyncResume { gen_handle, source } => {
                    let st = promise_slot::current_state(&source);
                    if st == promise_slot::STATE_PENDING {
                        // awaited ainda nao settlou: re-enfileira no FIM da fila
                        // para re-checar no proximo ciclo do drain.
                        MICROTASK_QUEUE.with(|q| {
                            q.borrow_mut()
                                .push(Microtask::AsyncResume { gen_handle, source })
                        });
                    } else {
                        let value = promise_slot::current_value(&source);
                        let rejected = st == promise_slot::STATE_REJECTED;
                        // Injeta valor/erro e roda o proximo passo da SM. Se o
                        // step suspender de novo, ele re-enfileira outro
                        // AsyncResume internamente (interleaving).
                        // generator::async_sm_resume fica no collector do
                        // rts-runtime; chamamos via wrapper extern (resolve por
                        // link) p/ não criar ciclo std→runtime.
                        unsafe extern "C" {
                            fn __RTS_FN_RT_ASYNC_SM_RESUME(h: u64, value: i64, rejected: i64);
                        }
                        unsafe {
                            __RTS_FN_RT_ASYNC_SM_RESUME(
                                gen_handle,
                                value,
                                if rejected { 1 } else { 0 },
                            );
                        }
                    }
                }
            }
        }
        // (cross-runtime #344/#393) Batch done — drop the in-flight roots.
        MICROTASK_INFLIGHT.with(|f| f.borrow_mut().clear());
    }
}

/// Invoca o callback de microtask (then/catch/finally) prepended com bound_args.
/// (cross-runtime closures) Usa o registry de ABI: callbacks com param `number`
/// agora compilam como `(f64)->...`; invocá-los via ABI i64 crua segfaultava
/// (`Promise.resolve(4).then((n:number)=>n+1)`). invoke_fn_ptr_with_registry usa
/// invoke_typed com os param_kinds reais (e normaliza args number-cru), ou
/// invoke_n quando a fn é toda-i64 (idêntico ao transmute de antes).
unsafe fn invoke_microtask_callback(fn_ptr: u64, bound: &[i64], extra: Option<i64>) -> i64 {
    let mut args: Vec<i64> = bound.to_vec();
    if let Some(v) = extra {
        args.push(v);
    }
    rts_shared::globals::function::ops::invoke_fn_ptr_with_registry(fn_ptr, &args)
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
pub extern "C" fn __RTS_FN_GL_TEXTENC_ENCODE_INSTANCE(_self_h: u64, ptr: i64, len: i64) -> u64 {
    __RTS_FN_GL_TEXTENC_ENCODE(ptr, len)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TEXTDEC_DECODE_INSTANCE(_self_h: u64, buf_h: u64) -> u64 {
    __RTS_FN_GL_TEXTENC_DECODE(buf_h)
}
