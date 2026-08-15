//! Constant folding over literals, at emit time.
//!
//! # Why this exists, and it is not "optimisation"
//!
//! A runtime call site costs roughly 100 microseconds of Cranelift codegen —
//! measured, not estimated: four otherwise-identical 400-statement programs,
//! differing only in how many calls each emits, took 3.3 ms with none and
//! 199.6 ms with four apiece. `1 + 2` with both operands proven numbers still
//! reaches [`super::expr::emit_binary`] and (absent this module) still emits
//! a call, because nothing upstream of it distinguishes "proven number" from
//! "known at compile time". Folding here removes the call site entirely
//! rather than making the call faster.
//!
//! # Why this lives apart from `emit_binary`, and not inside it
//!
//! `emit_binary` operates on [`ValueId`](rts_cranelift::ir::ValueId)s — values
//! already materialised in the function being built. A fold has to happen
//! *before* that: on the [`Literal`]s the tree holds, so that when both sides
//! fold, the operand expressions are never emitted at all and no `ValueId`
//! for them ever exists. This is a pure function over the tree's own literal
//! type, deliberately without a `FuncBuilder` or a `Ctx` — it needs neither,
//! and giving it either would let it drift into deciding a machine question,
// which rule 2 forbids.
//!
//! # What is folded, and what is deliberately not
//!
//! Only **both operands already literal**. `a + b` where `a` is a local the
//! type pass proved numeric is not reached here — proving that fold is safe
//! needs knowing `a` has no side effect on every path to this point, which is
//! a data-flow question this module has no way to ask and is not the one
//! rule 3 assigns it. A caller with a proven-constant local will want this
//! one day; it needs its own analysis, not an extension of this file.
//!
//! `+` folds only number-with-number and string-with-string. A mixed pair
//! (`"a" + 1`) is decided by `ToPrimitive` on both operands and then by what
//! *came back*, not by their static types — `[] + {}` concatenates and
//! neither operand is a string. Getting that right for a literal pair is
//! possible (neither literal is ever an object) but not worth the surface: a
//! mixed literal pair is rare, and folding it needs restating `+`'s decision
//! procedure a second place, which rule 3 exists to prevent. So it is left as
//! a call, correctly.

use crate::syntax::{BinaryOp, Literal};

/// Folds a binary operator over two literals, when the result can be computed
/// here and expressed as one literal.
///
/// `None` means "emit the operator as usual" — either because this operator
/// is not one of the ones folded, or because the specific operands (a mixed
/// `+`, anything that is not two numbers or two strings) are outside the set
/// this module answers for.
pub(super) fn fold_binary(op: BinaryOp, left: &Literal, right: &Literal) -> Option<Literal> {
    match (left, right) {
        (Literal::Number(a), Literal::Number(b)) => fold_numeric(op, *a, *b),
        // Concatenation only, and over CODE UNITS rather than through Rust
        // text: `"\uD83D" + "\uDE00"` folds to the pair, and a fold that went
        // through `format!` would have folded two replacement characters. The
        // result is still a `Literal::String` at this point, and it goes
        // through the ordinary string-literal path (the interned-text table)
        // exactly as if it had been typed that way.
        (Literal::String(a), Literal::String(b)) if op == BinaryOp::Add => {
            Some(Literal::String(a.concat(b)))
        }
        _ => None,
    }
}

/// Folds unary `-` over a literal number.
///
/// Only a number: `-"3"` coerces through `ToNumber` first, and `-{}` throws
/// after trying `ToPrimitive` — neither is a fold over the literal alone.
pub(super) fn fold_negate(operand: &Literal) -> Option<Literal> {
    match operand {
        // Rust's unary `-` on `f64` flips the sign bit and nothing else, which
        // is exactly `Number::unaryMinus`: `-(0.0)` is `-0.0`, `-(-0.0)` is
        // `0.0`, `-(f64::INFINITY)` is `-f64::INFINITY`, `-NaN` is `NaN`. The
        // engine's own emitted instruction for `-x` deliberately avoids `0 -
        // x` for the same reason (`0.0 - 0.0` is `+0`, losing the sign), and
        // folding a literal has no reason to make that mistake either.
        Literal::Number(n) => Some(Literal::Number(-n)),
        _ => None,
    }
}

/// Two numbers, `+ - * / %` and the four relational operators, `===`/`!==`,
/// and the bitwise operators.
fn fold_numeric(op: BinaryOp, a: f64, b: f64) -> Option<Literal> {
    match op {
        BinaryOp::Add => Some(Literal::Number(a + b)),
        BinaryOp::Sub => Some(Literal::Number(a - b)),
        BinaryOp::Mul => Some(Literal::Number(a * b)),
        // `1 / 0` is `Infinity`, `0 / 0` is `NaN` — IEEE division, which is
        // exactly what JavaScript division is (there is no integer division
        // to fall back to, and no operand is ever a divide-by-zero error).
        // Rust's `f64::/` already behaves this way; no zero check is added.
        BinaryOp::Div => Some(Literal::Number(a / b)),
        // `%` is a remainder — the result takes the sign of the DIVIDEND, not
        // a mathematical modulo. Rust's `f64::%` (`std::ops::Rem`) is defined
        // the same way, so this is direct rather than adjusted. `-5.0 % 3.0`
        // is `-2.0` in both, pinned in the tests below.
        BinaryOp::Rem => Some(Literal::Number(a % b)),

        // NaN is unordered: every one of the four relational operators must
        // answer `false` when either operand is NaN, INCLUDING `<=` and `>=`
        // — `!(a > b)` would get those two wrong, because negating a false
        // "greater" gives `true` for a NaN comparison that must be `false`.
        // Rust's `f64` comparison operators already implement IEEE 754
        // unordered comparison (any comparison with NaN is `false`), so using
        // them directly — never composing one from another's negation — is
        // what keeps this correct. Pinned in the tests below.
        BinaryOp::Less => Some(Literal::Boolean(a < b)),
        BinaryOp::LessEqual => Some(Literal::Boolean(a <= b)),
        BinaryOp::Greater => Some(Literal::Boolean(a > b)),
        BinaryOp::GreaterEqual => Some(Literal::Boolean(a >= b)),

        // `===` on two numbers is IEEE equality: `NaN === NaN` is `false` and
        // `+0 === -0` is `true`. Rust's `f64::eq` (what `==` calls) already
        // answers exactly that — it is IEEE 754 equality, not a bit compare —
        // so this is `a == b` directly rather than any bit-pattern comparison.
        // Using `to_bits() ==` here would get BOTH cases backwards.
        BinaryOp::StrictEqual => Some(Literal::Boolean(a == b)),
        BinaryOp::StrictNotEqual => Some(Literal::Boolean(a != b)),

        BinaryOp::BitAnd => Some(Literal::Number((to_int32(a) & to_int32(b)) as f64)),
        BinaryOp::BitOr => Some(Literal::Number((to_int32(a) | to_int32(b)) as f64)),
        BinaryOp::BitXor => Some(Literal::Number((to_int32(a) ^ to_int32(b)) as f64)),
        // A shift count keeps only its low 5 bits: `1 << 32` is `1`, not `0`.
        BinaryOp::Shl => {
            let shift = (to_int32(b) as u32) & 0x1f;
            Some(Literal::Number((to_int32(a) << shift) as f64))
        }
        BinaryOp::Shr => {
            let shift = (to_int32(b) as u32) & 0x1f;
            Some(Literal::Number((to_int32(a) >> shift) as f64))
        }
        // `>>>` reads its left operand as UNSIGNED and its result can outgrow
        // a signed 32-bit value: `(-1) >>> 0` is `4294967295`, not `-1`. This
        // is the one bitwise fold whose result is `to_uint32`, not `to_int32`.
        BinaryOp::UShr => {
            let shift = (to_int32(b) as u32) & 0x1f;
            Some(Literal::Number((to_uint32(a) >> shift) as f64))
        }

        _ => None,
    }
}

/// `ToInt32`, restated.
///
/// This crate depends on nothing below `rts-cranelift` — rule 1 — so it
/// cannot call `rts-core/src/value/convert.rs::to_int32`, the runtime's
/// implementation, even though this function must answer exactly what that
/// one answers: a fold that disagrees with the runtime is worse than no fold,
/// because it makes one program behave two ways depending on whether the
/// compiler could see through an expression. This is a second statement of
/// ONE rule, not a second rule, and every disagreement-prone case the
/// runtime's own doc comment names is pinned in the tests below:
///
/// - Not `as i32`: Rust's numeric cast SATURATES, so `2147483648.0 as i32`
///   would be `i32::MAX` where the specification wants `i32::MIN`.
/// - Not `%`: Rust's floating remainder keeps the sign of the dividend, which
///   disagrees with the specification's `modulo` for a negative input.
///   `rem_euclid` is what "modulo" means here.
/// - Non-finite (`NaN`, `±Infinity`) becomes `0`.
fn to_int32(number: f64) -> i32 {
    if !number.is_finite() {
        return 0;
    }
    let truncated = number.trunc();
    let wrapped = truncated.rem_euclid(4_294_967_296.0);
    wrapped as u32 as i32
}

/// `ToUint32`: what the left operand of `>>>` goes through.
fn to_uint32(number: f64) -> u32 {
    to_int32(number) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(value: f64) -> Literal {
        Literal::Number(value)
    }

    fn fold(op: BinaryOp, a: f64, b: f64) -> Literal {
        fold_binary(op, &n(a), &n(b)).expect("this pair folds")
    }

    #[test]
    fn nan_is_unordered_in_all_four_relational_operators() {
        // Every one of these must be `false` — including `<=` and `>=`, which
        // a fold written as `!(a > b)` gets backwards.
        assert_eq!(fold(BinaryOp::Less, f64::NAN, 1.0), Literal::Boolean(false));
        assert_eq!(
            fold(BinaryOp::Greater, f64::NAN, 1.0),
            Literal::Boolean(false)
        );
        assert_eq!(
            fold(BinaryOp::LessEqual, f64::NAN, 1.0),
            Literal::Boolean(false),
            "NaN <= 1 is false, not !(1 < NaN)"
        );
        assert_eq!(
            fold(BinaryOp::GreaterEqual, f64::NAN, 1.0),
            Literal::Boolean(false),
            "NaN >= 1 is false, not !(NaN < 1)"
        );
        // And the reverse operand order, since NaN is unordered either side.
        assert_eq!(fold(BinaryOp::Less, 1.0, f64::NAN), Literal::Boolean(false));
        assert_eq!(
            fold(BinaryOp::LessEqual, 1.0, f64::NAN),
            Literal::Boolean(false)
        );
    }

    #[test]
    fn strict_equal_on_numbers_is_ieee_equality_not_a_bit_compare() {
        assert_eq!(
            fold(BinaryOp::StrictEqual, 0.0, -0.0),
            Literal::Boolean(true),
            "+0 === -0"
        );
        assert_eq!(
            fold(BinaryOp::StrictEqual, f64::NAN, f64::NAN),
            Literal::Boolean(false),
            "NaN === NaN is false"
        );
        assert_eq!(
            fold(BinaryOp::StrictNotEqual, f64::NAN, f64::NAN),
            Literal::Boolean(true),
            "NaN !== NaN is true"
        );
    }

    #[test]
    fn division_by_zero_is_infinity_or_nan_not_refused() {
        let one_over_zero = fold(BinaryOp::Div, 1.0, 0.0);
        assert_eq!(one_over_zero, Literal::Number(f64::INFINITY), "1 / 0");

        let zero_over_zero = fold(BinaryOp::Div, 0.0, 0.0);
        let Literal::Number(result) = zero_over_zero else {
            panic!("expected a number");
        };
        assert!(result.is_nan(), "0 / 0 is NaN");
    }

    #[test]
    fn remainder_takes_the_sign_of_the_dividend() {
        assert_eq!(fold(BinaryOp::Rem, -5.0, 3.0), Literal::Number(-2.0));
    }

    #[test]
    fn to_int32_wraps_modulo_where_a_cast_would_saturate() {
        assert_eq!(
            fold(BinaryOp::BitOr, 4_294_967_296.0, 0.0),
            Literal::Number(0.0),
            "2**32 | 0 is 0"
        );
        assert_eq!(
            fold(BinaryOp::BitOr, 2_147_483_648.0, 0.0),
            Literal::Number(i32::MIN as f64),
            "2**31 | 0 is i32::MIN, not i32::MAX — `as i32` would saturate"
        );
    }

    #[test]
    fn string_concatenation_folds_to_one_literal() {
        assert_eq!(
            fold_binary(
                BinaryOp::Add,
                &Literal::String("foo".into()),
                &Literal::String("bar".into())
            ),
            Some(Literal::String("foobar".into()))
        );
    }

    #[test]
    fn a_mixed_add_does_not_fold() {
        // `"a" + 1` decides after ToPrimitive on both sides, not from static
        // types — folding it here would be restating that decision.
        assert_eq!(
            fold_binary(BinaryOp::Add, &Literal::String("a".into()), &n(1.0)),
            None
        );
    }

    #[test]
    fn negate_flips_the_sign_bit_including_zero() {
        let Some(Literal::Number(negated)) = fold_negate(&n(0.0)) else {
            panic!("expected a number");
        };
        assert!(negated.is_sign_negative(), "-(0) is -0");

        let Some(Literal::Number(back)) = fold_negate(&n(negated)) else {
            panic!("expected a number");
        };
        assert!(back.is_sign_positive(), "-(-0) is +0");
    }

    #[test]
    fn negate_of_a_string_does_not_fold() {
        assert_eq!(fold_negate(&Literal::String("3".into())), None);
    }
}
