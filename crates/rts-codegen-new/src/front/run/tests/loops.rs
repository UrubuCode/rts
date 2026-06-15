//! P5.10: LOOPS — C-style `for`, `for-of` (array + string), `for-in` (object),
//! plus `break`/`continue` (including nested loops). Each runs a REAL `.ts`
//! program end to end and asserts EXACT captured stdout; the bails assert the
//! out-of-subset forms (non-iterable for-of, labeled break) refuse explicitly.

use super::{assert_bails, assert_stdout};

// ---------------------------------------------------------------------------
// C-style `for (init; test; update)`.
// ---------------------------------------------------------------------------

#[test]
fn c_for_sum() {
    assert_stdout(
        "let s = 0; for (let i = 0; i < 5; i++) { s = s + i; } console.log(s);",
        "10\n",
    );
}

#[test]
fn c_for_continue_runs_update() {
    // `continue` must still run the `i++` update (else infinite loop / wrong sum).
    assert_stdout(
        "let s = 0; for (let i = 0; i < 5; i++) { if (i === 2) continue; s = s + i; } console.log(s);",
        "8\n",
    );
}

#[test]
fn c_for_break() {
    assert_stdout(
        "let s = 0; for (let i = 0; i < 10; i++) { if (i === 3) break; s = s + i; } console.log(s);",
        "3\n",
    );
}

#[test]
fn c_for_decrement() {
    assert_stdout(
        "let out = \"\"; for (let i = 3; i > 0; i--) { out = out + i; } console.log(out);",
        "321\n",
    );
}

#[test]
fn c_for_step() {
    assert_stdout(
        "let s = 0; for (let i = 0; i < 10; i += 2) { s = s + i; } console.log(s);",
        "20\n",
    );
}

#[test]
fn nested_c_for() {
    assert_stdout(
        "let n = 0; for (let i = 0; i < 3; i++) { for (let j = 0; j < 3; j++) { n++; } } console.log(n);",
        "9\n",
    );
}

#[test]
fn nested_for_inner_break() {
    // The inner `break` only escapes the inner loop.
    assert_stdout(
        "let n = 0; for (let i = 0; i < 3; i++) { for (let j = 0; j < 5; j++) { if (j === 2) break; n++; } } console.log(n);",
        "6\n",
    );
}

// ---------------------------------------------------------------------------
// `for-of` over an array.
// ---------------------------------------------------------------------------

#[test]
fn for_of_array_sum() {
    assert_stdout(
        "let s = 0; for (const x of [1, 2, 3, 4]) { s = s + x; } console.log(s);",
        "10\n",
    );
}

#[test]
fn tagged_element_into_int_local() {
    // Regression: accumulating a boxed array element into an `int` local —
    // `s = s + arr[i]` — must decode the Tagged number soundly (a boxed double or
    // a tagged int32), not read 0 from a tag-blind unbox (the pre-P5.10 bug).
    assert_stdout(
        "let a = [5]; let s = 0; s = s + a[0]; console.log(s);",
        "5\n",
    );
}

#[test]
fn for_of_array_local() {
    assert_stdout(
        "let a = [10, 20, 30]; let out = \"\"; for (const x of a) { out = out + x + \",\"; } console.log(out);",
        "10,20,30,\n",
    );
}

#[test]
fn for_of_array_method() {
    assert_stdout(
        "let total = 0; for (const x of [1, 2, 3]) { total += x * x; } console.log(total);",
        "14\n",
    );
}

#[test]
fn for_of_array_break_continue() {
    assert_stdout(
        "let s = 0; for (const x of [1, 2, 3, 4, 5]) { if (x === 2) continue; if (x === 5) break; s = s + x; } console.log(s);",
        "8\n",
    );
}

// ---------------------------------------------------------------------------
// `for-of` over a string (code points).
// ---------------------------------------------------------------------------

#[test]
fn for_of_string_reverse() {
    assert_stdout(
        "let r = \"\"; for (const c of \"abc\") { r = c + r; } console.log(r);",
        "cba\n",
    );
}

#[test]
fn for_of_string_count() {
    assert_stdout(
        "let n = 0; for (const c of \"hello\") { n++; } console.log(n);",
        "5\n",
    );
}

// ---------------------------------------------------------------------------
// `for-in` over an object (keys).
// ---------------------------------------------------------------------------

#[test]
fn for_in_object_keys() {
    assert_stdout(
        "let o = {a: 1, b: 2, c: 3}; let ks = \"\"; for (const k in o) { ks = ks + k; } console.log(ks);",
        "abc\n",
    );
}

#[test]
fn for_in_object_lookup() {
    // Use each key to read the object dynamically (o[k]) and sum the values.
    assert_stdout(
        "let o = {x: 10, y: 20}; let s = 0; for (const k in o) { s = s + o[k]; } console.log(s);",
        "30\n",
    );
}

// ---------------------------------------------------------------------------
// Bails — out-of-subset loop forms refuse explicitly (never wrong-but-closer).
// ---------------------------------------------------------------------------

#[test]
fn for_of_non_iterable_bails() {
    // for-of over a number is a TypeError in JS; the engine has no throw → bail.
    assert_bails("for (const x of 42) { console.log(x); }");
}

#[test]
fn labeled_break_bails() {
    assert_bails(
        "outer: for (let i = 0; i < 3; i++) { for (let j = 0; j < 3; j++) { if (j === 1) break outer; } } console.log(\"done\");",
    );
}
