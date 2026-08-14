//! `split` when both sides are narrow text, which is almost every call.
//!
//! # What this replaces, and what the measurement said
//!
//! `bench/analytic.ts`'s `string split 16` row — `"a,b,c,d,e,f,g,h".split(",")`,
//! eight pieces — read 1755.86 / 1791.70 / 1756.84 ns on 2026-08-13, against
//! 118–122 ns for `string slice 16`, which produces ONE string by the same
//! interning. So a piece cost roughly what a whole `slice` did, and the
//! difference was allocation the general path makes and this one does not:
//!
//! | per piece, on the general path | why it is not needed |
//! |---|---|
//! | a `String` for the match's group | `split` never reads the groups |
//! | a `String` for the piece | a piece of narrow text is a byte slice |
//! | `Str::from_str`'s ASCII scan and a second `Vec` | narrow in, narrow out |
//!
//! plus one `to_rust()` copy of the whole subject before any of it. What is
//! left is the interned cell itself, which is the same cost `slice` pays and
//! is not this module's question.
//!
//! `pattern.rs` recorded the general path's cost as "220 to 320 ns PER PIECE
//! and roughly flat", and named this rework as the thing that would touch it —
//! "the pieces would have to be built as narrow strings directly". This is
//! that, kept apart from `pattern.rs` because that file is at 488 lines against
//! a 500-line ceiling.
//!
//! # Why it is a fast path and not the implementation
//!
//! It answers `None` for everything it does not handle, and the general path
//! follows unchanged: a wide subject or separator, a regular expression, an
//! empty separator, an absent one. Every one of those has a rule of its own in
//! `pattern.rs` — the empty separator splits between code units and produces no
//! trailing piece, an absent one is the whole string as one piece — and
//! restating any of them here is how the two spellings would come to disagree.

use super::super::with_current;
use crate::text::Str;
use crate::value::Value;

/// The pieces of `this.split(separator)` as narrow bytes, or `None` when this
/// call is not one this path answers.
///
/// Borrows the subject rather than copying it: the pieces are slices of it
/// until the moment each becomes a cell, which is the whole of the saving.
fn pieces(context: &super::super::Context, this: u64, separator: u64) -> Option<Vec<Vec<u8>>> {
    let subject_cell = Value(this).as_slot()?;
    let separator_cell = Value(separator).as_slot()?;
    // A regular expression is a slot holding text too, so this has to be asked
    // before the text is read rather than after.
    if context.regexp_at(separator_cell).is_some() {
        return None;
    }
    let subject = context.text_at(subject_cell)?.narrow()?;
    let needle = context.text_at(separator_cell)?.narrow()?;
    // The empty separator is the specification's other rule, not a degenerate
    // case of this one — see the module doc.
    if needle.is_empty() {
        return None;
    }

    // The same `memmem` two-way search with an SIMD prefilter that the general
    // scan uses, over the bytes directly rather than over a `str` rebuilt from
    // them.
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(offset) = memchr::memmem::find(&subject[at..], needle) {
        found.push(subject[at..at + offset].to_vec());
        at += offset + needle.len();
    }
    // The tail is a piece whether or not it is empty: `"a,".split(",")` is two
    // pieces, and a loop that only pushed on a match would answer one.
    found.push(subject[at..].to_vec());
    Some(found)
}

/// `s.split(separator, limit)` for narrow text, or `None` to fall through.
pub(super) fn split(this: u64, separator: u64, limit: u64) -> Option<u64> {
    let mut parts = with_current(|context| pieces(context, this, separator))?;
    // Truncated before anything is allocated on the heap, so a limit smaller
    // than the number of pieces does not pay for the pieces it discards.
    if let Some(wanted) = Value(limit).numeric().filter(|wanted| *wanted >= 0.0) {
        parts.truncate(wanted as usize);
    }
    // Outside the borrow: making the array may collect, and a collection needs
    // the context the closure above is holding.
    let array = super::super::array::array_new(parts.len() as i64);
    // Written STRAIGHT INTO the array, one piece at a time.
    //
    // Interning a piece ALLOCATES and an allocation collects, so the pieces
    // made so far have to be reachable — and a piece that has landed in the
    // array is reachable through it, where a piece sitting in a second `Vec` on
    // the Rust heap was not. That second vector was also a second allocation
    // for one list, thrown away against the one `array_new` had already sized.
    Some(with_current(|context| {
        let Some(cell) = Value(array).as_slot() else {
            return array;
        };
        for (at, bytes) in parts.into_iter().enumerate() {
            let value = context.intern_value(Str::owning_latin1(bytes)).bits();
            if let Some(elements) = context.elements_at_mut(cell) {
                elements[at] = value;
            }
        }
        array
    }))
}
