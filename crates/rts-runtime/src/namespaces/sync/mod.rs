//! `sync` namespace — synchronisation primitives over `std::sync` (Mutex,
//! RwLock, Once).
//!
//! Mutex/RwLock guard um valor i64 interno; guards atravessam chamadas
//! extern "C" via mapas thread-local, ancorados por um clone do `Arc` para que
//! o lock sobreviva mesmo a um `free` precoce do handle (#280).
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Once, RwLock, RwLockReadGuard, RwLockWriteGuard};

use rts_abi::ty::{Handle, I64, U64};
use rts_macro::rts_namespace;

use crate::namespaces::gc::handles::{Entry, alloc_entry, free_handle, with_entry};

// ── Mutex guard storage ──────────────────────────────────────────────────────
struct OwnedMutexGuard {
    _arc: Arc<Mutex<i64>>,
    guard: MutexGuard<'static, i64>,
}

thread_local! {
    static MUTEX_GUARDS: RefCell<HashMap<u64, OwnedMutexGuard>> = RefCell::new(HashMap::new());
}

fn mutex_arc(handle: u64) -> Option<Arc<Mutex<i64>>> {
    with_entry(handle, |entry| match entry {
        Some(Entry::SyncMutex(m)) => Some(m.clone()),
        _ => None,
    })
}

// ── RwLock guard storage ─────────────────────────────────────────────────────
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
    static RWLOCK_GUARDS: RefCell<HashMap<u64, GuardSlot>> = RefCell::new(HashMap::new());
}

static GUARD_ID: AtomicU64 = AtomicU64::new(1);

fn next_guard_id() -> u64 {
    GUARD_ID.fetch_add(1, Ordering::SeqCst)
}

fn rwlock_arc(handle: u64) -> Option<Arc<RwLock<i64>>> {
    with_entry(handle, |entry| match entry {
        Some(Entry::SyncRwLock(r)) => Some(r.clone()),
        _ => None,
    })
}

/// std::sync: Mutex<i64>, RwLock<i64>, Once.
#[rts_namespace(sync)]
impl SyncNs {
    /// Aloca um Mutex<i64> inicializado e retorna o handle.
    #[rts_fn]
    pub fn mutex_new(initial: I64) -> Handle {
        alloc_entry(Entry::SyncMutex(Arc::new(Mutex::new(initial))))
    }

    /// Adquire o lock (bloqueante) e retorna o valor atual. 0 se handle invalido.
    #[rts_fn]
    pub fn mutex_lock(mutex: U64) -> I64 {
        let Some(arc) = mutex_arc(mutex) else {
            return 0;
        };
        // SAFETY: o Arc clone movido para o slot ancora o Mutex enquanto o guard existir.
        let ptr: *const Mutex<i64> = Arc::as_ptr(&arc);
        let m: &'static Mutex<i64> = unsafe { &*ptr };
        let g: MutexGuard<'static, i64> = m.lock().unwrap_or_else(|e| e.into_inner());
        let value = *g;
        MUTEX_GUARDS.with(|cell| {
            cell.borrow_mut().insert(
                mutex,
                OwnedMutexGuard {
                    _arc: arc,
                    guard: g,
                },
            );
        });
        value
    }

    /// Tenta adquirir o lock sem bloquear. Retorna o valor, ou 0 se ocupado/invalido.
    #[rts_fn]
    pub fn mutex_try_lock(mutex: U64) -> I64 {
        let Some(arc) = mutex_arc(mutex) else {
            return 0;
        };
        let ptr: *const Mutex<i64> = Arc::as_ptr(&arc);
        let m: &'static Mutex<i64> = unsafe { &*ptr };
        match m.try_lock() {
            Ok(g) => {
                let value = *g;
                MUTEX_GUARDS.with(|cell| {
                    cell.borrow_mut().insert(
                        mutex,
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

    /// Escreve `value` no Mutex (deve estar locado pela thread atual).
    #[rts_fn]
    pub fn mutex_set(mutex: U64, value: I64) {
        MUTEX_GUARDS.with(|cell| {
            if let Some(owned) = cell.borrow_mut().get_mut(&mutex) {
                *owned.guard = value;
            }
        });
    }

    /// Libera o lock detido pela thread atual.
    #[rts_fn]
    pub fn mutex_unlock(mutex: U64) {
        MUTEX_GUARDS.with(|cell| {
            cell.borrow_mut().remove(&mutex);
        });
    }

    /// Libera o handle do Mutex (remove guard pendente antes).
    #[rts_fn]
    pub fn mutex_free(mutex: U64) {
        MUTEX_GUARDS.with(|cell| {
            cell.borrow_mut().remove(&mutex);
        });
        free_handle(mutex);
    }

    /// Aloca um RwLock<i64> inicializado e retorna o handle.
    #[rts_fn]
    pub fn rwlock_new(initial: I64) -> Handle {
        alloc_entry(Entry::SyncRwLock(Arc::new(RwLock::new(initial))))
    }

    /// Adquire um read-guard e retorna um id de guard (0 se handle invalido).
    #[rts_fn]
    pub fn rwlock_read(rwlock: U64) -> Handle {
        let Some(arc) = rwlock_arc(rwlock) else {
            return 0;
        };
        let ptr: *const RwLock<i64> = Arc::as_ptr(&arc);
        let r: &'static RwLock<i64> = unsafe { &*ptr };
        let g: RwLockReadGuard<'static, i64> = r.read().unwrap_or_else(|e| e.into_inner());
        let id = next_guard_id();
        RWLOCK_GUARDS.with(|cell| {
            cell.borrow_mut()
                .insert(id, GuardSlot::Read(ReadSlot { arc, guard: g }));
        });
        id
    }

    /// Adquire um write-guard e retorna um id de guard (0 se handle invalido).
    #[rts_fn]
    pub fn rwlock_write(rwlock: U64) -> Handle {
        let Some(arc) = rwlock_arc(rwlock) else {
            return 0;
        };
        let ptr: *const RwLock<i64> = Arc::as_ptr(&arc);
        let r: &'static RwLock<i64> = unsafe { &*ptr };
        let g: RwLockWriteGuard<'static, i64> = r.write().unwrap_or_else(|e| e.into_inner());
        let id = next_guard_id();
        RWLOCK_GUARDS.with(|cell| {
            cell.borrow_mut()
                .insert(id, GuardSlot::Write(WriteSlot { arc, guard: g }));
        });
        id
    }

    /// Libera o read/write guard com o id dado.
    #[rts_fn]
    pub fn rwlock_unlock(guard: U64) {
        RWLOCK_GUARDS.with(|cell| {
            cell.borrow_mut().remove(&guard);
        });
    }

    /// Aloca um `Once` e retorna o handle.
    #[rts_fn]
    pub fn once_new() -> Handle {
        alloc_entry(Entry::SyncOnce(Box::new(Once::new())))
    }

    /// Invoca `fn_ptr` (extern "C" fn()) apenas uma vez; chamadas seguintes sao no-op.
    #[rts_fn]
    pub fn once_call(once: U64, fn_ptr: I64) {
        if fn_ptr == 0 {
            return;
        }
        let once_ptr: Option<*const Once> = with_entry(once, |entry| match entry {
            Some(Entry::SyncOnce(o)) => Some(o.as_ref() as *const Once),
            _ => None,
        });
        let Some(once_ptr) = once_ptr else { return };
        // SAFETY: once_ptr valido enquanto o slot existir; fn_ptr e' `extern "C" fn()` por contrato.
        let once_ref: &'static Once = unsafe { &*once_ptr };
        let f: extern "C" fn() = unsafe { std::mem::transmute(fn_ptr as usize) };
        once_ref.call_once(|| f());
    }
}
