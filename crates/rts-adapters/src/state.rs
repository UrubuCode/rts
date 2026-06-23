//! Explicit, top-level drain for the engine's PROCESS-GLOBAL mutable state.
//!
//! The new engine carries process-global mutable tables that are NOT owned by a
//! single `Program`: the codegen-owned runtime side-tables (`value::funcops`
//! instance→ctor and function-property maps, `value::ctorval` constructor-thunk
//! address set), the thread-local pending throw slot (`value::errslot`), and the
//! compile-time global shape registry (`shape`). They exist as process globals by
//! necessity: the trampolines that fill them are `extern "C"` symbols the JIT'd
//! program calls with NO engine context to hang per-`Program` state off of.
//!
//! Across repeated compile/run cycles in ONE process (eval / hot-reload / the
//! test binary / a future browser-devtools backend) these accumulate — each run's
//! residue becomes the next run's growth. [`reset_codegen_state`] drains them all.
//!
//! ## DANGER — only safe when NO program/eval is live (NOT auto-called)
//!
//! This is deliberately NOT wired into `compile_program`. The state OUTLIVES a
//! single compile: a `GlobalShapeId` is baked into LIVE heap objects (read by the
//! inspect path) and `funcops::ctor_table` is keyed by LIVE instance words. A
//! reset while any program that built such state is still running CORRUPTS it —
//! we proved this: auto-resetting per compile made the parallel test binary (many
//! concurrent compiles) crash with STATUS_ILLEGAL_INSTRUCTION when one run reset a
//! shape id another live run then read. The SAME hazard exists in production for
//! NESTED compilation (`eval` / `new Function` / dynamic import compile a new
//! program WHILE the outer one is live).
//!
//! Therefore: call this ONLY at a quiescent top-level boundary — between two fully
//! independent top-level runs, with NO outer program frame and NO concurrent
//! compile in flight (e.g. a devtools "reset session" between hot-reloads). Never
//! mid-run, never from inside a compile.
//!
//! ## The real fix this does NOT replace
//!
//! Bounding the WITHIN-a-run growth of `funcops::ctor_table` (one entry per `new`
//! executed) needs the constructor identity stored INLINE in the object's
//! shape/slot (or GC-tied pruning), not a global drain — a separate increment.
//! The fully sound long-term shape is per-`Program` ownership of all of this,
//! which the `extern "C"` trampoline ABI does not currently allow.

/// Drain every process-global engine table to its empty state. Cheap and
/// idempotent. SAFE ONLY at a quiescent top-level boundary — see the module-level
/// DANGER note. Never call while a program/eval is live or a compile is in flight.
pub fn reset_codegen_state() {
    crate::value::funcops::reset_state();
    crate::value::ctorval::reset_state();
    crate::value::errslot::reset_state();
    crate::shape::reset_global_shapes();
}
