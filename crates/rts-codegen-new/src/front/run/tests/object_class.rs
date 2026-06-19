//! The PRIMORDIAL `Object` instance-method library + factory as a `.ts` prelude
//! class (`rts-primitives/src/object.ts`). Object is NOT a primitive with an
//! autobox — every `{}` is already a shape-based object, so an object-receiver
//! method call (`o.hasOwnProperty(k)`, `o.toString()`) routes into the ambient
//! `class Object` with the object as `this` (the OBJECT-receiver branch in
//! `method::try_method_dispatch`); membership rides the shape-aware
//! `engine.obj_has` bridge. The STATIC surface (`Object.keys`/…) stays
//! codegen-native (`objstatic.rs`) and is transparent to this instance-only class.
//!
//! Each test runs a REAL `.ts` program end to end and asserts EXACT captured
//! stdout — the honesty floor (no hardcoding, real output).

use super::assert_stdout;

// ---------------------------------------------------------------------------
// Instance methods — routed to the `.ts` class with the object as `this`.
// ---------------------------------------------------------------------------

#[test]
fn has_own_property_true() {
    assert_stdout(
        r#"const o = { a: 1, b: 2 }; console.log(o.hasOwnProperty("a"));"#,
        "true\n",
    );
}

#[test]
fn has_own_property_false() {
    assert_stdout(
        r#"const o = { a: 1 }; console.log(o.hasOwnProperty("z"));"#,
        "false\n",
    );
}

#[test]
fn property_is_enumerable() {
    assert_stdout(
        r#"const o = { a: 1, b: 2 }; console.log(o.propertyIsEnumerable("b"));"#,
        "true\n",
    );
}

#[test]
fn to_string_default_tag() {
    assert_stdout(
        r#"const o = { a: 1 }; console.log(o.toString());"#,
        "[object Object]\n",
    );
}

#[test]
fn object_literal_receiver() {
    // A bare object-LITERAL receiver also routes (no local binding).
    assert_stdout(r#"console.log(({ x: 1 }).hasOwnProperty("x"));"#, "true\n");
}

// ---------------------------------------------------------------------------
// STATIC surface must KEEP WORKING — the ambient `class Object` is transparent
// to `Object.keys`/`values`/`entries` (the `name != "Object"` carve-out in
// objstatic.rs). Regression guard.
// ---------------------------------------------------------------------------

#[test]
fn static_keys_still_works() {
    assert_stdout(
        r#"const o = { a: 1, b: 2 }; console.log(Object.keys(o).join(","));"#,
        "a,b\n",
    );
}

#[test]
fn static_values_still_works() {
    assert_stdout(
        r#"const o = { a: 1, b: 2 }; console.log(Object.values(o).join(","));"#,
        "1,2\n",
    );
}

// ---------------------------------------------------------------------------
// `Object(x)` FACTORY + `new Object()` — both yield an object (typeof "object").
// ---------------------------------------------------------------------------

#[test]
fn factory_null_makes_object() {
    assert_stdout("console.log(typeof Object(null));", "object\n");
}

#[test]
fn factory_object_passthrough() {
    assert_stdout("console.log(typeof Object({ x: 1 }));", "object\n");
}

#[test]
fn new_object_is_object() {
    assert_stdout("console.log(typeof new Object());", "object\n");
}
