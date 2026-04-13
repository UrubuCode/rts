//! Quiescence tracking and safe-collect entry points.
//!
//! The runtime calls `enter_scope()` / `exit_scope()` around every JS
//! function call, class method call, and closure execution. When the last
//! scope on the thread unwinds, `exit_scope()` triggers a full GC pass on the
//! thread-local arena — the "safe point" guarantee: no `Gc<'gc, _>` pointers
//! are live on the Rust stack at that moment.
//!
//! `safe_collect()` is the explicit form: call it anywhere you can guarantee
//! no arena pointers are live (e.g. after processing a batch of TS calls from
//! the eval loop).

use std::cell::Cell;

use super::arena;

// ── Thread-local scope depth ────────────────────────────────────────────────

thread_local! {
    /// Number of active JS scopes on this thread.
    static SCOPE_DEPTH: Cell<u32> = const { Cell::new(0) };

    /// Allocations since last collect (used for pressure-based collect_all).
    static ALLOC_PRESSURE: Cell<u64> = const { Cell::new(0) };
}

/// Threshold: after this many allocations within a top-level scope, trigger a
/// full collect on scope exit rather than just `collect_debt`.
const PRESSURE_THRESHOLD: u64 = 512;

// ── Scope markers ───────────────────────────────────────────────────────────

/// Mark entering a GC-managed scope (function call, class method, closure).
/// Must be paired with a matching `exit_scope()`.
#[inline]
pub fn enter_scope() {
    SCOPE_DEPTH.with(|d| d.set(d.get().saturating_add(1)));
}

/// Mark exiting a scope.
///
/// When the depth returns to zero (top-level quiescence):
/// - if allocation pressure is above threshold → `collect_all()`
/// - otherwise → `collect_debt()` (amortised, cheaper)
///
/// A compactacao do `ValueStore` e disparada por `__rts_call_dispatch`
/// (em `abi.rs`) ao voltar para quiescencia top-level. O codegen protege
/// handles vivos com pin/unpin ao redor de chamadas dinamicas para evitar
/// liberar slots ainda em uso pelo chamador.
#[inline]
pub fn exit_scope() {
    SCOPE_DEPTH.with(|depth| {
        let prev = depth.get();
        if prev > 0 {
            depth.set(prev - 1);
        }

        if prev <= 1 {
            // Back at quiescence — safe to collect
            let pressure = ALLOC_PRESSURE.with(|p| {
                let v = p.get();
                p.set(0);
                v
            });

            if pressure >= PRESSURE_THRESHOLD {
                arena::collect_all();
            } else {
                arena::collect_debt();
            }
        }
    });
}

// ── Explicit safe collect ───────────────────────────────────────────────────

/// Explicit full GC at a known quiescence point.
///
/// Guarantees: no `Gc<'gc, _>` pointers live on the Rust call stack when
/// this is called. Resets allocation pressure for this thread.
#[inline]
pub fn safe_collect() {
    ALLOC_PRESSURE.with(|p| p.set(0));
    arena::collect_all();
}

/// Notify the collector that an allocation was made in the current scope.
/// Called automatically by `gc.alloc` dispatch.
#[inline]
pub fn notify_alloc() {
    ALLOC_PRESSURE.with(|p| p.set(p.get().saturating_add(1)));
}

/// Current scope depth on this thread (0 = quiescent).
#[inline]
pub fn scope_depth() -> u32 {
    SCOPE_DEPTH.with(Cell::get)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespaces::gc::arena::{GcArena, GcBlob};

    #[test]
    fn scope_depth_tracks_enter_exit() {
        assert_eq!(scope_depth(), 0);
        enter_scope();
        enter_scope();
        assert_eq!(scope_depth(), 2);
        exit_scope();
        assert_eq!(scope_depth(), 1);
        exit_scope();
        assert_eq!(scope_depth(), 0);
    }

    #[test]
    fn safe_collect_resets_pressure() {
        notify_alloc();
        notify_alloc();
        safe_collect();
        // After safe_collect, pressure is reset; no panic
        ALLOC_PRESSURE.with(|p| assert_eq!(p.get(), 0));
    }

    #[test]
    fn exit_scope_at_quiescence_triggers_collect() {
        // Allocate something, enter a scope, exit: should not panic.
        let mut arena = GcArena::new();
        let h = arena.alloc(GcBlob::bool(true));
        arena.free(h);

        enter_scope();
        for _ in 0..PRESSURE_THRESHOLD + 1 {
            notify_alloc();
        }
        // exit_scope calls collect_all via the thread-local arena — just
        // verify it doesn't panic.
        exit_scope();
    }
}
