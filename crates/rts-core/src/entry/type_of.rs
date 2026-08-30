//! What `typeof` answers, and the fused comparison against it.
//!
//! Beside `text.rs` rather than inside it, for the ceiling rule: that file was
//! already at 502 lines against this crate's 500, and new code belongs in a
//! small focused module rather than appended to one already over. What makes
//! this one cohesive is that all of it turns on a single question — which of
//! the nine names a value has — and every part of the file exists to keep that
//! question answered in exactly one place.
//!
//! [`type_name_of`] is that place. [`type_of`] turns its answer into the string
//! a program can hold; [`type_of_is`] compares it against a literal without
//! building any string at all. A second copy of the match is how `typeof x` and
//! `typeof x === "…"` would come to disagree about a value nobody tested.

use super::{Context, with_current};
use crate::text::Str;
use crate::value::{Kind, Value};

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
        // The INDEX into `TYPE_NAMES`, not the text: the string it names is
        // built at most once per run and cached, because building one allocates
        // a cell — see `Context::type_names`.
        let name = type_name_of(context, value);
        type_name(context, name)
    })
}

/// Whether `typeof value` is the string the literal at `which` spells.
///
/// # Why this exists beside [`type_of`] instead of being written at the call
///
/// Because `typeof x === "string"` is what programs actually write, and spelling
/// it out cost THREE crossings: this, to build a string; `string_const`, to
/// build the other one; and `strict_equals`, to compare their text — each with
/// the throw check a crossing implies, to answer a question decided by a tag and
/// a cell header. Measured 2026-08-29 at 24.0 ns against 8.3 for the bare
/// `typeof`, so the comparison cost nearly twice what the operation did.
///
/// # Why the literal's INDEX and not a name this crate numbers
///
/// Rule 3: one number space, however many tables feed it. The literal table is
/// an agreement the compiler and this crate already have, and the alternative —
/// the language passing a number for `"string"` that this crate also writes down
/// — is a second numbering of the nine names, which is exactly the drift
/// [`TypeName`] exists to prevent one level down.
///
/// The comparison is against `TYPE_NAMES` rather than against an interned
/// value, and allocates nothing: every one of the nine is ASCII, so its UTF-16
/// length is its byte length and `starts_with_ascii` settles the rest. Comparing
/// the interned words instead would have been one instruction and would have
/// been a bet on `intern_value` deduplicating, which is not what its name
/// promises.
#[rtse::entry]
pub fn type_of_is(value: u64, which: i64) -> bool {
    with_current(|context| {
        let name = type_name_of(context, value) as usize;
        let Some(literal) = context.literals.get(which as usize).copied() else {
            return false;
        };
        let Some(slot) = Value(literal).as_slot() else {
            return false;
        };
        let Some(text) = context.text_at(slot) else {
            return false;
        };
        let spelled = super::TYPE_NAMES[name];
        text.len() == spelled.len() && text.starts_with_ascii(spelled)
    })
}

/// Which of the nine `typeof` answers a value has, without building the string.
///
/// Split out of [`type_of`] so that [`type_of_is`] asks the same question and
/// cannot come to answer it differently — the whole of `typeof` is this match,
/// and a second copy of it is how `typeof x` and `typeof x === "…"` start
/// disagreeing about a value nobody tested.
fn type_name_of(context: &mut Context, value: u64) -> TypeName {
    match Value(value).kind() {
            Kind::Float | Kind::Int => TypeName::Number,
            Kind::Bool => TypeName::Boolean,
            Kind::Singleton(number) => {
                if number == context.singletons.undefined {
                    TypeName::Undefined
                } else {
                    // `typeof null` is "object".
                    TypeName::Object
                }
            }
            // The two the language declared for itself. Answered from the TAG,
            // with no side table consulted — which is the whole difference
            // between a primitive and a cell wearing a marker, and the reason
            // `typeof` on a symbol used to need a lookup and now does not.
            Kind::Client { tag, .. } if tag == context.kinds.symbol => TypeName::Symbol,
            Kind::Client { tag, .. } if tag == context.kinds.bigint => TypeName::Bigint,
            // A tag the language declared and this was never told about. There
            // is no honest answer, and `"undefined"` is the one that makes a
            // wiring mistake look like a value.
            Kind::Client { .. } => TypeName::Unknown,
            Kind::Reference(slot) => {
                let slot = slot as u32;
                match context.region.type_of(slot) {
                    _ if context.callable_at(slot).is_some() => TypeName::Function,
                    // A string.s cell, which `shape_of` already refuses to
                    // treat as an object for exactly this reason: what a
                    // reference IS is readable from beside the cell rather
                    // than from the encoding.
                    _ if context.text_at(slot).is_some() => TypeName::String,
                    _ => TypeName::Object,
                }
            }
    }
}

/// Which of [`super::TYPE_NAMES`] an answer is.
///
/// A name rather than an index literal at each arm: the arms and the table are
/// two lists that have to agree, and `TYPE_NAMES[3]` written in a match arm is
/// the spelling where they come to disagree silently.
#[derive(Clone, Copy)]
enum TypeName {
    Number,
    Boolean,
    Undefined,
    Object,
    Symbol,
    Bigint,
    String,
    Function,
    Unknown,
}

/// The string for one of the nine, built on first use and kept for the run.
///
/// See [`super::Context::type_names`] for why this is a cache at all: the
/// answer is one of nine constant words and building one ALLOCATES.
fn type_name(context: &mut Context, name: TypeName) -> u64 {
    let at = name as usize;
    if let Some(held) = context.type_names[at] {
        return held;
    }
    let made = context
        .intern_value(Str::from_str(super::TYPE_NAMES[at]))
        .bits();
    context.type_names[at] = Some(made);
    made
}
