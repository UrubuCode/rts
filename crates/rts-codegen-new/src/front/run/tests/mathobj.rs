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
fn math_min_with_spread_bails() {
    // A spread arg is flattened to an array word by the HIR — coercing it to a
    // bogus scalar is forbidden; must bail.
    assert_bails("let xs = [1, 2, 3]; console.log(Math.min(...xs));");
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
