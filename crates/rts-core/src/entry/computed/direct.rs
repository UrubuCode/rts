//! `a[i]` where the compiler has proved `i` a number.
//!
//! The generic read (`access.rs::read_on`) is written for a key that may be
//! anything: it opens the key, asks whether a proxy is involved, and only then
//! reaches the element — three borrows of the context and a resolution whose
//! answer, for a number over an array, is always "an element". `a[i & 1023]`
//! cost 14.7 ns a read against 1.7 for the loop around it
//! (`bench`-style isolated row, release, 2026-09-29), and `typed[i]` 23.6.
//!
//! Here the key arrives as the `f64` it already was, so nothing is converted
//! and nothing is resolved: one borrow, the element store asked first and a
//! typed array's bytes second. Everything the generic read answers
//! differently stays the generic read's — a proxy anywhere in the program, a
//! key that is not a canonical index (negative, fractional, `NaN`, at or past
//! `DENSE_LIMIT`), a receiver that is neither an array nor a view: a string, a
//! plain object with a numeric key, a primitive. Those call `get_indexed` with
//! the same two values, so the door cannot answer what the read would not.
//!
//! This is NOT the bounded load `docs/codegen/element-load.md` records as
//! refused: no address leaves the runtime, and the array is an operand of the
//! call on every read, so it stays reachable for as long as it is read.

use super::super::array::{DENSE_LIMIT, visible};
use super::super::objects::undefined_of;
use super::super::with_current;
use crate::value::Value;

/// `object[index]`, `index` a number.
#[rtse::entry]
pub fn index_number_direct(object: u64, index: f64) -> u64 {
    let answered = with_current(|context| {
        // `!(index >= 0.0)` and not `index < 0.0`: it refuses `NaN` too.
        if context.any_proxy() || !(index >= 0.0) || index.fract() != 0.0 {
            return None;
        }
        if index >= DENSE_LIMIT as f64 {
            return None;
        }
        let cell = Value(object).as_slot()?;
        let at = index as usize;
        if let Some(elements) = context.elements_at(cell) {
            // Past the end is absent, and so is a hole: `[1, 2][9]` and
            // `[, 1][0]` are both `undefined`, as the generic read answers.
            return Some(match elements.get(at).copied() {
                Some(held) => visible(context, held),
                None => undefined_of(context),
            });
        }
        super::super::buffers::indexed_get(context, cell, Value::from_f64(index))
    });
    match answered {
        Some(value) => value,
        None => super::get_indexed(object, Value::from_f64(index).bits()),
    }
}
