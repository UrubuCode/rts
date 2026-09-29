//! `s.charCodeAt(i)`, reached directly.
//!
//! The member's own body is already one borrow and one read of a code unit
//! (`basic.rs::char_code_at`); what `s.charCodeAt(i & 15)` paid 71.5 ns for
//! (release, 2026-09-29) is the way there — a property read that walks from the
//! primitive to `String.prototype`, a native dispatch, the slots rebuilt. Where
//! the whole program leaves `String` as the language defines it, the compiler
//! emits this instead: a receiver that IS a string and an index that is a
//! number answer from the text, and anything else — a `String` wrapper, an
//! object with a `charCodeAt` of its own, an index that has to be converted by
//! running code — takes `direct_call::through_the_method`, the call the
//! compiler would have emitted.

use super::super::direct_call::through_the_method;
use super::super::with_current;
use crate::value::{Kind, Value};

/// `s.charCodeAt(index)`.
#[rtse::entry]
pub fn string_char_code_at_direct(this: u64, index: u64, name: i64) -> u64 {
    let direct = with_current(|context| {
        // A number or nothing else: any other index converts through code the
        // member runs, in the order the member runs it.
        if !matches!(Value(index).kind(), Kind::Float | Kind::Int) {
            return None;
        }
        let cell = Value(this).as_slot()?;
        let text = context.text_at(cell)?;
        let at = super::integer_arg(context, index);
        if at < 0.0 || at >= text.len() as f64 {
            return Some(Value::from_f64(f64::NAN).bits());
        }
        Some(match text.unit_at(at as usize) {
            Some(unit) => Value::from_f64(f64::from(unit)).bits(),
            None => Value::from_f64(f64::NAN).bits(),
        })
    });
    match direct {
        Some(answer) => answer,
        None => through_the_method(this, "charCodeAt", name, &[index]),
    }
}
