//! P4.8: polymorphic operators on Tagged PolyValues + untyped function params.
//!
//! Most JS is untyped; an unannotated param (`x => x*2`) is `Repr::Tagged` and
//! every operator on it routes through the ONE generic `__rtsadp_*` path. These
//! tests exercise that path end to end (parse → HIR → run-lowering → JIT →
//! captured stdout) against EXACT Node/Bun output — the proven-numeric fast path
//! stays native, only Tagged operands use the trampolines.

use super::assert_stdout;

// ===========================================================================
// Untyped arrow callbacks in Array methods (the headline real-world unlock).
// ===========================================================================

#[test]
fn untyped_arrow_map() {
    assert_stdout("let a=[1,2,3]; console.log(a.map(x=>x*2).join(\",\"));", "2,4,6\n");
}

#[test]
fn untyped_arrow_filter() {
    assert_stdout("let a=[1,2,3,4]; console.log(a.filter(x=>x>2).join(\",\"));", "3,4\n");
}

#[test]
fn untyped_arrow_reduce() {
    assert_stdout("console.log([1,2,3,4].reduce((a,b)=>a+b,0));", "10\n");
}

#[test]
fn chained_untyped_callbacks() {
    assert_stdout(
        "console.log([1,2,3,4].filter(x=>x%2===0).map(x=>x*10).join(\",\"));",
        "20,40\n",
    );
}

// ===========================================================================
// Untyped named functions: `+` is polymorphic over number vs string.
// ===========================================================================

#[test]
fn untyped_function_add_number_and_string() {
    assert_stdout(
        "function add(a,b){ return a+b; } console.log(add(3,4)); console.log(add(\"x\",\"y\"));",
        "7\nxy\n",
    );
}

#[test]
fn untyped_function_subtract() {
    assert_stdout("function sub(a,b){ return a-b; } console.log(sub(10,3));", "7\n");
}

#[test]
fn untyped_function_multiply_and_divide() {
    assert_stdout(
        "function f(a,b){ return a*b; } function g(a,b){ return a/b; } console.log(f(6,7), g(5,2));",
        "42 2.5\n",
    );
}

// ===========================================================================
// Generic arithmetic via an `any` annotation (if it parses) — covered also by
// the untyped-param forms above.
// ===========================================================================

#[test]
fn any_annotated_arithmetic() {
    // `let x: any = 5` — x is Tagged; x*3, x-2, x/2 all route generic.
    assert_stdout("let x:any = 5; console.log(x*3, x-2, x/2);", "15 3 2.5\n");
}

// ===========================================================================
// Modulo — native int, native-float-via-generic, and untyped.
// ===========================================================================

#[test]
fn modulo_int_and_float() {
    // 7 % 3 = 1 (native int srem); 7.5 % 2 = 1.5 (float → generic fmod).
    assert_stdout("console.log(7 % 3, 7.5 % 2);", "1 1.5\n");
}

#[test]
fn modulo_untyped() {
    assert_stdout("function m(a,b){ return a%b; } console.log(m(17,5));", "2\n");
}

// ===========================================================================
// Comparison on untyped operands (numeric + string lexicographic).
// ===========================================================================

#[test]
fn untyped_comparison_number_and_string() {
    assert_stdout(
        "function cmp(a,b){ return a<b; } console.log(cmp(1,2), cmp(\"b\",\"a\"));",
        "true false\n",
    );
}

#[test]
fn untyped_relational_all() {
    assert_stdout(
        "function f(a,b){ return a<=b; } function g(a,b){ return a>=b; } console.log(f(2,2), g(3,5));",
        "true false\n",
    );
}

// ===========================================================================
// Unary `-` and `~` on untyped/Tagged operands.
// ===========================================================================

#[test]
fn untyped_unary_neg() {
    assert_stdout("function neg(n){ return -n; } console.log(neg(5));", "-5\n");
}

#[test]
fn unary_bitnot_native_and_untyped() {
    // ~5 = -6, native int; through an untyped param too.
    assert_stdout("console.log(~5); function bn(n){ return ~n; } console.log(bn(0));", "-6\n-1\n");
}

// ===========================================================================
// Bitwise / shifts — native (int literals) and untyped (Tagged).
// ===========================================================================

#[test]
fn bitwise_native() {
    assert_stdout("console.log(5 & 3, 5 | 2, 1 << 4);", "1 7 16\n");
}

#[test]
fn bitwise_untyped() {
    assert_stdout(
        "function f(a,b){ return a&b; } function g(a,b){ return a<<b; } console.log(f(6,3), g(2,3));",
        "2 16\n",
    );
}

// ===========================================================================
// String + boolean coercion through the generic `+`.
// ===========================================================================

#[test]
fn string_plus_bool() {
    assert_stdout(r#"console.log("v=" + true);"#, "v=true\n");
}

#[test]
fn string_plus_number_untyped() {
    assert_stdout(
        "function tag(p,v){ return p+\": \"+v; } console.log(tag(\"x\", 42));",
        "x: 42\n",
    );
}

// ===========================================================================
// `**` exponentiation (no native Cranelift op → generic pow).
// ===========================================================================

#[test]
fn exponentiation() {
    assert_stdout("console.log(2 ** 10, 3 ** 3);", "1024 27\n");
}

// ===========================================================================
// P5.6: the HIR-ambiguity bails are LIFTED — rts-hir now distinguishes the ops.
// (Intentional, justified change from the prior `assert_bails`.)
// ===========================================================================

#[test]
fn loose_eq_cross_kind_now_works() {
    // rts-hir now lowers `==` to a DISTINCT `Eq` op (not conflated with `===`), so
    // cross-kind `1 == "1"` runs the real JS Abstract Equality (→ `true`).
    assert_stdout(r#"console.log(1 == "1");"#, "true\n");
}

#[test]
fn unary_not_now_works() {
    // rts-hir now lowers `!` to a DISTINCT `Not` op (not conflated with unary-`+`),
    // so `!0` is `true`.
    assert_stdout("console.log(!0);", "true\n");
}
