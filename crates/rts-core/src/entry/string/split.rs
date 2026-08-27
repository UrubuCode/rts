//! `split`, in the three shapes the specification gives it.
//!
//! # Why one file rather than a fast path beside the method
//!
//! The specification states `split` as three rules that barely resemble each
//! other — an ABSENT separator is the whole string as one piece, an EMPTY one
//! cuts between every code unit and produces no trailing piece, and anything
//! else searches — and the third is the only one that needs the subject as Rust
//! text at all. They were in two files, and each restated the others' bounds:
//! this file said "the empty separator is the specification's other rule, not a
//! degenerate case of this one — see the module doc", and that doc was in
//! `pattern.rs`. One rule, two places, is what this crate keeps refusing.
//!
//! # What the code-unit rules buy beyond tidiness
//!
//! They stop needing valid Unicode. `"a\u{D83D}b".split("")` went through
//! `Str::to_rust`, which answers nothing for a lone surrogate, so the whole call
//! answered `undefined`; the version that got as far as producing pieces built
//! each one with `String::from_utf16_lossy`, which turns half a pair into
//! U+FFFD — a DIFFERENT string, silently. `split("")` cutting between units is
//! precisely the operation that produces lone surrogates, so it is the one that
//! must not be written over text that cannot hold them.
//!
//! # What the narrow path replaces, and what the measurement said
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
//! left is the interned cell itself, which is the same cost `slice` pays.

use super::super::native::Native;
use super::super::with_current;
use super::pattern::{pattern_of, scan};
use super::{absent, arg_units, nothing, text_of};
use crate::text::Str;
use crate::value::Value;

/// The one method this file installs.
pub(super) const NATIVES: &[(&str, Native)] = &[("split", split)];

/// `s.split(separator, limit)`.
///
/// # Why the general path keeps `to_rust`, when five string methods stopped
///
/// Because its cost is not the input. Measured, release, 2026-08-12, 5e5 calls:
/// a subject of 8 pieces takes 2.58 us and one of 64 takes 14.1 us — 220 to 320
/// ns PER PIECE and roughly flat, so the work scales with what comes out rather
/// than with what goes in. A narrow path over the subject would touch the
/// smaller half of that, and [`narrow`] is that path for the calls where it is
/// the whole of it.
extern "C" fn split(_e: u64, this: u64, separator: u64, limit: u64, _a2: u64, _a3: u64) -> u64 {
    // `ToString(RequireObjectCoercible(this))`, before any borrow — see
    // `super::coerce_receiver`.
    let Some(this) = super::coerce_receiver(this) else {
        return super::refused();
    };
    // `Symbol.split` FIRST, before anything decides the separator is text —
    // see [`super::pattern::hooked`] for why the order is the specification's
    // and not a preference.
    if let Some(answered) = super::pattern::hooked(this, separator, super::super::symbol::SPLIT, Some(limit)) {
        return answered;
    }
    // `ToUint32(limit)` — and the `ToPrimitive` it starts with, which is why it
    // is HERE and not inside `listed`: an object limit runs user code, and every
    // path below is inside a borrow. The hook above still sees the argument
    // unconverted, which is what the specification hands it.
    let limit = super::super::primitive::to_primitive(limit, crate::coerce::Hint::Number);
    let limit = with_current(|context| wanted(context, limit));
    // Narrow subject, narrow literal separator: the pieces are slices of the
    // subject until each becomes a cell, and none of the allocation below
    // happens.
    if let Some(parts) = with_current(|context| narrow(context, this, separator)) {
        return listed(parts, limit);
    }
    // The two rules stated over code units, which is what lets them answer for
    // a subject that is not valid Unicode.
    if let Some(parts) = with_current(|context| by_units(context, this, separator)) {
        return listed(parts, limit);
    }
    let collected = with_current(|context| {
        let subject = text_of(context, this)?.to_rust()?;
        let sought = pattern_of(context, separator)?;
        let found = scan(context, &subject, &sought, true);
        let mut pieces = Vec::new();
        let mut at = 0;
        for one in &found {
            // Um match VAZIO onde a peça anterior acabou, ou no fim do sujeito,
            // não separa nada — a especificação avança sem cortar. Cortar ali
            // produzia peças vazias a mais nas duas pontas.
            if one.from() == one.to() && (one.from() == at || one.from() == subject.len()) {
                continue;
            }
            pieces.push(Some(subject[at..one.from()].to_string()));
            // As capturas entram ENTRE as peças, que é o que faz
            // `"a1b22c".split(/(\d+)/)` responder `["a","1","b","22","c"]`. Eram
            // deitadas fora, e com elas metade do que o `split` com grupos serve
            // para fazer.
            pieces.extend(one.groups.iter().skip(1).cloned());
            at = one.to();
        }
        pieces.push(Some(subject[at..].to_string()));
        Some(pieces)
    });

    let Some(mut pieces) = collected else {
        return with_current(|context| nothing(context));
    };
    if let Some(count) = limit {
        pieces.truncate(count);
    }
    let array = super::super::array::array_new(pieces.len() as i64);
    with_current(|context| {
        super::pattern::fill(context, array, pieces);
        array
    })
}

/// The two rules that are stated over code units rather than over text.
///
/// An **absent** separator is the whole string as one piece — not every
/// character, which is what an empty separator means. The two are a sentence
/// apart in the specification and a common confusion.
///
/// An **empty** separator cuts between every code unit, and by UNIT rather than
/// by character: `[..."a😀"]` has two elements and `"a😀".split("")` has three,
/// because the pair comes apart. That is what makes this the one operation in
/// the language whose ordinary output is a lone surrogate.
///
/// `None` for every other separator, which sends the caller to the scan.
fn by_units(context: &super::Context, this: u64, separator: u64) -> Option<Vec<Str>> {
    let units = arg_units(context, this)?;
    if absent(context, separator) {
        return Some(vec![Str::from_utf16(&units)]);
    }
    // A regular expression is a slot holding text too — its source — so an
    // empty pattern would look like an empty separator and take this path,
    // where the scan is what states what `/(?:)/` matches.
    if Value(separator)
        .as_slot()
        .is_some_and(|cell| context.regexp_at(cell).is_some())
    {
        return None;
    }
    if !arg_units(context, separator)?.is_empty() {
        return None;
    }
    // No trailing empty piece, which is the other half of the empty
    // separator's rule: `"ab".split("")` is two pieces, where a scan finding an
    // empty match at every position would answer three.
    Some(units.into_iter().map(|unit| Str::from_utf16(&[unit])).collect())
}

/// The pieces of `this.split(separator)` for narrow text and a narrow literal
/// separator, or `None` when this call is not one this path answers.
///
/// Borrows the subject rather than copying it: the pieces are slices of it until
/// the moment each becomes a cell, which is the whole of the saving.
fn narrow(context: &super::Context, this: u64, separator: u64) -> Option<Vec<Str>> {
    let subject_cell = Value(this).as_slot()?;
    let separator_cell = Value(separator).as_slot()?;
    // A regular expression is a slot holding text too, so this has to be asked
    // before the text is read rather than after.
    if context.regexp_at(separator_cell).is_some() {
        return None;
    }
    let subject = context.text_at(subject_cell)?.narrow()?;
    let needle = context.text_at(separator_cell)?.narrow()?;
    // The empty separator is [`by_units`]'s rule, not a degenerate case of this
    // one.
    if needle.is_empty() {
        return None;
    }

    // The same `memmem` two-way search with an SIMD prefilter that the general
    // scan uses, over the bytes directly rather than over a `str` rebuilt from
    // them.
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(offset) = memchr::memmem::find(&subject[at..], needle) {
        found.push(Str::from_latin1(&subject[at..at + offset]));
        at += offset + needle.len();
    }
    // The tail is a piece whether or not it is empty: `"a,".split(",")` is two
    // pieces, and a loop that only pushed on a match would answer one.
    found.push(Str::from_latin1(&subject[at..]));
    Some(found)
}

/// How many pieces the caller asked for — `ToUint32(limit)`, or `None` for an
/// absent limit.
///
/// # Why `ToUint32` and not "a non-negative number"
///
/// That is what the specification names, and the difference is not pedantry:
/// `ToUint32` takes the value **modulo 2³²**, so `split(",", 2 ** 32)` asks for
/// ZERO pieces and `split(",", 2 ** 32 + 3)` asks for three. It also reads a
/// string, a boolean and `null` as numbers, where reading the raw value made
/// every one of those "not a number" and therefore unlimited.
///
/// Measured against Node and Bun by `string/claude-split-limit-touint32`:
/// fourteen of its rows answered the whole five-piece list, including
/// `split(",", "2")` and `split(",", NaN)`.
///
/// `NaN` and both infinities are zero, which `ToUint32` gives for free and a
/// hand-written clamp would have had to remember; the argument arrives already
/// through `ToPrimitive`, so an object is a number by the time it is here.
fn wanted(context: &super::super::Context, limit: u64) -> Option<usize> {
    if super::absent(context, limit) {
        return None;
    }
    let number = super::super::operators::as_number(context, Value(limit)).unwrap_or(f64::NAN);
    Some(super::super::bitwise::to_uint32(number) as usize)
}

/// Pieces already built, as the array `split` answers.
///
/// Shared by both paths above, which is what keeps `limit` from being applied
/// in one and forgotten in the other — it WAS forgotten, in the two rules
/// [`by_units`] now states, so `"abc".split(undefined, 0)` answered one piece
/// where the language answers none.
fn listed(mut parts: Vec<Str>, limit: Option<usize>) -> u64 {
    // Truncated before anything is allocated on the heap, so a limit smaller
    // than the number of pieces does not pay for the pieces it discards.
    if let Some(count) = limit {
        parts.truncate(count);
    }
    // Outside the borrow: making the array may collect, and a collection needs
    // the context the callers above were holding.
    let array = super::super::array::array_new(parts.len() as i64);
    // Written STRAIGHT INTO the array, one piece at a time.
    //
    // Interning a piece ALLOCATES and an allocation collects, so the pieces
    // made so far have to be reachable — and a piece that has landed in the
    // array is reachable through it, where a piece sitting in a second `Vec` on
    // the Rust heap was not. That second vector was also a second allocation
    // for one list, thrown away against the one `array_new` had already sized.
    with_current(|context| {
        let Some(cell) = Value(array).as_slot() else {
            return array;
        };
        for (at, text) in parts.into_iter().enumerate() {
            let value = context.intern_value(text).bits();
            if let Some(elements) = context.elements_at_mut(cell) {
                elements[at] = value;
            }
        }
        array
    })
}
