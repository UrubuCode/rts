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

use super::with_current;
use crate::coerce::{Sum, add as add_primitives, number_to_string as print_number};
use crate::text::Str;
use crate::value::{Value, strict_equals as values_strict_equals, to_boolean as values_to_boolean};

/// `a + b`, on values already reduced to primitives.
///
/// An entry point because joining two strings allocates. The caller has already
/// resolved `ToPrimitive` in the order [`crate::coerce::add_operand_order`]
/// states — this cannot do it, because running a `valueOf` is calling.
#[rtse::entry]
pub fn add(left: u64, right: u64) -> u64 {
    with_current(|context| {
        let text_of = |value: Value| {
            value
                .as_slot()
                .and_then(|slot| context.text_at(slot))
                .cloned()
        };

        // `ToString` of a primitive, which is what the non-string side of a
        // concatenation becomes. Separate from `text_of` because that one
        // answers "is this already a string" and decides *whether* to
        // concatenate — a single function doing both would make `1 + 2` answer
        // `"12"`.
        let stringify = |value: Value| super::text::to_text(context, value);

        match add_primitives(Value(left), Value(right), text_of, stringify) {
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
        values_strict_equals(Value(left), Value(right), |a, b| context.same_text(a, b))
    })
}

/// `ToBoolean`.
///
/// An entry point for one case out of seven: the empty string. Every other
/// falsy value is decided by arithmetic, and a lowering that proved its operand
/// is a number should emit the comparison rather than call this.
#[rtse::entry]
pub fn to_boolean(value: u64) -> bool {
    with_current(|context| {
        let singletons = context.singletons;
        values_to_boolean(Value(value), singletons, |slot| {
            context.text_at(slot as u32).is_some_and(Str::is_empty)
        })
    })
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
