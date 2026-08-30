//! `ToPrimitive` — the conversion that runs user code.
//!
//! # Why this is here and not in `coerce`
//!
//! `coerce` states, in its own header, that it never calls anything: it decides
//! *which* operand needs a primitive and *with which hint*, and hands that back
//! as [`crate::coerce::Needs`] for a caller that can call. This is that caller.
//! The split is not ceremony — the order the operands are converted in is what
//! is easy to get wrong, and `coerce` owning the order while owning no calls is
//! what keeps the two decisions from being made in one place by accident.
//!
//! # The gap this closes
//!
//! `Needs`, `Hint` and `Side` were written and never wired. Nothing called
//! `ToPrimitive`, so every object reaching an operator fell out of
//! `entry::text::to_text` as `None` and was reported as `NaN` or `undefined`:
//! `'x' + {}` was `NaN`, `String([1, 2])` was `"undefined"`, a template literal
//! holding an object was `NaN`. All three were wrong answers that ran, which is
//! worse than a refusal — nothing downstream could tell them from a real result.
//!
//! # Why nothing here holds a borrow
//!
//! Every statement is one of three: intern a name, read a property, call a
//! method. The second and third are entry points that take their own borrow, and
//! the method is user code whose first act may be to call the runtime. An
//! `extern "C"` frame cannot unwind, so a second borrow aborts the process
//! rather than failing — which is why the pattern is collect, drop, call, and
//! never a `with_current` wrapped around the loop.

use super::objects::undefined_of;
use super::{Context, with_current};
use crate::coerce::{Hint, Side};
use crate::text::Str;
use crate::value::{Kind, Value};

/// Whether `ToPrimitive` has anything to do.
///
/// A string is a reference and is already a primitive, so the test cannot be
/// `Kind::Reference` alone — that is what `coerce::needs_primitive` answers, and
/// it answers it about a value whose slot it cannot look inside. Here the heap
/// is in hand, so the question can be asked properly.
pub(super) fn is_object_in(context: &Context, value: u64) -> bool {
    match Value(value).kind() {
        // `is_text_at` and not `text_at(…).is_none()`: this asks WHETHER the
        // cell is a string and never looks at the text, and resolving the text
        // to throw it away costs a field read and a slab lookup on every
        // computed property key, every `Map` operand and every arithmetic
        // operand that is a reference.
        Kind::Reference(slot) => !context.is_text_at(slot as u32),
        _ => false,
    }
}

/// `ToPrimitive(value, hint)`.
///
/// `OrdinaryToPrimitive`: try two methods in the order the hint states and take
/// the first whose answer is not an object. A primitive is answered unchanged,
/// which is what makes this safe to call on every operand rather than only on
/// the ones that look like they need it.
///
/// An object whose `valueOf` and `toString` both answer objects **raises a
/// `TypeError`** and is then answered unchanged. The raise is the language's
/// answer; answering the object back after it is what lets the caller's
/// existing "this is not a primitive" path stay as it was, rather than every
/// caller learning a second failure shape. Inventing `undefined` here would
/// still turn a contract violation into a value, which is why it is not that.
///
/// `Symbol.toPrimitive` is consulted first, ahead of both methods: an object
/// defining it decides its own conversion for every hint, and `valueOf`/
/// `toString` never run when it does — including when it answers an object,
/// which is a `TypeError` rather than a reason to try the pair.
pub(super) fn to_primitive(value: u64, hint: Hint) -> u64 {
    // Only a reference can be a string or an object. Every other tag is already
    // primitive, so it cannot consult a hook and does not need to borrow the
    // runtime context just to answer that fact. This is the common path for
    // numeric array joins, arithmetic and template substitutions.
    if !matches!(Value(value).kind(), Kind::Reference(_)) {
        return value;
    }
    if !with_current(|context| is_object_in(context, value)) {
        return value;
    }

    // `Symbol.toPrimitive`, asked BEFORE `valueOf`/`toString`: the language
    // says a user method under this name overrides both rather than sitting
    // beside them.
    if let Some((key, hint_text)) = with_current(|context| {
        Some((
            super::symbol::well_known(context, "toPrimitive"),
            context.intern_value(Str::from_str(match hint {
                Hint::String => "string",
                Hint::Number => "number",
                Hint::Default => "default",
            })).bits(),
        ))
    }) {
        let method = super::computed::get_indexed(value, key);
        if with_current(|context| super::modules::is_callable_in(context, method)) {
            let absent = with_current(|context| undefined_of(context));
            let answer = super::functions::call(method, value, hint_text, absent, absent, absent);
            // Rule 8: the method is user code, and this native carries the
            // answer forward — `undefined` is a value, so a throw left
            // unasked would be spent as though the method had answered it.
            if super::throw::in_flight() {
                return value;
            }
            if !with_current(|context| is_object_in(context, answer)) {
                return answer;
            }
            // An object back from `Symbol.toPrimitive` is a `TypeError`, and it
            // does NOT fall through to `valueOf`/`toString`: the hook replaced
            // both, so running them would answer where the language refuses.
            //
            // This file used to fall through, with a note calling that "the
            // honest absence for a throw it cannot raise". A native CAN raise
            // now — rule 8 of this crate's README — so the absence is over, and
            // it was measurable: `+v`, `String(v)` and `v == 1` over a hook
            // answering `{}` each ran to a wrong value where Node and Bun throw.
            super::throw::type_error(
                "Cannot convert object to primitive value",
            );
            return value;
        }
    }

    // `Hint::Default` behaves as `Hint::Number` for every object without a
    // `Symbol.toPrimitive`, which is every object here. They stay distinct in
    // `Hint` because an object that defines one receives the string `"default"`
    // and may branch on it — collapsing them at the type would lose that.
    let order = match hint {
        Hint::String => ["toString", "valueOf"],
        Hint::Default | Hint::Number => ["valueOf", "toString"],
    };

    for name in order {
        let (key, absent) = with_current(|context| {
            (
                context.intern_value(Str::from_str(name)).bits(),
                undefined_of(context),
            )
        });
        // The read is user-visible on its own: the method may be reached through
        // a getter, and that getter is user code too. So it goes through the
        // ordinary property path with no borrow held.
        let method = super::computed::get_indexed(value, key);
        if !with_current(|context| super::modules::is_callable_in(context, method)) {
            continue;
        }
        let answer = super::functions::call(method, value, absent, absent, absent, absent);
        // Rule 8, for the same reason the hook above states it: a `valueOf` that
        // threw answers `undefined`, and `undefined` is a primitive — so the
        // conversion would SUCCEED with it and the throw would be spent.
        if super::throw::in_flight() {
            return value;
        }
        if !with_current(|context| is_object_in(context, answer)) {
            return answer;
        }
    }

    // Neither method answered a primitive. `Object.create(null)` reaches here,
    // and so does an object whose two methods both answer objects.
    super::throw::type_error("Cannot convert object to primitive value");
    value
}

/// ToString for a host entry point, preserving a throw raised by user code.
///
/// `Ok(Some(text))` is a materialised string, `Ok(None)` is a primitive that
/// cannot be represented as Rust text (notably Symbol), and `Err(())` means a
/// user conversion already left a throw in flight. The three outcomes are kept
/// distinct so a host can raise its own specification error without replacing a
/// callback's exception.
pub fn string_for_host(value: u64) -> Result<Option<String>, ()> {
    let primitive = to_primitive(value, Hint::String);
    if super::throw::in_flight() {
        return Err(());
    }
    Ok(with_current(|context| {
        super::text::to_text(context, Value(primitive)).and_then(|text| text.to_rust())
    }))
}

/// Both operands of a binary operator, converted in the order the operator specifies.
///
/// The order is the part that is easy to get wrong and invisible until an
/// operand has a side-effecting `valueOf`, which is why it comes from
/// [`crate::coerce`] rather than being written out at each call site: `+` and
/// the arithmetic operators convert left then right, while `a <= b` is
/// specified as `!(b < a)` and converts the **right** operand first.
pub(super) fn operands(left: u64, right: u64, order: [Side; 2], hint: Hint) -> (u64, u64) {
    let mut pair = [left, right];
    for side in order {
        let at = match side {
            Side::Left => 0,
            Side::Right => 1,
        };
        pair[at] = to_primitive(pair[at], hint);
    }
    (pair[0], pair[1])
}
