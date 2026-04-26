//! RwLock<i64> — read/write guards retornados como handles distintos.
//!
//! Cada chamada a `rwlock_read`/`rwlock_write` aloca um id de guard novo
//! (contador atomico) e armazena o guard em mapa thread-local. `unlock`
//! consome o id, dropando o guard.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use super::super::gc::handles::{Entry, table};

/// Os campos parecem "dead" para o compilador, mas existem pelo seu
/// efeito Drop: liberar o lock subjacente ao serem removidos do mapa.
#[allow(dead_code)]
enum GuardSlot {
    Read(RwLockReadGuard<'static, i64>),
    Write(RwLockWriteGuard<'static, i64>),
}

thread_local! {
    static GUARDS: RefCell<HashMap<u64, GuardSlot>> = RefCell::new(HashMap::new());
}

static GUARD_ID: AtomicU64 = AtomicU64::new(1);

fn next_guard_id() -> u64 {
    GUARD_ID.fetch_add(1, Ordering::SeqCst)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_RWLOCK_NEW(initial: i64) -> u64 {
    table()
        .lock()
        .unwrap()
        .alloc(Entry::SyncRwLock(Box::new(RwLock::new(initial))))
}

fn rwlock_ptr(handle: u64) -> Option<*const RwLock<i64>> {
    let guard = table().lock().unwrap();
    match guard.get(handle) {
        Some(Entry::SyncRwLock(r)) => Some(r.as_ref() as *const RwLock<i64>),
        _ => None,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_RWLOCK_READ(handle: u64) -> u64 {
    let Some(ptr) = rwlock_ptr(handle) else {
        return 0;
    };
    // SAFETY: ptr permanece valido enquanto o slot da HandleTable vive
    // (caller mantem a ordem unlock antes de free).
    let r: &'static RwLock<i64> = unsafe { &*ptr };
    let g = r.read().unwrap_or_else(|e| e.into_inner());
    let id = next_guard_id();
    GUARDS.with(|cell| {
        cell.borrow_mut().insert(id, GuardSlot::Read(g));
    });
    id
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_RWLOCK_WRITE(handle: u64) -> u64 {
    let Some(ptr) = rwlock_ptr(handle) else {
        return 0;
    };
    let r: &'static RwLock<i64> = unsafe { &*ptr };
    let g = r.write().unwrap_or_else(|e| e.into_inner());
    let id = next_guard_id();
    GUARDS.with(|cell| {
        cell.borrow_mut().insert(id, GuardSlot::Write(g));
    });
    id
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_RWLOCK_UNLOCK(guard: u64) {
    GUARDS.with(|cell| {
        cell.borrow_mut().remove(&guard);
    });
}
