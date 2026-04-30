//! GC collection entry points backed by gc-arena.
//!
//! The old manual mark+sweep (conservative i64 scanning) is replaced by
//! gc-arena's incremental tri-color mark+sweep. Collection is driven by:
//!
//! 1. `alloc_entry` calls `collect_debt()` after every allocation
//!    (automatic pacing — collector keeps up with allocation rate).
//! 2. Codegen quiescence points (function returns, scope exits) may call
//!    `__RTS_FN_NS_GC_COLLECT_DEBT` for fine-grained incremental steps.
//! 3. Userland can trigger a full cycle via `gc.collect()`.
//!
//! Since handles are now freed explicitly via `free_handle` (which
//! removes the `Gc` from the root's `SlotMap`), the old "pass roots,
//! sweep everything else" contract no longer applies. The `root`
//! parameter in the legacy ABI functions is accepted but ignored —
//! liveness is determined by whether `free_handle` was called.

use super::handles::{collect_debt, finish_cycle, live_handle_count};

/// Triggers a full gc-arena collection cycle.
/// The `root` parameter is kept for ABI compatibility but is ignored:
/// with gc-arena, liveness is determined by presence in `HandleRoot.slots`,
/// not by explicit root sets.
pub fn collect(_roots: &[u64]) -> u64 {
    let before = live_handle_count() as u64;
    finish_cycle();
    let after = live_handle_count() as u64;
    before.saturating_sub(after)
}

// ─── Extern ABI ──────────────────────────────────────────────────────────────

/// Full collection cycle triggered from userland (`gc.collect(root)`).
/// Returns estimated number of handles swept (may be 0 if nothing was
/// unreachable — all handles freed explicitly via `free_handle`).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_COLLECT(root: u64) -> i64 {
    let _ = root; // legacy parameter, ignored
    collect(&[]) as i64
}

/// Collects with a Vec of roots (legacy multi-root API).
/// All parameters ignored; triggers a full cycle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_COLLECT_VEC(roots_vec: u64) -> i64 {
    let _ = roots_vec;
    collect(&[]) as i64
}

/// Incremental collection step. Call at quiescence points to pay off
/// GC debt without stopping the world.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_COLLECT_DEBT() {
    collect_debt();
}

/// Live handle count. Useful for benchmarks and leak detection.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_LIVE_COUNT() -> i64 {
    live_handle_count() as i64
}
