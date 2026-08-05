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

/// `ToString` of a primitive.
///
/// # Why this is shared rather than written where it is needed
///
/// Two operations need it and they look unrelated: `+` converts the non-string
/// side of a concatenation, and a computed property key converts whatever was
/// written between the brackets. Both are `ToString`, and the first version had
/// it inline in `+` — where a second copy for property keys would have been the
/// third statement of the same table.
///
/// An object answers `None` rather than `"[object Object]"`. `ToPrimitive` on
/// one runs a `toString`, which is user code an entry point cannot call, so the
/// absence is a contract violation the caller reports rather than a conversion
/// this can perform.
pub(super) fn to_text(context: &Context, value: Value) -> Option<Str> {
    match value.kind() {
        Kind::Float | Kind::Int => Some(crate::coerce::number_to_string(value.numeric()?)),
        Kind::Bool => Some(Str::from_str(
            if rts_cranelift::tags::payload_of(value.bits()) == rts_cranelift::tags::BOOL_TRUE {
                "true"
            } else {
                "false"
            },
        )),
        Kind::Singleton(number) => Some(Str::from_str(if number == context.singletons.undefined {
            "undefined"
        } else {
            "null"
        })),
        // A string is its own text; anything else on the heap is an object.
        Kind::Reference(slot) => context.text_at(slot as u32).cloned(),
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
                    // Asked first, because a symbol's cell is an ordinary
                    // object's and every question below would answer for it.
                    // `typeof Symbol.iterator` is the one observable place the
                    // encoding stops being an implementation detail.
                    _ if context.symbol_at(slot).is_some() => "symbol",
                    _ if context.callable_at(slot).is_some() => "function",
                    // A string.s cell, which `shape_of` already refuses to
                    // treat as an object for exactly this reason: what a
                    // reference IS is readable from beside the cell rather
                    // than from the encoding.
                    _ if context.text_at(slot).is_some() => "string",
                    _ => "object",
                }
            }
        };
        context.intern_value(Str::from_str(text)).bits()
    })
}

/// The text a value has, for a host that has to report a result.
///
/// # Why the host cannot do this itself
///
/// A string is a cell in the region and its bytes are beside it in the slab, so
/// reading one needs the context — which the host installs for the run and takes
/// back afterwards. By the time `run` returns, the only thing left is a word.
///
/// `None` for anything whose conversion runs user code, which is an object: this
/// is a report, and reaching back into the program that produced it to ask what
/// it would like to be called is not what a report should do.
pub fn described(value: u64) -> Option<String> {
    with_current(|context| to_text(context, Value(value))?.to_rust())
}

/// Seeds the property-key numbering from what the compilation resolved.
///
/// # Why the texts and not just how many
///
/// A key the compiler resolved crosses as a number and needs no text: both
/// sides hold the same number. A key the program **computes** does need it —
/// `o[k]` arrives here as a string and has to reach the number the compiler
/// already chose, which a count cannot say.
///
/// Interned in the order given, because interning is what mints the numbers.
/// The compiler orders them by key for exactly that reason, and a different
/// order is a different mapping rather than a cosmetic difference.
pub fn declare_keys(context: &mut Context, texts: &[String]) {
    for text in texts {
        let text = Str::from_str(text);
        context.interner.intern(&text, &mut context.keys);
    }
}
