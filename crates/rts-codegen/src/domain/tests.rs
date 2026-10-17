//! The domain's own tests, apart from `mod.rs` for the ceiling rule.

use super::*;

/// The axis. One numeric type here, two there.
#[test]
fn an_int32_joined_with_a_double_is_a_double() {
    let domain = Js::new();
    assert_eq!(domain.join(&Type::Int32, &Type::Double), Type::Double);
    assert_eq!(domain.join(&Type::Int32, &Type::Int32), Type::Int32);
    assert_eq!(domain.join(&Type::Int32, &Type::Str), Type::Anything);
}

#[test]
fn bottom_is_the_identity_of_the_join() {
    let domain = Js::new();
    assert_eq!(domain.join(&Type::Nothing, &Type::Str), Type::Str);
    assert_eq!(domain.join(&Type::Str, &Type::Nothing), Type::Str);
    assert_eq!(domain.join(&Type::Nothing, &Type::Nothing), Type::Nothing);
}

/// The seven falsy values, as the one thing they imply about a type.
#[test]
fn a_number_and_a_string_have_no_decidable_truth_here() {
    let domain = Js::new();
    assert_eq!(domain.truth_of(&Type::Int32), None);
    assert_eq!(domain.truth_of(&Type::Double), None);
    assert_eq!(domain.truth_of(&Type::Str), None);
    assert_eq!(domain.truth_of(&Type::Undefined), Some(false));
    assert_eq!(domain.truth_of(&Type::Null), Some(false));
    assert_eq!(domain.truth_of(&Type::Object), Some(true));
    assert_eq!(domain.truth_of(&Type::Callable), Some(true));
    assert_eq!(domain.truth_of(&Type::Bool(Some(true))), Some(true));
    assert_eq!(domain.truth_of(&Type::Bool(None)), None);
}

/// The defect a table written the obvious way would ship: two int32s added
/// are not an int32 at the one value that matters.
#[test]
fn two_int32s_added_are_a_double_because_the_sum_may_not_fit() {
    let domain = Js::new();
    let add = domain.prim(JsPrim::Add);
    assert_eq!(
        domain.transfer(add, &[Type::Int32, Type::Int32]),
        Type::Double
    );
    assert_eq!(domain.transfer(add, &[Type::Str, Type::Int32]), Type::Str);
    assert_eq!(domain.transfer(add, &[Type::Int32, Type::Str]), Type::Str);
}

/// Division answers a double even of two integers -- and only a number where one
/// side rules a BigInt out. This test asserted `Double` over two unknowns, which is
/// what the table said and what the language does not: `10n / 3n` is `3n`.
#[test]
fn division_answers_a_number_unless_both_sides_may_be_a_bigint() {
    let domain = Js::new();
    let divide = domain.prim(JsPrim::Divide);
    assert_eq!(
        domain.transfer(divide, &[Type::Int32, Type::Int32]),
        Type::Double
    );
    assert_eq!(
        domain.transfer(divide, &[Type::Anything, Type::Int32]),
        Type::Double,
        "mixing a BigInt with a Number throws, so one side is enough"
    );
    assert_eq!(
        domain.transfer(divide, &[Type::Anything, Type::Anything]),
        Type::Anything
    );
}

/// The same for every ToNumeric row, the bitwise ones included: `x | 0` still proves
/// an Int32 -- the reason a program writes it -- and `~x` over an unknown may be
/// `~1n`.
#[test]
fn a_numeric_row_over_two_possible_bigints_proves_nothing() {
    let domain = Js::new();
    for which in [JsPrim::Subtract, JsPrim::Multiply, JsPrim::Remainder] {
        let prim = domain.prim(which);
        assert_eq!(
            domain.transfer(prim, &[Type::Object, Type::Anything]),
            Type::Anything,
            "{which:?}"
        );
        assert_eq!(
            domain.transfer(prim, &[Type::Str, Type::Anything]),
            Type::Double,
            "{which:?}"
        );
    }
    let bitwise = domain.prim(JsPrim::BitAnd);
    assert_eq!(
        domain.transfer(bitwise, &[Type::Anything, Type::Int32]),
        Type::Int32
    );
    assert_eq!(
        domain.transfer(bitwise, &[Type::Anything, Type::Anything]),
        Type::Anything
    );
    let not = domain.prim(JsPrim::BitwiseNot);
    assert_eq!(domain.transfer(not, &[Type::Anything]), Type::Anything);
    let negate = domain.prim(JsPrim::Negate);
    assert_eq!(domain.transfer(negate, &[Type::Anything]), Type::Anything);
    assert_eq!(domain.transfer(negate, &[Type::Int32]), Type::Double);
}

/// A field write answers the VALUE written -- the third operand -- and not the key.
#[test]
fn a_field_write_answers_what_was_written() {
    let domain = Js::new();
    let write = domain.prim(JsPrim::FieldWrite);
    assert_eq!(
        domain.transfer(write, &[Type::Object, Type::Str, Type::Int32]),
        Type::Int32
    );
}

/// What a guard buys, which is the only reason a specialised tier knows more.
#[test]
fn a_guard_narrows_and_never_widens() {
    let mut domain = Js::new();
    let is_int32 = domain.assertion(JsAssertion::IsInt32);
    let is_double = domain.assertion(JsAssertion::IsDouble);
    assert_eq!(domain.narrow(is_int32, &Type::Anything), Type::Int32);
    // Asserting the weaker fact about the stronger one keeps the stronger.
    assert_eq!(domain.narrow(is_double, &Type::Int32), Type::Int32);
    assert_eq!(domain.narrow(is_double, &Type::Anything), Type::Double);
}

/// Two guards asserting one thing must be one assertion, or CSE cannot see
/// them as the same.
#[test]
fn an_assertion_is_interned() {
    let mut domain = Js::new();
    let first = domain.assertion(JsAssertion::HasShape(4));
    let again = domain.assertion(JsAssertion::HasShape(4));
    let other = domain.assertion(JsAssertion::HasShape(5));
    assert_eq!(first, again);
    assert_ne!(first, other);
    assert_eq!(domain.asserted(first), Some(JsAssertion::HasShape(4)));
}

/// The reason the effect takes the operand types: it is what a guard is for.
#[test]
fn an_addition_of_proven_numbers_calls_nothing() {
    let domain = Js::new();
    let add = domain.prim(JsPrim::Add);
    let proven = domain.effect_of(add, &[Type::Int32, Type::Int32]);
    assert!(proven.is_pure());

    let unknown = domain.effect_of(add, &[Type::Anything, Type::Int32]);
    assert!(unknown.has(Effect::CALLS_USER));
    assert!(unknown.has(Effect::THROWS));
    assert!(!unknown.falls_through());

    // And concatenation allocates even though nothing coerces.
    let text = domain.effect_of(add, &[Type::Str, Type::Str]);
    assert!(text.has(Effect::ALLOCATES));
    assert!(text.may_collect());
}

#[test]
fn a_shaped_field_read_reads_and_an_unshaped_one_may_run_a_getter() {
    let domain = Js::new();
    let read = domain.prim(JsPrim::FieldRead);
    let shaped = domain.effect_of(read, &[Type::Shaped(1)]);
    assert!(shaped.has(Effect::READS));
    assert!(!shaped.has(Effect::CALLS_USER));

    let unshaped = domain.effect_of(read, &[Type::Object]);
    assert!(unshaped.has(Effect::CALLS_USER));
}

/// Rule 4: the index is opaque to the IR, and this is the only file that may
/// turn one into a meaning.
#[test]
fn every_primitive_round_trips_through_its_index() {
    let domain = Js::new();
    for which in Js::PRIMS {
        assert_eq!(domain.meaning(domain.prim(*which)), Some(*which));
    }
    assert_eq!(domain.meaning(Prim(Js::PRIMS.len() as u32)), None);
}

/// A `strictEquals` cannot call user code, which is what makes it the one
/// comparison worth its own row.
#[test]
fn strict_equality_coerces_nothing() {
    let domain = Js::new();
    let equals = domain.prim(JsPrim::StrictEquals);
    assert!(
        domain
            .effect_of(equals, &[Type::Anything, Type::Anything])
            .is_pure()
    );
    let less = domain.prim(JsPrim::LessThan);
    assert!(
        !domain
            .effect_of(less, &[Type::Anything, Type::Anything])
            .is_pure()
    );
}
/// Coercing a string or a singleton is not a call, which is the arm the first
/// version of this table got wrong.
#[test]
fn coercing_anything_but_an_object_calls_nothing() {
    let domain = Js::new();
    let times = domain.prim(JsPrim::Multiply);
    assert!(domain.effect_of(times, &[Type::Str, Type::Str]).is_pure());
    assert!(
        domain
            .effect_of(times, &[Type::Undefined, Type::Int32])
            .is_pure()
    );
    // An OBJECT is the one that reaches valueOf.
    assert!(
        domain
            .effect_of(times, &[Type::Shaped(1), Type::Int32])
            .has(Effect::CALLS_USER)
    );
    assert!(
        domain
            .effect_of(times, &[Type::Object, Type::Int32])
            .has(Effect::CALLS_USER)
    );
}

/// Two ranges join to their hull, and the whole range IS `Int32` -- one name for one set,
/// so `==` is set equality and the fixed point's "did it change" is exact.
#[test]
fn integer_ranges_join_to_their_hull() {
    let domain = Js::new();
    assert_eq!(
        domain.join(&Type::int(0, 3), &Type::int(10, 12)),
        Type::Ranged(0, 12)
    );
    assert_eq!(domain.join(&Type::int(5, 5), &Type::int(5, 5)), Type::Ranged(5, 5));
    assert_eq!(
        domain.join(&Type::Ranged(0, 3), &Type::Int32),
        Type::Int32,
        "the whole range absorbs"
    );
    assert_eq!(
        domain.join(&Type::int(i64::from(i32::MIN), 0), &Type::int(0, i64::from(i32::MAX))),
        Type::Int32,
        "a hull that covers every i32 is Int32, not a Ranged spelling of it"
    );
    assert_eq!(domain.join(&Type::Ranged(0, 3), &Type::Double), Type::Double);
    assert_eq!(domain.join(&Type::Ranged(0, 3), &Type::Str), Type::Anything);
    // Commutative.
    assert_eq!(
        domain.join(&Type::int(10, 12), &Type::int(0, 3)),
        domain.join(&Type::int(0, 3), &Type::int(10, 12))
    );
}

/// THE NO-OVERFLOW ARGUMENT, at its boundary: an integer only where both ends of the result
/// interval fit an `i32`, and a double at the first value that does not.
#[test]
fn integer_arithmetic_is_an_integer_only_when_it_provably_fits() {
    let domain = Js::new();
    let (add, sub, mul) = (
        domain.prim(JsPrim::Add),
        domain.prim(JsPrim::Subtract),
        domain.prim(JsPrim::Multiply),
    );
    let max = i64::from(i32::MAX);
    let min = i64::from(i32::MIN);
    assert_eq!(
        domain.transfer(add, &[Type::int(0, 10), Type::int(0, 10)]),
        Type::Ranged(0, 20)
    );
    // Exactly the largest i32 is still an integer; one more is a double.
    assert_eq!(
        domain.transfer(add, &[Type::int(max - 1, max - 1), Type::int(1, 1)]),
        Type::int(max, max)
    );
    assert_eq!(
        domain.transfer(add, &[Type::int(max, max), Type::int(1, 1)]),
        Type::Double
    );
    assert_eq!(
        domain.transfer(add, &[Type::Int32, Type::int(1, 1)]),
        Type::Double
    );
    assert_eq!(
        domain.transfer(add, &[Type::int(0, max), Type::int(0, 1)]),
        Type::Double,
        "one end of the interval is past the range"
    );
    assert_eq!(
        domain.transfer(sub, &[Type::int(min + 1, min + 1), Type::int(1, 1)]),
        Type::int(min, min)
    );
    assert_eq!(
        domain.transfer(sub, &[Type::int(min, min), Type::int(1, 1)]),
        Type::Double
    );
    assert_eq!(
        domain.transfer(sub, &[Type::int(0, 0), Type::int(min, min)]),
        Type::Double,
        "0 - i32::MIN is 2^31"
    );
    // A product: 46340^2 fits, 46341^2 does not.
    assert_eq!(
        domain.transfer(mul, &[Type::int(0, 46340), Type::int(0, 46340)]),
        Type::int(0, 46340 * 46340)
    );
    assert_eq!(
        domain.transfer(mul, &[Type::int(0, 46341), Type::int(0, 46341)]),
        Type::Double
    );
    // A product with a negative factor may be `-0` (`0 * -5`), which no integer names.
    assert_eq!(
        domain.transfer(mul, &[Type::int(-5, -5), Type::int(0, 0)]),
        Type::Double
    );
    assert_eq!(
        domain.transfer(mul, &[Type::int(-3, 3), Type::int(2, 2)]),
        Type::Double
    );
    // A non-integer operand is the old answer.
    assert_eq!(
        domain.transfer(add, &[Type::Double, Type::int(1, 1)]),
        Type::Double
    );
    assert_eq!(
        domain.transfer(add, &[Type::Anything, Type::int(1, 1)]),
        Type::Anything
    );
}

/// WIDENING contains both arguments and moves a bound at most once.
#[test]
fn widening_jumps_a_moving_bound_to_the_extreme() {
    let domain = Js::new();
    assert_eq!(
        domain.widen(&Type::int(0, 0), &Type::int(0, 1)),
        Type::Ranged(0, i32::MAX)
    );
    assert_eq!(
        domain.widen(&Type::int(0, 5), &Type::int(-1, 5)),
        Type::Ranged(i32::MIN, 5)
    );
    // New inside old: old stays, so the chain a parameter climbs never descends.
    assert_eq!(domain.widen(&Type::int(0, 10), &Type::int(2, 3)), Type::Ranged(0, 10));
    assert_eq!(domain.widen(&Type::int(0, 10), &Type::int(0, i64::from(i32::MAX))), Type::Ranged(0, i32::MAX));
    // Not a range climb: the new answer.
    assert_eq!(domain.widen(&Type::Double, &Type::int(0, 1)), Type::Ranged(0, 1));
    assert_eq!(domain.widen(&Type::Nothing, &Type::int(0, 1)), Type::Ranged(0, 1));
    assert_eq!(domain.widen(&Type::int(0, 1), &Type::Double), Type::Double);
}

/// WHAT A COMPARISON TEACHES, edge by edge, and what it must not claim.
#[test]
fn a_comparison_narrows_integer_operands_on_each_edge() {
    let domain = Js::new();
    let less = domain.prim(JsPrim::LessThan);
    let at_most = domain.prim(JsPrim::LessOrEqual);
    let greater = domain.prim(JsPrim::GreaterThan);
    let i = Type::int(0, i64::from(i32::MAX));
    let n = Type::int(1000, 1000);
    let args = [i.clone(), n.clone()];
    // i < 1000: i is at most 999 where it held, at least 1000 where it did not.
    assert_eq!(domain.refine(less, &args, true, 0), Some(Type::Ranged(0, 999)));
    assert_eq!(domain.refine(less, &args, false, 0), Some(Type::Ranged(1000, i32::MAX)));
    // i <= 1000.
    assert_eq!(domain.refine(at_most, &args, true, 0), Some(Type::Ranged(0, 1000)));
    assert_eq!(domain.refine(at_most, &args, false, 0), Some(Type::Ranged(1001, i32::MAX)));
    // 1000 > i, the bound on the left.
    let flipped = [n.clone(), i.clone()];
    assert_eq!(domain.refine(greater, &flipped, true, 1), Some(Type::Ranged(0, 999)));
    // The other operand learns too: i < n with n a range raises n above i's floor.
    let range = [Type::int(5, 5), Type::int(0, 100)];
    assert_eq!(domain.refine(less, &range, true, 1), Some(Type::Ranged(6, 100)));
    // An infeasible edge teaches nothing rather than inventing a type.
    assert_eq!(domain.refine(less, &[Type::int(5, 5), Type::int(3, 3)], true, 0), None);
    // Operands that are not both integers teach nothing: `NaN < 1` is false and so is
    // `NaN >= 1`, which is why "not less" is only "at least" over integers.
    assert_eq!(domain.refine(less, &[Type::Double, n.clone()], false, 0), None);
    assert_eq!(domain.refine(less, &[Type::Anything, n], true, 0), None);
    // And a truth value restates its operand's.
    assert_eq!(
        domain.forwards_truth(domain.prim(JsPrim::Truthy)),
        Some(Forward::Same)
    );
    assert_eq!(
        domain.forwards_truth(domain.prim(JsPrim::Not)),
        Some(Forward::Negated)
    );
    assert_eq!(domain.forwards_truth(less), None);
}
