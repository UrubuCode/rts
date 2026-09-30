//! The list `f(...xs)` hands the door, where `xs` is already that list.
//!
//! # What this removes, measured
//!
//! `f(...xs)` cost 320 to 355 ns for three elements (release, 2026-09-30) and
//! the call itself about 20: the rest was `iterate`, which answers every
//! iterable as a FRESH array — a copy, so that a loop walking it cannot walk
//! its own additions. A call has no such loop. The door reads the elements
//! into the convention's slots and pushes the list for a rest parameter or
//! `arguments` to read, and both of those COPY what they read, so the array
//! written is the list the door wants, as it is.
//!
//! # Where the copy stays
//!
//! An array with a HOLE, because a spread reads a hole as `undefined` and a
//! rest parameter built over the array itself would inherit the hole; and
//! anything that is not an array — a `Set`, a string, an iterator — which is
//! `iterate`'s to walk. Both take the road they took.

use super::with_current;
use crate::value::Value;

/// `iterable`, where it is a hole-free array; what it iterates to otherwise.
#[rtse::entry]
pub fn spread_list(iterable: u64) -> u64 {
    let as_written = with_current(|context| {
        let cell = Value(iterable).as_slot()?;
        let elements = context.elements_at(cell)?;
        elements
            .iter()
            .all(|held| !super::array::is_hole(context, *held))
            .then_some(iterable)
    });
    match as_written {
        Some(list) => list,
        None => super::iterate::iterate(iterable),
    }
}
