//! `sync` namespace — primitivas de sincronizacao baseadas em
//! `std::sync` (Mutex, RwLock, Once).
//!
//! Mutex e RwLock guardam um valor i64 interno (handle ou inteiro) — o
//! caller TS-side e responsavel por chamar lock/unlock corretamente.
//! Os guards atravessam chamadas extern "C" via mapa thread-local:
//! `lock`/`read`/`write` armazenam o guard `'static` (ancorado por um
//! clone do `Arc` que vive na mesma struct), e `unlock` o remove e dropa.
//!
//! Soundness (#280): cada guard armazena um clone do `Arc<Mutex/RwLock>`,
//! ancorando o lifetime real do guard. Mesmo se `free` vier antes de
//! `unlock`, o Arc clone mantem a primitiva viva ate o drop.
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`). `Handle`-returning members carry an
//! explicit `ts = "...: number"` override (handles surface as `number`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Once, RwLock, RwLockReadGuard, RwLockWriteGuard};

use super::gc::handles::{Entry, alloc_entry, free_handle, with_entry};
use rts_abi::ty::{Handle, I64, U64};
use rts_macro::rts_namespace;

// ── Mutex guard storage ────────────────────────────────────────────

/// Guard owned: clona o Arc para ancorar o Mutex enquanto o guard existe.
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

// ── RwLock guard storage ───────────────────────────────────────────

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
    static RW_GUARDS: RefCell<HashMap<u64, GuardSlot>> = RefCell::new(HashMap::new());
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

/// Primitivas de sincronizacao (Mutex, RwLock, OnceLock) baseadas em std::sync.
#[rts_namespace(sync)]
impl SyncNs {
    // ── Mutex ──────────────────────────────────────────────────────

    /// Aloca um Mutex protegendo um valor i64 inicializado com `initial`.
    #[rts_fn(ts = "mutex_new(initial: number): number")]
    pub fn mutex_new(initial: I64) -> Handle {
        alloc_entry(Entry::SyncMutex(Arc::new(Mutex::new(initial))))
    }

    /// Bloqueia ate adquirir o Mutex e retorna o valor interno protegido. 0 se handle invalido.
    #[rts_fn(ts = "mutex_lock(mutex: number): number")]
    pub fn mutex_lock(handle: U64) -> I64 {
        let Some(arc) = mutex_arc(handle) else {
            return 0;
        };
        // SAFETY: ancoramos pelo Arc clone, locamos via Arc::as_ptr.
        let ptr: *const Mutex<i64> = Arc::as_ptr(&arc);
        let m: &'static Mutex<i64> = unsafe { &*ptr };
        let g: MutexGuard<'static, i64> = m.lock().unwrap_or_else(|e| e.into_inner());
        let value = *g;
        MUTEX_GUARDS.with(|cell| {
            cell.borrow_mut()
                .insert(handle, OwnedMutexGuard { _arc: arc, guard: g });
        });
        value
    }

    /// Tenta adquirir o Mutex sem bloquear. Retorna o valor interno em caso de sucesso, 0 se ja estava lockado ou handle invalido.
    #[rts_fn(ts = "mutex_try_lock(mutex: number): number")]
    pub fn mutex_try_lock(handle: U64) -> I64 {
        let Some(arc) = mutex_arc(handle) else {
            return 0;
        };
        let ptr: *const Mutex<i64> = Arc::as_ptr(&arc);
        let m: &'static Mutex<i64> = unsafe { &*ptr };
        match m.try_lock() {
            Ok(g) => {
                let value = *g;
                MUTEX_GUARDS.with(|cell| {
                    cell.borrow_mut()
                        .insert(handle, OwnedMutexGuard { _arc: arc, guard: g });
                });
                value
            }
            Err(_) => 0,
        }
    }

    /// Escreve `value` no Mutex. Caller deve ter chamado lock/try_lock antes (responsabilidade do caller).
    #[rts_fn(ts = "mutex_set(mutex: number, value: number): void")]
    pub fn mutex_set(handle: U64, value: I64) {
        MUTEX_GUARDS.with(|cell| {
            if let Some(owned) = cell.borrow_mut().get_mut(&handle) {
                *owned.guard = value;
            }
        });
    }

    /// Libera o Mutex previamente adquirido por lock/try_lock. No-op se nao havia guard ativo.
    #[rts_fn(ts = "mutex_unlock(mutex: number): void")]
    pub fn mutex_unlock(handle: U64) {
        MUTEX_GUARDS.with(|cell| {
            cell.borrow_mut().remove(&handle);
        });
    }

    /// Libera o Mutex e seu slot na HandleTable.
    #[rts_fn(ts = "mutex_free(mutex: number): void")]
    pub fn mutex_free(handle: U64) {
        MUTEX_GUARDS.with(|cell| {
            cell.borrow_mut().remove(&handle);
        });
        free_handle(handle);
    }

    // ── RwLock ─────────────────────────────────────────────────────

    /// Aloca um RwLock protegendo um valor i64 inicializado com `initial`.
    #[rts_fn(ts = "rwlock_new(initial: number): number")]
    pub fn rwlock_new(initial: I64) -> Handle {
        alloc_entry(Entry::SyncRwLock(Arc::new(RwLock::new(initial))))
    }

    /// Adquire um read guard (compartilhado) e retorna um handle de guard. Liberar via rwlock_unlock(guard). 0 se handle invalido.
    #[rts_fn(ts = "rwlock_read(rwlock: number): number")]
    pub fn rwlock_read(handle: U64) -> Handle {
        let Some(arc) = rwlock_arc(handle) else {
            return 0;
        };
        let ptr: *const RwLock<i64> = Arc::as_ptr(&arc);
        let r: &'static RwLock<i64> = unsafe { &*ptr };
        let g: RwLockReadGuard<'static, i64> = r.read().unwrap_or_else(|e| e.into_inner());
        let id = next_guard_id();
        RW_GUARDS.with(|cell| {
            cell.borrow_mut()
                .insert(id, GuardSlot::Read(ReadSlot { arc, guard: g }));
        });
        id
    }

    /// Adquire um write guard (exclusivo) e retorna um handle de guard. Liberar via rwlock_unlock(guard). 0 se handle invalido.
    #[rts_fn(ts = "rwlock_write(rwlock: number): number")]
    pub fn rwlock_write(handle: U64) -> Handle {
        let Some(arc) = rwlock_arc(handle) else {
            return 0;
        };
        let ptr: *const RwLock<i64> = Arc::as_ptr(&arc);
        let r: &'static RwLock<i64> = unsafe { &*ptr };
        let g: RwLockWriteGuard<'static, i64> = r.write().unwrap_or_else(|e| e.into_inner());
        let id = next_guard_id();
        RW_GUARDS.with(|cell| {
            cell.borrow_mut()
                .insert(id, GuardSlot::Write(WriteSlot { arc, guard: g }));
        });
        id
    }

    /// Libera um guard previamente adquirido via rwlock_read/rwlock_write.
    #[rts_fn(ts = "rwlock_unlock(guard: number): void")]
    pub fn rwlock_unlock(guard: U64) {
        RW_GUARDS.with(|cell| {
            cell.borrow_mut().remove(&guard);
        });
    }

    // ── OnceLock ───────────────────────────────────────────────────

    /// Aloca um OnceLock e retorna o handle.
    #[rts_fn(ts = "once_new(): number")]
    pub fn once_new() -> Handle {
        alloc_entry(Entry::SyncOnce(Box::new(Once::new())))
    }

    /// Executa `fn_ptr` (ponteiro para `extern "C" fn()`) exatamente uma vez por OnceLock. Chamadas subsequentes sao no-op.
    #[rts_fn(ts = "once_call(once: number, fn_ptr: number): void")]
    pub fn once_call(handle: U64, fn_ptr: I64) {
        if fn_ptr == 0 {
            return;
        }
        let once_ptr: Option<*const Once> = with_entry(handle, |entry| match entry {
            Some(Entry::SyncOnce(o)) => Some(o.as_ref() as *const Once),
            _ => None,
        });
        let Some(once_ptr) = once_ptr else { return };
        // SAFETY: once_ptr valido enquanto o slot da HandleTable existir.
        // fn_ptr e' tratado como `extern "C" fn()` por contrato com o codegen.
        let once: &'static Once = unsafe { &*once_ptr };
        let f: extern "C" fn() = unsafe { std::mem::transmute(fn_ptr as usize) };
        once.call_once(|| f());
    }
}
