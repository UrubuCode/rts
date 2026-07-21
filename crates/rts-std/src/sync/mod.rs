//! `sync` namespace — synchronisation primitives over `std::sync` (Mutex,
//! RwLock, Once).
//!
//! Mutex/RwLock guard um valor i64 interno; guards atravessam chamadas
//! extern "C" via mapas thread-local, ancorados por um clone do `Arc` para que
//! o lock sobreviva mesmo a um `free` precoce do handle (#280).
//!
//! Migrado do `#[rts_namespace]` pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`; ver pilotos hint/hash/ptr/mem/runtime).

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Once, RwLock, RwLockReadGuard, RwLockWriteGuard};

use rts_engine::abi::ty::{Handle, I64, U64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use rts_engine::heap::handles::{Entry, alloc_entry, free_handle, with_entry};

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

/// Aloca um Mutex<i64> inicializado e retorna o handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_MUTEX_NEW(initial: I64) -> Handle {
    alloc_entry(Entry::SyncMutex(Arc::new(Mutex::new(initial))))
}

/// Adquire o lock (bloqueante) e retorna o valor atual. 0 se handle invalido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_MUTEX_LOCK(mutex: U64) -> I64 {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_MUTEX_TRY_LOCK(mutex: U64) -> I64 {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_MUTEX_SET(mutex: U64, value: I64) {
    MUTEX_GUARDS.with(|cell| {
        if let Some(owned) = cell.borrow_mut().get_mut(&mutex) {
            *owned.guard = value;
        }
    });
}

/// Libera o lock detido pela thread atual.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_MUTEX_UNLOCK(mutex: U64) {
    MUTEX_GUARDS.with(|cell| {
        cell.borrow_mut().remove(&mutex);
    });
}

/// Libera o handle do Mutex (remove guard pendente antes).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_MUTEX_FREE(mutex: U64) {
    MUTEX_GUARDS.with(|cell| {
        cell.borrow_mut().remove(&mutex);
    });
    free_handle(mutex);
}

/// Aloca um RwLock<i64> inicializado e retorna o handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_RWLOCK_NEW(initial: I64) -> Handle {
    alloc_entry(Entry::SyncRwLock(Arc::new(RwLock::new(initial))))
}

/// Adquire um read-guard e retorna um id de guard (0 se handle invalido).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_RWLOCK_READ(rwlock: U64) -> Handle {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_RWLOCK_WRITE(rwlock: U64) -> Handle {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_RWLOCK_UNLOCK(guard: U64) {
    RWLOCK_GUARDS.with(|cell| {
        cell.borrow_mut().remove(&guard);
    });
}

/// Aloca um `Once` e retorna o handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_ONCE_NEW() -> Handle {
    alloc_entry(Entry::SyncOnce(Box::new(Once::new())))
}

/// Invoca `fn_ptr` (extern "C" fn()) apenas uma vez; chamadas seguintes sao no-op.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_SYNC_ONCE_CALL(once: U64, fn_ptr: I64) {
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

/// Função `sync.f(args)`.
fn func(name: &str, symbol: &str, sig: Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: false,
        emit: None,
    }
}

/// Registra a namespace `sync` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("sync")
        .doc("std::sync: Mutex<i64>, RwLock<i64>, Once.")
        .member(func(
            "mutex_new",
            "__RTS_FN_NS_SYNC_MUTEX_NEW",
            Sig::new(vec![AbiType::I64], AbiType::Handle),
            "mutex_new(initial: number): number",
            "Aloca um Mutex<i64> inicializado e retorna o handle.",
            __RTS_FN_NS_SYNC_MUTEX_NEW as *const u8,
        ))
        .member(func(
            "mutex_lock",
            "__RTS_FN_NS_SYNC_MUTEX_LOCK",
            Sig::new(vec![AbiType::U64], AbiType::I64),
            "mutex_lock(mutex: number): number",
            "Adquire o lock (bloqueante) e retorna o valor atual. 0 se handle invalido.",
            __RTS_FN_NS_SYNC_MUTEX_LOCK as *const u8,
        ))
        .member(func(
            "mutex_try_lock",
            "__RTS_FN_NS_SYNC_MUTEX_TRY_LOCK",
            Sig::new(vec![AbiType::U64], AbiType::I64),
            "mutex_try_lock(mutex: number): number",
            "Tenta adquirir o lock sem bloquear. Retorna o valor, ou 0 se ocupado/invalido.",
            __RTS_FN_NS_SYNC_MUTEX_TRY_LOCK as *const u8,
        ))
        .member(func(
            "mutex_set",
            "__RTS_FN_NS_SYNC_MUTEX_SET",
            Sig::new(vec![AbiType::U64, AbiType::I64], AbiType::Void),
            "mutex_set(mutex: number, value: number): void",
            "Escreve `value` no Mutex (deve estar locado pela thread atual).",
            __RTS_FN_NS_SYNC_MUTEX_SET as *const u8,
        ))
        .member(func(
            "mutex_unlock",
            "__RTS_FN_NS_SYNC_MUTEX_UNLOCK",
            Sig::new(vec![AbiType::U64], AbiType::Void),
            "mutex_unlock(mutex: number): void",
            "Libera o lock detido pela thread atual.",
            __RTS_FN_NS_SYNC_MUTEX_UNLOCK as *const u8,
        ))
        .member(func(
            "mutex_free",
            "__RTS_FN_NS_SYNC_MUTEX_FREE",
            Sig::new(vec![AbiType::U64], AbiType::Void),
            "mutex_free(mutex: number): void",
            "Libera o handle do Mutex (remove guard pendente antes).",
            __RTS_FN_NS_SYNC_MUTEX_FREE as *const u8,
        ))
        .member(func(
            "rwlock_new",
            "__RTS_FN_NS_SYNC_RWLOCK_NEW",
            Sig::new(vec![AbiType::I64], AbiType::Handle),
            "rwlock_new(initial: number): number",
            "Aloca um RwLock<i64> inicializado e retorna o handle.",
            __RTS_FN_NS_SYNC_RWLOCK_NEW as *const u8,
        ))
        .member(func(
            "rwlock_read",
            "__RTS_FN_NS_SYNC_RWLOCK_READ",
            Sig::new(vec![AbiType::U64], AbiType::Handle),
            "rwlock_read(rwlock: number): number",
            "Adquire um read-guard e retorna um id de guard (0 se handle invalido).",
            __RTS_FN_NS_SYNC_RWLOCK_READ as *const u8,
        ))
        .member(func(
            "rwlock_write",
            "__RTS_FN_NS_SYNC_RWLOCK_WRITE",
            Sig::new(vec![AbiType::U64], AbiType::Handle),
            "rwlock_write(rwlock: number): number",
            "Adquire um write-guard e retorna um id de guard (0 se handle invalido).",
            __RTS_FN_NS_SYNC_RWLOCK_WRITE as *const u8,
        ))
        .member(func(
            "rwlock_unlock",
            "__RTS_FN_NS_SYNC_RWLOCK_UNLOCK",
            Sig::new(vec![AbiType::U64], AbiType::Void),
            "rwlock_unlock(guard: number): void",
            "Libera o read/write guard com o id dado.",
            __RTS_FN_NS_SYNC_RWLOCK_UNLOCK as *const u8,
        ))
        .member(func(
            "once_new",
            "__RTS_FN_NS_SYNC_ONCE_NEW",
            Sig::new(vec![], AbiType::Handle),
            "once_new(): number",
            "Aloca um `Once` e retorna o handle.",
            __RTS_FN_NS_SYNC_ONCE_NEW as *const u8,
        ))
        .member(func(
            "once_call",
            "__RTS_FN_NS_SYNC_ONCE_CALL",
            Sig::new(vec![AbiType::U64, AbiType::I64], AbiType::Void),
            "once_call(once: number, fn_ptr: number): void",
            "Invoca `fn_ptr` (extern \"C\" fn()) apenas uma vez; chamadas seguintes sao no-op.",
            __RTS_FN_NS_SYNC_ONCE_CALL as *const u8,
        ))
        .done();
}
