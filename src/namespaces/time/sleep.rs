//! Blocking sleeps via `std::thread::sleep`.

use std::thread;
use std::time::Duration;

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TIME_SLEEP_MS(ms: i64) {
    if ms > 0 {
        thread::sleep(Duration::from_millis(ms as u64));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TIME_SLEEP_NS(ns: i64) {
    if ns > 0 {
        thread::sleep(Duration::from_nanos(ns as u64));
    }
}
