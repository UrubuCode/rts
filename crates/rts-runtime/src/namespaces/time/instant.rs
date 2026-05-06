//! Monotonic timestamps anchored at process start.
//!
//! Uses a `std::sync::OnceLock` to fix the anchor on first call so that
//! repeated calls return an elapsed time that only grows forward, even
//! across suspended laptops or NTP adjustments (`std::time::Instant`
//! guarantees monotonicity). i64 return fits ~292 years in nanoseconds
//! and ~292 million years in milliseconds; the i64 cast is lossless
//! for any realistic process lifetime.

use std::sync::OnceLock;
use std::time::Instant;

fn anchor() -> Instant {
    static ANCHOR: OnceLock<Instant> = OnceLock::new();
    *ANCHOR.get_or_init(Instant::now)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TIME_NOW_MS() -> i64 {
    anchor().elapsed().as_millis() as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TIME_NOW_NS() -> i64 {
    anchor().elapsed().as_nanos() as i64
}
