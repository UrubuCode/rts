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

/// Run `src` (console.log captured via the real-pool-backed sink) and assert its
/// rendered stdout equals `expected`.
pub(crate) fn assert_stdout(src: &str, expected: &str) {
    match render_source(src) {
        Ok(out) => assert_eq!(out, expected, "stdout mismatch for source:\n{src}"),
        Err(e) => panic!("render_source failed for:\n{src}\n  -> {e}"),
    }
}

/// Like [`assert_stdout`], but compiles `prelude` (a declarations-only TS stdlib)
/// ahead of `src`, merged into one module, so the prelude's classes/functions are
/// ambient in `src` (a prelude `class Map` shadows the native Map).
pub(super) fn assert_stdout_with_prelude(prelude: &str, src: &str, expected: &str) {
    match render_source_with_prelude(prelude, src) {
        Ok(out) => assert_eq!(out, expected, "stdout mismatch for source:\n{src}"),
        Err(e) => panic!("render_source_with_prelude failed for:\n{src}\n  -> {e}"),
    }
}

/// Assert that running `src` BAILS (an explicit `Unsupported`, never a wrong
/// value). Used by every negative test.
pub(crate) fn assert_bails(src: &str) {
    let res = run_source(src);
    assert!(res.is_err(), "expected an Unsupported bail, got {res:?} for:\n{src}");
}

mod ambient_declare;
mod array;
mod arraycb;
mod class;
mod date;
mod dyndispatch;
mod globals;
mod class_inherit;
mod closure;
mod destructure;
mod fn_ctor;
mod fn_props;
mod free_this;
mod funcval;
mod globalclass;
mod heap_field;
mod inspect;
mod instanceof_dyn;
mod loops;
mod math_tagged;
mod mathobj;
mod method;
mod number;
mod objmethod;
mod object;
mod objdyn;
mod objstatic;
mod optchain;
mod poly;
mod precision;
mod prelude;
mod regex;
mod run;
mod stdlib_parity;
mod string;
mod string_class;
mod template;
mod toprimitive;
mod trycatch;
mod type_erasure;
mod update_tagged;
