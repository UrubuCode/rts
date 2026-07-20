//! Process-global-state leak verification — ISOLATED integration binary.
//!
//! These checks read/clear the engine's PROCESS-GLOBAL tables (the shape
//! registry, the `funcops` side-tables). That state is shared by every compile in
//! the process, so the assertions are only deterministic when this code is the
//! SOLE compiler running — which is exactly why this lives in its own integration
//! binary as a SINGLE `#[test]` (cargo runs it apart from the parallel lib-unit
//! binary, and one test fn means no intra-binary concurrency). Running the same
//! logic inside the parallel lib suite is unsound: another thread's `compile_program`
//! mutates the same globals mid-assert (it also makes a global reset corrupt a
//! concurrently-live run — see `rts_codegen_new::state`).
//!
//! What this proves, honestly:
//! 1. Distinct programs ACCUMULATE global shapes across runs with no auto-reset —
//!    i.e. the cross-run leak is real (there is no per-compile drain, by design:
//!    an auto-drain would corrupt nested `eval`/live programs).
//! 2. `reset_codegen_state()` DRAINS every global table back to empty — the safe,
//!    explicit top-level tool for a quiescent boundary (e.g. devtools reload).

use rts_codegen_new::front::run::render_source;
use rts_codegen_new::shape::global_shape_count;
use rts_codegen_new::state::reset_codegen_state;
use rts_codegen_new::value::funcops::table_lens;

#[test]
fn global_state_accumulates_then_drains() {
    // Quiescent start: drain so the counts below are measured from empty.
    reset_codegen_state();
    assert_eq!(
        global_shape_count(),
        0,
        "reset should leave the shape registry empty"
    );
    assert_eq!(
        table_lens(),
        (0, 0),
        "reset should leave the funcops tables empty"
    );

    // Run a program that interns an object shape `{a}`.
    let out = render_source("const o = { a: 1 }; console.log(o.a);")
        .expect("object-literal program A should run");
    assert_eq!(out, "1\n");
    let after_a = global_shape_count();
    assert!(
        after_a > 0,
        "program A should have interned at least one global shape"
    );

    // Run a SECOND program with a DISTINCT shape `{x,y}` and NO reset between.
    // Because nothing drains between runs, the registry now holds BOTH shapes —
    // this is the cross-run accumulation (the real leak the engine has today).
    let out = render_source("const o = { x: 1, y: 2 }; console.log(o.x + o.y);")
        .expect("object-literal program B should run");
    assert_eq!(out, "3\n");
    let after_b = global_shape_count();
    assert!(
        after_b > after_a,
        "a distinct second program must ADD shapes (cross-run accumulation): \
         after_a={after_a}, after_b={after_b}"
    );

    // The explicit drain returns every global table to empty.
    reset_codegen_state();
    assert_eq!(
        global_shape_count(),
        0,
        "reset_codegen_state() must drain the global shape registry"
    );
    assert_eq!(
        table_lens(),
        (0, 0),
        "reset_codegen_state() must drain the funcops side-tables"
    );
}
