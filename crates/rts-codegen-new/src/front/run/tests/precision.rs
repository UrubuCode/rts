//! P5.6 precision tests — the constructs the now-precise rts-hir distinguishes
//! (they used to bail because rts-hir conflated them): loose `==`/`!=` (distinct
//! from `===`/`!==`), unary `+`/`!`, `**`/`**=`, logical-assign `&&=`/`||=`/`??=`,
//! object-typed param routing, and argument/array spread.

use super::{assert_bails, assert_stdout};

// ===========================================================================
// Loose vs strict equality (rts-hir now lowers Eq/Ne distinct from StrictEq/Ne).
// ===========================================================================

#[test]
fn loose_vs_strict_equality() {
    assert_stdout(
        r#"console.log(1 == "1", 1 === "1", null == undefined, null === undefined);"#,
        "true false true false\n",
    );
}

#[test]
fn loose_zero_empty_string() {
    assert_stdout(r#"console.log(0 == "", 0 === "");"#, "true false\n");
}

// ===========================================================================
// Unary `+` (ToNumber) and `!` (ToBoolean-invert).
// ===========================================================================

#[test]
fn unary_plus_and_not() {
    assert_stdout(r#"console.log(+"42", +true, !0, !"");"#, "42 1 true true\n");
}

// ===========================================================================
// `**` and `**=`.
// ===========================================================================

#[test]
fn exponent_operator() {
    assert_stdout("console.log(2 ** 10, 3 ** 2);", "1024 9\n");
}

#[test]
fn exponent_compound_assign() {
    assert_stdout("let a = 2; a **= 3; console.log(a);", "8\n");
}

// ===========================================================================
// Logical-assign `||=` / `&&=` / `??=`.
// ===========================================================================

#[test]
fn logical_assign_or_and() {
    assert_stdout(
        "let x = 0; x ||= 5; console.log(x); let y = 1; y &&= 9; console.log(y);",
        "5\n9\n",
    );
}

// ===========================================================================
// Spread into variadic Math.
// ===========================================================================

#[test]
fn spread_math_max_min() {
    assert_stdout(
        "let xs = [3, 7, 2]; console.log(Math.max(...xs), Math.min(...xs));",
        "7 2\n",
    );
}

// ===========================================================================
// Spread into String.fromCharCode.
// ===========================================================================

#[test]
fn spread_from_char_code() {
    assert_stdout(
        r#"let cs = [72, 105]; console.log(String.fromCharCode(...cs));"#,
        "Hi\n",
    );
}

// ===========================================================================
// Array-literal spread.
// ===========================================================================

#[test]
fn array_literal_spread() {
    assert_stdout(
        r#"let a = [1,2]; let b = [...a, 3, 4]; console.log(b.join(","));"#,
        "1,2,3,4\n",
    );
}

#[test]
fn array_literal_spread_two() {
    assert_stdout(
        r#"let a = [1,2]; let b = [...a, 3, 4]; let c = [...a, ...b]; console.log(c.length);"#,
        "6\n",
    );
}

// ===========================================================================
// User function call with spread.
// ===========================================================================

#[test]
fn user_fn_spread() {
    assert_stdout(
        "function add3(a:number,b:number,c:number){ return a+b+c; } \
         let args = [1,2,3]; console.log(add3(...args));",
        "6\n",
    );
}

// ===========================================================================
// Object-typed param routing.
// ===========================================================================

#[test]
fn object_typed_param() {
    // The `: object` keyword annotation reaches the engine as `HirType::Object`
    // (rts-hir `parse_type_annotation("object")`), so `o.name` routes through the
    // dynamic property access. (The `{name: string}` object-LITERAL annotation
    // form does NOT — see `object_typed_param_literal_annotation_bails`.)
    assert_stdout(
        r#"function getName(o: object){ return o.name; } console.log(getName({name:"rts"}));"#,
        "rts\n",
    );
}

#[test]
fn object_typed_param_literal_annotation_bails() {
    // A `{name: string}` object-LITERAL type annotation is captured by rts-parser
    // as the raw source string "{name: string}", which rts-hir's
    // `parse_type_annotation` maps to `HirType::Unknown` (NOT `Object`) — so the
    // param's shape is unproven and `o.name` bails. This is an rts-parser/rts-hir
    // string-mapping gap (those crates are consumed read-only); the engine routes
    // `HirType::Object` correctly (see `object_typed_param`). Documented as a sound
    // bail, never a wrong value.
    assert_bails(
        r#"function getName(o: {name: string}){ return o.name; } console.log(getName({name:"rts"}));"#,
    );
}

// ===========================================================================
// Sound bails preserved: `delete` (slot removal) and spread forms not covered.
// ===========================================================================

#[test]
fn delete_still_bails() {
    assert_bails("let o = { a: 1 }; delete o.a; console.log(o.a);");
}
