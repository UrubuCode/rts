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
//! # What a mixed operation answers, and why it is still not a throw
//!
//! `NaN`, and the reason recorded here used to be that raising was impossible —
//! that `entry/throw.rs` ended the program. **That is no longer true**: a native
//! raises a catchable error, and the shift refusals below do it, so a program
//! catches `1n << 2n**40n` and carries on.
//!
//! So this is now a CHOICE and not a limit, and it is the smaller of two: the
//! `TypeError` belongs on every mixed operation at once — `+`, `-`, `*`, `/`,
//! `%` and the comparisons — because raising it on the three that happen to have
//! been touched would leave one language rule with two answers depending on the
//! operator. That change also has to move those call sites out of their
//! borrows first, for the reason [`settled`] states.
//!
//! Until then `NaN` is what the operation would have produced if the bigint had
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
///
/// `tag` because `BigInt.prototype[Symbol.toStringTag]` is `"BigInt"` in the
/// language and answered `undefined` here — so a boxed BigInt described itself
/// as `[object Object]`, and the descriptor for a property the specification
/// requires was absent.
#[rtse::class("BigInt", tag)]
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
///
/// The inner `Err` is a refusal that has to become a `RangeError`, and it is
/// carried OUT rather than raised here: this runs holding the context's
/// `RefCell`, and `throw::range_error` builds the error object through that same
/// cell. Raising in place is not a worse style, it is an abort — the first
/// attempt panicked `RefCell already borrowed` inside `_rts_shift_left`, which
/// cannot unwind. [`settled`] is where the caller turns it into a throw, after
/// the borrow has ended.
pub(super) fn binary(
    context: &mut Context,
    op: Op,
    left: u64,
    right: u64,
) -> Option<Result<u64, Refused>> {
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
                _ => return Some(Ok(Value::from_bool(false).bits())),
            }
        }
        // Arithmetic with one side only. The language does not coerce ACROSS the
        // boundary and does not answer either — it **refuses**, and the refusal
        // is a `TypeError`, which is the whole point of the type: a bigint that
        // silently became a double would lose exactly the range it exists for.
        //
        // This answered `NaN`, with a note saying it could not throw. It could;
        // the note was describing a caller that held a borrow, and the five
        // callers in `operators.rs` now ask in a borrow of their own the way
        // `bitwise.rs` always did. `NaN` is the worse half of the two answers:
        // `1n + 1` produced a number that then propagated through arithmetic
        // nobody could trace back to the mixing.
        _ => return Some(Err(MIXED)),
    };

    let produced = match op {
        Op::Add => a.add(&b),
        Op::Sub => a.sub(&b),
        Op::Mul => a.mul(&b),
        // Division by zero is a `RangeError`; `undefined` is the stated answer.
        Op::Div => match a.div(&b) {
            Some(held) => held,
            None => return Some(Ok(undefined_of(context))),
        },
        Op::Rem => match a.rem(&b) {
            Some(held) => held,
            None => return Some(Ok(undefined_of(context))),
        },
        Op::BitAnd => a.bit_and(&b),
        Op::BitOr => a.bit_or(&b),
        Op::BitXor => a.bit_xor(&b),
        // A shift and an exponent take their right operand as a COUNT rather
        // than as a second operand, which is why they cannot go through the
        // pairwise helpers above. A refused count RAISES, unlike the divisions
        // above: the count is the whole reason a result can be too large to
        // build, and answering `undefined` there hands the program a value for
        // a request the machine could not honour. `throw::range_error` is what
        // `buffer::alloc` already does with a negative size, for that reason.
        Op::Shl => match shifted(&a, &b, true) {
            Ok(held) => held,
            Err(why) => return Some(Err(why)),
        },
        Op::Shr => match shifted(&a, &b, false) {
            Ok(held) => held,
            Err(why) => return Some(Err(why)),
        },
        Op::Pow => match raised(&a, &b) {
            Ok(held) => held,
            Err(why) => return Some(Err(why)),
        },
        // A comparison answers a boolean rather than a bigint, so it leaves
        // before the value is stored.
        Op::Compare(want) => {
            return Some(Ok(Value::from_bool(want.holds(a.cmp(&b))).bits()));
        }
    };
    Some(Ok(context.bigint_value(produced)))
}

/// How large a result these operations will build before refusing.
///
/// A gigabit, which is what V8 answers `RangeError: Maximum BigInt size
/// exceeded` past — matched rather than invented so that a program that works
/// in Node does not meet a different wall here. There has to be one: the
/// operand of `<<` and of `**` is a COUNT, so `1n << 2n**40n` asks for a
/// terabyte of digits from an expression that fits on one line.
const MAX_BITS: u64 = 1 << 30;

/// Why an operation was refused: the message, and **which** error carries it.
///
/// The reasons are told apart rather than merged into one because a program
/// meets them for different mistakes: `2n ** -1n` is a sign error, `1n <<
/// 2n**40n` is a size the machine cannot hold, and `1n + 1` is a type confusion.
/// "Out of range" for all three would tell the reader none of them.
///
/// The CLASS travels with the text rather than being decided at the raise, and
/// that is the point of the pair: mixing is the one refusal here the language
/// spells `TypeError`, and a `settled` that only knew how to build a
/// `RangeError` would have reported the right sentence under the wrong name —
/// which a program catching `TypeError` around arithmetic would then miss.
pub(super) struct Refused {
    /// V8's wording, so a message a user searches for finds the same page.
    message: &'static str,
    /// Whether the language spells this refusal `TypeError`.
    type_error: bool,
}

const TOO_LARGE: Refused = Refused {
    message: "Maximum BigInt size exceeded",
    type_error: false,
};
const NEGATIVE_EXPONENT: Refused = Refused {
    message: "Exponent must be non-negative",
    type_error: false,
};
const MIXED: Refused = Refused {
    message: "Cannot mix BigInt and other types, use explicit conversions",
    type_error: true,
};

/// Turns what [`binary`] carried out into a value, raising if it refused.
///
/// **Call this outside `with_current`.** Building the error object borrows the
/// context, and doing that while `binary`'s borrow is still live aborts the
/// process rather than failing — the panic cannot unwind through the entry
/// point. The signature keeps no `Context`, which is what makes the rule hard to
/// break by accident.
///
/// The value answered on a refusal is never read: the compiled call site
/// re-raises what `throw::range_error` recorded. It exists because the entry
/// point returns `u64`.
///
/// `operators.rs` and `primitives.rs` used to call this inside a borrow, on the
/// argument that `+`, `-`, `*`, `/`, `%` and the comparisons could not refuse.
/// They can now — mixing a bigint with anything else is a `TypeError` — so those
/// six ask in a borrow of their own, exactly as `bitwise.rs` always did. There
/// is no caller left that holds one across this.
pub(super) fn settled(outcome: Result<u64, Refused>) -> u64 {
    match outcome {
        Ok(value) => value,
        Err(why) => {
            match why.type_error {
                true => super::throw::type_error(why.message),
                false => super::throw::range_error(why.message),
            }
            super::modules::undefined_value()
        }
    }
}

/// `a << b` and `a >> b`, which differ in a direction and share everything else.
///
/// A negative count reverses the direction — that is the language's definition
/// rather than a convenience — so both spellings reach both shifts and the
/// decision is made once.
fn shifted(value: &BigInt, amount: &BigInt, left: bool) -> Result<BigInt, Refused> {
    let toward_left = left != amount.is_negative();
    let magnitude = amount.as_i64().map(i64::unsigned_abs);
    if !toward_left {
        // A right shift only ever loses bits, so a count past the width and a
        // count of the width answer the same thing — which is what lets a count
        // too large to name saturate instead of being refused.
        let by = magnitude.and_then(|m| u32::try_from(m).ok()).unwrap_or(u32::MAX);
        return Ok(value.shr(by));
    }
    // A left count too large to even NAME is already too large to build, so the
    // two ways of being too big answer the same refusal.
    let bits = magnitude
        .and_then(|m| u32::try_from(m).ok())
        .ok_or(TOO_LARGE)?;
    match value.bit_len() as u64 + u64::from(bits) <= MAX_BITS {
        true => Ok(value.shl(bits)),
        false => Err(TOO_LARGE),
    }
}

/// `a ** b`.
///
/// A negative exponent is a `RangeError` in the language, which is the same
/// shape [`BigInt::pow`]'s unsigned parameter already states.
fn raised(value: &BigInt, exponent: &BigInt) -> Result<BigInt, Refused> {
    let count = exponent.as_i64().ok_or(TOO_LARGE)?;
    if count < 0 {
        return Err(NEGATIVE_EXPONENT);
    }
    let power = u32::try_from(count).map_err(|_| TOO_LARGE)?;
    // Checked before the multiplying starts: the size is known from the operand
    // and the count, and a check after the fact is a check that never runs.
    match (value.bit_len() as u64).saturating_mul(u64::from(power)) <= MAX_BITS {
        true => Ok(value.pow(power)),
        false => Err(TOO_LARGE),
    }
}

/// The double a bigint stands for, when the caller is allowed to ask.
///
/// `Number(1n)` is `1` and `1n + 1` is a `TypeError`, so this is deliberately
/// NOT part of [`super::operators::as_number`]: putting it there would make
/// every arithmetic operator coerce a bigint silently, which is the one thing
/// the type exists to prevent.
pub(super) fn as_f64(context: &Context, value: u64) -> Option<f64> {
    super::bigints::digits_of(context, value).map(BigInt::to_f64)
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
    // Um bigint e um numero ja pronto respondem dentro de UM emprestimo, que e
    // o caminho de toda a aritmetica compilada.
    enum Held {
        Big(BigInt),
        Number(f64),
        /// Um objeto: `ToPrimitive` chama codigo do utilizador, e isso nao pode
        /// acontecer com o contexto emprestado.
        Ask,
    }
    let held = with_current(|context| {
        if let Some(held) = super::bigints::digits_of(context, value).map(BigInt::neg) {
            return Held::Big(held);
        }
        match super::operators::as_number(context, Value(value)) {
            Some(number) => Held::Number(number),
            None => Held::Ask,
        }
    });
    let number = match held {
        Held::Big(held) => return with_current(|context| context.bigint_value(held)),
        Held::Number(number) => number,
        // `as_number` respondia `None` e o `unwrap_or(f64::NAN)` transformava
        // isso em `NaN` — entao `-{valueOf(){return 5}}` era `NaN` e `-[]` era
        // `NaN` em vez de `-0`. O `+x` unario ja fazia isto certo pelo caminho
        // de fora do emprestimo; era so o `-x` que decidia sozinho.
        Held::Ask => super::class_support::to_number(value),
    };
    Value::from_f64(-number).bits()
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
    /// `<<`
    Shl,
    /// `>>`
    Shr,
    /// `**`
    Pow,
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
