//! P5.7: CLOSURES — arrows/functions that CAPTURE outer locals BY VALUE.
//!
//! A capturing arrow used as a value is lifted to a CLOSURE: its captured free
//! vars are snapshotted (by value) into an env array at closure-creation, and the
//! closure thunk reads them back from the env. This unlocks capturing callbacks
//! (`arr.map(x => x * factor)`) and returned closures (`adder(5)` → `x => x + 5`).
//!
//! The soundness boundary (BAIL, never wrong): a closure that ASSIGNS a captured
//! var (mutable capture), a captured var reassigned in the outer scope (a stale
//! snapshot), or a capture of `this` / an unknown name. Capture-by-value is only
//! accepted where the captured value does not change between capture and call.

use super::{assert_bails, assert_stdout};

// ===========================================================================
// Capturing array callbacks (the headline functional gap P4.6/P4.7 bailed on).
// ===========================================================================

#[test]
fn capturing_map() {
    assert_stdout(
        "let f = 10; let a = [1,2,3]; console.log(a.map((x:number) => x * f).join(\",\"));",
        "10,20,30\n",
    );
}

#[test]
fn capturing_filter() {
    assert_stdout(
        "let lo = 2; let a = [1,2,3,4]; console.log(a.filter((x:number) => x > lo).join(\",\"));",
        "3,4\n",
    );
}

#[test]
fn capturing_reduce() {
    assert_stdout(
        "let base = 100; console.log([1,2,3].reduce((acc:number, x:number) => acc + x + base, 0));",
        "306\n",
    );
}

#[test]
fn capturing_map_with_index_arg() {
    // A capture (`off`) AND the callback's own index arg both reach the body: the
    // capture rides the env, the index rides a1. `100 + 1 + 0`, `100 + 2 + 1`.
    assert_stdout(
        "let off = 100; let r = [1,2].map((x:number, i:number) => x + i + off); console.log(r.join(\",\"));",
        "101,103\n",
    );
}

// ===========================================================================
// Returned closures + closures in a variable.
// ===========================================================================

#[test]
fn closure_returned_then_called() {
    // `n` is captured by value at the point `adder(5)` builds the closure.
    assert_stdout(
        "function adder(n: number) { return (x: number) => x + n; } let add5 = adder(5); console.log(add5(10));",
        "15\n",
    );
}

#[test]
fn closure_in_a_variable() {
    assert_stdout(
        "let k = 3; let g = (x: number) => x * k; console.log(g(4));",
        "12\n",
    );
}

#[test]
fn multiple_captures() {
    assert_stdout(
        "let a = 1; let b = 2; let h = (x: number) => x + a + b; console.log(h(10));",
        "13\n",
    );
}

#[test]
fn capture_a_string() {
    // The captured value is a string (a Tagged PolyValue) — the env snapshot boxes
    // it verbatim and the body's `+` concatenates.
    assert_stdout(
        "let prefix = \"hi-\"; let g = (x: number) => prefix + x; console.log(g(5));",
        "hi-5\n",
    );
}

// ===========================================================================
// Two closures over the SAME captured var keep independent by-value snapshots.
// ===========================================================================

#[test]
fn two_closures_same_capture() {
    assert_stdout(
        "let n = 7; let f = (x: number) => x + n; let g = (x: number) => x * n; console.log(f(1), g(2));",
        "8 14\n",
    );
}

// ===========================================================================
// Soundness boundary — BAIL (explicit Unsupported, never a wrong value).
// ===========================================================================

#[test]
fn closure_assigns_captured_top_level_var() {
    // The closure WRITES a captured TOP-LEVEL `let` — now supported (epic #195):
    // `c` is promoted to a module-global CELL, so the write is visible to the outer
    // scope (no by-value snapshot). Previously a documented bail.
    assert_stdout(
        "let c = 0; let inc = () => { c = c + 1; }; inc(); console.log(c);",
        "1\n",
    );
}

#[test]
fn captured_var_reassigned_in_outer_scope_bails() {
    // `factor` is reassigned AFTER the closure is built → the by-value snapshot
    // would be observably stale. Conservatively BAIL on any outer reassignment.
    assert_bails(
        "let factor = 2; let g = (x: number) => x * factor; factor = 3; console.log(g(10));",
    );
}

#[test]
fn capture_of_this_bails() {
    // `this` is not a simple capturable local. BAIL (no env entry for it).
    assert_bails("let g = (x: number) => x + this.k; console.log(g(1));");
}

#[test]
fn top_level_runtime_const_read_in_function() {
    // A top-level `const` initialized to a RUNTIME value (a CALL — not a
    // re-materializable literal) read from inside a plain function resolves to the
    // SAME value via a shared cell — was "unbound identifier `v`". A literal const
    // stays on the by-value path (`capture_a_string`).
    assert_stdout(
        "function mk(): number { return 7; } const v = mk(); \
         function rd(): number { return v + 1; } console.log(rd());",
        "8\n",
    );
}
