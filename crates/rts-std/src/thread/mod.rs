//! `thread` namespace — thread primitives over `std::thread` + the shared tokio
//! runtime.
//!
//! `spawn` takes an `extern "C" fn(u64) -> u64` pointer + a u64 arg; the thread
//! returns a u64 collected by `join`. Variants: std-thread spawn/join, a global
//! worker pool (spawn_detached), and tokio spawn_blocking (spawn_async*). `id()`
//! is a stable per-thread u64 (ThreadId::as_u64 is still unstable).
//!
//! Migrado do `#[rts_namespace]` pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`; ver pilotos hint/hash/ptr/mem/runtime). Os
//! `spawn*` carregam `raw_bits_arg` (o ptr da fn passa como bits crus, sem
//! coerção). `scope`/`scope_with_ud` chamam `__RTS_FN_NS_THREAD_JOIN` direto.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use rts_engine::abi::ty::{I64, U64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use rts_engine::heap::handles::{Entry, alloc_entry, free_handle, with_entry_mut};
use rts_engine::collector::thread_registry;

// ── scope tracking ────────────────────────────────────────────────────────────
thread_local! {
    static SCOPE_STACK: RefCell<Vec<Vec<u64>>> = RefCell::new(Vec::new());
}

fn record_scoped_handle(handle: u64) {
    SCOPE_STACK.with(|s| {
        if let Some(top) = s.borrow_mut().last_mut() {
            top.push(handle);
        }
    });
}

// ── per-thread id ─────────────────────────────────────────────────────────────
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
thread_local! {
    static THREAD_ID: Cell<u64> = const { Cell::new(0) };
}

// ── std JoinHandle take ───────────────────────────────────────────────────────
fn take_join_handle(handle: u64) -> Option<Box<std::thread::JoinHandle<u64>>> {
    let taken = with_entry_mut(handle, |entry| match entry {
        Some(e @ Entry::JoinHandle(_)) => {
            let prev = std::mem::replace(e, Entry::Free);
            if let Entry::JoinHandle(h) = prev {
                Some(h)
            } else {
                None
            }
        }
        _ => None,
    });
    free_handle(handle);
    taken
}

// ── global worker pool (spawn_detached) ───────────────────────────────────────
struct Job {
    fn_ptr: u64,
    arg: u64,
    ud: Option<u64>,
}

struct Pool {
    queue: Mutex<Vec<Job>>,
    cv: Condvar,
}

fn pool() -> &'static Pool {
    static P: OnceLock<Pool> = OnceLock::new();
    P.get_or_init(|| {
        let p = Pool {
            queue: Mutex::new(Vec::new()),
            cv: Condvar::new(),
        };
        let n: usize = std::env::var("RTS_THREAD_POOL_SIZE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8);
        for _ in 0..n {
            thread::spawn(worker_loop);
        }
        p
    })
}

fn worker_loop() -> ! {
    thread_registry::register_current();
    loop {
        let job = {
            let mut q = pool().queue.lock().unwrap_or_else(|e| e.into_inner());
            while q.is_empty() {
                q = pool().cv.wait(q).unwrap_or_else(|e| e.into_inner());
            }
            q.pop().expect("queue not empty")
        };
        // SAFETY: caller guarantees a valid fn_ptr (same as THREAD_SPAWN).
        unsafe {
            match job.ud {
                None => {
                    let f: extern "C" fn(u64) -> u64 = std::mem::transmute(job.fn_ptr as usize);
                    let _ = f(job.arg);
                }
                Some(ud) => {
                    let f: extern "C" fn(u64, u64) -> u64 =
                        std::mem::transmute(job.fn_ptr as usize);
                    let _ = f(ud, job.arg);
                }
            }
        }
    }
}

fn pool_submit(fn_ptr: u64, arg: u64) {
    if fn_ptr == 0 {
        return;
    }
    let p = pool();
    p.queue.lock().unwrap_or_else(|e| e.into_inner()).push(Job {
        fn_ptr,
        arg,
        ud: None,
    });
    p.cv.notify_one();
}

// ── tokio JoinHandle store (spawn_async_join) ─────────────────────────────────
fn join_store() -> &'static Mutex<HashMap<u64, tokio::task::JoinHandle<u64>>> {
    static S: OnceLock<Mutex<HashMap<u64, tokio::task::JoinHandle<u64>>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_join_id() -> u64 {
    static N: AtomicU64 = AtomicU64::new(1);
    N.fetch_add(1, Ordering::Relaxed)
}

/// Invoke a worker `fn_ptr(arg)` honoring the registered FN_KINDS ABI: a
/// worker declaring `arg: number` compiled with an f64 param (xmm) — the raw
/// `fn(u64)` transmute misloads it (#247). When the codegen registered the
/// kinds (via `getPointer`), the f64 form reads `arg`'s BITS back into the
/// float register; otherwise the historical raw-i64 call is unchanged.
fn invoke_worker(fn_ptr: u64, arg: u64) -> u64 {
    let f64_param = rts_primitives::function::ops::registered_fn_kinds(fn_ptr)
        .is_some_and(|(kinds, _)| kinds.first().copied() == Some(1));
    if f64_param {
        // SAFETY: codegen contract for a kinds-registered f64-param worker.
        let f: extern "C" fn(f64) -> u64 = unsafe { std::mem::transmute(fn_ptr as usize) };
        f(f64::from_bits(arg))
    } else {
        // SAFETY: codegen contract — `extern "C" fn(u64) -> u64`.
        let f: extern "C" fn(u64) -> u64 = unsafe { std::mem::transmute(fn_ptr as usize) };
        f(arg)
    }
}

/// Spawns an OS thread running `fn_ptr(arg)`. JoinHandle, 0 on null fn.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_THREAD_SPAWN(fn_ptr: U64, arg: U64) -> U64 {
    if fn_ptr == 0 {
        return 0;
    }
    // Snapshot the SPAWNING thread's module-global cells so the worker reads the same
    // `let`/`const` values (a JS module global is shared across a program's threads).
    // `GCELLS` is thread_local, so without this the worker reads every module-global
    // as `0` (e.g. a top-level `const c = atomic.i64_new(0)` → handle `0` → its writes
    // vanish; thread_basic's worker never updated the shared counter).
    let gcells = crate::collector::collector::gcell_share();
    let jh = thread::spawn(move || {
        thread_registry::register_current();
        crate::collector::collector::gcell_attach(gcells);
        let r = invoke_worker(fn_ptr, arg);
        thread_registry::unregister_current();
        r
    });
    let h = alloc_entry(Entry::JoinHandle(Box::new(jh)));
    record_scoped_handle(h);
    h
}

/// Fire-and-forget `fn_ptr(arg)` on the shared tokio runtime (spawn_blocking).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_THREAD_SPAWN_ASYNC(fn_ptr: U64, arg: U64) {
    if fn_ptr == 0 {
        return;
    }
    let gcells = crate::collector::collector::gcell_share();
    crate::runtime::async_rt::handle().spawn_blocking(move || {
        crate::collector::collector::gcell_attach(gcells);
        let _ = invoke_worker(fn_ptr, arg);
    });
}

/// Like spawn_async but returns an id for `join_async`. 0 on null fn.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_THREAD_SPAWN_ASYNC_JOIN(fn_ptr: U64, arg: U64) -> U64 {
    if fn_ptr == 0 {
        return 0;
    }
    let gcells = crate::collector::collector::gcell_share();
    let jh = crate::runtime::async_rt::handle().spawn_blocking(move || {
        crate::collector::collector::gcell_attach(gcells);
        invoke_worker(fn_ptr, arg)
    });
    let id = next_join_id();
    join_store()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id, jh);
    id
}

/// Awaits the spawn_async_join task `id` and returns its value. 0 if invalid.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_THREAD_JOIN_ASYNC(id: U64) -> U64 {
    let jh = {
        let mut map = join_store().lock().unwrap_or_else(|e| e.into_inner());
        map.remove(&id)
    };
    let Some(jh) = jh else { return 0 };
    match crate::runtime::async_rt::rt().block_on(jh) {
        Ok(v) => v,
        Err(_) => 0,
    }
}

/// Submits `fn_ptr(arg)` to the global worker pool (fire-and-forget).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_THREAD_SPAWN_DETACHED(fn_ptr: U64, arg: U64) {
    pool_submit(fn_ptr, arg);
}

/// Spawns an OS thread running `fn_ptr(userdata, arg)`. JoinHandle, 0 on null fn.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_THREAD_SPAWN_WITH_UD(fn_ptr: U64, arg: U64, userdata: U64) -> U64 {
    if fn_ptr == 0 {
        return 0;
    }
    // SAFETY: codegen contract — `extern "C" fn(u64, u64) -> u64`.
    let f: extern "C" fn(u64, u64) -> u64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    let gcells = crate::collector::collector::gcell_share();
    let jh = thread::spawn(move || {
        thread_registry::register_current();
        crate::collector::collector::gcell_attach(gcells);
        let r = f(userdata, arg);
        thread_registry::unregister_current();
        r
    });
    let h = alloc_entry(Entry::JoinHandle(Box::new(jh)));
    record_scoped_handle(h);
    h
}

/// Runs `body()` in a scope that auto-joins every thread it spawned.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_THREAD_SCOPE(body: U64) {
    if body == 0 {
        return;
    }
    SCOPE_STACK.with(|s| s.borrow_mut().push(Vec::new()));
    // SAFETY: codegen synthetic trampoline `extern "C" fn()`.
    let f: extern "C" fn() = unsafe { std::mem::transmute(body as usize) };
    f();
    let handles = SCOPE_STACK.with(|s| s.borrow_mut().pop().unwrap_or_default());
    for h in handles {
        __RTS_FN_NS_THREAD_JOIN(h);
    }
}

/// `scope` variant whose body captures `this` (userdata).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_THREAD_SCOPE_WITH_UD(body: U64, userdata: U64) {
    if body == 0 {
        return;
    }
    SCOPE_STACK.with(|s| s.borrow_mut().push(Vec::new()));
    // SAFETY: `extern "C" fn(u64)`.
    let f: extern "C" fn(u64) = unsafe { std::mem::transmute(body as usize) };
    f(userdata);
    let handles = SCOPE_STACK.with(|s| s.borrow_mut().pop().unwrap_or_default());
    for h in handles {
        __RTS_FN_NS_THREAD_JOIN(h);
    }
}

/// Joins the thread handle, returning its value. Consumes the handle. 0 if invalid.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_THREAD_JOIN(thread: U64) -> U64 {
    let Some(jh) = take_join_handle(thread) else {
        return 0;
    };
    match jh.join() {
        Ok(value) => value,
        Err(_) => 0,
    }
}

/// Detaches (drops) the thread handle without joining.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_THREAD_DETACH(thread: U64) {
    drop(take_join_handle(thread));
}

/// Stable per-thread id (assigned lazily; never 0).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_THREAD_ID() -> U64 {
    THREAD_ID.with(|cell| {
        let id = cell.get();
        if id != 0 {
            return id;
        }
        let new = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        cell.set(new);
        new
    })
}

/// Sleeps the current thread for `ms` milliseconds.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_THREAD_SLEEP_MS(ms: I64) {
    let ms = if ms < 0 { 0u64 } else { ms as u64 };
    thread::sleep(Duration::from_millis(ms));
}

/// Função `thread.f(args)` com flags de codegen-emit explícitas.
#[allow(clippy::too_many_arguments)]
fn func(
    name: &str,
    symbol: &str,
    sig: Sig,
    ts: &str,
    doc: &str,
    fp: *const u8,
    flags: MemberFlags,
) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: false,
        intrinsic: None,
        emit: None,
    }
}

/// Registra a namespace `thread` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("thread")
        .doc("Thread primitives: std-thread spawn/join, worker pool, tokio spawn_blocking.")
        .member(func(
            "spawn",
            "__RTS_FN_NS_THREAD_SPAWN",
            Sig::new(vec![AbiType::U64, AbiType::U64], AbiType::U64),
            "spawn(fn_ptr: number, arg: number): number",
            "Spawns an OS thread running `fn_ptr(arg)`. JoinHandle, 0 on null fn.",
            __RTS_FN_NS_THREAD_SPAWN as *const u8,
            MemberFlags::RAW_BITS_ARG,
        ))
        .member(func(
            "spawn_async",
            "__RTS_FN_NS_THREAD_SPAWN_ASYNC",
            Sig::new(vec![AbiType::U64, AbiType::U64], AbiType::Void),
            "spawn_async(fn_ptr: number, arg: number): void",
            "Fire-and-forget `fn_ptr(arg)` on the shared tokio runtime (spawn_blocking).",
            __RTS_FN_NS_THREAD_SPAWN_ASYNC as *const u8,
            MemberFlags::RAW_BITS_ARG,
        ))
        .member(func(
            "spawn_async_join",
            "__RTS_FN_NS_THREAD_SPAWN_ASYNC_JOIN",
            Sig::new(vec![AbiType::U64, AbiType::U64], AbiType::U64),
            "spawn_async_join(fn_ptr: number, arg: number): number",
            "Like spawn_async but returns an id for `join_async`. 0 on null fn.",
            __RTS_FN_NS_THREAD_SPAWN_ASYNC_JOIN as *const u8,
            MemberFlags::RAW_BITS_ARG,
        ))
        .member(func(
            "join_async",
            "__RTS_FN_NS_THREAD_JOIN_ASYNC",
            Sig::new(vec![AbiType::U64], AbiType::U64),
            "join_async(id: number): number",
            "Awaits the spawn_async_join task `id` and returns its value. 0 if invalid.",
            __RTS_FN_NS_THREAD_JOIN_ASYNC as *const u8,
            MemberFlags::NONE,
        ))
        .member(func(
            "spawn_detached",
            "__RTS_FN_NS_THREAD_SPAWN_DETACHED",
            Sig::new(vec![AbiType::U64, AbiType::U64], AbiType::Void),
            "spawn_detached(fn_ptr: number, arg: number): void",
            "Submits `fn_ptr(arg)` to the global worker pool (fire-and-forget).",
            __RTS_FN_NS_THREAD_SPAWN_DETACHED as *const u8,
            MemberFlags::RAW_BITS_ARG,
        ))
        .member(func(
            "spawn_with_ud",
            "__RTS_FN_NS_THREAD_SPAWN_WITH_UD",
            Sig::new(vec![AbiType::U64, AbiType::U64, AbiType::U64], AbiType::U64),
            "spawn_with_ud(fn_ptr: number, arg: number, userdata: number): number",
            "Spawns an OS thread running `fn_ptr(userdata, arg)`. JoinHandle, 0 on null fn.",
            __RTS_FN_NS_THREAD_SPAWN_WITH_UD as *const u8,
            MemberFlags::RAW_BITS_ARG,
        ))
        .member(func(
            "scope",
            "__RTS_FN_NS_THREAD_SCOPE",
            Sig::new(vec![AbiType::U64], AbiType::Void),
            "scope(body: () => void): void",
            "Runs `body()` in a scope that auto-joins every thread it spawned.",
            __RTS_FN_NS_THREAD_SCOPE as *const u8,
            MemberFlags::NONE,
        ))
        .member(func(
            "scope_with_ud",
            "__RTS_FN_NS_THREAD_SCOPE_WITH_UD",
            Sig::new(vec![AbiType::U64, AbiType::U64], AbiType::Void),
            "scope_with_ud(body: number, userdata: number): void",
            "`scope` variant whose body captures `this` (userdata).",
            __RTS_FN_NS_THREAD_SCOPE_WITH_UD as *const u8,
            MemberFlags::NONE,
        ))
        .member(func(
            "join",
            "__RTS_FN_NS_THREAD_JOIN",
            Sig::new(vec![AbiType::U64], AbiType::U64),
            "join(thread: number): number",
            "Joins the thread handle, returning its value. Consumes the handle. 0 if invalid.",
            __RTS_FN_NS_THREAD_JOIN as *const u8,
            MemberFlags::NONE,
        ))
        .member(func(
            "detach",
            "__RTS_FN_NS_THREAD_DETACH",
            Sig::new(vec![AbiType::U64], AbiType::Void),
            "detach(thread: number): void",
            "Detaches (drops) the thread handle without joining.",
            __RTS_FN_NS_THREAD_DETACH as *const u8,
            MemberFlags::NONE,
        ))
        .member(func(
            "id",
            "__RTS_FN_NS_THREAD_ID",
            Sig::new(vec![], AbiType::U64),
            "id(): number",
            "Stable per-thread id (assigned lazily; never 0).",
            __RTS_FN_NS_THREAD_ID as *const u8,
            MemberFlags::NONE,
        ))
        .member(func(
            "sleep_ms",
            "__RTS_FN_NS_THREAD_SLEEP_MS",
            Sig::new(vec![AbiType::I64], AbiType::Void),
            "sleep_ms(ms: number): void",
            "Sleeps the current thread for `ms` milliseconds.",
            __RTS_FN_NS_THREAD_SLEEP_MS as *const u8,
            MemberFlags::NONE,
        ))
        .done();
}
