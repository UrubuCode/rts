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
    eprintln!(
        "[rts-timing] {label:<28} {:>8.2} ms",
        t0.elapsed().as_secs_f64() * 1e3
    );
    out
}

/// Report an already-measured duration (for phases that can't be wrapped in a
/// closure because of borrow flow).
pub fn report(label: &str, t0: Instant) {
    if enabled() {
        eprintln!(
            "[rts-timing] {label:<28} {:>8.2} ms",
            t0.elapsed().as_secs_f64() * 1e3
        );
    }
}

/// Report a plain count/size datum (prelude line count, symbol count, …).
pub fn note(label: &str, value: usize) {
    if enabled() {
        eprintln!("[rts-timing] {label:<28} {value:>8}");
    }
}

// ---- module-mutation accounting inside the serial IR phase -------------------

thread_local! {
    /// Nanoseconds spent inside `Module` MUTATIONS while building function IR
    /// (`declare_function`, `declare_data`/`define_data`, the `FuncRef` import),
    /// plus how many happened.
    ///
    /// This is the number that decides `CRANELIFT_IMPLEMENTATION.md` §7 item 6.
    /// Building IR is serial ONLY because it holds `&mut dyn Module` for these
    /// calls; the rest of the phase is per-function work that could run on any
    /// core. So the phase splits into "the part that forces serialization" and
    /// "the part parallelising would win", and item 6's ceiling is the second
    /// number — not the whole 10.8 ms.
    static DECL_NS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static DECL_N: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Time one module mutation. Free (an `enabled()` read) when timing is off.
pub fn declare<T>(f: impl FnOnce() -> T) -> T {
    if !enabled() {
        return f();
    }
    let t0 = Instant::now();
    let out = f();
    mark_dirty();
    let ns = t0.elapsed().as_nanos() as u64;
    DECL_NS.with(|c| c.set(c.get() + ns));
    DECL_N.with(|c| c.set(c.get() + 1));
    out
}

/// Report the accumulated module-mutation time, then reset it.
pub fn report_declares(label: &str) {
    if !enabled() {
        return;
    }
    let ns = DECL_NS.with(|c| c.replace(0));
    let n = DECL_N.with(|c| c.replace(0));
    eprintln!(
        "[rts-timing] {label:<28} {:>8.2} ms  ({n} calls)",
        ns as f64 / 1e6
    );
}

thread_local! {
    /// Did the function currently being lowered request a module MUTATION?
    /// Feeds [`note_fn_mutated`] — the count that decides whether a speculative
    /// read-only parallel lowering could work at all (a function that mutates
    /// would have to be redone serially).
    static FN_DIRTY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static FNS_DIRTY: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static FNS_TOTAL: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Mark the function being lowered as having mutated the module.
pub fn mark_dirty() {
    if enabled() {
        FN_DIRTY.with(|c| c.set(true));
    }
}

/// Close the books on one function's lowering.
pub fn end_fn() {
    if !enabled() {
        return;
    }
    FNS_TOTAL.with(|c| c.set(c.get() + 1));
    if FN_DIRTY.with(|c| c.replace(false)) {
        FNS_DIRTY.with(|c| c.set(c.get() + 1));
    }
}

/// Report how many lowered functions touched the module, and reset.
pub fn report_dirty(label: &str) {
    if !enabled() {
        return;
    }
    let d = FNS_DIRTY.with(|c| c.replace(0));
    let t = FNS_TOTAL.with(|c| c.replace(0));
    eprintln!("[rts-timing] {label:<28} {d} of {t} fns mutate the module");
}
