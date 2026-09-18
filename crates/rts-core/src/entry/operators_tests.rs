//! The tests of `operators.rs`, apart so that file stays under its ceiling.
//!
//! A child of `operators` through `#[path]`, exactly as `context_tests.rs` is
//! of `mod.rs`: `use super::*` reaches the private helpers as it did inline.

use super::*;
use crate::entry::{Context, with_context};
use crate::text::Str;
use crate::value::Singletons;
use rts_cranelift::tags;

fn singletons() -> Singletons {
    Singletons { undefined: 0, null: 1, hole: 2 }
}

/// Runs a body with a context installed, as a compiled program would.
fn hosted<T>(body: impl FnOnce() -> T) -> T {
    let (_context, value) = with_context(Context::new(singletons(), crate::value::Kinds::in_declaration_order()), body);
    value
}

fn number(bits: u64) -> f64 {
    tags::decode_double(bits)
}

#[test]
fn subtraction_of_two_numbers_is_arithmetic() {
    hosted(|| {
        let a = Value::from_f64(5.0).bits();
        let b = Value::from_f64(3.0).bits();
        assert_eq!(number(subtract(a, b)), 2.0);
    });
}

#[test]
fn dividing_by_zero_answers_infinity_rather_than_failing() {
    // IEEE-754 is the language's arithmetic, so this is the correct answer
    // and not an edge case to guard. A guard here would replace what
    // JavaScript says with something else.
    hosted(|| {
        let one = Value::from_f64(1.0).bits();
        let zero = Value::from_f64(0.0).bits();
        assert_eq!(number(divide(one, zero)), f64::INFINITY);
        assert!(number(divide(zero, zero)).is_nan());
    });
}

#[test]
fn remainder_takes_the_sign_of_the_dividend() {
    // `-5 % 3` is `-2`, not `1`. Pinned because the two differ in most
    // languages and Rust agreeing with JavaScript here is a coincidence
    // this test is what protects.
    hosted(|| {
        let minus_five = Value::from_f64(-5.0).bits();
        let three = Value::from_f64(3.0).bits();
        assert_eq!(number(remainder(minus_five, three)), -2.0);
    });
}

#[test]
fn a_boolean_operand_converts_before_the_operator_runs() {
    // `true - 1` is `0`: ToNumber of `true` is 1. Nothing about this is
    // special-cased here — it is what `to_number` already answers, and the
    // test is that this path asks it.
    hosted(|| {
        let yes = Value::from_bool(true).bits();
        let one = Value::from_f64(1.0).bits();
        assert_eq!(number(subtract(yes, one)), 0.0);
    });
}

#[test]
fn nan_is_unordered_so_every_comparison_with_it_is_false() {
    // The one that catches an implementation written as negations: if
    // `a <= b` were `!(a > b)`, this pair would answer true and false
    // instead of false and false.
    hosted(|| {
        let nan = Value::from_f64(f64::NAN).bits();
        assert!(!less_equal(nan, nan));
        assert!(!greater_equal(nan, nan));
        assert!(!less(nan, nan));
        assert!(!greater(nan, nan));
    });
}

#[test]
fn numbers_compare_numerically() {
    hosted(|| {
        let two = Value::from_f64(2.0).bits();
        let ten = Value::from_f64(10.0).bits();
        assert!(less(two, ten));
        assert!(!greater(two, ten));
        assert!(less_equal(two, two));
    });
}

#[test]
fn two_strings_compare_as_text_and_a_string_and_a_number_do_not() {
    // The reason `<` cannot be a comparison instruction. `"2" < "10"` is
    // FALSE — code-unit order puts "10" first — while `2 < 10` is true, and
    // `"2" < 10` converts and is true again. One operator, three answers,
    // decided by what the operands turn out to be.
    hosted(|| {
        let two = crate::entry::with_current(|context| {
            context.intern_value(Str::from_str("2")).bits()
        });
        let ten = crate::entry::with_current(|context| {
            context.intern_value(Str::from_str("10")).bits()
        });
        assert!(!less(two, ten), "as text, \"2\" comes after \"10\"");

        let ten_number = Value::from_f64(10.0).bits();
        assert!(less(two, ten_number), "with a number, both convert");
    });
}
