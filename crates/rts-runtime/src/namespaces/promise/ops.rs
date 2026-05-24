//! Implementacao do namespace `promise` (issue #412).
//!
//! Ponte entre handles GC e `PromiseSlot` (que vive em
//! `crate::namespaces::gc::promise_slot`). Cada fn extern "C" pega
//! handle u64, busca o `Arc<PromiseSlot>` no HandleTable, delega.

use crate::namespaces::gc::handles::{Entry, alloc_entry, with_entry};
use crate::namespaces::gc::promise_slot;

/// (#376) Contador global de tasks tokio pendentes spawnadas por promise.create.
/// Drain do pipeline aguarda chegar a 0 para que async fns fire-and-forget
/// (sem await no top-level) terminem antes do processo sair.
use std::sync::atomic::{AtomicUsize, Ordering};
pub(crate) static PENDING_PROMISE_TASKS: AtomicUsize = AtomicUsize::new(0);

/// Bloqueia ate todas as tasks de promise.create completarem, com deadline
/// de 5s para evitar hang em tasks que nunca settle.
pub fn drain_pending_promises() {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(5);
    while PENDING_PROMISE_TASKS.load(Ordering::Acquire) > 0 {
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn with_slot<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&std::sync::Arc<crate::namespaces::gc::handles::PromiseSlot>) -> R,
{
    with_entry(handle, |entry| match entry {
        Some(Entry::PromiseAsync(arc)) => f(arc),
        _ => default,
    })
}

/// Resolve `fp` para ponteiro de codigo executavel.
///
/// Se `fp` for handle de `Entry::Function` (criada via `new Function` ou
/// via reify de user fn), extrai o `fn_ptr` interno + bound_args. Senao,
/// trata como ponteiro extern "C" direto (path legado).
///
/// Retorna `(fn_ptr, bound_args, has_bound_this, bound_this)`.
fn resolve_callback_ptr(fp: u64) -> (u64, Vec<i64>, bool, i64) {
    if fp == 0 {
        return (0, Vec::new(), false, 0);
    }
    let resolved = with_entry(fp, |entry| {
        if let Some(Entry::Function(fd)) = entry {
            Some((fd.fn_ptr, fd.bound_args.clone(), fd.has_bound_this, fd.bound_this))
        } else {
            None
        }
    });
    resolved.unwrap_or((fp, Vec::new(), false, 0))
}

/// Invoca um ponteiro de fn extern "C" com 0 ou 1 arg, prepende
/// bound_args. Retorna i64. Limite de aridade total = 8 (limite do
/// trampolim em globals/function/ops).
unsafe fn invoke_callback(fn_ptr: u64, bound: &[i64], extra: Option<i64>) -> i64 {
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
            4 => transmute::<u64, extern "C" fn(i64, i64, i64, i64) -> i64>(fn_ptr)(
                args[0], args[1], args[2], args[3],
            ),
            5 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64) -> i64>(fn_ptr)(
                args[0], args[1], args[2], args[3], args[4],
            ),
            6 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64, i64) -> i64>(fn_ptr)(
                args[0], args[1], args[2], args[3], args[4], args[5],
            ),
            7 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64, i64, i64) -> i64>(fn_ptr)(
                args[0], args[1], args[2], args[3], args[4], args[5], args[6],
            ),
            8 => transmute::<
                u64,
                extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i64) -> i64,
            >(fn_ptr)(
                args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7],
            ),
            _ => 0,
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_NEW_PENDING() -> u64 {
    let slot = promise_slot::new_pending();
    alloc_entry(Entry::PromiseAsync(slot))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_NEW_RESOLVED(value: i64) -> u64 {
    let slot = promise_slot::new_fulfilled(value);
    alloc_entry(Entry::PromiseAsync(slot))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_NEW_REJECTED(error: i64) -> u64 {
    let slot = promise_slot::new_rejected(error);
    alloc_entry(Entry::PromiseAsync(slot))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_RESOLVE(handle: u64, value: i64) -> i64 {
    with_slot(handle, 0, |slot| {
        if promise_slot::resolve(slot, value) { 1 } else { 0 }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_REJECT(handle: u64, error: i64) -> i64 {
    with_slot(handle, 0, |slot| {
        if promise_slot::reject(slot, error) { 1 } else { 0 }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_STATE(handle: u64) -> i64 {
    with_slot(handle, -1, |slot| promise_slot::current_state(slot) as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_WAIT(handle: u64) -> i64 {
    // Clona o Arc fora do `with_entry` pra liberar o lock do shard
    // antes de bloquear em wait_blocking (que pode esperar minutos).
    // Sem isso, qualquer outra op no mesmo shard fica bloqueada.
    let slot_arc = with_entry(handle, |entry| match entry {
        Some(Entry::PromiseAsync(arc)) => Some(arc.clone()),
        _ => None,
    });
    let Some(arc) = slot_arc else { return 0 };
    // (cross-runtime #56/#285) Drena microtasks antes de bloquear: o
    // fast-path de Promise.then settled enfileira callbacks que precisam
    // executar para o slot que estamos aguardando ser settled. Sem isso,
    // `await Promise.resolve(x).then(f)` ficaria hang esperando o
    // microtask drenar (que so' aconteceria no fim do __RTS_MAIN).
    if promise_slot::current_state(&arc) == promise_slot::STATE_PENDING {
        crate::namespaces::globals::text_encoding::instance::drain_microtasks();
    }
    let (state, value) = promise_slot::wait_blocking(&arc);
    // F5 (#416): se Promise rejected, propaga via slot de erro
    // thread-local que `try/catch` ja' le. O slot eh da thread atual
    // (caller do `promise.wait`) — wait_blocking nao migra threads.
    if state == promise_slot::STATE_REJECTED {
        crate::namespaces::gc::error::__RTS_FN_RT_ERROR_SET(value as u64);
    }
    value
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_TRY_VALUE(handle: u64) -> i64 {
    with_slot(handle, 0, |slot| {
        if promise_slot::current_state(slot) == promise_slot::STATE_PENDING {
            0
        } else {
            promise_slot::current_value(slot)
        }
    })
}

// ─── then/catch/finally como fns de namespace (F6 #417) ─────────────
//
// Versao namespace de `.then/.catch/.finally`. Mais simples que
// instance methods (que tem caminho de codegen fragil pra Promise).
// Recomendado: `promise.then(p, fn)` em TS — funciona sempre.

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_THEN_NS(p_handle: u64, fp: u64) -> u64 {
    let slot_arc = with_entry(p_handle, |entry| match entry {
        Some(Entry::PromiseAsync(arc)) => Some(arc.clone()),
        _ => None,
    });
    let Some(arc) = slot_arc else { return p_handle };
    let result = promise_slot::new_pending();
    let result_clone = result.clone();
    let result_handle = alloc_entry(Entry::PromiseAsync(result));

    // Resolve handle Function -> fn_ptr + bound args (ou usa fp como ptr direto).
    let (fn_ptr, bound, _has_this, _this) = resolve_callback_ptr(fp);

    // (cross-runtime #56/#285) Fast-path: se a Promise ja' esta settled,
    // enfileira no microtask queue em vez de spawn_blocking. Isso preserva
    // a ordem JS spec (microtask FIFO) entre queueMicrotask e
    // Promise.resolve().then(). Sem isso, spawn_blocking pode rodar antes
    // do drain do microtask queue.
    let state_now = promise_slot::current_state(&arc);
    if state_now != promise_slot::STATE_PENDING {
        let value = promise_slot::current_value(&arc);
        let fulfilled = state_now == promise_slot::STATE_FULFILLED;
        crate::namespaces::globals::text_encoding::instance::enqueue_microtask_settled(
            fn_ptr, bound, value, fulfilled, result_clone,
        );
        return result_handle;
    }

    let rt = crate::runtime::async_rt::handle();
    rt.spawn_blocking(move || {
        let (state, value) = promise_slot::wait_blocking(&arc);
        if state == promise_slot::STATE_FULFILLED {
            if fn_ptr == 0 {
                promise_slot::resolve(&result_clone, value);
            } else {
                let r = unsafe { invoke_callback(fn_ptr, &bound, Some(value)) };
                promise_slot::resolve(&result_clone, r);
            }
        } else {
            promise_slot::reject(&result_clone, value);
        }
    });
    result_handle
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_CATCH_NS(p_handle: u64, fp: u64) -> u64 {
    let slot_arc = with_entry(p_handle, |entry| match entry {
        Some(Entry::PromiseAsync(arc)) => Some(arc.clone()),
        _ => None,
    });
    let Some(arc) = slot_arc else { return p_handle };
    let result = promise_slot::new_pending();
    let result_clone = result.clone();
    let result_handle = alloc_entry(Entry::PromiseAsync(result));

    let (fn_ptr, bound, _h, _t) = resolve_callback_ptr(fp);
    let rt = crate::runtime::async_rt::handle();
    rt.spawn_blocking(move || {
        let (state, value) = promise_slot::wait_blocking(&arc);
        if state == promise_slot::STATE_FULFILLED {
            promise_slot::resolve(&result_clone, value);
        } else {
            if fn_ptr == 0 {
                promise_slot::reject(&result_clone, value);
            } else {
                let r = unsafe { invoke_callback(fn_ptr, &bound, Some(value)) };
                promise_slot::resolve(&result_clone, r);
            }
        }
    });
    result_handle
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_FINALLY_NS(p_handle: u64, fp: u64) -> u64 {
    let slot_arc = with_entry(p_handle, |entry| match entry {
        Some(Entry::PromiseAsync(arc)) => Some(arc.clone()),
        _ => None,
    });
    let Some(arc) = slot_arc else { return p_handle };
    let result = promise_slot::new_pending();
    let result_clone = result.clone();
    let result_handle = alloc_entry(Entry::PromiseAsync(result));

    let (fn_ptr, bound, _h, _t) = resolve_callback_ptr(fp);
    let rt = crate::runtime::async_rt::handle();
    rt.spawn_blocking(move || {
        let (state, value) = promise_slot::wait_blocking(&arc);
        if fn_ptr != 0 {
            let _ = unsafe { invoke_callback(fn_ptr, &bound, None) };
        }
        if state == promise_slot::STATE_FULFILLED {
            promise_slot::resolve(&result_clone, value);
        } else {
            promise_slot::reject(&result_clone, value);
        }
    });
    result_handle
}

// ─── Error slot bridge (F5 #416) ─────────────────────────────────────
//
// Helpers expostos pro codegen do watcher chamar apos `inner`. Lê e
// limpa o slot de erro thread-local. Se inner lancou (throw), o slot
// fica com handle do erro — precisamos converter pra `promise.reject`.

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_TAKE_ERROR() -> i64 {
    use crate::namespaces::gc::error;
    let h = error::__RTS_FN_RT_ERROR_GET();
    if h != 0 {
        error::__RTS_FN_RT_ERROR_CLEAR();
    }
    h as i64
}

// ─── Combinators (F4 #415) ───────────────────────────────────────────
//
// Modelo: cada combinator copia o Vec de handles de Promise pendentes,
// spawna uma task tokio que aguarda conforme a semantica desejada e
// resolve a Promise resultante. Usa `wait_blocking` em sequencia — como
// cada Promise ja' roda na sua propria thread tokio (F2), aguardar
// sequencial nao serializa o trabalho real. So' o overhead de waitset
// e' linear em N.

/// Le o Vec<i64> de handles de Promise.
fn collect_promise_handles(vec_handle: u64) -> Vec<u64> {
    use crate::namespaces::gc::handles::Entry;
    with_entry(vec_handle, |entry| match entry {
        Some(Entry::Vec(v)) => v.iter().map(|x| *x as u64).collect(),
        _ => Vec::new(),
    })
}

/// Clone os Arc<PromiseSlot> de cada handle. Handle invalido vira None
/// (filtrado depois).
fn collect_slots(
    handles: &[u64],
) -> Vec<Option<std::sync::Arc<crate::namespaces::gc::handles::PromiseSlot>>> {
    handles
        .iter()
        .map(|h| {
            with_entry(*h, |entry| match entry {
                Some(Entry::PromiseAsync(arc)) => Some(arc.clone()),
                _ => None,
            })
        })
        .collect()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_ALL(vec_handle: u64) -> u64 {
    let handles = collect_promise_handles(vec_handle);
    let slots = collect_slots(&handles);
    let result = promise_slot::new_pending();
    let result_handle = alloc_entry(Entry::PromiseAsync(result.clone()));

    // (cross-runtime #806) Fast-path: todos settled -> processa sync.
    let all_settled_sync = slots.iter().all(|s| {
        s.as_ref()
            .map(|arc| promise_slot::current_state(arc) != promise_slot::STATE_PENDING)
            .unwrap_or(false)
    });
    if all_settled_sync && !slots.is_empty() {
        let mut values: Vec<i64> = Vec::with_capacity(slots.len());
        for slot in slots.iter() {
            let s = slot.as_ref().unwrap();
            let state = promise_slot::current_state(s);
            let value = promise_slot::current_value(s);
            if state == promise_slot::STATE_REJECTED {
                promise_slot::reject(&result, value);
                return result_handle;
            }
            values.push(value);
        }
        let result_vec = alloc_entry(Entry::Vec(Box::new(values)));
        promise_slot::resolve(&result, result_vec as i64);
        return result_handle;
    }
    let result_clone = result.clone();
    let _ = result;

    let rt = crate::runtime::async_rt::handle();
    rt.spawn_blocking(move || {
        let mut values: Vec<i64> = Vec::with_capacity(slots.len());
        for slot in slots.iter() {
            let Some(s) = slot else {
                // Handle invalido — rejeita com mensagem fallback.
                let msg = b"Invalid promise handle in collection".to_vec();
                let err_handle = alloc_entry(Entry::String(msg));
                promise_slot::reject(&result_clone, err_handle as i64);
                return;
            };
            let (state, value) = promise_slot::wait_blocking(s);
            if state == promise_slot::STATE_REJECTED {
                promise_slot::reject(&result_clone, value);
                return;
            }
            values.push(value);
        }
        // Todos resolveram — empacota num Vec novo.
        let result_vec = alloc_entry(Entry::Vec(Box::new(values)));
        promise_slot::resolve(&result_clone, result_vec as i64);
    });

    result_handle
}

/// `Array.fromAsync(iterable, mapper?)` — coleta valores de iteravel sync ou
/// async para Promise<Array>. Implementacao minima (issue #861):
/// - Vec handle (array sync): aplica mapper a cada elem, wrap em Promise.resolve
///   se nao for Promise ja, depois Promise.all.
/// - Async iterable (gen()): nao suportado ate #211 (generator state machine).
///   Retorna Promise rejeitado.
///
/// `fn_ptr=0` => sem mapper. Caller faz invoke_typed (1 arg, retorna i64/handle).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ARRAY_FROM_ASYNC(iterable: u64, mapper_handle: u64) -> u64 {
    // Suporte minimo: iterable e' Vec. Aplica mapper sync (se houver),
    // wrap cada valor em Promise.resolve se ja' nao for, faz Promise.all.
    use crate::namespaces::globals::function::ops::__RTS_FN_GL_FUNCTION_APPLY_TYPED;

    let snapshot: Option<Vec<i64>> = with_entry(iterable, |e| match e {
        Some(Entry::Vec(v)) => Some(v.as_ref().clone()),
        _ => None,
    });
    let Some(items) = snapshot else {
        // Async iterable / outros tipos — retorna Promise rejected.
        let result = promise_slot::new_pending();
        let result_handle = alloc_entry(Entry::PromiseAsync(result.clone()));
        let msg = b"Array.fromAsync: only sync iterables supported (issue #211 blocks async generators)".to_vec();
        let err_handle = alloc_entry(Entry::String(msg));
        promise_slot::reject(&result, err_handle as i64);
        return result_handle;
    };

    // Aplica mapper se fornecido. mapper(value, index) -> mapped.
    let mapped: Vec<i64> = if mapper_handle != 0 {
        items.iter().enumerate().map(|(i, &v)| {
            let args_vec = alloc_entry(Entry::Vec(Box::new(vec![v, i as i64])));
            __RTS_FN_GL_FUNCTION_APPLY_TYPED(mapper_handle, 0, args_vec)
        }).collect()
    } else {
        items
    };

    // Wrap cada valor: se ja' for Promise handle, mantem; senao Promise.resolve.
    let wrapped: Vec<i64> = mapped.iter().map(|&v| {
        let is_promise = with_entry(v as u64, |e| matches!(e, Some(Entry::PromiseAsync(_))));
        if is_promise {
            v
        } else {
            let slot = promise_slot::new_pending();
            promise_slot::resolve(&slot, v);
            alloc_entry(Entry::PromiseAsync(slot)) as i64
        }
    }).collect();

    let promises_vec = alloc_entry(Entry::Vec(Box::new(wrapped)));
    __RTS_FN_NS_PROMISE_ALL(promises_vec)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_RACE(vec_handle: u64) -> u64 {
    let handles = collect_promise_handles(vec_handle);
    let slots = collect_slots(&handles);
    let result = promise_slot::new_pending();
    let result_handle = alloc_entry(Entry::PromiseAsync(result.clone()));

    // (cross-runtime #779) Fast-path: se algum slot ja' esta settled em
    // ordem de iteracao, resolve sync — preserva FIFO microtask order do
    // JS. Sem isto, spawn_blocking pode reordenar resolves de promises
    // ja' settled (Promise.resolve sync) e quebrar a ordem esperada do
    // event loop.
    for slot in slots.iter() {
        if let Some(s) = slot {
            let state = promise_slot::current_state(s);
            if state != promise_slot::STATE_PENDING {
                let value = promise_slot::current_value(s);
                if state == promise_slot::STATE_FULFILLED {
                    promise_slot::resolve(&result, value);
                } else {
                    promise_slot::reject(&result, value);
                }
                return result_handle;
            }
        }
    }

    let rt = crate::runtime::async_rt::handle();
    // Cada slot e' aguardado numa task separada — primeira a settle
    // resolve a result. Demais resolves sao no-op (idempotencia).
    for slot in slots {
        let result_clone = result.clone();
        rt.spawn_blocking(move || {
            let Some(s) = slot else {
                promise_slot::reject(&result_clone, 0);
                return;
            };
            let (state, value) = promise_slot::wait_blocking(&s);
            if state == promise_slot::STATE_FULFILLED {
                promise_slot::resolve(&result_clone, value);
            } else {
                promise_slot::reject(&result_clone, value);
            }
        });
    }

    result_handle
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_ANY(vec_handle: u64) -> u64 {
    let handles = collect_promise_handles(vec_handle);
    let slots = collect_slots(&handles);
    let result = promise_slot::new_pending();
    let result_handle = alloc_entry(Entry::PromiseAsync(result.clone()));

    // (cross-runtime #779) Fast-path: primeira fulfilled em ordem.
    // Mesma motivacao de PROMISE_RACE — preserva FIFO microtask order
    // pra promises ja' settled.
    let mut all_already_rejected = true;
    let mut any_pending = false;
    for slot in slots.iter() {
        if let Some(s) = slot {
            let state = promise_slot::current_state(s);
            if state == promise_slot::STATE_PENDING {
                any_pending = true;
                all_already_rejected = false;
            } else if state == promise_slot::STATE_FULFILLED {
                let value = promise_slot::current_value(s);
                promise_slot::resolve(&result, value);
                return result_handle;
            }
            // rejected — continua
        } else {
            any_pending = true;
            all_already_rejected = false;
        }
    }
    if !any_pending && all_already_rejected {
        let msg = b"All promises were rejected".to_vec();
        let err_handle = alloc_entry(Entry::String(msg));
        promise_slot::reject(&result, err_handle as i64);
        return result_handle;
    }
    let result_clone = result.clone();
    let _ = result;

    let rt = crate::runtime::async_rt::handle();
    rt.spawn_blocking(move || {
        let mut all_rejected = true;
        for slot in slots.iter() {
            let Some(s) = slot else { continue };
            let (state, value) = promise_slot::wait_blocking(s);
            if state == promise_slot::STATE_FULFILLED {
                promise_slot::resolve(&result_clone, value);
                return;
            }
            // rejected — registra mas continua tentando proxima
            all_rejected = all_rejected && true;
            let _ = value;
        }
        if all_rejected {
            // Todas rejeitaram — JS daria AggregateError. Aqui usamos
            // handle de string fallback (nao 0, senao slot de erro
            // thread-local nao dispara catch — semantica de "no error"
            // e' value=0).
            let msg = b"All promises were rejected".to_vec();
            let err_handle = alloc_entry(Entry::String(msg));
            promise_slot::reject(&result_clone, err_handle as i64);
        }
    });

    result_handle
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_ALL_SETTLED(vec_handle: u64) -> u64 {
    let handles = collect_promise_handles(vec_handle);
    let slots = collect_slots(&handles);
    let result = promise_slot::new_pending();
    let result_handle = alloc_entry(Entry::PromiseAsync(result.clone()));

    // (cross-runtime #806) Fast-path: se todos os slots ja' estao settled
    // (Promise.resolve/reject), monta result sync sem spawn. Preserva
    // ordem FIFO de microtasks e garante que .then() em chain rode antes
    // do setTimeout terminar.
    let all_settled_sync = slots.iter().all(|s| {
        s.as_ref()
            .map(|arc| promise_slot::current_state(arc) != promise_slot::STATE_PENDING)
            .unwrap_or(true) // None tambem conta como "settled" (rejected default)
    });
    if all_settled_sync {
        use indexmap::IndexMap;
        let mk_str = |s: &[u8]| -> i64 {
            alloc_entry(Entry::String(s.to_vec())) as i64
        };
        let mut result_vec: Vec<i64> = Vec::with_capacity(slots.len());
        for slot in slots.iter() {
            let mut obj: IndexMap<String, i64> = IndexMap::new();
            let Some(s) = slot else {
                obj.insert("status".to_string(), mk_str(b"rejected"));
                obj.insert("reason".to_string(), 0);
                let h = alloc_entry(Entry::Map(Box::new(obj)));
                result_vec.push(h as i64);
                continue;
            };
            let state = promise_slot::current_state(s);
            let value = promise_slot::current_value(s);
            if state == promise_slot::STATE_FULFILLED {
                obj.insert("status".to_string(), mk_str(b"fulfilled"));
                obj.insert("value".to_string(), value);
            } else {
                obj.insert("status".to_string(), mk_str(b"rejected"));
                obj.insert("reason".to_string(), value);
            }
            let h = alloc_entry(Entry::Map(Box::new(obj)));
            result_vec.push(h as i64);
        }
        let result_vec_h = alloc_entry(Entry::Vec(Box::new(result_vec)));
        promise_slot::resolve(&result, result_vec_h as i64);
        return result_handle;
    }

    let result_clone = result.clone();
    let _ = result;

    let rt = crate::runtime::async_rt::handle();
    rt.spawn_blocking(move || {
        // JS spec: array de { status: "fulfilled", value } | { status: "rejected", reason }.
        // Cada elemento e' um Map (objeto JS) com chaves "status" + "value"/"reason"
        // armazenando handles de string ("fulfilled"/"rejected") e o valor/reason raw.
        use indexmap::IndexMap;
        let mk_str = |s: &[u8]| -> i64 {
            alloc_entry(Entry::String(s.to_vec())) as i64
        };
        let mut result_vec: Vec<i64> = Vec::with_capacity(slots.len());
        for slot in slots.iter() {
            let mut obj: IndexMap<String, i64> = IndexMap::new();
            let Some(s) = slot else {
                obj.insert("status".to_string(), mk_str(b"rejected"));
                obj.insert("reason".to_string(), 0);
                let h = alloc_entry(Entry::Map(Box::new(obj)));
                result_vec.push(h as i64);
                continue;
            };
            let (state, value) = promise_slot::wait_blocking(s);
            if state == promise_slot::STATE_FULFILLED {
                obj.insert("status".to_string(), mk_str(b"fulfilled"));
                obj.insert("value".to_string(), value);
            } else {
                obj.insert("status".to_string(), mk_str(b"rejected"));
                obj.insert("reason".to_string(), value);
            }
            let h = alloc_entry(Entry::Map(Box::new(obj)));
            result_vec.push(h as i64);
        }
        let result_vec_h = alloc_entry(Entry::Vec(Box::new(result_vec)));
        promise_slot::resolve(&result_clone, result_vec_h as i64);
    });

    result_handle
}

// ─── Promise-centric API (drysius design, #359 follow-up) ────────────
//
// `promise.create(fn_handle, args_vec)` — cria PromiseAsync que executa
// `fn_handle` com `args_vec` em uma tokio task. Concentra spawn+state
// dentro do Promise em vez de espalhar pelo `expand_async_functions`.
//
// `args_vec` pode ser handle de Vec<i64> ou 0 (sem args).
// `fn_handle` deve ser handle de Entry::Function (criado via reify ou
// new Function). Tambem aceita ptr extern "C" direto pelo path legado
// de `resolve_callback_ptr`.

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_CREATE(fn_handle: u64, args_vec_handle: u64) -> u64 {
    let result = promise_slot::new_pending();
    let result_clone = result.clone();
    let handle = alloc_entry(Entry::PromiseAsync(result));

    let (fn_ptr, bound, _h, _t) = resolve_callback_ptr(fn_handle);
    if fn_ptr == 0 {
        // fn invalida — Promise rejeitada com 0.
        promise_slot::reject(&result_clone, 0);
        return handle;
    }

    let extra_args = read_promise_vec(args_vec_handle);

    PENDING_PROMISE_TASKS.fetch_add(1, Ordering::AcqRel);
    let rt = crate::runtime::async_rt::handle();
    rt.spawn_blocking(move || {
        // Combina bound (de bind) + extra_args do caller.
        let mut all: Vec<i64> = bound;
        all.extend(extra_args);
        // invoke_callback aceita 0 ou 1 extra; aqui ja' temos tudo
        // empacotado, entao desempacotamos manualmente.
        let r = unsafe { invoke_callback_full(fn_ptr, &all) };
        // Checa error slot pra detectar throw dentro do body.
        let err = crate::namespaces::gc::error::__RTS_FN_RT_ERROR_GET();
        if err != 0 {
            crate::namespaces::gc::error::__RTS_FN_RT_ERROR_CLEAR();
            promise_slot::reject(&result_clone, err as i64);
        } else {
            promise_slot::resolve(&result_clone, r);
        }
        PENDING_PROMISE_TASKS.fetch_sub(1, Ordering::AcqRel);
    });
    handle
}

/// Variante de invoke_callback que recebe args ja' empacotados (sem extra).
unsafe fn invoke_callback_full(fn_ptr: u64, args: &[i64]) -> i64 {
    use std::mem::transmute;
    unsafe {
        match args.len() {
            0 => transmute::<u64, extern "C" fn() -> i64>(fn_ptr)(),
            1 => transmute::<u64, extern "C" fn(i64) -> i64>(fn_ptr)(args[0]),
            2 => transmute::<u64, extern "C" fn(i64, i64) -> i64>(fn_ptr)(args[0], args[1]),
            3 => transmute::<u64, extern "C" fn(i64, i64, i64) -> i64>(fn_ptr)(
                args[0], args[1], args[2],
            ),
            4 => transmute::<u64, extern "C" fn(i64, i64, i64, i64) -> i64>(fn_ptr)(
                args[0], args[1], args[2], args[3],
            ),
            5 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64) -> i64>(fn_ptr)(
                args[0], args[1], args[2], args[3], args[4],
            ),
            6 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64, i64) -> i64>(fn_ptr)(
                args[0], args[1], args[2], args[3], args[4], args[5],
            ),
            7 => transmute::<u64, extern "C" fn(i64, i64, i64, i64, i64, i64, i64) -> i64>(fn_ptr)(
                args[0], args[1], args[2], args[3], args[4], args[5], args[6],
            ),
            8 => transmute::<
                u64,
                extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i64) -> i64,
            >(fn_ptr)(
                args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7],
            ),
            _ => 0,
        }
    }
}

/// Le o conteudo de um Vec<i64> handle (ou retorna vazio se nao for Vec).
fn read_promise_vec(h: u64) -> Vec<i64> {
    if h == 0 {
        return Vec::new();
    }
    with_entry(h, |entry| {
        if let Some(Entry::Vec(v)) = entry {
            v.iter().copied().collect()
        } else {
            Vec::new()
        }
    })
}
