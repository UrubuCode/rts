//! P5.5: DYNAMIC property access — `obj.key` / `obj[k]` where the object's exact
//! SHAPE is known only at RUNTIME (a reassigned object local, a computed key).
//!
//! The receiver must be PROVEN to be a keyed object (a reassigned object local, or
//! a known-shape object literal for a computed key); the key index is resolved at
//! runtime via the `__rtsadp_obj_get`/`_set` trampolines (slot-0 shape-id + the
//! global shape registry). A fully-Unknown receiver (a bare param/return) stays a
//! bail — the engine refuses to guess whether it is an object or a primitive.

use super::{assert_bails, assert_stdout};

// ===========================================================================
// Reassigned object local — unknown shape, dynamic read.
// ===========================================================================

#[test]
fn reassigned_same_shape_read() {
    // `o` is reassigned to a DIFFERENT object literal; `o.a` resolves the slot at
    // runtime through the slot-0 shape-id.
    assert_stdout("let o = {a: 1}; o = {a: 5}; console.log(o.a);", "5\n");
}

#[test]
fn reassigned_different_shape_read() {
    // The reassigned literal has a different key set; the runtime lookup finds `b`.
    assert_stdout(
        "let o = {a: 1}; o = {b: 7, c: 8}; console.log(o.b + o.c);",
        "15\n",
    );
}

#[test]
fn reassigned_string_value_read() {
    assert_stdout(
        r#"let o = {name: "old"}; o = {name: "rts"}; console.log(o.name);"#,
        "rts\n",
    );
}

#[test]
fn reassigned_missing_key_is_undefined() {
    // A key absent from the (reassigned) shape reads `undefined` at runtime.
    assert_stdout(
        "let o = {a: 1}; o = {a: 5}; console.log(o.z);",
        "undefined\n",
    );
}

#[test]
fn reassigned_dynamic_write_then_read() {
    // Write to an EXISTING key of a reassigned (unknown-shape) object, then read.
    assert_stdout(
        "let o = {c: 0}; o = {c: 1}; o.c = 42; console.log(o.c);",
        "42\n",
    );
}

#[test]
fn reassigned_object_console_log() {
    // console.log of a reassigned (unknown-shape) object still renders `{ k: v }`.
    assert_stdout(
        "let o = {a: 1}; o = {a: 2, b: 3}; console.log(o);",
        "{ a: 2, b: 3 }\n",
    );
}

// ===========================================================================
// Computed key `obj[k]` on a known-shape object.
// ===========================================================================

#[test]
fn computed_key_read() {
    assert_stdout(
        r#"let o = {foo: 7}; let k = "foo"; console.log(o[k]);"#,
        "7\n",
    );
}

#[test]
fn computed_key_read_string_value() {
    assert_stdout(
        r#"let o = {greet: "hi"}; let k = "greet"; console.log(o[k]);"#,
        "hi\n",
    );
}

#[test]
fn computed_key_write_then_read() {
    assert_stdout(
        r#"let o = {n: 0}; let k = "n"; o[k] = 9; console.log(o.n);"#,
        "9\n",
    );
}

#[test]
fn computed_key_literal_string() {
    // A string-LITERAL computed key (`o["x"]`) also routes dynamically.
    assert_stdout(r#"let o = {x: 11, y: 22}; console.log(o["y"]);"#, "22\n");
}

#[test]
fn computed_key_missing_is_undefined() {
    assert_stdout(
        r#"let o = {a: 1}; let k = "zzz"; console.log(o[k]);"#,
        "undefined\n",
    );
}

#[test]
fn computed_key_two_props() {
    assert_stdout(
        r#"let o = {a: 3, b: 4}; let k1 = "a"; let k2 = "b"; console.log(o[k1] + o[k2]);"#,
        "7\n",
    );
}

// ===========================================================================
// Negative — soundness: an unknown/primitive receiver bails (never guesses).
// ===========================================================================

#[test]
fn object_param_member_now_dynamic() {
    // INTENTIONAL change (was `object_param_member_bails`): a `.prop` read on a bare
    // param of unproven shape now falls back to the runtime `__rtsadp_obj_get`
    // trampoline instead of bailing. For an object arg it reads the slot; for a
    // primitive arg the trampoline reads `undefined` (JS: `("x").name` is `undefined`
    // too), so the fallback is JS-correct for any receiver.
    assert_stdout(
        "function getName(o){ return o.name; } console.log(getName({name: 1}));",
        "1\n",
    );
}

#[test]
fn numeric_index_into_object_now_dynamic() {
    // INTENTIONAL change (was `numeric_index_into_object_bails`): a NUMERIC index into
    // an object now coerces the key ToString (`o[0]` keys on "0", JS-correct) and reads
    // it dynamically; "0" is absent from `{a:1}`, so the result is `undefined`.
    assert_stdout(
        "let o = {a: 1}; let i = 0; console.log(o[i]);",
        "undefined\n",
    );
}

#[test]
fn add_new_key_to_known_shape_bails() {
    // Adding a brand-new key to a KNOWN-shape object is the transition tree (a
    // later increment) — still a compile-time bail (the static write path).
    assert_bails("let o = {a: 1}; o.b = 2; console.log(o.b);");
}
