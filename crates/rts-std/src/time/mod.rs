//! `time` namespace — monotonic and wall-clock timestamps, plus blocking sleeps.
//!
//! `now_*` are monotonic (anchored at process start via OnceLock, never goes
//! backward). `unix_*` are wall-clock from the UNIX epoch (can jump on clock
//! adjustments). `sleep_ms` is an event-loop quiescence point (pumps timers).
//!
//! Migrado pro modelo builder do `rts-engine` (Fase 2; ver `namespaces/hint`).

use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rts_engine::Engine;

fn anchor() -> Instant {
    static ANCHOR: OnceLock<Instant> = OnceLock::new();
    *ANCHOR.get_or_init(Instant::now)
}

/// Monotonic milliseconds since process start.
#[rtse::function(module = "time", value = "now_ms")]
fn now_ms() -> i64 {
    anchor().elapsed().as_millis() as i64
}

/// Monotonic nanoseconds since process start.
#[rtse::function(module = "time", value = "now_ns")]
fn now_ns() -> i64 {
    anchor().elapsed().as_nanos() as i64
}

/// Wall-clock milliseconds since the UNIX epoch.
#[rtse::function(module = "time", value = "unix_ms")]
fn unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Wall-clock nanoseconds since the UNIX epoch.
#[rtse::function(module = "time", value = "unix_ns")]
fn unix_ns() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

/// Sleeps the current thread for `ms` milliseconds.
#[rtse::function(module = "time", value = "sleep_ms")]
fn sleep_ms(ms: i64) {
    // (#207 timer ordering / cross-runtime #393) sleep eh ponto de quiescencia
    // do event loop: faz pump dirigido por tempo ate `target`.
    let target = Instant::now() + Duration::from_millis(ms.max(0) as u64);
    crate::globals::timers::instance::pump_until(target);
}

/// Sleeps the current thread for `ns` nanoseconds.
#[rtse::function(module = "time", value = "sleep_ns")]
fn sleep_ns(ns: i64) {
    if ns > 0 {
        std::thread::sleep(Duration::from_nanos(ns as u64));
    }
}

/// Registra a namespace `time` no motor (Fase 2).
pub fn register(e: &mut Engine) {
    e.module("time", |m| {
        m.doc("Monotonic and wall-clock timestamps, plus blocking sleeps.");
        m.registry(now_ms_entry());
        m.registry(now_ns_entry());
        m.registry(unix_ms_entry());
        m.registry(unix_ns_entry());
        m.registry(sleep_ms_entry());
        m.registry(sleep_ns_entry());
    });
}
