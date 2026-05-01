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
