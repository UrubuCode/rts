//! Whole-program run tests — REAL `.ts` programs run end to end and print
//! correctly (parse → rts-hir → run-lowering → whole-module JIT → execute), each
//! asserting EXACT captured stdout against what Node/Bun would print.
//!
//! Out-of-subset constructs bail with an explicit `Unsupported` (the negative
//! tests), never a silent wrong value.
//!
//! Split by topic to keep each file under the 500-line module rule:
//! - [`run`]    — numeric/string `+`, typeof, formatting, cross-fn calls,
//!   control flow, equality, and the negative HIR-ambiguity bails.
//! - [`object`] — object/array literals + property/index access (P3).
//! - [`array`]  — Array instance methods without callbacks (P4.5).
//! - [`string`] — String instance methods (P4).
//! - [`number`] — Number instance methods (P4).
//! - [`method`] — method-dispatch bail cases (callbacks, dynamic receiver, …).
//! - [`funcval`] — first-class FUNCTION values (P4.6): reify + indirect invoke.

use super::{render_source, render_source_with_prelude, run_source};

/// Serializes every compile+run in this binary and releases the previous
/// program's pinned GC roots before each one. EVERY test here must go through it.
///
/// ## Why this is needed
///
/// These tests each compile and run a whole TS program in ONE process. Each
/// compile PINS its string constants as permanent GC roots (they live only as
/// `iconst` immediates, which the conservative scanner cannot see), and nothing
/// released them when the program finished. So the root set grew monotonically
/// across the suite and the live handle count could only rise. Once it passed
/// `GC_LIVE_FLOOR` (500k — a few dozen tests in) the periodic collector began
/// running on every 256th allocation for the rest of the process, and the binary
/// either crawled or wedged outright. The hang point moved between runs because it
/// depends on exactly when that threshold is crossed.
///
/// ## Why this drain, and not the full `reset_codegen_state`
///
/// Deliberately the NARROW [`crate::state::reset_program_pins`]. The full reset
/// also drains the global shape registry and the `funcops` tables, and those are
/// shared by design with state that outlives one compile: `globalThis` properties
/// and module singletons are reached through shape ids minted by an EARLIER
/// compile. Draining shapes per test orphans them — measured, `globalThis.x = 42`
/// followed by a read printed `undefined`, and three tests in the `globals` /
/// `class_as_value` / `modules_e2e` families failed. Only the pins have a strictly
/// one-program lifetime, and only the pins drive the growth above.
///
/// ## Why draining HERE is safe
///
/// Unpinning is sound once the owning program has finished — its code never runs
/// again, so nothing can dereference the stale immediate — but only if no OTHER
/// program is still live. `cargo test` runs these in parallel, so the lock is what
/// supplies that: held across the whole compile+run, it guarantees that at the
/// moment we drain, this thread owns the engine and no other test's program is
/// live or compiling. Serializing costs wall-clock, which is the correct trade for
/// a drain that would otherwise be unsound.
fn with_engine<R>(f: impl FnOnce() -> R) -> R {
    use std::sync::{Mutex, OnceLock};
    static ENGINE: OnceLock<Mutex<()>> = OnceLock::new();
    let lock = ENGINE.get_or_init(|| Mutex::new(()));
    // A panicking test (an assert failure) poisons the mutex; the engine state is
    // still drained at the top of the next acquisition, so recover and continue
    // rather than cascading one real failure into every later test.
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    crate::state::reset_program_pins();
    f()
}

/// Run `src` (console.log captured via the real-pool-backed sink) and assert its
/// rendered stdout equals `expected`.
pub(crate) fn assert_stdout(src: &str, expected: &str) {
    match with_engine(|| render_source(src)) {
        Ok(out) => assert_eq!(out, expected, "stdout mismatch for source:\n{src}"),
        Err(e) => panic!("render_source failed for:\n{src}\n  -> {e}"),
    }
}

/// Like [`assert_stdout`], but compiles `prelude` (a declarations-only TS stdlib)
/// ahead of `src`, merged into one module, so the prelude's classes/functions are
/// ambient in `src` (a prelude `class Map` shadows the native Map).
pub(super) fn assert_stdout_with_prelude(prelude: &str, src: &str, expected: &str) {
    match with_engine(|| render_source_with_prelude(prelude, src)) {
        Ok(out) => assert_eq!(out, expected, "stdout mismatch for source:\n{src}"),
        Err(e) => panic!("render_source_with_prelude failed for:\n{src}\n  -> {e}"),
    }
}

/// [`run_source`] under the shared engine guard — the REAL-stdout path (no
/// capture). Same reset+serialize contract as [`assert_stdout`].
pub(super) fn run_source_guarded(src: &str) -> super::FrontResult<()> {
    with_engine(|| run_source(src))
}

/// Assert that running `src` BAILS (an explicit `Unsupported`, never a wrong
/// value). Used by every negative test.
pub(crate) fn assert_bails(src: &str) {
    let res = with_engine(|| run_source(src));
    assert!(
        res.is_err(),
        "expected an Unsupported bail, got {res:?} for:\n{src}"
    );
}

mod ambient_declare;
mod arr_cb_dyn;
mod array;
mod array_ops;
mod arraycb;
mod boolean_class;
mod builtin_import;
mod class;
mod class_as_value;
mod class_inherit;
mod closure;
mod console_methods;
mod date;
mod destructure;
mod dyn_prop;
mod dyndispatch;
mod error_cause;
mod error_class;
mod fn_ctor;
mod fn_props;
mod free_this;
mod funcval;
mod gcell;
mod globalclass;
mod globals;
mod heap_field;
mod inspect;
mod instanceof_dyn;
mod json_parse;
mod logical;
mod loops;
mod math_tagged;
mod mathobj;
mod method;
mod modules_e2e;
mod number;
mod number_class;
mod numeric_coercion;
mod objdyn;
mod object;
mod object_class;
mod object_static;
mod objmethod;
mod objstatic;
mod optchain;
mod param_class_dispatch;
mod poly;
mod precision;
mod prelude;
mod proxy;
mod reflect;
mod regex;
mod relet;
mod ret_class_chain;
mod run;
mod stdlib_parity;
mod string;
mod string_class;
mod string_methods;
mod switch;
mod symbol;
mod template;
mod toprimitive;
mod trycatch;
mod type_erasure;
mod update_tagged;
mod url;
mod void_op;
