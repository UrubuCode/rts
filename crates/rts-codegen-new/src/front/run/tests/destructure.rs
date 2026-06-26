//! P5.11 destructuring — array `[a, b, ...rest]` / object `{x, y: z, w = 5}`
//! patterns in `let`/`const`, function params, and for-of bindings. Each test runs a
//! REAL `.ts` program end to end and asserts EXACT captured stdout; the bails assert
//! an explicit `Unsupported` (never a silently wrong binding).

use super::{assert_bails, assert_stdout};

// ---- array destructuring ----

#[test]
fn array_basic() {
    assert_stdout(
        "const [a, b, c] = [1, 2, 3]; console.log(a + b + c);",
        "6\n",
    );
}

#[test]
fn array_rest() {
    assert_stdout(
        r#"const [first, ...rest] = [10, 20, 30]; console.log(first, rest.join(","));"#,
        "10 20,30\n",
    );
}

#[test]
fn array_default() {
    assert_stdout("const [x = 5, y = 9] = [1]; console.log(x, y);", "1 9\n");
}

#[test]
fn array_hole() {
    assert_stdout("const [, second] = [7, 8]; console.log(second);", "8\n");
}

// ---- object destructuring ----

#[test]
fn object_basic() {
    assert_stdout(
        "const o = {a: 1, b: 2}; const {a, b} = o; console.log(a + b);",
        "3\n",
    );
}

#[test]
fn object_rename() {
    assert_stdout("const {a: x} = {a: 42}; console.log(x);", "42\n");
}

#[test]
fn object_default() {
    assert_stdout("const {p = 100} = {q: 1}; console.log(p);", "100\n");
}

// ---- parameter destructuring ----

#[test]
fn param_array() {
    assert_stdout(
        "function add([a, b]) { return a + b; } console.log(add([3, 4]));",
        "7\n",
    );
}

#[test]
fn param_object() {
    assert_stdout(
        r#"function name({first, last}) { return first + " " + last; } console.log(name({first: "a", last: "b"}));"#,
        "a b\n",
    );
}

// ---- for-of destructuring ----

#[test]
fn for_of_pair() {
    assert_stdout(
        "let s = 0; for (const [a, b] of [[1, 2], [3, 4]]) { s = s + a * b; } console.log(s);",
        "14\n",
    );
}

#[test]
fn for_of_object() {
    assert_stdout(
        "let s = 0; for (const {x} of [{x: 5}, {x: 7}]) { s = s + x; } console.log(s);",
        "12\n",
    );
}

// ---- bails (sound refusals, never a wrong value) ----

#[test]
fn bail_object_rest() {
    // Object rest `{a, ...rest}` needs a new object of the remaining keys (a
    // shape-minus-a transition) — not modeled; the pattern stays flattened and bails.
    assert_bails("const {a, ...rest} = {a: 1, b: 2, c: 3}; console.log(a);");
}

#[test]
fn bail_assignment_target() {
    // Assignment-target destructuring `[a, b] = arr` (no let/const): the HIR is a
    // bare assignment to a non-ident target, which bails.
    assert_bails("let a = 0; let b = 0; [a, b] = [1, 2]; console.log(a + b);");
}

// ---- nested patterns (temp + recurse) ----

#[test]
fn nested_array() {
    // A nested element binds a fresh temp holding the intermediate read, then
    // re-expands off it — index/prop access on the `Tagged` temp lowers fine.
    assert_stdout("const [[a], b] = [[1], 2]; console.log(a + b);", "3\n");
}

#[test]
fn nested_object() {
    assert_stdout(
        "const {outer: {inner}} = {outer: {inner: 99}}; console.log(inner);",
        "99\n",
    );
}

#[test]
fn nested_mixed() {
    assert_stdout(
        "const [{x}, [y]] = [{x: 5}, [7]]; console.log(x + y);",
        "12\n",
    );
}

#[test]
fn nested_pattern_with_default() {
    // `{ obj: { z } = {…} }` — the inner pattern itself carries a default.
    assert_stdout(
        "const {obj: {z} = {z: 0}} = {obj: {z: 99}}; console.log(z);",
        "99\n",
    );
}
