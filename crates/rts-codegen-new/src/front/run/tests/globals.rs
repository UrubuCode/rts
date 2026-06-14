//! P5.2: GLOBAL constants/functions + Array/String statics — `NaN`/`Infinity`/
//! `undefined`, `Number`/`String`/`Boolean`/`parseInt`/`parseFloat`/`isNaN`/
//! `isFinite`, `Array.isArray`/`of`/`from`/`Array(n)`, `String.fromCharCode`,
//! all through the codegen-owned `__rtsadp_*` trampolines (no fake values).

use super::{assert_bails, assert_stdout};

// ---- global value constants ----

#[test]
fn global_nan_infinity() {
    assert_stdout("console.log(NaN, Infinity, -Infinity);", "NaN Infinity -Infinity\n");
}

#[test]
fn global_undefined() {
    assert_stdout("console.log(undefined);", "undefined\n");
}

// ---- isNaN / isFinite ----

#[test]
fn is_nan_is_finite() {
    assert_stdout("console.log(isNaN(NaN), isFinite(1), isNaN(5));", "true true false\n");
}

// ---- Number / String / Boolean coercions ----

#[test]
fn number_string_boolean() {
    assert_stdout(r#"console.log(Number("42"), String(7), Boolean(0));"#, "42 7 false\n");
}

#[test]
fn boolean_truthy() {
    assert_stdout(r#"console.log(Boolean(1), Boolean(""), Boolean("x"));"#, "true false true\n");
}

// ---- parseInt / parseFloat ----

#[test]
fn parse_int_radix() {
    assert_stdout(r#"console.log(parseInt("1010", 2), parseFloat("3.14x"));"#, "10 3.14\n");
}

#[test]
fn parse_int_default_decimal() {
    assert_stdout(r#"console.log(parseInt("42abc"), parseInt("0xFF", 16));"#, "42 255\n");
}

#[test]
fn parse_float_plain() {
    assert_stdout(r#"console.log(parseFloat("  2.5e2zzz"));"#, "250\n");
}

// ---- Array statics ----

#[test]
fn array_is_array() {
    assert_stdout(r#"console.log(Array.isArray([1,2]), Array.isArray("x"));"#, "true false\n");
}

#[test]
fn array_of() {
    assert_stdout(r#"console.log(Array.of(1,2,3).join(","));"#, "1,2,3\n");
}

#[test]
fn array_from_string() {
    assert_stdout(r#"console.log(Array.from("abc").join("-"));"#, "a-b-c\n");
}

#[test]
fn array_sized_ctor() {
    // `new Array(3)` → three undefined holes; length is 3.
    assert_stdout("let a = new Array(3); console.log(a.length);", "3\n");
    assert_stdout("let a = Array(2); console.log(a.length);", "2\n");
}

// ---- String statics ----

#[test]
fn string_from_char_code() {
    assert_stdout("console.log(String.fromCharCode(72, 105));", "Hi\n");
}

// ---- string-coercing `+` end to end ----

#[test]
fn coerce_in_concat() {
    assert_stdout(r#"console.log("n=" + String(5));"#, "n=5\n");
    assert_stdout(r#"console.log("x=" + [1,2,3].join("-"));"#, "x=1-2-3\n");
}

// ---- bails for genuinely-unsupported forms (honesty floor) ----

#[test]
fn global_this_bails() {
    // globalThis has no value model here → still an unbound-identifier bail.
    assert_bails("console.log(globalThis);");
}

#[test]
fn array_from_map_bails_on_use() {
    // Array.from over a non-string/non-array source is out of scope; a Map literal
    // itself is unsupported, so this bails before reaching Array.from.
    assert_bails("console.log(Array.from(new Map()).length);");
}

#[test]
fn from_char_code_spread_now_works() {
    // P5.6 (intentional, justified change from the prior bail): rts-hir now
    // PRESERVES the spread flag (`HirExprKind::Spread`), so `fromCharCode(...codes)`
    // folds `fromCharCode` over the array elements at runtime → "Hi".
    assert_stdout(
        "let codes = [72, 105]; console.log(String.fromCharCode(...codes));",
        "Hi\n",
    );
}

#[test]
fn array_of_spread_bails() {
    assert_bails("let xs = [1, 2, 3]; console.log(Array.of(...xs).length);");
}
