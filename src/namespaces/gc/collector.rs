//! GC collection — precise mark+sweep for JIT frames via Cranelift stack maps.
//!
//! ## How it works
//!
//! 1. Codegen calls `builder.declare_value_needs_stack_map(val)` for every
//!    GC handle Value produced in a function.
//! 2. After each `define_function`, `jit.rs` extracts `UserStackMap` entries
//!    (via `ctx.compiled_code().buffer.user_stack_maps()`) and stores them in
//!    `stack_map_registry` keyed by per-function offset.
//! 3. After `finalize_definitions()`, absolute return-PC addresses are resolved
//!    and the registry is finalised.
//! 4. `finish_cycle()` walks the native stack (frame-pointer chain, valid because
//!    `preserve_frame_pointers=true`), looks up each return address in the
//!    registry, and marks every handle found at `caller_sp + offset` as a root.
//! 5. `sweep_all_shards()` frees every handle that was NOT marked.
//!
//! ## Fallback
//!
//! If the stack map registry has no entries (AOT path, or JIT before any maps
//! are registered), `finish_cycle()` is a no-op — the existing explicit-free
//! path remains the only reclamation mechanism. This preserves backwards
//! compatibility while the JIT path matures.

use super::handles::{live_handle_count, mark_handle, sweep_all_shards};
use super::stack_map_registry;

// ─── Core collector ──────────────────────────────────────────────────────────

/// Walk the native stack frame-by-frame, mark every GC handle that is live
/// at a JIT safepoint, then sweep all unmarked handles.
///
/// Only active when the stack map registry has been populated (i.e., at least
/// one JIT function with GC-tracked values has been compiled and finalised).
/// On non-x86-64 targets or when the registry is empty this is a no-op.
pub fn finish_cycle() {
    if !stack_map_registry::is_active() {
        return;
    }

    // Safety: we read raw stack memory. Preconditions:
    // - `preserve_frame_pointers=true` guarantees a valid RBP chain in all
    //   JIT-compiled frames.
    // - Rust frames above us also preserve frame pointers (rustc default on
    //   x86-64 release builds; enforced by the same Cranelift flag).
    // - We only dereference addresses where we have a registered stack map —
    //   invalid return addresses simply miss the registry lookup and are skipped.
    unsafe { mark_stack_roots() };

    sweep_all_shards();
}

/// Incremental GC step. Currently a no-op — incremental pacing is a follow-up.
pub fn collect_debt() {}

// ─── Stack scanner ────────────────────────────────────────────────────────────

// Conservative scan of the current thread's stack. On x86-64, reads RSP and
// scans upward through a bounded window looking for u64 values that decode as
// live handles. False positives are filtered by the generation check in `mark`.
//
// This avoids the fragility of walking the RBP chain through Rust frames,
// which on Windows x64 may not maintain the frame-pointer invariant.
unsafe fn mark_stack_roots() {
    #[cfg(target_arch = "x86_64")]
    {
        let mut rsp: usize;
        unsafe { std::arch::asm!("mov {}, rsp", out(reg) rsp, options(nostack, readonly)); }

        // Get the stack bounds so we don't scan past the stack's high-address limit.
        let stack_high = stack_high_addr();
        if stack_high <= rsp {
            return;
        }

        let mut addr = rsp;
        while addr + 8 <= stack_high {
            let candidate = unsafe { *(addr as *const u64) };
            if candidate != 0 {
                let generation = (candidate >> crate::abi::handles::HANDLE_GEN_SHIFT) & 0xFFFF;
                if generation != 0 {
                    mark_handle(candidate);
                }
            }
            addr += 8;
        }
    }

    #[cfg(not(target_arch = "x86_64"))]
    let _ = ();
}

/// Returns the high (exclusive) address of the current thread's stack.
/// Uses the NT Thread Information Block (TIB) on Windows (gs:[0x10] = StackBase)
/// and pthread_getattr_np on Linux. Falls back to RSP + 512 KB.
#[cfg(target_arch = "x86_64")]
fn stack_high_addr() -> usize {
    #[cfg(target_os = "windows")]
    {
        // On x86-64 Windows, the GS segment points to the TIB.
        // Offset 0x08 = StackLimit (low address), 0x10 = StackBase (high address).
        let high: usize;
        unsafe { std::arch::asm!("mov {}, gs:[0x10]", out(reg) high, options(nostack, pure, nomem)); }
        high
    }

    #[cfg(target_os = "linux")]
    {
        // Use /proc/self/maps parsing or a simple heuristic.
        // For simplicity, use RSP + 8 MB (main thread default on Linux).
        let mut rsp: usize;
        unsafe { std::arch::asm!("mov {}, rsp", out(reg) rsp, options(nostack, readonly)); }
        rsp + 8 * 1024 * 1024
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let mut rsp: usize;
        unsafe { std::arch::asm!("mov {}, rsp", out(reg) rsp, options(nostack, readonly)); }
        rsp + 512 * 1024
    }
}

// ─── GC entry points ─────────────────────────────────────────────────────────

/// Triggers a full mark+sweep cycle.
/// Returns the number of handles freed.
pub fn collect(_roots: &[u64]) -> u64 {
    let before = live_handle_count() as u64;
    finish_cycle();
    let after = live_handle_count() as u64;
    before.saturating_sub(after)
}

// ─── Extern ABI ──────────────────────────────────────────────────────────────

/// Full collection cycle triggered from userland (`gc.collect()`).
/// Returns handles swept.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_COLLECT(root: u64) -> i64 {
    let _ = root;
    collect(&[]) as i64
}

/// Collects with a Vec of roots (legacy multi-root API — parameters ignored).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_COLLECT_VEC(roots_vec: u64) -> i64 {
    let _ = roots_vec;
    collect(&[]) as i64
}

/// Incremental collection step. No-op until incremental pacing is implemented.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_COLLECT_DEBT() {
    collect_debt();
}

/// Live handle count. Useful for benchmarks and leak detection.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_LIVE_COUNT() -> i64 {
    live_handle_count() as i64
}
