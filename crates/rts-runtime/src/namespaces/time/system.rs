//! Wall-clock timestamps measured from the UNIX epoch.
//!
//! Unlike `instant::*` these can jump backward when the user adjusts
//! their system clock. Use only when communicating time externally
//! (logs, protocols) — for measuring elapsed time, prefer the
//! monotonic `now_*` helpers.

use std::time::{SystemTime, UNIX_EPOCH};

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TIME_UNIX_MS() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TIME_UNIX_NS() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}
