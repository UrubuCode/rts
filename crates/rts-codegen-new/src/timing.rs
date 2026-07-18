//! Startup/compile PHASE TIMING — opt-in via `RTS_TIMING=1`.
//!
//! A `rts run` of an EMPTY program costs the same fixed startup as a real one
//! (Registry build + the ~5k-line embedded `.ts` prelude parsed, lowered and
//! Cranelift-compiled on every process). This module makes that cost VISIBLE per
//! phase so an optimization can be measured instead of guessed.
//!
//! Zero cost when the env var is unset: [`enabled`] is a `OnceLock<bool>` read
//! and every `phase` closure runs unwrapped either way.

use std::sync::OnceLock;
use std::time::Instant;

/// Is `RTS_TIMING` set to something other than `0`/empty?
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| match std::env::var("RTS_TIMING") {
        Ok(v) => !v.is_empty() && v != "0",
        Err(_) => false,
    })
}

/// Run `f`, reporting its wall time as `label` on stderr when timing is on.
/// Returns `f`'s value untouched — wrapping a phase never changes behavior.
pub fn phase<T>(label: &str, f: impl FnOnce() -> T) -> T {
    if !enabled() {
        return f();
    }
    let t0 = Instant::now();
    let out = f();
    eprintln!("[rts-timing] {label:<28} {:>8.2} ms", t0.elapsed().as_secs_f64() * 1e3);
    out
}

/// Report an already-measured duration (for phases that can't be wrapped in a
/// closure because of borrow flow).
pub fn report(label: &str, t0: Instant) {
    if enabled() {
        eprintln!("[rts-timing] {label:<28} {:>8.2} ms", t0.elapsed().as_secs_f64() * 1e3);
    }
}

/// Report a plain count/size datum (prelude line count, symbol count, …).
pub fn note(label: &str, value: usize) {
    if enabled() {
        eprintln!("[rts-timing] {label:<28} {value:>8}");
    }
}
