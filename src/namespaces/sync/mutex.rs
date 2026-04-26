//! Mutex<i64> — lock/unlock atravessam chamadas extern "C" via mapa
//! thread-local que armazena os `MutexGuard` enquanto a thread os detem.
//!
//! Seguranca:
//! - O `Mutex<i64>` esta em `Box` dentro da HandleTable; seu endereco e
//!   estavel enquanto o slot vive. Codigo TS-side e responsavel por
//!   `mutex_unlock`/`mutex_free` em ordem (unlock antes de free).
//! - O guard e estendido para `'static` via `transmute` — soundness
//!   depende do Box continuar vivo enquanto o guard estiver no mapa
//!   (caller obriga isso pela ordem de chamadas).

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use super::super::gc::handles::{Entry, table};

thread_local! {
    /// Guarda ativos por handle de mutex, para esta thread.
    static GUARDS: RefCell<HashMap<u64, MutexGuard<'static, i64>>> =
        RefCell::new(HashMap::new());
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_MUTEX_NEW(initial: i64) -> u64 {
    table()
        .lock()
        .unwrap()
        .alloc(Entry::SyncMutex(Box::new(Mutex::new(initial))))
}

/// Helper: obtem ponteiro estavel para o `Mutex<i64>` referenciado pelo
/// handle. Retorna `None` se o handle for invalido. O ponteiro e valido
/// enquanto o slot na HandleTable nao for liberado.
fn mutex_ptr(handle: u64) -> Option<*const Mutex<i64>> {
    let guard = table().lock().unwrap();
    match guard.get(handle) {
        Some(Entry::SyncMutex(m)) => Some(m.as_ref() as *const Mutex<i64>),
        _ => None,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_MUTEX_LOCK(handle: u64) -> i64 {
    let Some(ptr) = mutex_ptr(handle) else {
        return 0;
    };
    // SAFETY: ptr aponta para Mutex<i64> dentro de Box vivo (caller nao
    // chamou free antes de unlock). Estendemos o guard para 'static
    // ancorado pela permanencia do Box.
    let m: &'static Mutex<i64> = unsafe { &*ptr };
    let g = m.lock().unwrap_or_else(|e| e.into_inner());
    let value = *g;
    GUARDS.with(|cell| {
        cell.borrow_mut().insert(handle, g);
    });
    value
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_MUTEX_TRY_LOCK(handle: u64) -> i64 {
    let Some(ptr) = mutex_ptr(handle) else {
        return 0;
    };
    let m: &'static Mutex<i64> = unsafe { &*ptr };
    match m.try_lock() {
        Ok(g) => {
            let value = *g;
            GUARDS.with(|cell| {
                cell.borrow_mut().insert(handle, g);
            });
            value
        }
        Err(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_MUTEX_SET(handle: u64, value: i64) {
    GUARDS.with(|cell| {
        if let Some(g) = cell.borrow_mut().get_mut(&handle) {
            **g = value;
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
    table().lock().unwrap().free(handle);
}
