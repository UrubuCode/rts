//! `time` namespace — monotonic and wall-clock timestamps, plus blocking sleeps.
//!
//! `now_*` are monotonic (anchored at process start via OnceLock, never goes
//! backward). `unix_*` are wall-clock from the UNIX epoch (can jump on clock
//! adjustments). `sleep_ms` is an event-loop quiescence point (pumps timers).
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`).

use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rts_abi::ty::I64;
use rts_macro::rts_namespace;

fn anchor() -> Instant {
    static ANCHOR: OnceLock<Instant> = OnceLock::new();
    *ANCHOR.get_or_init(Instant::now)
}

/// Monotonic and wall-clock timestamps, plus blocking sleeps.
#[rts_namespace(time)]
impl TimeNs {
    /// Monotonic milliseconds since process start.
    #[rts_fn]
    pub fn now_ms() -> I64 {
        anchor().elapsed().as_millis() as i64
    }

    /// Monotonic nanoseconds since process start.
    #[rts_fn]
    pub fn now_ns() -> I64 {
        anchor().elapsed().as_nanos() as i64
    }

    /// Wall-clock milliseconds since the UNIX epoch.
    #[rts_fn]
    pub fn unix_ms() -> I64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }

    /// Wall-clock nanoseconds since the UNIX epoch.
    #[rts_fn]
    pub fn unix_ns() -> I64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0)
    }

    /// Sleeps the current thread for `ms` milliseconds.
    #[rts_fn]
    pub fn sleep_ms(ms: I64) {
        // (#207 timer ordering / cross-runtime #393) sleep eh ponto de
        // quiescencia do event loop: faz pump dirigido por tempo ate `target`,
        // disparando microtasks, setImmediate e setTimeout que vencerem dentro
        // do intervalo, em ordem (deadline, seq) deterministica.
        let target = Instant::now() + Duration::from_millis(ms.max(0) as u64);
        crate::namespaces::globals::timers::instance::pump_until(target);
    }

    /// Sleeps the current thread for `ns` nanoseconds.
    #[rts_fn]
    pub fn sleep_ns(ns: I64) {
        if ns > 0 {
            std::thread::sleep(Duration::from_nanos(ns as u64));
        }
    }
}
