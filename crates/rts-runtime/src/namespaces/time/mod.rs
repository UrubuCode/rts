//! `time` namespace — monotonic and wall-clock timestamps, plus blocking sleeps.
//!
//! `now_*` are monotonic (anchored at process start via OnceLock, never goes
//! backward). `unix_*` are wall-clock from the UNIX epoch (can jump on clock
//! adjustments). `sleep_ms` is an event-loop quiescence point (pumps timers).
//!
//! Migrado pro modelo builder do `rts-engine` (Fase 2; ver `namespaces/hint`).

use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rts_engine::{sig, Engine, FnPtr, Member, MemberFlags, MemberKind};

fn anchor() -> Instant {
    static ANCHOR: OnceLock<Instant> = OnceLock::new();
    *ANCHOR.get_or_init(Instant::now)
}

/// Monotonic milliseconds since process start.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TIME_NOW_MS() -> i64 {
    anchor().elapsed().as_millis() as i64
}

/// Monotonic nanoseconds since process start.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TIME_NOW_NS() -> i64 {
    anchor().elapsed().as_nanos() as i64
}

/// Wall-clock milliseconds since the UNIX epoch.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TIME_UNIX_MS() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Wall-clock nanoseconds since the UNIX epoch.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TIME_UNIX_NS() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

/// Sleeps the current thread for `ms` milliseconds.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TIME_SLEEP_MS(ms: i64) {
    // (#207 timer ordering / cross-runtime #393) sleep eh ponto de quiescencia
    // do event loop: faz pump dirigido por tempo ate `target`.
    let target = Instant::now() + Duration::from_millis(ms.max(0) as u64);
    crate::namespaces::globals::timers::instance::pump_until(target);
}

/// Sleeps the current thread for `ns` nanoseconds.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TIME_SLEEP_NS(ns: i64) {
    if ns > 0 {
        std::thread::sleep(Duration::from_nanos(ns as u64));
    }
}

fn func(name: &str, symbol: &str, sig: rts_engine::Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
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
    }
}

/// Registra a namespace `time` no motor (Fase 2).
pub fn register(e: &mut Engine) {
    e.ns("time")
        .doc("Monotonic and wall-clock timestamps, plus blocking sleeps.")
        .member(func(
            "now_ms",
            "__RTS_FN_NS_TIME_NOW_MS",
            sig!(=> I64),
            "now_ms(): number",
            "Monotonic milliseconds since process start.",
            __RTS_FN_NS_TIME_NOW_MS as *const u8,
        ))
        .member(func(
            "now_ns",
            "__RTS_FN_NS_TIME_NOW_NS",
            sig!(=> I64),
            "now_ns(): number",
            "Monotonic nanoseconds since process start.",
            __RTS_FN_NS_TIME_NOW_NS as *const u8,
        ))
        .member(func(
            "unix_ms",
            "__RTS_FN_NS_TIME_UNIX_MS",
            sig!(=> I64),
            "unix_ms(): number",
            "Wall-clock milliseconds since the UNIX epoch.",
            __RTS_FN_NS_TIME_UNIX_MS as *const u8,
        ))
        .member(func(
            "unix_ns",
            "__RTS_FN_NS_TIME_UNIX_NS",
            sig!(=> I64),
            "unix_ns(): number",
            "Wall-clock nanoseconds since the UNIX epoch.",
            __RTS_FN_NS_TIME_UNIX_NS as *const u8,
        ))
        .member(func(
            "sleep_ms",
            "__RTS_FN_NS_TIME_SLEEP_MS",
            sig!(I64 => Void),
            "sleep_ms(ms: number): void",
            "Sleeps the current thread for `ms` milliseconds.",
            __RTS_FN_NS_TIME_SLEEP_MS as *const u8,
        ))
        .member(func(
            "sleep_ns",
            "__RTS_FN_NS_TIME_SLEEP_NS",
            sig!(I64 => Void),
            "sleep_ns(ns: number): void",
            "Sleeps the current thread for `ns` nanoseconds.",
            __RTS_FN_NS_TIME_SLEEP_NS as *const u8,
        ))
        .done();
}
