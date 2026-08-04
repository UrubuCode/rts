//! Strings a program wrote down, and what `typeof` answers.
//!
//! # Why a literal is a number here
//!
//! A string is a heap value, and two occurrences of `"a"` in a program are *the
//! same string* — which is interning, which reads a table. So a literal cannot
//! be an immediate in the compiled code, and what the code carries instead is
//! **which** literal.
//!
//! The text arrives separately: the compiler collects every literal it saw, the
//! host seeds this table with them before the program runs, and the number in
//! the code indexes it. That is the same shape as the property-key numbering and
//! the singleton numbering, and it has the same failure mode — a number that
//! names nothing — handled the same way, by the host seeding from what the
//! compilation actually produced.
//!
//! # Why the values are made once, at seeding
//!
//! A literal evaluated twice must be the same string, and making one per
//! evaluation would allocate on every pass of a loop. So the table holds
//! *values*, interned when it is seeded, and reaching a literal is an index
//! rather than an allocation.

use super::objects::undefined_of;
use super::{Context, with_current};
use crate::text::Str;
use crate::value::{Kind, Value};

/// Seeds the literal table for a program about to run.
///
/// Called by the host with what the compilation collected, in the order it
/// collected them — the index the code carries is a position in that list, so
/// the order is the agreement rather than an incidental detail.
pub fn declare_literals(context: &mut Context, texts: &[String]) {
    // A loop rather than a `map`, because interning needs the context mutably
    // and so does the field being filled. Pushing one at a time is also what
    // makes the index a position in `texts` rather than something a collect
    // happened to preserve.
    context.literals.clear();
    for text in texts {
        let value = context.intern_value(Str::from_str(text)).bits();
        context.literals.push(value);
    }
}

/// The string a literal number names.
///
/// Answers `undefined` for a number the table does not have, which is a host
/// that seeded fewer literals than the code refers to — a defect in the wiring
/// rather than anything a program can express, and visible as a wrong value
/// rather than as a read of whatever was next in memory.
#[rtse::entry]
pub fn string_const(which: i64) -> u64 {
    with_current(|context| match context.literals.get(which as usize) {
        Some(value) => *value,
        None => undefined_of(context),
    })
}

/// `typeof v`.
///
/// # Why `null` answers `"object"`
///
/// Because that is what JavaScript does. It is a mistake from 1995 that the
/// language cannot take back, and it is written here rather than corrected,
/// because a program asking `typeof` wants what JavaScript does and not what it
/// should have done.
///
/// # Why a function is distinguished by its layout
///
/// A callable is a cell like any other and the tag says only "a reference". So
/// the answer comes from the cell's header, which is the same mechanism a
/// property read uses to find out that a string is not an object — the machine's
/// own answer to a tag space with no room for a kind.
#[rtse::entry]
pub fn type_of(value: u64) -> u64 {
    with_current(|context| {
        let text = match Value(value).kind() {
            Kind::Float | Kind::Int => "number",
            Kind::Bool => "boolean",
            Kind::Singleton(number) => {
                if number == context.singletons.undefined {
                    "undefined"
                } else {
                    // `typeof null` is "object".
                    "object"
                }
            }
            Kind::Reference(slot) => {
                let slot = slot as u32;
                match context.region.type_of(slot) {
                    Some(ty) if ty as usize == context.closure_type().index() => "function",
                    // A string's cell, which `shape_of` already refuses to
                    // treat as an object for exactly this reason: what a
                    // reference IS is readable from the cell rather than from
                    // the encoding.
                    _ if context.text_at(slot).is_some() => "string",
                    _ => "object",
                }
            }
        };
        context.intern_value(Str::from_str(text)).bits()
    })
}
