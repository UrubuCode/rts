//! P5.4: `Object.*` static methods over a statically-proven object shape.

use super::{assert_bails, assert_stdout};

#[test]
fn object_keys() {
    assert_stdout(
        "let o = {a: 1, b: 2, c: 3}; console.log(Object.keys(o).join(\",\"));",
        "a,b,c\n",
    );
}

#[test]
fn object_values() {
    assert_stdout(
        "let o = {a: 1, b: 2}; console.log(Object.values(o).join(\",\"));",
        "1,2\n",
    );
}

#[test]
fn object_entries() {
    // entries → an array of [key, value] sub-arrays. `console.log` now renders each
    // arg through `engine.display` (the Node-style inspect), so a whole sub-array
    // `e[0]` prints `[ 'x', 1 ]` (bracketed) — matching bun/node, where the old
    // hardcoded path produced the ToString form `x,1`. Justified format change.
    assert_stdout(
        "let o = {x: 1}; let e = Object.entries(o); console.log(e.length, e[0]);",
        "1 [ 'x', 1 ]\n",
    );
}

#[test]
fn object_keys_empty() {
    assert_stdout("console.log(Object.keys({}).length);", "0\n");
}

#[test]
fn object_keys_length() {
    assert_stdout(
        "let o = {a: 1, b: 2, c: 3}; console.log(Object.keys(o).length);",
        "3\n",
    );
}

#[test]
fn object_values_strings() {
    assert_stdout(
        r#"let o = {name: "rts", lang: "ts"}; console.log(Object.values(o).join("/"));"#,
        "rts/ts\n",
    );
}

#[test]
fn object_get_own_property_names_alias() {
    assert_stdout(
        "let o = {p: 1, q: 2}; console.log(Object.getOwnPropertyNames(o).join(\",\"));",
        "p,q\n",
    );
}

#[test]
fn object_freeze_is_noop_keeps_object_readable() {
    // freeze is a semantic no-op (does not throw on later mutation in this
    // increment): calling it leaves the object fully readable afterward — the
    // common `Object.freeze(o)` idiom (freeze-then-read).
    assert_stdout("let o = {a: 5}; Object.freeze(o); console.log(o.a);", "5\n");
}

#[test]
fn object_assign_two_arg() {
    // assign(target, src) copies src's slots into target (keys already in target).
    assert_stdout(
        "let t = {a: 1, b: 2}; let s = {b: 9}; Object.assign(t, s); console.log(t.b);",
        "9\n",
    );
}

#[test]
fn object_entries_two_pairs() {
    assert_stdout(
        "let o = {a: 1, b: 2}; let e = Object.entries(o); console.log(e.length, e[1]);",
        "2 [ 'b', 2 ]\n",
    );
}

// ===========================================================================
// Negative: dynamic / unknown-shape receiver bails.
// ===========================================================================

#[test]
fn object_keys_on_unknown_shape_param_runs_dynamically() {
    // `o` is a param of unknown static shape — Object.keys now recovers the keys at
    // RUNTIME from the object's slot-0 shape-id (Object is primordial → engine-direct
    // dynamic enumeration), so this runs instead of bailing.
    assert_stdout(
        "function f(o: any){ return Object.keys(o).length; } let r = {a: 1, b: 2}; console.log(f(r));",
        "2\n",
    );
}

#[test]
fn object_assign_adding_key_bails() {
    // assign that would ADD a key not in the target's shape bails (no transition).
    assert_bails("let t = {a: 1}; let s = {b: 2}; Object.assign(t, s); console.log(t.b);");
}

#[test]
fn object_from_entries_array_source() {
    // fromEntries now builds an object from an array of [k, v] pairs (was a bail).
    assert_stdout(
        "let e = [[\"a\", 1], [\"b\", 2]]; let o = Object.fromEntries(e); console.log(o.a, o.b);",
        "1 2\n",
    );
}

#[test]
fn object_from_entries_non_array_bails() {
    // A non-array source (a Map/iterator) still needs iteration — explicit bail
    // (honesty floor: no empty/wrong object). `m` is an `any` param of unknown shape.
    assert_bails(
        "function f(m: any): any { return Object.fromEntries(m); } console.log(f([[\"a\", 1]]));",
    );
}
