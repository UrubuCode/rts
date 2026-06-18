//! The PRIMORDIAL `Boolean.prototype` methods as a `.ts` prelude class
//! (`rts-primitives/src/boolean.ts`). A method called on a PRIMITIVE bool
//! receiver (`true.toString()`, `flag.valueOf()`) is routed into the ambient
//! `class Boolean` with the primitive BOXED as `this` (shape-based dispatch — the
//! engine resolves `(method, arity)` on the class at compile time, NOT JS
//! prototypes). This is the PROVER of the primitive → prelude-`.ts`-class
//! method-dispatch mechanism that String/Number will reuse.
//!
//! Each test runs a REAL `.ts` program end to end and asserts EXACT captured
//! stdout — the honesty floor (no hardcoding, real output).

use super::assert_stdout;

// ---------------------------------------------------------------------------
// toString — the core: a method on a primitive bool routes to the `.ts` class,
// reading `this` AS THE PRIMITIVE (`this ? "true" : "false"`).
// ---------------------------------------------------------------------------

#[test]
fn true_to_string() {
    assert_stdout(r#"console.log(true.toString());"#, "true\n");
}

#[test]
fn false_to_string() {
    assert_stdout(r#"console.log(false.toString());"#, "false\n");
}

#[test]
fn bool_var_to_string() {
    // A bool bound to a local still routes (proven Bool repr).
    assert_stdout(
        r#"let flag = true; console.log(flag.toString());"#,
        "true\n",
    );
}

#[test]
fn computed_bool_to_string() {
    // The receiver is a COMPUTED bool (a comparison), proven Bool.
    assert_stdout(r#"console.log((1 < 2).toString());"#, "true\n");
}

// ---------------------------------------------------------------------------
// valueOf — returns the primitive boolean itself.
// ---------------------------------------------------------------------------

#[test]
fn true_value_of() {
    assert_stdout(r#"console.log(true.valueOf());"#, "true\n");
}

#[test]
fn false_value_of() {
    assert_stdout(r#"console.log(false.valueOf());"#, "false\n");
}

#[test]
fn value_of_in_condition() {
    // The `valueOf()` result is a real boolean usable in control flow.
    assert_stdout(
        r#"if (true.valueOf()) { console.log("yes"); } else { console.log("no"); }"#,
        "yes\n",
    );
}

// ---------------------------------------------------------------------------
// Coercion paths that must KEEP WORKING (regression guard): these do NOT go
// through the prelude class — `String(b)` / `${b}` / a bool in a string `+` use
// the runtime ToString/`+` trampolines (unchanged by this migration).
// ---------------------------------------------------------------------------

#[test]
fn string_of_bool() {
    assert_stdout(r#"console.log(String(true), String(false));"#, "true false\n");
}

#[test]
fn bool_in_template() {
    assert_stdout(r#"console.log(`${false} ${true}`);"#, "false true\n");
}

#[test]
fn bool_in_string_concat() {
    assert_stdout(r#"console.log("v=" + true);"#, "v=true\n");
}

// ---------------------------------------------------------------------------
// `Boolean(x)` FACTORY call (not `new`) — the `.ts` `BooleanFactory` returns a
// PRIMITIVE boolean (typeof === "boolean") via the engine's native ToBoolean.
// ---------------------------------------------------------------------------

#[test]
fn boolean_coercion_call() {
    assert_stdout(
        r#"console.log(Boolean(0), Boolean(1), Boolean(""), Boolean("x"));"#,
        "false true false true\n",
    );
}

#[test]
fn factory_returns_primitive() {
    assert_stdout("console.log(typeof Boolean(1));", "boolean\n");
}

// ---------------------------------------------------------------------------
// `new Boolean(x)` WRAPPER — now the `.ts` `class Boolean` constructed via the
// normal user-class path (a shape-based object, typeof === "object"). Its methods
// route to the SAME `.ts` bodies as the primitive autobox path (dual `this`),
// reading the primitive from the `__prim` slot.
// ---------------------------------------------------------------------------

#[test]
fn new_boolean_is_object() {
    assert_stdout(r#"console.log(typeof new Boolean(true));"#, "object\n");
}

#[test]
fn new_boolean_instanceof() {
    assert_stdout("console.log(new Boolean(0) instanceof Boolean);", "true\n");
}

#[test]
fn wrapper_value_of() {
    // ToBoolean(0) === false, unwrapped from `this.__prim` (dual-`this`).
    assert_stdout("console.log(new Boolean(0).valueOf());", "false\n");
}

#[test]
fn wrapper_to_string() {
    assert_stdout("console.log(new Boolean(1).toString());", "true\n");
}
