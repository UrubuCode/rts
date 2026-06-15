//! The PRIMORDIAL `Error` family as a `.ts` prelude class
//! (`rts-primitives/src/error.ts`, included by the engine ahead of the user
//! program). `new Error("x")` constructs a shape-based object via the normal
//! user-class path; `.message`/`.name`/`.stack` are ordinary slots, `toString()`
//! is the `.ts` method, `instanceof` rides the user-class inheritance chain, and a
//! thrown-then-caught Error interoperates with throw/catch (the thrown
//! `TAG_OBJECT` word's shape slots are read via the dynamic `__rtsadp_obj_get`
//! fallback). Each test runs a REAL `.ts` program end to end and asserts exact
//! captured stdout — the honesty floor (no hardcoding, real output).

use super::assert_stdout;

// ---------------------------------------------------------------------------
// Construction + fields.
// ---------------------------------------------------------------------------

#[test]
fn new_error_message() {
    assert_stdout(
        r#"console.log(new Error("boom").message);"#,
        "boom\n",
    );
}

#[test]
fn new_error_default_name() {
    assert_stdout(r#"console.log(new Error("x").name);"#, "Error\n");
}

#[test]
fn error_no_message_is_empty() {
    // `new Error()` (no arg) → `this.message = message ?? ""` → "".
    assert_stdout(r#"let e = new Error(); console.log(e.message === "");"#, "true\n");
}

#[test]
fn type_error_name_inline() {
    assert_stdout(
        r#"console.log(new TypeError("x").name === "TypeError");"#,
        "true\n",
    );
}

#[test]
fn each_subtype_name() {
    assert_stdout(
        r#"console.log(
             new RangeError("a").name,
             new ReferenceError("b").name,
             new SyntaxError("c").name,
             new URIError("d").name,
             new EvalError("e").name
           );"#,
        "RangeError ReferenceError SyntaxError URIError EvalError\n",
    );
}

// ---------------------------------------------------------------------------
// toString / coercion.
// ---------------------------------------------------------------------------

#[test]
fn error_to_string_method() {
    assert_stdout(
        r#"console.log(new Error("boom").toString());"#,
        "Error: boom\n",
    );
}

#[test]
fn error_to_string_empty_message_is_just_name() {
    // JS: `new Error().toString()` → "Error" (no trailing ": ").
    assert_stdout(r#"console.log(new Error().toString());"#, "Error\n");
}

#[test]
fn error_string_coercion() {
    // `${e}` / `String(e)` use the `.ts` `toString()` via the normal user-class
    // ToPrimitive path (no `__rtsadp_err_to_string` trampoline).
    assert_stdout(
        r#"let e = new TypeError("nope"); console.log(`${e}`, String(e));"#,
        "TypeError: nope TypeError: nope\n",
    );
}

// ---------------------------------------------------------------------------
// instanceof.
// ---------------------------------------------------------------------------

#[test]
fn error_instanceof_error() {
    assert_stdout(
        r#"console.log(new Error("x") instanceof Error);"#,
        "true\n",
    );
}

#[test]
fn subtype_instanceof_error() {
    // A subtype IS-A Error (the subclass chains to the `.ts` Error base).
    assert_stdout(
        r#"let e = new RangeError("r"); console.log(e instanceof RangeError, e instanceof Error);"#,
        "true true\n",
    );
}

#[test]
fn user_class_extends_error_custom_name() {
    assert_stdout(
        r#"class MyErr extends Error { constructor(m: string){ super(m); this.name = "MyErr"; } }
           let e = new MyErr("oops");
           console.log(e.name, e.message, e instanceof MyErr, e instanceof Error);"#,
        "MyErr oops true true\n",
    );
}

// ---------------------------------------------------------------------------
// throw / catch interop (the critical correctness point): a thrown `.ts` Error
// object is caught and its shape slots read via the dynamic property fallback.
// ---------------------------------------------------------------------------

#[test]
fn throw_catch_message_and_name() {
    assert_stdout(
        r#"try { throw new RangeError("r"); }
           catch (e) { console.log(e.name, e.message); }"#,
        "RangeError r\n",
    );
}

#[test]
fn throw_catch_instanceof() {
    assert_stdout(
        r#"try { throw new TypeError("bad"); }
           catch (e) { console.log(e instanceof Error); }"#,
        "true\n",
    );
}

// NOTE (known limitation, not a regression of a tested path): calling a METHOD on
// a CAUGHT error — `catch (e) { e.toString() }` — needs SHAPE-KEYED dynamic method
// dispatch (the receiver `e` is opaque/Tagged, so the engine cannot statically
// resolve the `.ts` Error class to call its `toString`). That is the design's
// inline-cache dispatch increment (shape-id → class method), not implemented here.
// Today the dynamic `toString` returns the generic object default. The DATA surface
// on a caught error — `e.message`/`e.name`/`e instanceof Error` — works (proven by
// the tests above), which covers the common `catch` patterns. `e.toString()` is
// correct when `e` has a STATIC class (`new Error("x").toString()` →
// `globalclass::error_to_string`).

#[test]
fn throw_user_extends_error_caught() {
    assert_stdout(
        r#"class HttpError extends Error { constructor(m: string){ super(m); this.name = "HttpError"; } }
           try { throw new HttpError("nope"); }
           catch (e) { console.log(e.name, e.message); }"#,
        "HttpError nope\n",
    );
}
