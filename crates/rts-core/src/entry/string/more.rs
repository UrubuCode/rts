//! The string methods that are neither indexed, searched, nor patterned.
//!
//! # What the three trims have in common, and why they are one function here
//!
//! The whitespace set. `trim` lived in [`super::basic`] with the predicate
//! spelled inline and `trimStart`/`trimEnd` lived here with it spelled again —
//! two statements of the same table, which this file's own comment named as
//! "the rule written twice that the crate keeps refusing" and left for the next
//! change to fix. This is that change: all three go through [`trimmed`], and the
//! set itself is [`crate::text::is_white_space`], which is stated once for the
//! whole crate.
//!
//! Moving it also fixed it. The inline predicates were `char::is_whitespace`
//! plus U+FEFF, and `char::is_whitespace` includes **U+0085 NEXT LINE**, which
//! the language does not trim — so `"\u{85}x".trim()` lost a character here and
//! keeps it everywhere else.
//!
//! # Why the trims read UNITS where they used to read text
//!
//! Because `Str::to_rust` answers nothing for a string holding a lone
//! surrogate, so `(" " + half).trim()` answered `undefined` — a missing method
//! rather than a wrong answer. No code point in the whitespace set is a
//! surrogate, so asking the question one unit at a time is the same question
//! asked where it can always be answered.

use super::super::native::Native;
use super::super::with_current;
use super::{absent, answer, arg_units, nothing, relative, units_of};
use crate::text::{Form, is_white_space, normalized};
use crate::value::Value;

/// What a string's prototype holds beyond the indexed, searched and patterned.
pub(super) const NATIVES: &[(&str, Native)] = &[
    ("trim", trim),
    ("trimStart", trim_start),
    ("trimEnd", trim_end),
    ("substr", substr),
    ("localeCompare", locale_compare),
    ("normalize", normalize),
    ("toString", text_value),
    ("valueOf", value_of),
    ("isWellFormed", is_well_formed),
    ("toWellFormed", to_well_formed),
];

/// `s.trim()`.
extern "C" fn trim(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    trimmed(this, true, true)
}

/// `s.trimStart()`.
extern "C" fn trim_start(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    trimmed(this, true, false)
}

/// `s.trimEnd()`.
extern "C" fn trim_end(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    trimmed(this, false, true)
}

/// All three trims, which differ only in which end is eaten.
fn trimmed(this: u64, at_start: bool, at_end: bool) -> u64 {
    with_current(|context| {
        // `arg_units` and not `units_of`, because the receiver of a borrowed
        // string method need not be a string: `String.prototype.trim.call(5)`
        // is `"5"`, and the language converts before it trims.
        let Some(units) = arg_units(context, this) else {
            return nothing(context);
        };
        let mut from = 0;
        let mut to = units.len();
        if at_start {
            while from < to && is_white_space(units[from]) {
                from += 1;
            }
        }
        if at_end {
            while to > from && is_white_space(units[to - 1]) {
                to -= 1;
            }
        }
        answer(context, &units[from..to])
    })
}

/// `s.substr(from, count)` — a length, not an end.
///
/// The one method here the specification keeps only for compatibility, and it
/// earns its place by being the one programs get wrong: `"abcd".substr(1, 2)` is
/// `"bc"` where `"abcd".substring(1, 2)` is `"b"`. A second argument meaning two
/// different things is why both are implemented rather than aliased.
extern "C" fn substr(_e: u64, this: u64, from: u64, count: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some(units) = units_of(context, this) else {
            return nothing(context);
        };
        let start = relative(super::integer_arg(context, from), units.len());
        let left = units.len() - start;
        let taken = if absent(context, count) {
            left
        } else {
            super::integer_arg(context, count).clamp(0.0, left as f64) as usize
        };
        answer(context, &units[start..start + taken])
    })
}

/// `a.localeCompare(b)` — negative, zero or positive.
///
/// **By code unit order, not by collation.** There is no collation table in this
/// crate and none is coming: a real one is per-locale data measured in hundreds
/// of kilobytes, and it fails rule 1 of the crate — it is not something every
/// target has, it is something a program has to be shipped with.
///
/// So this answers the same order `<` does. That is wrong for a program sorting
/// names in a language with accents, and it is wrong *visibly and consistently*:
/// `"é".localeCompare("z")` is positive here and negative under a French
/// collation, and a program that sorted with it gets one stable order rather
/// than an order that changes with the locale it happens to run under. The
/// alternative — a table for one locale, dressed as all of them — is the version
/// that is right in testing and wrong in production.
///
/// Normalisation is a different question and is answered: see [`normalize`].
/// A decomposition table has ONE answer everywhere, where a collation has one
/// per language, which is the whole reason this crate carries the first and
/// refuses the second.
extern "C" fn locale_compare(_e: u64, this: u64, other: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let (Some(units), Some(against)) = (units_of(context, this), arg_units(context, other))
        else {
            return nothing(context);
        };
        let order = match ascii_locale_order(&units, &against) {
            std::cmp::Ordering::Less => -1.0,
            std::cmp::Ordering::Equal => 0.0,
            std::cmp::Ordering::Greater => 1.0,
        };
        Value::from_f64(order).bits()
    })
}

/// Case-insensitive primary order, ASCII case as the tiebreak — the one piece
/// of collation this crate's plain-code-unit order does not need a table for.
///
/// Two strings that differ only in ASCII case are not equal under a real
/// collation, and the direction is the opposite of code-unit order:
/// `"a".localeCompare("A")` is `-1` in Node/ICU (lowercase sorts first),
/// where `'a'` (0x61) is numerically *greater* than `'A'` (0x41). So this
/// compares case-folded units first, and only when a whole prefix folds equal
/// does it fall back to the first position that differed by case — inverted,
/// so the lowercase side wins.
fn ascii_locale_order(a: &[u16], b: &[u16]) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let fold = |unit: u16| -> u16 {
        match u8::try_from(unit) {
            Ok(byte) if byte.is_ascii_uppercase() => u16::from(byte.to_ascii_lowercase()),
            _ => unit,
        }
    };
    let mut case_tiebreak: Option<Ordering> = None;
    for (&x, &y) in a.iter().zip(b.iter()) {
        let (fx, fy) = (fold(x), fold(y));
        if fx != fy {
            return fx.cmp(&fy);
        }
        if case_tiebreak.is_none() && x != y {
            // Same letter, different case. `x` is the one that is not the
            // fold's own value exactly when it was uppercase.
            case_tiebreak = Some(match x == fx {
                // `x` was already lowercase (folding did nothing to it) and
                // `y` was uppercase: lowercase sorts first.
                true => Ordering::Less,
                false => Ordering::Greater,
            });
        }
    }
    match a.len().cmp(&b.len()) {
        Ordering::Equal => case_tiebreak.unwrap_or(Ordering::Equal),
        by_length => by_length,
    }
}

/// `s.normalize(form)` — the receiver in one of the four Unicode forms.
///
/// # What this replaces, and why the reason it was refused does not hold
///
/// It was the IDENTITY, documented as a stated wrong answer: `"e\u{301}"` and
/// `"\u{e9}"` compared unequal after normalising, which is the exact comparison
/// the method exists to make true. The reason given was that this crate carries
/// no Unicode table — and that reason belongs to `localeCompare`, not here.
/// A collation is per-locale data a program is shipped with; a normalisation is
/// one answer, the same on every target, which is what rule 1 asks of anything
/// in this crate. [`crate::text::normalize`] carries the argument in full.
///
/// A form the language does not name is a `RangeError`, not a silent NFC: a
/// program that misspells `"NFKD"` gets told, where guessing would give it text
/// it did not ask for. Absent means NFC, which is the specification's default.
extern "C" fn normalize(_e: u64, this: u64, form: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let asked = with_current(|context| match absent(context, form) {
        true => Some(Form::Nfc),
        false => super::text_of(context, form)
            .and_then(|text| text.to_rust())
            .and_then(|name| Form::named(&name)),
    });
    let Some(form) = asked else {
        // Outside no borrow to release — `range_error` takes the context's own,
        // and this closure has already given it back.
        super::super::throw::range_error(
            "The normalization form should be one of NFC, NFD, NFKC, NFKD.",
        );
        return with_current(|context| nothing(context));
    };
    with_current(|context| {
        let Some(text) = super::text_of(context, this) else {
            return nothing(context);
        };
        context.intern_value(normalized(&text, form)).bits()
    })
}

/// `s.toString()`.
extern "C" fn text_value(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    identity(this)
}

/// `s.valueOf()`.
///
/// The same as `toString` here and genuinely different in the language, where
/// the receiver may be a wrapper object and the two unwrap it differently. There
/// are no wrappers in this engine — `string/mod.rs` says why — so the receiver is
/// already primitive and both are the identity.
extern "C" fn value_of(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    identity(this)
}

/// The receiver itself, when it is a string.
///
/// The receiver's own value rather than a fresh string built from its units:
/// interning already made every equal string one cell, and re-interning would be
/// a lookup and a copy to arrive back where it started. Written once because the
/// two methods that are the identity must not come to differ about what a
/// non-string receiver answers.
fn identity(this: u64) -> u64 {
    with_current(|context| match units_of(context, this) {
        Some(_) => this,
        None => nothing(context),
    })
}

/// `s.isWellFormed()` — whether every surrogate is part of a pair.
///
/// A JavaScript string is a sequence of code units and may legally hold half a
/// pair; text handed to anything that speaks UTF-8 may not. That is the whole
/// reason the pair of methods exists, and it is why they cannot be written over
/// `char`s: converting first is what would hide the answer.
extern "C" fn is_well_formed(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let well =
        with_current(|context| units_of(context, this).is_some_and(|units| lone(&units).is_none()));
    Value::from_bool(well).bits()
}

/// `s.toWellFormed()` — each lone surrogate replaced by U+FFFD.
///
/// Replaced rather than dropped, which the specification requires and which is
/// the difference a program notices: dropping shifts every later index by one.
extern "C" fn to_well_formed(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some(mut units) = units_of(context, this) else {
            return nothing(context);
        };
        while let Some(at) = lone(&units) {
            units[at] = 0xFFFD;
        }
        answer(context, &units)
    })
}

/// Where the first unpaired surrogate is, if there is one.
///
/// A high surrogate counts as paired only when a low one follows it, and a low
/// one only when a high one precedes it — so the scan cannot be written as
/// "is a surrogate", which would report every valid pair.
fn lone(units: &[u16]) -> Option<usize> {
    let high = |unit: u16| (0xD800..0xDC00).contains(&unit);
    let low = |unit: u16| (0xDC00..0xE000).contains(&unit);
    let mut at = 0;
    while at < units.len() {
        let unit = units[at];
        if high(unit) {
            if at + 1 < units.len() && low(units[at + 1]) {
                at += 2;
                continue;
            }
            return Some(at);
        }
        if low(unit) {
            return Some(at);
        }
        at += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lone_surrogate_is_found_and_a_pair_is_not() {
        // The corner the pair of methods exists for: both halves of a pair are
        // surrogates, so "is a surrogate" is not the question being asked.
        let pair: Vec<u16> = "😀".encode_utf16().collect();
        assert_eq!(lone(&pair), None);
        assert_eq!(lone(&[0xD83Du16]), Some(0));
        assert_eq!(lone(&[0x0061u16, 0xDE00]), Some(1));
    }
}
