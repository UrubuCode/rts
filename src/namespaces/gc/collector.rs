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

// ─── Stack walker ─────────────────────────────────────────────────────────────

unsafe fn mark_stack_roots() {
    #[cfg(target_arch = "x86_64")]
    {
        let mut fp: usize;
        // Read the current frame pointer.
        unsafe { std::arch::asm!("mov {}, rbp", out(reg) fp, options(nostack, readonly)); }

        // Walk the frame-pointer chain. Each frame has the layout:
        //   [fp + 0] = saved caller rbp
        //   [fp + 8] = return address into caller
        //
        // The stack map at `return_addr` gives SP-relative offsets.
        // With frame pointers: caller_sp = fp + 16
        // (caller pushed ret addr then callee pushed rbp before mov rbp,rsp).
        const MAX_FRAMES: usize = 4096;
        let mut frame_count = 0;
        while fp != 0 && frame_count < MAX_FRAMES {
            // Guard against obviously invalid frame pointers.
            if fp < 4096 {
                break;
            }

            let ret_addr = unsafe { *(fp as *const usize).add(1) };
            if ret_addr == 0 {
                break;
            }

            if let Some(offsets) = stack_map_registry::lookup(ret_addr) {
                // caller_sp is the SP value in the caller frame at the call site.
                // The stack map entries give `caller_sp + offset` = handle location.
                let caller_sp = fp + 16;
                for offset in offsets {
                    let handle_addr = (caller_sp + offset as usize) as *const u64;
                    let handle = unsafe { *handle_addr };
                    if handle != 0 {
                        mark_handle(handle);
                    }
                }
            }

            // Follow frame pointer chain upward.
            let prev_fp = unsafe { *(fp as *const usize) };
            // Guard: frame pointer must strictly increase (stack grows down).
            if prev_fp <= fp {
                break;
            }
            fp = prev_fp;
            frame_count += 1;
        }
    }

    // On non-x86-64 targets: no-op, explicit-free path remains active.
    #[cfg(not(target_arch = "x86_64"))]
    let _ = ();
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
