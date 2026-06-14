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

use super::{render_source, run_source};

/// Run `src` (console.log captured via the real-pool-backed sink) and assert its
/// rendered stdout equals `expected`.
pub(crate) fn assert_stdout(src: &str, expected: &str) {
    match render_source(src) {
        Ok(out) => assert_eq!(out, expected, "stdout mismatch for source:\n{src}"),
        Err(e) => panic!("render_source failed for:\n{src}\n  -> {e}"),
    }
}

/// Assert that running `src` BAILS (an explicit `Unsupported`, never a wrong
/// value). Used by every negative test.
pub(crate) fn assert_bails(src: &str) {
    let res = run_source(src);
    assert!(res.is_err(), "expected an Unsupported bail, got {res:?} for:\n{src}");
}

mod array;
mod arraycb;
mod class;
mod globals;
mod class_inherit;
mod closure;
mod funcval;
mod globalclass;
mod inspect;
mod mathobj;
mod method;
mod number;
mod object;
mod objdyn;
mod objstatic;
mod poly;
mod precision;
mod run;
mod string;
