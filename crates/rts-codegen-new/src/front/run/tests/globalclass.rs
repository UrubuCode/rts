//! `new <RuntimeClass>()` + instance methods + `extends Error` + `instanceof`
//! (P5.3) — REAL `.ts` run end to end, exact captured stdout.
//!
//! Covers the histogram's #2 cluster: collections (`Map`/`Set`), the error family
//! (`Error`/`TypeError` + `class X extends Error`), and the wrapper objects
//! (`new Number(5)` etc.). Each positive test asserts exact stdout; the bails
//! pin the honesty floor (an unmodeled construct refuses, never a wrong value).

use super::{assert_bails, assert_stdout};

// ---------------------------------------------------------------------------
// Map.
// ---------------------------------------------------------------------------

#[test]
fn map_set_get_size() {
    assert_stdout(
        r#"let m = new Map(); m.set("a", 1); m.set("b", 2); console.log(m.get("a"), m.size);"#,
        "1 2\n",
    );
}

#[test]
fn map_has() {
    assert_stdout(
        r#"let m = new Map(); m.set("a", 1); console.log(m.has("a"), m.has("z"));"#,
        "true false\n",
    );
}

#[test]
fn map_delete_then_size() {
    assert_stdout(
        r#"let m = new Map(); m.set("a", 1); m.set("b", 2); m.delete("a"); console.log(m.size, m.has("a"));"#,
        "1 false\n",
    );
}

#[test]
fn map_string_values() {
    assert_stdout(
        r#"let m = new Map(); m.set("name", "rts"); console.log(m.get("name"));"#,
        "rts\n",
    );
}

#[test]
fn map_get_missing_is_undefined() {
    assert_stdout(
        r#"let m = new Map(); console.log(m.get("nope"));"#,
        "undefined\n",
    );
}

// ---------------------------------------------------------------------------
// Set.
// ---------------------------------------------------------------------------

#[test]
fn set_add_dedup_size_has() {
    assert_stdout(
        r#"let s = new Set(); s.add(1); s.add(1); s.add(2); console.log(s.size, s.has(1));"#,
        "2 true\n",
    );
}

#[test]
fn set_delete() {
    assert_stdout(
        r#"let s = new Set(); s.add(1); s.add(2); s.delete(1); console.log(s.size, s.has(1));"#,
        "1 false\n",
    );
}

#[test]
fn set_string_elements() {
    assert_stdout(
        r#"let s = new Set(); s.add("a"); s.add("a"); s.add("b"); console.log(s.size);"#,
        "2\n",
    );
}

// ---------------------------------------------------------------------------
// Error family.
// ---------------------------------------------------------------------------

#[test]
fn error_message() {
    assert_stdout(r#"let e = new Error("boom"); console.log(e.message);"#, "boom\n");
}

#[test]
fn error_name_default() {
    assert_stdout(r#"let e = new Error("x"); console.log(e.name);"#, "Error\n");
}

#[test]
fn type_error_name() {
    assert_stdout(r#"console.log(new TypeError("x").name);"#, "TypeError\n");
}

#[test]
fn range_error_name() {
    assert_stdout(r#"let e = new RangeError("oops"); console.log(e.name, e.message);"#, "RangeError oops\n");
}

#[test]
fn error_to_string() {
    assert_stdout(r#"let e = new Error("boom"); console.log(e.toString());"#, "Error: boom\n");
}

// ---------------------------------------------------------------------------
// `class X extends Error`.
// ---------------------------------------------------------------------------

#[test]
fn extends_error_custom_name() {
    assert_stdout(
        r#"class MyErr extends Error { constructor(m: string){ super(m); this.name = "MyErr"; } }
           let e = new MyErr("oops"); console.log(e.name, e.message);"#,
        "MyErr oops\n",
    );
}

#[test]
fn extends_error_own_method() {
    assert_stdout(
        r#"class MyErr extends Error {
               constructor(m: string){ super(m); this.name = "MyErr"; }
               describe(): string { return this.name; }
           }
           let e = new MyErr("oops"); console.log(e.describe(), e.message);"#,
        "MyErr oops\n",
    );
}

#[test]
fn extends_error_inherited_tostring() {
    // The inherited Error.toString() renders `<name>: <message>` with the
    // subclass-overridden name.
    assert_stdout(
        r#"class MyErr extends Error { constructor(m: string){ super(m); this.name = "MyErr"; } }
           let e = new MyErr("bad"); console.log(e.toString());"#,
        "MyErr: bad\n",
    );
}

#[test]
fn extends_type_error() {
    assert_stdout(
        r#"class HttpError extends TypeError { constructor(m: string){ super(m); this.name = "HttpError"; } }
           let e = new HttpError("nope"); console.log(e.name, e.message);"#,
        "HttpError nope\n",
    );
}

// ---------------------------------------------------------------------------
// Wrapper objects (typeof === "object").
// ---------------------------------------------------------------------------

#[test]
fn typeof_number_wrapper() {
    assert_stdout(r#"console.log(typeof new Number(5));"#, "object\n");
}

#[test]
fn typeof_boolean_wrapper() {
    assert_stdout(r#"console.log(typeof new Boolean(true));"#, "object\n");
}

#[test]
fn typeof_string_wrapper() {
    assert_stdout(r#"console.log(typeof new String("hi"));"#, "object\n");
}

// ---------------------------------------------------------------------------
// instanceof.
// ---------------------------------------------------------------------------

#[test]
fn instanceof_error() {
    assert_stdout(r#"let e = new Error("x"); console.log(e instanceof Error);"#, "true\n");
}

#[test]
fn instanceof_map() {
    assert_stdout(r#"let m = new Map(); console.log(m instanceof Map);"#, "true\n");
}

#[test]
fn instanceof_set() {
    assert_stdout(r#"let s = new Set(); console.log(s instanceof Set);"#, "true\n");
}

#[test]
fn instanceof_map_not_set() {
    assert_stdout(r#"let m = new Map(); console.log(m instanceof Set);"#, "false\n");
}

#[test]
fn instanceof_user_extends_error() {
    assert_stdout(
        r#"class MyErr extends Error { constructor(m: string){ super(m); } }
           let e = new MyErr("x"); console.log(e instanceof MyErr, e instanceof Error);"#,
        "true true\n",
    );
}

#[test]
fn typeof_map_object() {
    assert_stdout(r#"let m = new Map(); console.log(typeof m);"#, "object\n");
}

// ---------------------------------------------------------------------------
// Bails — honesty floor: an unmodeled form refuses, never a wrong value.
// ---------------------------------------------------------------------------

#[test]
fn bail_map_init_from_array() {
    // `new Map([["a",1]])` (init from an iterable) is a later increment.
    assert_bails(r#"let m = new Map([["a", 1]]); console.log(m.size);"#);
}

#[test]
fn map_object_key_supported() {
    // The TS Map keys by identity (===), so distinct object keys are distinct
    // entries — a capability the native engine bailed (now provided by the TS
    // stdlib that shadows the native Map).
    assert_stdout(
        r#"let m = new Map(); let a = {x:1}; let b = {x:1};
           m.set(a, 1); m.set(b, 2);
           console.log(m.size, m.get(a), m.get(b));"#,
        "2 1 2\n",
    );
}

#[test]
fn bail_error_non_string_message() {
    // A non-string message would need ToString coercion (a later increment).
    assert_bails(r#"let e = new Error(42); console.log(e.message);"#);
}

#[test]
fn bail_extends_non_error_builtin() {
    // Extending a non-error builtin (Array/Map) needs real exotic-object behavior.
    assert_bails(
        r#"class MyArr extends Array { constructor(){ super(); } }
           let a = new MyArr(); console.log(a);"#,
    );
}

#[test]
fn bail_unknown_map_method() {
    // A method not in the Map metadata rows bails (never a guess).
    assert_bails(r#"let m = new Map(); m.forEach(() => {}); console.log(m.size);"#);
}
