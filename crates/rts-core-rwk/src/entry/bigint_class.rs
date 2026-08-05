//! `BigInt` — the constructor, the prototype, and the arithmetic behind them.
//!
//! # Why the arithmetic is here and not in the operators
//!
//! `a + b` reaches [`super::primitives::add`], which converts to primitives and
//! either adds doubles or joins text. A bigint is neither, and the language is
//! emphatic about the difference: **mixing a bigint with a number is a
//! `TypeError`**, not a coercion. `1n + 1` does not answer `2`.
//!
//! That rule is the whole reason bigint arithmetic cannot be folded into the
//! numeric path as one more coercion. It is a separate case that has to be
//! recognised before conversion, which is what the operator entry points now do:
//! ask this module first, and fall through only when neither side is one.
//!
//! # What a mixed operation answers, since it cannot throw
//!
//! `NaN`, and that is a stated divergence rather than a choice: `entry/throw.rs`
//! ends the program, so a `TypeError` here would kill a program over one
//! expression. `NaN` is what the operation would have produced if the bigint had
//! converted and failed, so it is the least surprising wrong answer available.
//!
//! **`===` is the exception and it is exact.** `1n === 1` is false and `1n === 1n`
//! is true, and neither needs to throw — so equality has no divergence at all.
//!
//! # What is deliberately absent
//!
//! A `BigInt` **wrapper object**. `Object(1n)` makes one in the language and
//! nothing here does, for the reason `String` records: a wrapper that compared
//! equal to a primitive everywhere except where it did not is the kind of wrong
//! that is hard to find.

use super::objects::undefined_of;
use super::{Context, with_current};
use crate::bigint::BigInt;
use crate::value::Value;

/// `BigInt`.
#[rtse::class("BigInt")]
impl BigIntClass {
    /// `BigInt(x)` — from a number, a string, or a boolean.
    ///
    /// A non-integer number is a `RangeError` in the language; this answers
    /// `undefined`, which the module doc explains. `new BigInt(x)` is a
    /// `TypeError` there and answers the object `construct` made here, the same
    /// divergence `Number` has.
    #[construct]
    fn convert(this: u64, value: u64) -> u64 {
        let _ = this;
        with_current(|context| match parsed(context, value) {
            Some(held) => context.bigint_value(held),
            None => undefined_of(context),
        })
    }

    /// `BigInt.asIntN(bits, value)` — the low bits, read as signed.
    #[stat]
    fn as_int_n(bits: f64, value: u64) -> u64 {
        wrapped(bits, value, true)
    }

    /// `BigInt.asUintN(bits, value)` — the low bits, read as unsigned.
    #[stat]
    fn as_uint_n(bits: f64, value: u64) -> u64 {
        wrapped(bits, value, false)
    }

    /// `big.toString(radix)`.
    fn to_string(this: u64, radix: u64) -> u64 {
        let base = super::number::radix_of(radix);
        with_current(|context| {
            let Some(held) = super::bigints::digits_of(context, this) else {
                return undefined_of(context);
            };
            let text = match base {
                0 | 10 => held.to_decimal(),
                base if (2..=36).contains(&base) => held.to_radix(base as u32),
                // The specification throws a `RangeError`; answering the decimal
                // form is the least wrong of the values available.
                _ => held.to_decimal(),
            };
            context.intern_value(crate::text::Str::from_str(&text)).bits()
        })
    }

    /// `big.valueOf()` — the bigint itself.
    fn value_of(this: u64) -> u64 {
        this
    }

    /// `big.toLocaleString()` — the same digits as `toString`, since there is no
    /// locale data and one invented would be right only where it was tested.
    fn to_locale_string(this: u64) -> u64 {
        decimal(this)
    }
}

/// The digits of a receiver, in base ten.
///
/// Shared by `toLocaleString` and by the base-ten arm of `toString`, so the two
/// cannot come to disagree about how a negative sign is written.
fn decimal(this: u64) -> u64 {
    with_current(|context| match super::bigints::digits_of(context, this) {
        Some(held) => {
            let text = held.to_decimal();
            context.intern_value(crate::text::Str::from_str(&text)).bits()
        }
        None => undefined_of(context),
    })
}

/// The bigint a value converts to, as far as `BigInt(x)` goes.
///
/// `None` where the language throws: a non-integer double, a string that is not
/// a bigint literal, `undefined`, `null`, a symbol, an object.
fn parsed(context: &Context, value: u64) -> Option<BigInt> {
    if let Some(held) = super::bigints::digits_of(context, value) {
        return Some(held.clone());
    }
    if let Some(number) = Value(value).as_f64() {
        return BigInt::from_f64(number);
    }
    if let Some(flag) = Value(value).as_bool() {
        return Some(BigInt::from_i64(i64::from(flag)));
    }
    let cell = Value(value).as_slot()?;
    let text = context.text_at(cell)?.to_rust()?;
    // The empty string is `0n`, which is what `BigInt("")` answers and what a
    // parser rejecting empty input would have got wrong.
    match text.trim().is_empty() {
        true => Some(BigInt::zero()),
        false => BigInt::parse(text.trim()),
    }
}

/// `asIntN` and `asUintN`, which differ in one flag.
fn wrapped(bits: f64, value: u64, signed: bool) -> u64 {
    let width = match bits.is_finite() && bits >= 0.0 {
        true => bits.trunc() as u32,
        false => return with_current(|context| undefined_of(context)),
    };
    with_current(|context| {
        let Some(held) = super::bigints::digits_of(context, value).map(|held| held.wrap_to_bits(width, signed))
        else {
            return undefined_of(context);
        };
        context.bigint_value(held)
    })
}

/// A bigint value from a literal the compiler wrote down.
///
/// # Why the digits cross as an interned string rather than as bytes
///
/// A literal is arbitrary precision and an immediate is sixty-four bits, so
/// something has to carry the digits. It could be a slice — the ABI has one —
/// and it is a string **value** instead, which is the same shape a
/// regular-expression literal uses and chosen for the same reason: `BigInt("…")`
/// reaches this with a value, so one path serves both spellings. A slice would
/// serve the literal and have nothing to say about the other.
///
/// The text is interned once for the whole program, so a literal inside a loop
/// costs the parse and the digits per evaluation and not the text.
#[rtse::entry]
pub fn bigint_new(digits: u64) -> u64 {
    with_current(|context| {
        let Some(text) = Value(digits)
            .as_slot()
            .and_then(|cell| context.text_at(cell))
            .and_then(|text| text.to_rust())
        else {
            return undefined_of(context);
        };
        match BigInt::parse(&text) {
            Some(held) => context.bigint_value(held),
            // A literal the parser accepted and this could not read is a defect
            // in the wiring rather than anything a program can express.
            None => undefined_of(context),
        }
    })
}

/// What a binary operator does when either side is a bigint.
///
/// `None` means neither side is one, and the caller carries on with the numeric
/// path — which is what keeps every operation that has nothing to do with
/// bigints paying one comparison rather than a conversion.
pub(super) fn binary(context: &mut Context, op: Op, left: u64, right: u64) -> Option<u64> {
    let held = |value: u64| super::bigints::digits_of(context, value).cloned();
    let (a, b) = (held(left), held(right));
    if a.is_none() && b.is_none() {
        return None;
    }
    // A COMPARISON between a bigint and a number is legal — only arithmetic
    // between them is refused — so the other side is brought across rather than
    // rejected. Exactly, when it is an integer: converting the bigint to a
    // double instead would make `9007199254740993n > 9007199254740992` false,
    // which is the one range the type exists for.
    let (a, b) = match (a, b) {
        (Some(a), Some(b)) => (a, b),
        (a, b) if matches!(op, Op::Compare(_)) => {
            let across = |held: Option<BigInt>, value: u64| match held {
                Some(held) => Some(held),
                None => BigInt::from_f64(super::operators::as_number(context, Value(value))?),
            };
            match (across(a, left), across(b, right)) {
                (Some(a), Some(b)) => (a, b),
                // A fraction, a `NaN` or an infinity: no bigint says the same
                // thing, and every relational operator answers false for an
                // unordered pair — which is what `NaN` already produces.
                _ => return Some(Value::from_bool(false).bits()),
            }
        }
        // Arithmetic with one side only. The language refuses to coerce, and
        // this cannot throw.
        _ => return Some(Value::from_f64(f64::NAN).bits()),
    };

    let produced = match op {
        Op::Add => a.add(&b),
        Op::Sub => a.sub(&b),
        Op::Mul => a.mul(&b),
        // Division by zero is a `RangeError`; `undefined` is the stated answer.
        Op::Div => match a.div(&b) {
            Some(held) => held,
            None => return Some(undefined_of(context)),
        },
        Op::Rem => match a.rem(&b) {
            Some(held) => held,
            None => return Some(undefined_of(context)),
        },
        Op::BitAnd => a.bit_and(&b),
        Op::BitOr => a.bit_or(&b),
        Op::BitXor => a.bit_xor(&b),
        // A comparison answers a boolean rather than a bigint, so it leaves
        // before the value is stored.
        Op::Compare(want) => {
            return Some(Value::from_bool(want.holds(a.cmp(&b))).bits());
        }
    };
    Some(context.bigint_value(produced))
}

/// `-x` where `x` is a bigint.
///
/// # Why unary minus is its own entry point
///
/// It was emitted as `x * -1`, which is exactly right for a double and exactly
/// wrong here: `-1` is a **number**, so a bigint operand made the multiply a
/// mixed operation — which the language refuses and this answers `NaN` for. So
/// `-1n` was `NaN`, and with it every negative literal, `BigInt.asIntN` reading
/// back, and `-1n & 3n`.
///
/// Negating a double directly is also better than multiplying: `-0.0` comes out
/// of `0.0 * -1.0` correctly but through a multiply nobody needed.
#[rtse::entry]
pub fn negate(value: u64) -> u64 {
    with_current(|context| {
        if let Some(held) = super::bigints::digits_of(context, value).map(BigInt::neg) {
            return context.bigint_value(held);
        }
        let number = super::operators::as_number(context, Value(value)).unwrap_or(f64::NAN);
        Value::from_f64(-number).bits()
    })
}

/// Which operation a caller wants.
#[derive(Clone, Copy)]
pub(super) enum Op {
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `%`
    Rem,
    /// `&`
    BitAnd,
    /// `|`
    BitOr,
    /// `^`
    BitXor,
    /// One of the four relational operators.
    Compare(Relation),
}

/// Which way a comparison has to fall for the answer to be true.
#[derive(Clone, Copy)]
pub(super) enum Relation {
    /// `<`
    Less,
    /// `<=`
    LessEqual,
    /// `>`
    Greater,
    /// `>=`
    GreaterEqual,
}

impl Relation {
    /// Whether an ordering satisfies this relation.
    fn holds(self, ordering: core::cmp::Ordering) -> bool {
        use core::cmp::Ordering::{Greater, Less};
        match self {
            Relation::Less => ordering == Less,
            Relation::LessEqual => ordering != Greater,
            Relation::Greater => ordering == Greater,
            Relation::GreaterEqual => ordering != Less,
        }
    }
}
