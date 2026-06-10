//! `thread` namespace — thread primitives over `std::thread` + the shared tokio
//! runtime.
//!
//! `spawn` takes an `extern "C" fn(u64) -> u64` pointer + a u64 arg; the thread
//! returns a u64 collected by `join`. Variants: std-thread spawn/join, a global
//! worker pool (spawn_detached), and tokio spawn_blocking (spawn_async*). `id()`
//! is a stable per-thread u64 (ThreadId::as_u64 is still unstable).
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use rts_abi::ty::{I64, U64};
use rts_macro::rts_namespace;

use crate::namespaces::gc::handles::{Entry, alloc_entry, free_handle, with_entry_mut};
use crate::namespaces::gc::thread_registry;

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

/// Thread primitives: std-thread spawn/join, worker pool, tokio spawn_blocking.
#[rts_namespace(thread)]
impl ThreadNs {
    /// Spawns an OS thread running `fn_ptr(arg)`. JoinHandle, 0 on null fn.
    #[rts_fn(raw_bits_arg)]
    pub fn spawn(fn_ptr: U64, arg: U64) -> U64 {
        if fn_ptr == 0 {
            return 0;
        }
        // SAFETY: codegen contract — `fn_ptr` is `extern "C" fn(u64) -> u64`.
        let f: extern "C" fn(u64) -> u64 = unsafe { std::mem::transmute(fn_ptr as usize) };
        let jh = thread::spawn(move || {
            thread_registry::register_current();
            let r = f(arg);
            thread_registry::unregister_current();
            r
        });
        let h = alloc_entry(Entry::JoinHandle(Box::new(jh)));
        record_scoped_handle(h);
        h
    }

    /// Fire-and-forget `fn_ptr(arg)` on the shared tokio runtime (spawn_blocking).
    #[rts_fn(raw_bits_arg)]
    pub fn spawn_async(fn_ptr: U64, arg: U64) {
        if fn_ptr == 0 {
            return;
        }
        crate::runtime::async_rt::handle().spawn_blocking(move || {
            // SAFETY: codegen contract — `extern "C" fn(u64) -> u64`.
            let f: extern "C" fn(u64) -> u64 = unsafe { std::mem::transmute(fn_ptr as usize) };
            let _ = f(arg);
        });
    }

    /// Like spawn_async but returns an id for `join_async`. 0 on null fn.
    #[rts_fn(raw_bits_arg)]
    pub fn spawn_async_join(fn_ptr: U64, arg: U64) -> U64 {
        if fn_ptr == 0 {
            return 0;
        }
        let jh = crate::runtime::async_rt::handle().spawn_blocking(move || {
            // SAFETY: codegen contract — `extern "C" fn(u64) -> u64`.
            let f: extern "C" fn(u64) -> u64 = unsafe { std::mem::transmute(fn_ptr as usize) };
            f(arg)
        });
        let id = next_join_id();
        join_store()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, jh);
        id
    }

    /// Awaits the spawn_async_join task `id` and returns its value. 0 if invalid.
    #[rts_fn]
    pub fn join_async(id: U64) -> U64 {
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
    #[rts_fn(raw_bits_arg)]
    pub fn spawn_detached(fn_ptr: U64, arg: U64) {
        pool_submit(fn_ptr, arg);
    }

    /// Spawns an OS thread running `fn_ptr(userdata, arg)`. JoinHandle, 0 on null fn.
    #[rts_fn(raw_bits_arg)]
    pub fn spawn_with_ud(fn_ptr: U64, arg: U64, userdata: U64) -> U64 {
        if fn_ptr == 0 {
            return 0;
        }
        // SAFETY: codegen contract — `extern "C" fn(u64, u64) -> u64`.
        let f: extern "C" fn(u64, u64) -> u64 = unsafe { std::mem::transmute(fn_ptr as usize) };
        let jh = thread::spawn(move || {
            thread_registry::register_current();
            let r = f(userdata, arg);
            thread_registry::unregister_current();
            r
        });
        let h = alloc_entry(Entry::JoinHandle(Box::new(jh)));
        record_scoped_handle(h);
        h
    }

    /// Runs `body()` in a scope that auto-joins every thread it spawned.
    #[rts_fn(ts = "scope(body: () => void): void")]
    pub fn scope(body: U64) {
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
    #[rts_fn]
    pub fn scope_with_ud(body: U64, userdata: U64) {
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
    #[rts_fn]
    pub fn join(thread: U64) -> U64 {
        let Some(jh) = take_join_handle(thread) else {
            return 0;
        };
        match jh.join() {
            Ok(value) => value,
            Err(_) => 0,
        }
    }

    /// Detaches (drops) the thread handle without joining.
    #[rts_fn]
    pub fn detach(thread: U64) {
        drop(take_join_handle(thread));
    }

    /// Stable per-thread id (assigned lazily; never 0).
    #[rts_fn]
    pub fn id() -> U64 {
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
    #[rts_fn]
    pub fn sleep_ms(ms: I64) {
        let ms = if ms < 0 { 0u64 } else { ms as u64 };
        thread::sleep(Duration::from_millis(ms));
    }
}
