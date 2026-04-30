//! Mutex<i64> — lock/unlock atravessam chamadas extern "C" via mapa
//! thread-local que armazena os `MutexGuard` enquanto a thread os detem.
//!
//! Soundness (#280):
//! - O `Mutex<i64>` esta em `Arc` dentro da HandleTable. Cada lock
//!   clona o Arc e guarda junto com o `MutexGuard`, ancorando o lifetime
//!   real do guard. Antes era `Box` + transmute para `'static`, com UB
//!   se `free(m)` vinha antes de `unlock(m)`.
//! - Mesmo se `free(m)` for chamado enquanto a thread detem o lock,
//!   o Arc clone no mapa de guards mantem o Mutex vivo ate unlock.
//! - Thread exit dropa o thread_local, liberando guards naturalmente —
//!   o Mutex sobrevive se houver outras refs Arc, ou e' liberado se nao.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use super::super::gc::handles::{Entry, alloc_entry, free_handle, shard_for_handle};

/// Guard owned: clona o Arc para ancorar o Mutex enquanto o guard existe.
/// O guard e' tipado como `'static`, mas a soundness vem do Arc clone
/// que vive na mesma struct — o Mutex nao pode ser liberado enquanto
/// `_arc` esta presente.
struct OwnedMutexGuard {
    _arc: Arc<Mutex<i64>>,
    guard: MutexGuard<'static, i64>,
}

thread_local! {
    static GUARDS: RefCell<HashMap<u64, OwnedMutexGuard>> =
        RefCell::new(HashMap::new());
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_MUTEX_NEW(initial: i64) -> u64 {
    alloc_entry(Entry::SyncMutex(Arc::new(Mutex::new(initial))))
}

/// Helper: obtem um clone do Arc<Mutex<i64>> referenciado pelo handle.
/// Retorna `None` se o handle for invalido.
fn mutex_arc(handle: u64) -> Option<Arc<Mutex<i64>>> {
    let guard = shard_for_handle(handle).lock().unwrap();
    match guard.get(handle) {
        Some(Entry::SyncMutex(m)) => Some(m.clone()),
        _ => None,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_MUTEX_LOCK(handle: u64) -> i64 {
    let Some(arc) = mutex_arc(handle) else {
        return 0;
    };
    // SAFETY: locamos via ponteiro estavel do Arc (Arc::as_ptr).
    // O Arc e' movido para o slot junto com o guard 'static, ancorando
    // o Mutex enquanto o guard existir.
    let ptr: *const Mutex<i64> = Arc::as_ptr(&arc);
    let m: &'static Mutex<i64> = unsafe { &*ptr };
    let g: MutexGuard<'static, i64> = m.lock().unwrap_or_else(|e| e.into_inner());
    let value = *g;
    GUARDS.with(|cell| {
        cell.borrow_mut().insert(
            handle,
            OwnedMutexGuard {
                _arc: arc,
                guard: g,
            },
        );
    });
    value
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_MUTEX_TRY_LOCK(handle: u64) -> i64 {
    let Some(arc) = mutex_arc(handle) else {
        return 0;
    };
    let ptr: *const Mutex<i64> = Arc::as_ptr(&arc);
    let m: &'static Mutex<i64> = unsafe { &*ptr };
    match m.try_lock() {
        Ok(g) => {
            let value = *g;
            GUARDS.with(|cell| {
                cell.borrow_mut().insert(
                    handle,
                    OwnedMutexGuard {
                        _arc: arc,
                        guard: g,
                    },
                );
            });
            value
        }
        Err(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_MUTEX_SET(handle: u64, value: i64) {
    GUARDS.with(|cell| {
        if let Some(owned) = cell.borrow_mut().get_mut(&handle) {
            *owned.guard = value;
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_MUTEX_UNLOCK(handle: u64) {
    GUARDS.with(|cell| {
        cell.borrow_mut().remove(&handle);
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_MUTEX_FREE(handle: u64) {
    // Garante que nao deixamos guard pendurado apos free.
    GUARDS.with(|cell| {
        cell.borrow_mut().remove(&handle);
    });
    free_handle(handle);
}
