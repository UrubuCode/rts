//! thread::id / thread::sleep_ms.
//!
//! `ThreadId::as_u64` ainda e unstable em Rust 1.93; usamos um id
//! atribuido por thread via `thread_local` + contador atomico global.
//! Garantimos `id != 0` reservando 0 como sentinela.

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static THREAD_ID: Cell<u64> = const { Cell::new(0) };
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_THREAD_ID() -> u64 {
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

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_THREAD_SLEEP_MS(ms: i64) {
    let ms = if ms < 0 { 0u64 } else { ms as u64 };
    thread::sleep(Duration::from_millis(ms));
}
