//! P3: object/array literals + property/index access (scalar pulls) + the
//! statically-proven-shape negative bails.

use super::{assert_bails, assert_stdout};

#[test]
fn object_property_read() {
    assert_stdout("let o = {a: 1, b: 2}; console.log(o.a + o.b);", "3\n");
}

#[test]
fn object_property_write() {
    assert_stdout("let o = {x: 10}; o.x = o.x + 5; console.log(o.x);", "15\n");
}

#[test]
fn object_string_property() {
    assert_stdout(r#"let p = {name: "rts"}; console.log(p.name);"#, "rts\n");
}

#[test]
fn object_nested_scalar_through_function() {
    assert_stdout(
        "let c = {n: 7}; function dbl(v: number){return v*2;} console.log(dbl(c.n));",
        "14\n",
    );
}

#[test]
fn object_missing_key_is_undefined() {
    assert_stdout("let o = {a: 1}; console.log(o.b);", "undefined\n");
}

#[test]
fn array_index_read() {
    assert_stdout("let a = [10, 20, 30]; console.log(a[0] + a[2]);", "40\n");
}

#[test]
fn array_length() {
    assert_stdout("let a = [10, 20, 30]; console.log(a.length);", "3\n");
}

#[test]
fn array_index_write() {
    assert_stdout("let a = [1, 2, 3]; a[1] = 9; console.log(a[1]);", "9\n");
}

#[test]
fn heterogeneous_array_scalar() {
    assert_stdout(r#"let m = [1, "two", 3]; console.log(m[1]);"#, "two\n");
}

#[test]
fn typeof_object_and_array() {
    assert_stdout("console.log(typeof {}, typeof []);", "object object\n");
}

#[test]
fn const_array_and_object_shapes() {
    // `const` initializers record shapes the same as `let` (the fixture corpus
    // overwhelmingly uses `const a = [...]`).
    assert_stdout("const a = [4, 5]; console.log(a[1], a.length);", "5 2\n");
    assert_stdout("const o = {a: 1}; console.log(o.a);", "1\n");
}

#[test]
fn array_index_in_loop() {
    // Read array slots across loop iterations (exercises VEC_GET with a varying
    // index and the Tagged element path).
    assert_stdout(
        r#"
        let a = [5, 6, 7];
        let i = 0;
        while (i < a.length) {
            console.log(a[i]);
            i = i + 1;
        }
        "#,
        "5\n6\n7\n",
    );
}

#[test]
fn object_two_fields_string_and_number() {
    assert_stdout(
        r#"let o = {name: "x", count: 3}; console.log(o.name, o.count);"#,
        "x 3\n",
    );
}

// ===========================================================================
// P3 negative: shape must be statically proven; dynamic access bails.
// ===========================================================================

#[test]
fn member_on_unknown_shape_param_bails() {
    // `o` is a param of unknown shape — a property access on it needs the dynamic
    // inline cache (later increment). Must bail, not guess a slot.
    assert_bails("function f(o: any){ return o.a; } let r = {a: 1}; console.log(f(r));");
}

#[test]
fn computed_object_key_now_dynamic() {
    // P5.5 REGRESSION (intentional): `o[k]` with a dynamic string key on a
    // known-shape object NO LONGER bails — it routes through the dynamic
    // `__rtsadp_obj_get` (runtime slot-0 shape-id lookup). See `tests::objdyn`.
    assert_stdout(r#"let o = {a: 1}; let k = "a"; console.log(o[k]);"#, "1\n");
}

#[test]
fn adding_new_object_key_bails() {
    // Adding a key not in the literal's shape needs the transition tree — bail.
    assert_bails("let o = {a: 1}; o.b = 2; console.log(o.b);");
}
