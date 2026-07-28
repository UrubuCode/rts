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
    rts_runtime::adapters::value::funcops::reset_state();
    rts_runtime::adapters::value::ctorval::reset_state();
    rts_runtime::adapters::value::errslot::reset_state();
    crate::shape::reset_global_shapes();
    rts_runtime::adapters::value::globalthis::reset();
    // Every remaining process-global table keyed by / holding a HandleTable word
    // from the current program's pool — the reset drains that pool, so a stale
    // entry read by the next program points a recycled slot at the wrong value.
    // The module doc says this fn "drains them all"; these were the ones it did
    // not, which the sound (post-pin-leak) GC turned from latent into observable.
    rts_runtime::adapters::value::protos::reset_state();
    rts_runtime::adapters::value::objops::reset_state();
    rts_runtime::adapters::value::iterops::reset_state();
    // Module-level mutable globals (#195): this thread's gcell store holds the
    // finished program's PolyValue words and is scanned as a GC ROOT
    // (`mark_gcell_roots`). Its `high` watermark is monotonic and was never
    // cleared, so the next program's collection scanned stale handle words from
    // the previous one — a recycled slot the sound GC could follow into a corrupt
    // entry (the residual STACK_OVERFLOW/ILLEGAL_INSTRUCTION crash, distinct from
    // the globalThis root above). Zeroing it here is the fix.
    rts_runtime::namespaces::gc::gcells::reset();
    reset_program_pins();
}

/// Release the pinned GC roots of programs that have finished — the NARROW drain.
///
/// A pin keeps one compiled program's string constants (handles the JIT spliced
/// into its code as `iconst` immediates, invisible to the conservative scanner)
/// alive for as long as that code can run. Nothing ever released them, so every
/// compile in a long-lived process permanently enlarged the root set: the live
/// handle count could only rise, and once it passed `GC_LIVE_FLOOR` the periodic
/// collector ran on every 256th allocation for the rest of the process — the
/// engine-level cause of the unit-test binary's hang.
///
/// Split out from [`reset_codegen_state`] because it is far NARROWER, and callers
/// that need to bound growth usually must NOT take the rest. The full reset also
/// drains the global shape registry and the `funcops` tables, and those are shared
/// by design with state that outlives a single compile — `globalThis` properties
/// and module singletons are reached through shape ids minted by an EARLIER
/// compile, so draining shapes orphans them and they read back `undefined`
/// (observed: `globalThis.x = 42` then reading it printed `undefined`).
///
/// Unpinning is safe once the owning program has finished: its code will never run
/// again, so nothing can dereference the stale immediate. Same precondition as
/// [`reset_codegen_state`] — call it only at a quiescent boundary, never while the
/// program that took the pins can still execute.
pub fn reset_program_pins() {
    rts_runtime::namespaces::gc::handles::reset_pinned_roots();
}
