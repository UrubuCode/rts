//! Implementacao do namespace `promise` (issue #412).
//!
//! Ponte entre handles GC e `PromiseSlot` (que vive em
//! `crate::namespaces::gc::promise_slot`). Cada fn extern "C" pega
//! handle u64, busca o `Arc<PromiseSlot>` no HandleTable, delega.

use crate::namespaces::gc::handles::{Entry, alloc_entry, with_entry};
use crate::namespaces::gc::promise_slot;

fn with_slot<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&std::sync::Arc<crate::namespaces::gc::handles::PromiseSlot>) -> R,
{
    with_entry(handle, |entry| match entry {
        Some(Entry::PromiseAsync(arc)) => f(arc),
        _ => default,
    })
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
    let (state, value) = promise_slot::wait_blocking(&arc);
    let _ = state; // F5 (#416) usa state pra integrar try/catch
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
    let result_clone = result.clone();
    let result_handle = alloc_entry(Entry::PromiseAsync(result));

    let rt = crate::runtime::async_rt::handle();
    rt.spawn_blocking(move || {
        let mut values: Vec<i64> = Vec::with_capacity(slots.len());
        for slot in slots.iter() {
            let Some(s) = slot else {
                // Handle invalido — rejeita com 0
                promise_slot::reject(&result_clone, 0);
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

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_RACE(vec_handle: u64) -> u64 {
    let handles = collect_promise_handles(vec_handle);
    let slots = collect_slots(&handles);
    let result = promise_slot::new_pending();
    let result_handle = alloc_entry(Entry::PromiseAsync(result.clone()));

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
    let result_clone = result.clone();
    let result_handle = alloc_entry(Entry::PromiseAsync(result));

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
            // Todas rejeitaram — JS daria AggregateError; aqui rejeitamos
            // com 0 (placeholder ate AggregateError chegar).
            promise_slot::reject(&result_clone, 0);
        }
    });

    result_handle
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROMISE_ALL_SETTLED(vec_handle: u64) -> u64 {
    let handles = collect_promise_handles(vec_handle);
    let slots = collect_slots(&handles);
    let result = promise_slot::new_pending();
    let result_clone = result.clone();
    let result_handle = alloc_entry(Entry::PromiseAsync(result));

    let rt = crate::runtime::async_rt::handle();
    rt.spawn_blocking(move || {
        // Encoding: state * 1000 + value (clamp value pra evitar
        // overflow quando value >= 1000). Caller decodifica:
        //   encoded / 1000 = state (1=fulfilled, 2=rejected)
        //   encoded % 1000 = value (limitado)
        // Pra valores arbitrarios, F4-fase-2 vai usar Map ou Tuple.
        let mut encoded: Vec<i64> = Vec::with_capacity(slots.len());
        for slot in slots.iter() {
            let Some(s) = slot else {
                encoded.push(2 * 1000); // rejected com 0
                continue;
            };
            let (state, value) = promise_slot::wait_blocking(s);
            let v = value.clamp(0, 999);
            let st = state as i64;
            encoded.push(st * 1000 + v);
        }
        let result_vec = alloc_entry(Entry::Vec(Box::new(encoded)));
        promise_slot::resolve(&result_clone, result_vec as i64);
    });

    result_handle
}
