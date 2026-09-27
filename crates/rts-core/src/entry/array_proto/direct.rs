//! `a.push(v)`, reached directly.
//!
//! One value, appended in place, where the receiver is a real array that may
//! grow — the same two tests `stack::push` makes, and the same append. Anything
//! else, a frozen array included, takes `direct_call::through_the_method`,
//! which runs the member with the count and spelling the site wrote; that is
//! where the `TypeError` for a frozen array and the generic arm for an array-
//! like live, stated once, in the member.
//!
//! Measured before this existed: `a.push(i); a.pop()` at 107 ns on
//! `bench/analytic.ts`, of which the push was a property read, a dispatch and
//! an argument reconstruction around a `Vec::push`.

use super::super::direct_call::through_the_method;
use super::super::with_current;
use crate::value::Value;

/// `a.push(v)` — the new length.
#[rtse::entry]
pub fn array_push_direct(this: u64, value: u64, name: i64) -> u64 {
    let appended = with_current(|context| {
        let cell = Value(this)
            .as_slot()
            .filter(|cell| context.elements_at(*cell).is_some())?;
        if super::stack::refuses_append(context, cell) {
            return None;
        }
        let elements = context.elements_at_mut(cell)?;
        elements.push(value);
        let count = elements.len();
        super::super::array::set_length(context, cell, count);
        Some(Value::from_f64(count as f64).bits())
    });
    match appended {
        Some(length) => length,
        None => through_the_method(this, "push", name, &[value]),
    }
}
