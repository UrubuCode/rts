//! P5.4: `Math.*` methods + constants, `Number.*` static predicates + constants.

use super::{assert_bails, assert_stdout};

#[test]
fn math_rounding_and_abs() {
    assert_stdout(
        "console.log(Math.floor(3.7), Math.ceil(3.2), Math.round(2.5), Math.abs(-4));",
        "3 4 3 4\n",
    );
}

#[test]
fn math_min_max_sqrt() {
    assert_stdout(
        "console.log(Math.max(3, 7), Math.min(3, 7), Math.sqrt(16));",
        "7 3 4\n",
    );
}

#[test]
fn math_pow_sign_trunc() {
    assert_stdout(
        "console.log(Math.pow(2, 10), Math.sign(-5), Math.trunc(4.9));",
        "1024 -1 4\n",
    );
}

#[test]
fn math_constants() {
    assert_stdout("console.log(Math.PI > 3.14, Math.E > 2.7);", "true true\n");
}

#[test]
fn math_min_max_three_args_fold() {
    // Variadic min/max with 3 args folds pairwise.
    assert_stdout(
        "console.log(Math.max(3, 7, 5), Math.min(8, 2, 6));",
        "7 2\n",
    );
}

#[test]
fn math_hypot_two_arg() {
    assert_stdout("console.log(Math.hypot(3, 4));", "5\n");
}

#[test]
fn math_sqrt_inline_in_expression() {
    // sqrt inlines to fsqrt and composes with native arithmetic.
    assert_stdout("console.log(Math.sqrt(9) + Math.sqrt(4));", "5\n");
}

#[test]
fn math_const_member_in_arithmetic() {
    // A Math constant used as a plain f64 operand.
    assert_stdout(
        "console.log(Math.SQRT2 > 1.41 && Math.SQRT2 < 1.42);",
        "true\n",
    );
}

#[test]
fn number_predicates() {
    assert_stdout(
        "console.log(Number.isInteger(5), Number.isInteger(5.5), Number.isFinite(1/0));",
        "true false false\n",
    );
}

#[test]
fn number_max_safe_integer() {
    assert_stdout(
        "console.log(Number.MAX_SAFE_INTEGER);",
        "9007199254740991\n",
    );
}

#[test]
fn number_is_nan_and_safe_integer() {
    assert_stdout(
        "console.log(Number.isNaN(0/0), Number.isSafeInteger(10), Number.isSafeInteger(1.5));",
        "true true false\n",
    );
}

// ===========================================================================
// Computed-NaN canonicalization. A runtime-computed NaN on x86 is a NEGATIVE
// qNaN (`0xFFF8…`), which lands in the PolyValue boxed space — `emit_box_double`
// must canonicalize it to the POSITIVE `CANONICAL_NAN` so it reads back as a
// number (not a boxed TAG_OBJECT → "[object Object]" / a wrong `typeof`).
// ===========================================================================

#[test]
fn computed_nan_sqrt_negative() {
    assert_stdout("console.log(Math.sqrt(-4));", "NaN\n");
}

#[test]
fn computed_nan_div_zero_by_zero() {
    assert_stdout("console.log(0/0);", "NaN\n");
}

#[test]
fn computed_nan_typeof_is_number() {
    assert_stdout("console.log(typeof (0/0));", "number\n");
}

#[test]
fn computed_nan_zero_times_infinity() {
    // 0 * Infinity = NaN (another negative-qNaN producer).
    assert_stdout("console.log(0 * (1/0) * 1);", "NaN\n");
}

#[test]
fn nan_max_propagates() {
    // Math.max(NaN, 1) is NaN in JS (NaN-propagating).
    assert_stdout("console.log(Math.max(0/0, 1));", "NaN\n");
}

#[test]
fn non_nan_double_unaffected() {
    // sqrt(16) is a clean integer-valued double; must NOT be touched.
    assert_stdout("console.log(Math.sqrt(16));", "4\n");
}

#[test]
fn negative_zero_still_prints_zero() {
    // -0.0 is NOT a NaN; canonicalization must leave it alone (JS prints "0").
    assert_stdout("console.log(Math.sqrt(0) * -1);", "0\n");
}

#[test]
fn number_epsilon_constant() {
    assert_stdout(
        "console.log(Number.EPSILON > 0, Number.POSITIVE_INFINITY > 1e308);",
        "true true\n",
    );
}

// ===========================================================================
// Negative: out-of-subset forms bail (never a wrong value).
// ===========================================================================

#[test]
fn math_min_with_spread_now_works() {
    // P5.6 (intentional, justified change from the prior bail): rts-hir now
    // PRESERVES the spread flag, so `Math.min(...xs)` folds `min` over the array
    // elements at runtime via `__rtsadp_math_reduce` → 1.
    assert_stdout("let xs = [1, 2, 3]; console.log(Math.min(...xs));", "1\n");
}

#[test]
fn math_imul_bails() {
    // imul/clz32 are i32-domain (I64 ABI) — not in the f64 table; bail rather than
    // mis-marshal an f64 where the symbol wants i64.
    assert_bails("console.log(Math.imul(3, 4));");
}

#[test]
fn math_as_bare_value_bails() {
    // `Math` is a namespace, not a value — referencing it bare has no value model.
    assert_bails("let m = Math; console.log(m);");
}

#[test]
fn number_predicate_on_tagged_bails() {
    // The no-coerce predicate on a non-proven-number (Tagged) arg bails — never a
    // wrong `true`.
    assert_bails(r#"let s = "5"; console.log(Number.isInteger(s));"#);
}
