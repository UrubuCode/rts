//! The arithmetic and relational operators.
//!
//! # Why these are entry points at all
//!
//! `a - b` looks like arithmetic and is not, quite. It is
//! `ToNumber(ToPrimitive(a)) - ToNumber(ToPrimitive(b))`, and `ToNumber` of a
//! string reads that string's text out of the heap. So the membership rule
//! applies for the ordinary reason — *touches the heap* — and it is the
//! conversion rather than the subtraction that makes it true.
//!
//! Which is also why the subtraction itself is not interesting here. Once both
//! operands are numbers, every one of these is one machine instruction, and a
//! type pass that proves both sides are numbers should emit that instruction
//! instead of this call. These exist for the case where nothing was proved.
//!
//! # `+` is not here, and that is not an oversight
//!
//! It lives beside the others in [`super`] because it is the one operator of the
//! group that is not arithmetic: it decides between adding and concatenating
//! from what its operands turn out to be, and concatenating allocates. Every
//! operator in this file converts to a number and has no second answer.

use super::bigint_class::{Op, Relation};
use super::{Context, with_current};
use crate::coerce::{Relational, relational};
use crate::value::{Value, to_number};

/// `ToNumber`, including the part that needs the heap.
///
/// [`to_number`] answers for everything a machine word can settle and returns
/// `None` for a reference, because a reference might be a string — which
/// converts by reading its text — or an object, which converts by running a
/// `valueOf` and is therefore a call rather than a conversion.
///
/// This resolves the first case and leaves the second absent, which keeps the
/// distinction the caller needs: a string is finished here, an object is not.
pub(super) fn as_number(context: &Context, value: Value) -> Option<f64> {
    if let Some(number) = to_number(value, context.singletons) {
        return Some(number);
    }
    let text = context.text_at(value.as_slot()?)?;
    Some(crate::coerce::string_to_number(text))
}

/// Both operands as numbers, or `NaN` if either still needed `ToPrimitive`.
///
/// # Why `NaN` rather than a panic
///
/// The compiler is supposed to have resolved `ToPrimitive` before calling here,
/// because running a `valueOf` is calling user code and an entry point cannot do
/// that. Reaching this with an object is a contract violation.
///
/// `NaN` is what the operation would have produced for an object with no useful
/// `valueOf` anyway, so it is the answer least likely to be mistaken for a
/// correct one — and unlike a panic, it does not turn a compiler defect into a
/// dead process while the compiler is being written.
pub(super) fn operands(context: &Context, left: Value, right: Value) -> (f64, f64) {
    (
        as_number(context, left).unwrap_or(f64::NAN),
        as_number(context, right).unwrap_or(f64::NAN),
    )
}

/// The order every operator in this file converts its operands in.
///
/// Left then right, which is what `coerce::add_operand_order` states for `+`.
/// The same order, taken from the same place rather than written a second time:
/// the two have no reason to differ and every reason to be got wrong separately.
fn operand_order() -> [crate::coerce::Side; 2] {
    crate::coerce::add_operand_order()
}

/// `a - b`.
#[rtse::entry]
pub fn subtract(left: u64, right: u64) -> u64 {
    let (left, right) = super::primitive::operands(
        left,
        right,
        operand_order(),
        crate::coerce::Hint::Number,
    );
    with_current(|context| {
        // `settled` inside the borrow is safe for THIS op and not in general:
        // only `<<`, `>>` and `**` refuse, and they ask in a borrow of their own
        // for that reason (see `bitwise.rs`). The one that would break this is
        // division by zero — it answers `undefined` today where the language
        // throws, and making it raise means moving these five calls out of the
        // borrow first. Raising in place aborts; it does not fail.
        if let Some(outcome) = super::bigint_class::binary(context, Op::Sub, left, right) {
            return super::bigint_class::settled(outcome);
        }
        let (a, b) = operands(context, Value(left), Value(right));
        Value::from_f64(a - b).bits()
    })
}

/// `a * b`.
#[rtse::entry]
pub fn multiply(left: u64, right: u64) -> u64 {
    let (left, right) = super::primitive::operands(
        left,
        right,
        operand_order(),
        crate::coerce::Hint::Number,
    );
    with_current(|context| {
        if let Some(outcome) = super::bigint_class::binary(context, Op::Mul, left, right) {
            return super::bigint_class::settled(outcome);
        }
        let (a, b) = operands(context, Value(left), Value(right));
        Value::from_f64(a * b).bits()
    })
}

/// `a / b`.
///
/// No check for a zero divisor, and none is missing: JavaScript divides in
/// IEEE-754, where `1 / 0` is `Infinity` and `0 / 0` is `NaN`. A guard here
/// would be replacing the language's answer with a different one.
#[rtse::entry]
pub fn divide(left: u64, right: u64) -> u64 {
    let (left, right) = super::primitive::operands(
        left,
        right,
        operand_order(),
        crate::coerce::Hint::Number,
    );
    with_current(|context| {
        if let Some(outcome) = super::bigint_class::binary(context, Op::Div, left, right) {
            return super::bigint_class::settled(outcome);
        }
        let (a, b) = operands(context, Value(left), Value(right));
        Value::from_f64(a / b).bits()
    })
}

/// `a % b`.
///
/// Remainder, not modulo. The result takes the sign of the **dividend**, so
/// `-5 % 3` is `-2` and not `1`. Rust's `%` on `f64` is the same operation, so
/// this is a delegation rather than an implementation — worth saying, because
/// the two differ in most languages and agreeing here is luck that should be
/// recorded rather than relied on silently.
#[rtse::entry]
pub fn remainder(left: u64, right: u64) -> u64 {
    let (left, right) = super::primitive::operands(
        left,
        right,
        operand_order(),
        crate::coerce::Hint::Number,
    );
    with_current(|context| {
        if let Some(outcome) = super::bigint_class::binary(context, Op::Rem, left, right) {
            return super::bigint_class::settled(outcome);
        }
        let (a, b) = operands(context, Value(left), Value(right));
        Value::from_f64(a % b).bits()
    })
}

/// The shared body of the four relational operators.
///
/// # Why they are four entry points and not one with an operand
///
/// An operand would be a fifth argument at every call site, carrying a constant
/// the compiler already knows. Four symbols cost four rows in a table that is
/// read in one screen, and each call site passes only what varies.
fn compare(op: Relational, left: u64, right: u64) -> bool {
    // Outside the borrow, and in the operators own order: `a <= b` is specified
    // as `!(b < a)`, so it converts the RIGHT operand first. That is invisible
    // until an operand has a side-effecting `valueOf`, which is why the order
    // comes from `coerce` rather than being assumed to be left-to-right here.
    let (left, right) = super::primitive::operands(
        left,
        right,
        crate::coerce::relational_operand_order(op),
        crate::coerce::Hint::Number,
    );
    with_current(|context| {
        // Asked before the closures below, and before conversion: a bigint and
        // a number DO compare in the language — only *arithmetic* between them
        // is refused — but comparing them as doubles loses every value past
        // 2^53, which is the range a bigint exists for.
        let want = match op {
            Relational::Less => Relation::Less,
            Relational::LessEqual => Relation::LessEqual,
            Relational::Greater => Relation::Greater,
            Relational::GreaterEqual => Relation::GreaterEqual,
        };
        if let Some(outcome) = super::bigint_class::binary(context, Op::Compare(want), left, right) {
            return Value(super::bigint_class::settled(outcome))
                .as_bool()
                .unwrap_or(false);
        }

        let text_of = |value: Value| {
            value
                .as_slot()
                .and_then(|slot| context.text_at(slot))
                .cloned()
        };
        // `None` is *unordered*, which is what `NaN` produces on either side.
        // All four operators read it as false — which is why `NaN <= NaN` is
        // false rather than the negation of `NaN > NaN` being true.
        // Both closures come from the same context: one reads a string's text,
        // the other converts one to a number. A relational operator needs both
        // because it decides between them from what the operands are.
        let number_of = |value: Value| as_number(context, value);
        relational(op, Value(left), Value(right), text_of, number_of).unwrap_or(false)
    })
}

/// `a < b`.
#[rtse::entry]
pub fn less(left: u64, right: u64) -> bool {
    compare(Relational::Less, left, right)
}

/// `a <= b`.
#[rtse::entry]
pub fn less_equal(left: u64, right: u64) -> bool {
    compare(Relational::LessEqual, left, right)
}

/// `a > b`.
#[rtse::entry]
pub fn greater(left: u64, right: u64) -> bool {
    compare(Relational::Greater, left, right)
}

/// `a >= b`.
#[rtse::entry]
pub fn greater_equal(left: u64, right: u64) -> bool {
    compare(Relational::GreaterEqual, left, right)
}

#[cfg(test)]
mod tests {
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
}

/// `a == b`.
///
/// # Why this waited for a client
///
/// Loose equality is the operation whose specification most often reaches
/// `ToPrimitive`, and `ToPrimitive` on an object runs a `valueOf` — user code,
/// which an entry point cannot call. So it was left out until something needed
/// it, rather than being written with the interesting half missing and looking
/// finished.
///
/// # What it does, and the one case it declines
///
/// Everything that does not need a call: two values of the same kind are
/// compared strictly, `null` and `undefined` are equal to each other and to
/// nothing else, and every remaining pair is compared **as numbers** — which is
/// what the specification's table reduces to once objects are set aside, since
/// a boolean converts to a number and a string compared against a number
/// converts too.
///
/// An object against a primitive is converted first, by
/// [`super::primitive::to_primitive`] — which is what "waited for a client"
/// above was waiting for. It used to answer `false`, so `[] == 0` and `[] == ""`
/// were both wrong.
///
/// **Only when exactly one side is an object.** Two objects compare by identity
/// and must never be converted: `{} == {}` is false, and a pair converted first
/// would compare `"[object Object]"` against itself and answer true.
#[rtse::entry]
pub fn loose_equals(left: u64, right: u64) -> bool {
    // Outside the borrow, and before anything else reads the operands: a
    // conversion runs user code. Guarded so the identity rule above survives.
    let (left_object, right_object) = with_current(|context| {
        (
            super::primitive::is_object_in(context, left),
            super::primitive::is_object_in(context, right),
        )
    });
    let hint = crate::coerce::Hint::Default;
    let (left, right) = match (left_object, right_object) {
        (true, false) => (super::primitive::to_primitive(left, hint), right),
        (false, true) => (left, super::primitive::to_primitive(right, hint)),
        _ => (left, right),
    };

    with_current(|context| {
        let (left, right) = (Value(left), Value(right));

        let absent = |value: Value| {
            matches!(value.kind(), crate::value::Kind::Singleton(number)
                if number == context.singletons.undefined
                    || number == context.singletons.null)
        };
        if absent(left) || absent(right) {
            // `null == undefined` is true and neither is equal to anything
            // else — the one rule that is not a conversion.
            return absent(left) && absent(right);
        }

        // Same kind is strict equality, which also covers two strings (equal
        // when their text is) and two objects (equal when they are the same
        // one).
        if std::mem::discriminant(&left.kind()) == std::mem::discriminant(&right.kind()) {
            return crate::value::strict_equals(left, right, |a, b| context.same_text(a, b));
        }

        match (as_number(context, left), as_number(context, right)) {
            (Some(a), Some(b)) => a == b,
            // One of them is an object, so the answer needed `ToPrimitive`.
            _ => false,
        }
    })
}
