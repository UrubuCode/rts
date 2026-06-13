//! P3.5: console.log PRETTY-PRINT of whole ARRAYS — Bun/Node `util.inspect`
//! format. Object inspect is BAILED (see `value/inspect.rs`); the negative tests
//! pin that boundary so it can never silently emit a near-miss.

use super::{assert_bails, assert_stdout};

// ===========================================================================
// Arrays — the Bun/Node inspect format.
// ===========================================================================

#[test]
fn array_of_numbers() {
    assert_stdout("console.log([1, 2, 3]);", "[ 1, 2, 3 ]\n");
}

#[test]
fn empty_array() {
    assert_stdout("console.log([]);", "[]\n");
}

#[test]
fn array_of_strings_quoted() {
    assert_stdout(r#"console.log(["a", "b"]);"#, "[ 'a', 'b' ]\n");
}

#[test]
fn mixed_array() {
    assert_stdout(r#"console.log([1, "two", true]);"#, "[ 1, 'two', true ]\n");
}

#[test]
fn nested_arrays() {
    assert_stdout("console.log([[1, 2], [3, 4]]);", "[ [ 1, 2 ], [ 3, 4 ] ]\n");
}

#[test]
fn top_level_string_stays_bare_with_array() {
    // The top-level string arg prints BARE; the array arg uses the inspect form.
    assert_stdout(r#"console.log("plain", [1, 2]);"#, "plain [ 1, 2 ]\n");
}

#[test]
fn array_local_inspected() {
    // An identifier bound to a proven-ARRAY local renders the same as a literal.
    assert_stdout("let a = [10, 20]; console.log(a);", "[ 10, 20 ]\n");
}

#[test]
fn array_with_null() {
    // `undefined` is not a bound identifier in the current subset, so only `null`
    // (a real literal) is exercised here; the trampoline renders both bare.
    assert_stdout("console.log([null, null]);", "[ null, null ]\n");
}

#[test]
fn array_single_element() {
    assert_stdout("console.log([42]);", "[ 42 ]\n");
}

#[test]
fn deeply_nested_arrays() {
    assert_stdout("console.log([[[1]]]);", "[ [ [ 1 ] ] ]\n");
}

#[test]
fn array_with_float() {
    assert_stdout("console.log([1.5, 2.5]);", "[ 1.5, 2.5 ]\n");
}

#[test]
fn two_array_args() {
    assert_stdout("console.log([1], [2]);", "[ 1 ] [ 2 ]\n");
}

// ===========================================================================
// Scalar pulls from arrays still print bare (the ToString path, NOT inspect).
// ===========================================================================

#[test]
fn array_element_scalar_bare() {
    // A scalar pulled OUT of an array prints bare — only the WHOLE array inspects.
    assert_stdout(r#"let a = ["x", "y"]; console.log(a[0]);"#, "x\n");
}

#[test]
fn array_length_still_works() {
    assert_stdout("let a = [1, 2, 3]; console.log(a.length);", "3\n");
}

// ===========================================================================
// Object inspect — BAILED this increment (no runtime shape-id → no key recovery).
// ===========================================================================

#[test]
fn whole_object_log_bails() {
    assert_bails("console.log({a: 1, b: 2});");
}

#[test]
fn empty_object_log_bails() {
    assert_bails("console.log({});");
}

#[test]
fn object_local_log_bails() {
    assert_bails("let o = {a: 1}; console.log(o);");
}

#[test]
fn array_with_object_element_bails() {
    // Would render the object element as a keyless array — a near-miss. BAIL.
    assert_bails("console.log([{a: 1}]);");
}

#[test]
fn object_scalar_still_works_after_inspect() {
    // Property access still resolves (no slot shift was introduced for objects).
    assert_stdout("let o = {a: 1}; console.log(o.a);", "1\n");
}
