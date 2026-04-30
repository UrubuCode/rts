//! RwLock<i64> — read/write guards retornados como handles distintos.
//!
//! Cada chamada a `rwlock_read`/`rwlock_write` aloca um id de guard novo
//! (contador atomico) e armazena o guard em mapa thread-local. `unlock`
//! consome o id, dropando o guard.
//!
//! Soundness (#280): cada guard armazena um clone do `Arc<RwLock<i64>>`,
//! ancorando o lifetime real do guard. Antes era `Box` + transmute para
//! `'static`, com UB se `free` vinha antes de `unlock`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use super::super::gc::handles::{Entry, alloc_entry, shard_for_handle};

/// Cada slot ancora o Arc para garantir que o RwLock viva enquanto o
/// guard existir, mesmo apos free do handle original.
#[allow(dead_code)]
struct ReadSlot {
    arc: Arc<RwLock<i64>>,
    guard: RwLockReadGuard<'static, i64>,
}

#[allow(dead_code)]
struct WriteSlot {
    arc: Arc<RwLock<i64>>,
    guard: RwLockWriteGuard<'static, i64>,
}

#[allow(dead_code)]
enum GuardSlot {
    Read(ReadSlot),
    Write(WriteSlot),
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
    alloc_entry(Entry::SyncRwLock(Arc::new(RwLock::new(initial))))
}

fn rwlock_arc(handle: u64) -> Option<Arc<RwLock<i64>>> {
    let guard = shard_for_handle(handle).lock().unwrap();
    match guard.get(handle) {
        Some(Entry::SyncRwLock(r)) => Some(r.clone()),
        _ => None,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_RWLOCK_READ(handle: u64) -> u64 {
    let Some(arc) = rwlock_arc(handle) else {
        return 0;
    };
    // SAFETY: ancoramos pelo Arc clone, locamos via Arc::as_ptr.
    let ptr: *const RwLock<i64> = Arc::as_ptr(&arc);
    let r: &'static RwLock<i64> = unsafe { &*ptr };
    let g: RwLockReadGuard<'static, i64> = r.read().unwrap_or_else(|e| e.into_inner());
    let id = next_guard_id();
    GUARDS.with(|cell| {
        cell.borrow_mut().insert(id, GuardSlot::Read(ReadSlot { arc, guard: g }));
    });
    id
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_RWLOCK_WRITE(handle: u64) -> u64 {
    let Some(arc) = rwlock_arc(handle) else {
        return 0;
    };
    let ptr: *const RwLock<i64> = Arc::as_ptr(&arc);
    let r: &'static RwLock<i64> = unsafe { &*ptr };
    let g: RwLockWriteGuard<'static, i64> = r.write().unwrap_or_else(|e| e.into_inner());
    let id = next_guard_id();
    GUARDS.with(|cell| {
        cell.borrow_mut().insert(id, GuardSlot::Write(WriteSlot { arc, guard: g }));
    });
    id
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_RWLOCK_UNLOCK(guard: u64) {
    GUARDS.with(|cell| {
        cell.borrow_mut().remove(&guard);
    });
}
