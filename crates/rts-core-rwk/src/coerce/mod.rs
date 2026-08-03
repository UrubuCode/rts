//! Turning one kind of value into another.
//!
//! # This module never calls anything
//!
//! `ToPrimitive` on an object runs `valueOf` or `toString`, which is user code.
//! Nothing here can call, and it should not learn how — so an operation that
//! needs a primitive and is handed an object returns [`Needs`] saying *which
//! operand* and *with which hint*, and the caller — which can call — resolves it
//! and asks again.
//!
//! That is the same shape as `Found` in [`crate::object`], for the same reason:
//! what is easy to get wrong is not performing the conversion, it is performing
//! it on the right operand in the right order.
//!
//! # The order is the trap, twice
//!
//! Both of the operations here coerce their operands in an order that is not the
//! order they are written, and both are invisible until an operand has a
//! side-effecting `valueOf`:
//!
//! - `+` coerces left then right, and then decides string-or-number from **what
//!   came back** rather than from what it was given.
//! - `a <= b` is specified as `!(b < a)`, so it coerces the **right** operand
//!   first.
//!
//! So neither is exposed as a function taking two values and doing everything.
//! [`add_operand_order`] and [`relational_operand_order`] state the order, and
//! the operations themselves take primitives — which makes performing them out
//! of order something a caller has to do deliberately rather than by default.

mod number;

pub use number::{number_to_string, string_to_number};

use crate::heap::Slot;
use crate::text::Str;
use crate::value::{Kind, Value};

/// Which operand.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    /// The one written first.
    Left,
    /// The one written second.
    Right,
}

/// What `ToPrimitive` should prefer when an object defines both methods.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hint {
    /// No preference stated. What `+` and `==` pass.
    ///
    /// Not the same as [`Hint::Number`], even though an object without a
    /// `Symbol.toPrimitive` treats them alike. An object that defines one
    /// receives the string `"default"` and may branch on it — `Date` does
    /// exactly that, and reads it as `"string"`.
    Default,
    /// `toString` first. What a template literal and a property key pass.
    String,
    /// `valueOf` first. What arithmetic and relational comparison pass.
    Number,
}

/// An operation that cannot finish without user code running first.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Needs {
    /// Which operand is an object.
    pub side: Side,
    /// Which object.
    pub object: Slot,
    /// What to prefer.
    pub hint: Hint,
}

/// The order `+` coerces its operands in.
///
/// Left, then right — and completely: the left operand's `valueOf` runs before
/// the right operand is looked at. A caller that evaluated both to primitives in
/// whatever order was convenient would produce the right answer for every value
/// without a side effect, and the wrong sequence of side effects for the ones
/// with.
pub fn add_operand_order() -> [Side; 2] {
    [Side::Left, Side::Right]
}

/// `+`, on operands that are already primitives.
///
/// # It decides after coercion, not before
///
/// The string-or-number branch reads what `ToPrimitive` **returned**, not what
/// the operator was given. `[] + {}` concatenates — to `"[object Object]"` —
/// although neither operand is a string, because both became one. Testing the
/// original operands is a different algorithm that happens to agree on the easy
/// cases.
///
/// Returns `None` when either side is not a primitive, which a caller reaches by
/// resolving [`Needs`] first.
pub fn add(left: Value, right: Value, as_string: impl Fn(Value) -> Option<Str>) -> Option<Sum> {
    let (left_string, right_string) = (as_string(left), as_string(right));

    // Either side being a string makes this concatenation — decided here, on
    // the coerced values.
    if left_string.is_some() || right_string.is_some() {
        let left_text = left_string.or_else(|| as_string(left))?;
        let right_text = right_string.or_else(|| as_string(right))?;
        return Some(Sum::Text(left_text.concat(&right_text)));
    }

    Some(Sum::Number(left.numeric()? + right.numeric()?))
}

/// What `+` produced.
#[derive(Clone, PartialEq, Debug)]
pub enum Sum {
    /// Two numbers added.
    Number(f64),
    /// Two strings joined.
    Text(Str),
}

/// The four operators specified in terms of `<`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Relational {
    /// `<`.
    Less,
    /// `<=`.
    LessEqual,
    /// `>`.
    Greater,
    /// `>=`.
    GreaterEqual,
}

/// The order a relational operator coerces its operands in.
///
/// Two of the four coerce the **right** operand first, because they are
/// specified in terms of `<` with the operands swapped: `a <= b` is
/// `!(b < a)` and `a > b` is `b < a`.
///
/// The operands are still *evaluated* left to right — this is only about when
/// each one's `valueOf` runs, which is invisible until both have one.
pub fn relational_operand_order(op: Relational) -> [Side; 2] {
    match op {
        Relational::Less | Relational::GreaterEqual => [Side::Left, Side::Right],
        Relational::LessEqual | Relational::Greater => [Side::Right, Side::Left],
    }
}

/// A relational comparison of two primitives.
///
/// `None` means *unordered*, which happens when either side is `NaN`. All four
/// operators read that as `false`, which is why `NaN <= NaN` is false rather
/// than the negation of `NaN > NaN` being true.
///
/// Two strings compare by UTF-16 code unit, not by locale and not by code point
/// — so a surrogate pair compares as its two units.
pub fn relational(
    op: Relational,
    left: Value,
    right: Value,
    as_string: impl Fn(Value) -> Option<Str>,
) -> Option<bool> {
    let less = |a: Value, b: Value| -> Option<bool> {
        match (as_string(a), as_string(b)) {
            // Both strings: code-unit order.
            (Some(x), Some(y)) => Some(x.units().lt(y.units())),
            // Anything else: numbers, and NaN makes it unordered.
            _ => {
                let (x, y) = (a.numeric()?, b.numeric()?);
                if x.is_nan() || y.is_nan() {
                    return None;
                }
                Some(x < y)
            }
        }
    };

    match op {
        Relational::Less => less(left, right),
        Relational::Greater => less(right, left),
        Relational::LessEqual => less(right, left).map(|greater| !greater),
        Relational::GreaterEqual => less(left, right).map(|smaller| !smaller),
    }
}

/// Whether a value is already a primitive, or needs `ToPrimitive` first.
pub fn needs_primitive(value: Value, side: Side, hint: Hint) -> Option<Needs> {
    match value.kind() {
        Kind::Reference(slot) => Some(Needs {
            side,
            object: Slot(slot as u32),
            hint,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stands in for "this value is a string", which needs a heap this module
    /// does not have. The tests supply it, which is exactly how the real caller
    /// will.
    fn strings(pairs: Vec<(u64, &'static str)>) -> impl Fn(Value) -> Option<Str> {
        move |value: Value| {
            pairs
                .iter()
                .find(|(bits, _)| *bits == value.bits())
                .map(|(_, text)| Str::from_str(text))
        }
    }

    #[test]
    fn add_decides_from_what_coercion_returned_and_not_from_what_it_was_given() {
        // Two objects that both coerced to strings. Neither original was one.
        let left = Value::from_slot(1);
        let right = Value::from_slot(2);
        let as_string = strings(vec![(left.bits(), ""), (right.bits(), "[object Object]")]);

        assert_eq!(
            add(left, right, &as_string),
            Some(Sum::Text(Str::from_str("[object Object]"))),
            "[] + {{}} concatenates, though neither operand is a string"
        );
    }

    #[test]
    fn add_on_two_numbers_is_arithmetic() {
        let none = strings(vec![]);
        assert_eq!(
            add(Value::from_i32(2), Value::from_i32(3), &none),
            Some(Sum::Number(5.0))
        );
        assert_eq!(
            add(Value::from_f64(0.1), Value::from_f64(0.2), &none),
            Some(Sum::Number(0.1 + 0.2))
        );
    }

    #[test]
    fn one_string_is_enough_to_make_it_concatenation() {
        let text = Value::from_slot(1);
        let as_string = strings(vec![(text.bits(), "n=")]);

        // The number side has no string spelling here, so `add` cannot finish —
        // which is the honest answer: converting it needs the heap.
        assert_eq!(add(text, Value::from_i32(1), &as_string), None);
    }

    #[test]
    fn two_of_the_four_relational_operators_coerce_the_right_side_first() {
        assert_eq!(
            relational_operand_order(Relational::Less),
            [Side::Left, Side::Right]
        );
        assert_eq!(
            relational_operand_order(Relational::GreaterEqual),
            [Side::Left, Side::Right]
        );
        assert_eq!(
            relational_operand_order(Relational::LessEqual),
            [Side::Right, Side::Left],
            "a <= b is !(b < a), so b's valueOf runs first"
        );
        assert_eq!(
            relational_operand_order(Relational::Greater),
            [Side::Right, Side::Left]
        );
    }

    #[test]
    fn nan_is_unordered_and_every_operator_reads_that_as_false() {
        let none = strings(vec![]);
        let nan = Value::from_f64(f64::NAN);

        for op in [
            Relational::Less,
            Relational::LessEqual,
            Relational::Greater,
            Relational::GreaterEqual,
        ] {
            assert_eq!(
                relational(op, nan, nan, &none),
                None,
                "{op:?} on NaN is unordered, not false-because-negated"
            );
        }
    }

    #[test]
    fn the_four_operators_agree_with_arithmetic_where_nothing_is_nan() {
        let none = strings(vec![]);
        let one = Value::from_i32(1);
        let two = Value::from_i32(2);

        assert_eq!(relational(Relational::Less, one, two, &none), Some(true));
        assert_eq!(relational(Relational::Less, two, one, &none), Some(false));
        assert_eq!(
            relational(Relational::LessEqual, one, one, &none),
            Some(true)
        );
        assert_eq!(relational(Relational::Greater, two, one, &none), Some(true));
        assert_eq!(
            relational(Relational::GreaterEqual, one, one, &none),
            Some(true)
        );
    }

    #[test]
    fn two_strings_compare_by_code_unit_and_not_as_numbers() {
        let a = Value::from_slot(1);
        let b = Value::from_slot(2);
        let as_string = strings(vec![(a.bits(), "10"), (b.bits(), "9")]);

        assert_eq!(
            relational(Relational::Less, a, b, &as_string),
            Some(true),
            "\"10\" < \"9\" because '1' precedes '9' — string comparison, not \
             numeric"
        );
    }

    #[test]
    fn an_object_operand_says_which_side_and_which_hint() {
        let object = Value::from_slot(7);
        let needs = needs_primitive(object, Side::Left, Hint::Number).unwrap();

        assert_eq!(needs.side, Side::Left);
        assert_eq!(needs.object, Slot(7));
        assert_eq!(needs.hint, Hint::Number);

        assert!(
            needs_primitive(Value::from_i32(1), Side::Left, Hint::Number).is_none(),
            "a number is already a primitive"
        );
    }

    #[test]
    fn default_is_not_number_even_though_it_usually_behaves_like_it() {
        assert_ne!(
            Hint::Default,
            Hint::Number,
            "an object defining Symbol.toPrimitive receives the string \
             \"default\" and may branch on it — Date reads it as \"string\""
        );
    }

    #[test]
    fn add_coerces_left_before_right() {
        assert_eq!(
            add_operand_order(),
            [Side::Left, Side::Right],
            "and completely: the left operand's valueOf runs before the right is \
             looked at"
        );
    }
}
