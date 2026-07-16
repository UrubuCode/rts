//! Generic NATIVE EVENT SOURCES for the JS event loop.
//!
//! A backend module (`node:dgram`'s UDP reader, a future `node:net` server, …)
//! produces events on its OWN OS thread and must never touch the JS heap or
//! invoke a JS listener from there. It therefore registers a **pump**: an
//! `extern "C" fn() -> usize` that the event loop calls ON THE JS THREAD, which
//! drains that module's queue, builds the argument words, invokes the listeners
//! and returns how many events it delivered.
//!
//! This is a DATA table of fn-pointers plus a keep-alive counter — the loop
//! drains whatever registered itself without naming any module, and a producer
//! crate stays free of the loop's crate. It lives in `rts-engine` (the lowest
//! shared crate) so both producers (`rts-node`) and the consumer (the event loop
//! in `rts-std`) reach it with no layering cycle.
//!
//! Contrast with [`crate::watch_queue`]: that queue carries the EVENT DATA
//! itself (a listener + a path) because the fs-watch payload is plain data. A
//! pump is the general form — the producer keeps its own typed queue and only
//! exposes "drain yourself now, on this thread".

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

/// A source's drain function. Called on the JS thread; returns the number of
/// events it delivered (0 = it had nothing pending). Must not block.
pub type PumpFn = extern "C" fn() -> usize;

fn pumps() -> &'static Mutex<Vec<PumpFn>> {
    static P: OnceLock<Mutex<Vec<PumpFn>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(Vec::new()))
}

static ACTIVE: AtomicUsize = AtomicUsize::new(0);

/// Register a source's pump. Idempotent per fn-pointer, so a producer can call
/// it lazily (on its first live resource) from both the JIT and AOT paths.
pub fn register_pump(pump: PumpFn) {
    let mut list = pumps().lock().unwrap();
    if !list.iter().any(|p| std::ptr::fn_addr_eq(*p, pump)) {
        list.push(pump);
    }
}

/// Drain every registered source once, on the calling (JS) thread. Returns the
/// total number of events delivered.
pub fn pump_all() -> usize {
    // Snapshot: a pump invokes JS listeners, which may register another source.
    let snapshot: Vec<PumpFn> = pumps().lock().unwrap().clone();
    snapshot.iter().map(|p| p()).sum()
}

/// A ref'd resource opened — keeps the event loop draining (Node semantics: an
/// open, ref'd handle keeps the process alive).
pub fn inc_active() {
    ACTIVE.fetch_add(1, Ordering::AcqRel);
}

/// A ref'd resource closed (or `unref()`ed). Saturating — a double-close must
/// not underflow the counter.
pub fn dec_active() {
    let mut cur = ACTIVE.load(Ordering::Acquire);
    while cur > 0 {
        match ACTIVE.compare_exchange_weak(cur, cur - 1, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => break,
            Err(actual) => cur = actual,
        }
    }
}

/// Whether any ref'd source resource is still open.
pub fn active_count() -> usize {
    ACTIVE.load(Ordering::Acquire)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    static CALLS: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn pump() -> usize {
        CALLS.fetch_add(1, Ordering::AcqRel);
        2
    }

    #[test]
    fn register_is_idempotent_and_pump_all_sums() {
        register_pump(pump);
        register_pump(pump);
        assert_eq!(pump_all(), 2, "one registration despite two register calls");
        assert_eq!(CALLS.load(Ordering::Acquire), 1);
    }

    #[test]
    fn active_count_saturates_at_zero() {
        let base = active_count();
        dec_active();
        assert_eq!(active_count(), base.saturating_sub(1));
        inc_active();
        assert!(active_count() >= base);
    }
}
