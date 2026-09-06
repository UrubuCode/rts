//! The four operators that are not arithmetic.
//!
//! # Why they were in `mod.rs` and should not have been
//!
//! That file says it, three lines above where they sat: *"the operators are
//! defined in their own module and named from here, because a caller wants the
//! entry points in one place rather than a module tree"*. These four were the
//! exception the sentence did not mention.
//!
//! They belong together for a reason of their own. Each is an entry point for
//! something other than a conversion: `+` because it may concatenate rather
//! than add, `===` because two strings are equal when their text is,
//! `ToBoolean` because the empty string is falsy, `String(n)` because the
//! result is allocated. The arithmetic ones next door are all one sentence —
//! `ToNumber` of a string reads the heap — and these are four different ones.

use super::{Context, with_current};
use crate::coerce::{Sum, add as add_primitives, number_to_string as print_number};
use crate::text::Str;
use crate::value::{
    Value, same_value as values_same_value, strict_equals as values_strict_equals,
    to_boolean as values_to_boolean,
};

/// `a + b`, on values already reduced to primitives.
///
/// An entry point because joining two strings allocates. The caller has already
/// resolved `ToPrimitive` in the order [`crate::coerce::add_operand_order`]
/// states — this cannot do it, because running a `valueOf` is calling.
#[rtse::entry]
pub fn add(left: u64, right: u64) -> u64 {
    // Before the borrow, and left before right — which is what
    // `coerce::add_operand_order` states and the reason it is a function rather
    // than a comment: the left operand's `valueOf` runs to completion before the
    // right operand is looked at, so a pair of side-effecting conversions
    // observes the same sequence the language specifies. Converting both in
    // whatever order was convenient gives the right answer for every value
    // without a side effect and the wrong one for the values that have them.
    //
    // Outside `with_current` because a conversion calls user code, and a second
    // borrow inside an `extern "C"` frame aborts the process rather than
    // unwinding.
    let mut operands = [left, right];
    for side in crate::coerce::add_operand_order() {
        let at = match side {
            crate::coerce::Side::Left => 0,
            crate::coerce::Side::Right => 1,
        };
        operands[at] = super::primitive::to_primitive(operands[at], crate::coerce::Hint::Default);
    }
    let [left, right] = operands;

    // Asked before the numeric path and AFTER the string one, which is the order
    // the specification states and the order that is easy to get backwards:
    // `"" + 1n` concatenates and answers `"1"`, while `1 + 1n` is a `TypeError`.
    // A bigint checked first would have made the first of those `NaN`.
    //
    // In a borrow of its own, and `settled` after it ends: the mixed arm refuses
    // with a `TypeError` now, and building that error borrows the context again.
    if let Some(outcome) = with_current(|context| {
        let is_text = |value: u64| {
            Value(value)
                .as_slot()
                .is_some_and(|slot| context.text_at(slot).is_some())
        };
        match is_text(left) || is_text(right) {
            true => None,
            false => super::bigint_class::binary(context, super::bigint_class::Op::Add, left, right),
        }
    }) {
        return super::bigint_class::settled(outcome);
    }
    with_current(|context| {
        // Empresta. O `.cloned()` que estava aqui copiava o texto inteiro só
        // para satisfazer a assinatura de `add`, e a assinatura mudou.
        let text_of = |value: Value| value.as_slot().and_then(|slot| context.text_at(slot));

        // `ToString` of a primitive, which is what the non-string side of a
        // concatenation becomes. Separate from `text_of` because that one
        // answers "is this already a string" and decides *whether* to
        // concatenate — a single function doing both would make `1 + 2` answer
        // `"12"`.
        let stringify = |value: Value| super::text::to_text(context, value);

        let singletons = context.singletons;
        let numeric = move |value: Value| crate::value::to_number(value, singletons);

        match add_primitives(Value(left), Value(right), text_of, stringify, numeric) {
            Some(Sum::Number(number)) => Value::from_f64(number).bits(),
            Some(Sum::Text(text)) => context.intern_value(text).bits(),
            // Neither a number nor a string: the caller handed over something
            // still needing ToPrimitive. Answering NaN would be a wrong number;
            // this is a contract violation, and saying so beats inventing one.
            None => Value::from_f64(f64::NAN).bits(),
        }
    })
}

/// `a === b`.
///
/// An entry point because two strings are equal when their *text* is, which
/// needs the heap. Everything else about it is arithmetic.
#[rtse::entry]
pub fn strict_equals(left: u64, right: u64) -> bool {
    with_current(|context| {
        // A bigint compares by its DIGITS, exactly as a string compares by its
        // text: `1n === 1n` is true though the two were computed separately and
        // live in different slots. Comparing the words would answer false, which
        // is the one thing a primitive must not do.
        //
        // `1n === 1` stays false and needs nothing: a number and a bigint have
        // different tags, so the ordinary comparison already separates them.
        if super::bigints::same(context, left, right) {
            return true;
        }
        values_strict_equals(Value(left), Value(right), |a, b| context.same_text(a, b))
    })
}

/// `Object.is(a, b)` — `SameValue`, which `===` is not: it separates `+0` from
/// `-0` and treats `NaN` as equal to itself. Jest's `expect(x).toBe(y)` is this,
/// not `===` — the harness in `rts-std` called the wrong one.
#[rtse::entry]
pub fn same_value(left: u64, right: u64) -> bool {
    with_current(|context| {
        // As in `strict_equals`: a bigint compares by its digits, and SameValue
        // agrees with `===` on bigints (neither separates `1n` from a second
        // `1n` computed elsewhere), so the same check applies first.
        if super::bigints::same(context, left, right) {
            return true;
        }
        values_same_value(Value(left), Value(right), |a, b| context.same_text(a, b))
    })
}

/// `ToBoolean`.
///
/// An entry point for one case out of seven: the empty string. Every other
/// falsy value is decided by arithmetic, and a lowering that proved its operand
/// is a number should emit the comparison rather than call this.
#[rtse::entry]
pub fn to_boolean(value: u64) -> bool {
    with_current(|context| to_boolean_in(context, value))
}

/// `ToBoolean`, from a context already in hand.
///
/// The context-taking half, and it is not a convenience: every caller that reads
/// an option object is inside `with_runtime` by construction, so the ambient
/// form there is a nested borrow and an `extern "C"` frame cannot unwind past
/// it. `node:stream`'s own option reader said so in a comment and then called
/// the ambient form three lines below — `new Readable({objectMode: true})`
/// aborted the process, and nothing tested it.
pub fn to_boolean_in(context: &Context, value: u64) -> bool {
    {
        let singletons = context.singletons;
        let kinds = context.kinds;
        // The two heap questions the falsy rule cannot answer alone: an empty
        // string, and `0n`. A symbol reaches here and is never falsy, which is
        // said by answering false rather than by the value layer learning what a
        // symbol is.
        values_to_boolean(Value(value), singletons, |held| {
            if let Some(payload) = held.as_client(kinds.bigint) {
                return context.bigint_at(payload).is_none_or(|big| big.is_zero());
            }
            match held.as_slot() {
                Some(slot) => context.text_at(slot).is_some_and(Str::is_empty),
                None => false,
            }
        })
    }
}

/// `String(n)`.
///
/// An entry point because the result is allocated.
#[rtse::entry]
pub fn number_to_string(value: f64) -> u64 {
    with_current(|context| {
        let text = print_number(value);
        context.intern_value(text).bits()
    })
}
