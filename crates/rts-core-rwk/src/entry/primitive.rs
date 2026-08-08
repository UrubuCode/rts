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
        Kind::Reference(slot) => context.text_at(slot as u32).is_none(),
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
/// An object whose `valueOf` and `toString` both answer objects is answered
/// **unchanged**, not as `undefined`. The specification throws a `TypeError`
/// there; answering the object back leaves the caller's existing "this is not a
/// primitive" path to report it, which is the honest absence it already has.
/// Inventing `undefined` here would turn a contract violation into a value.
///
/// `Symbol.toPrimitive` is not consulted yet. It is looked up before the two
/// methods and is additive on top of this; leaving it out means an object that
/// defines it is converted by its `valueOf` instead, which is a wrong answer for
/// `Date` and for nothing else this engine can build today.
pub(super) fn to_primitive(value: u64, hint: Hint) -> u64 {
    if !with_current(|context| is_object_in(context, value)) {
        return value;
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
        if !with_current(|context| is_object_in(context, answer)) {
            return answer;
        }
    }

    value
}

/// Both operands of a binary operator, converted in the order the operator
/// specifies.
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
