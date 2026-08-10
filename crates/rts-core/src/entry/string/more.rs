//! The string methods left over, and the two `String` statics.
//!
//! # Why the two lists are in one file
//!
//! They are the same decision seen from both ends. `String.fromCharCode` builds
//! a string out of numbers and `s.codePointAt` takes one apart, and both have to
//! agree about what a surrogate pair is — a file each is where one of them would
//! learn about pairs and the other would not, which is `"😀".codePointAt(0)`
//! answering 55357 while `String.fromCodePoint(128512)` answers two units.
//!
//! # What is answered rather than thrown
//!
//! `normalize` and `localeCompare` are the two here that would need a Unicode
//! table this crate does not carry. Neither is stubbed silently: each says in
//! its own documentation what it does instead and why that is the honest answer
//! rather than the convenient one.

use super::super::native::Native;
use super::super::with_current;
use super::{absent, answer, arg_units, nothing, relative, text_of, units_of};
use crate::value::Value;

/// What a string's prototype holds beyond the basic and pattern methods.
pub(super) const NATIVES: &[(&str, Native)] = &[
    ("codePointAt", code_point_at),
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

/// What `String` itself holds.
///
/// Statics, because they build a string out of nothing — there is no receiver
/// for them to be a method on, which is exactly why the language put them on the
/// constructor rather than on the prototype.
pub(super) const STATICS: &[(&str, Native)] = &[
    ("fromCharCode", from_char_code),
    ("fromCodePoint", from_code_point),
    ("raw", raw),
];

/// `s.codePointAt(i)` — the whole character at a position, not half of one.
///
/// The difference from `charCodeAt` is the entire reason this method exists: at
/// index 0 of `"😀"` that one answers the high surrogate 55357 and this answers
/// 128512. An implementation that shared their bodies would have to pick one,
/// and either choice makes the other method pointless.
extern "C" fn code_point_at(_e: u64, this: u64, index: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some(units) = units_of(context, this) else {
            return nothing(context);
        };
        let asked = Value(index).numeric().unwrap_or(0.0);
        // `undefined` out of range, where `charCodeAt` answers `NaN`. The
        // language says which is which, and they are not interchangeable: only
        // one of them survives `typeof`.
        if asked < 0.0 || asked >= units.len() as f64 {
            return nothing(context);
        }
        let at = asked as usize;
        Value::from_f64(f64::from(point_at(&units, at))).bits()
    })
}

/// `s.trimStart()`.
extern "C" fn trim_start(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    trimmed(this, true, false)
}

/// `s.trimEnd()`.
extern "C" fn trim_end(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    trimmed(this, false, true)
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
        let start = relative(Value(from).numeric().unwrap_or(0.0), units.len());
        let left = units.len() - start;
        let taken = if absent(context, count) {
            left
        } else {
            Value(count)
                .numeric()
                .unwrap_or(0.0)
                .clamp(0.0, left as f64) as usize
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

/// `s.normalize(form)` — the receiver, unchanged.
///
/// Normalisation is a Unicode decomposition table, and this crate does not carry
/// one. Answering the receiver means `"e\u{301}".normalize() === "é"` is false
/// here where the language says true — a **stated** wrong answer, which a
/// program can find.
///
/// The rejected alternative was leaving the method off entirely. That fails
/// worse: `s.normalize()` would be a call on `undefined`, so every program that
/// normalises defensively before comparing — which is what the method is for —
/// would stop running rather than compare unnormalised text. The form argument
/// is accepted and ignored for the same reason.
extern "C" fn normalize(_e: u64, this: u64, _form: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    identity(this)
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

/// `String.fromCharCode(…)` — units, taken one at a time.
///
/// Each argument is truncated to sixteen bits the way `ToUint16` does, so
/// `String.fromCharCode(65601)` is `"A"`. That wrap is not a quirk to be
/// defended against: it is what makes the method total, and a version that
/// rejected out-of-range numbers would throw where the language wraps.
extern "C" fn from_char_code(_e: u64, _this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    with_current(|context| {
        // Past four the compiler spilled the rest into a vector the runtime
        // holds; `arguments_at` is the one place that reads it back. Folding
        // over `carried`'s four slots alone answered `"Hell"` for
        // `String.fromCharCode(72,101,108,108,111)` — the fifth argument was
        // never read, not merely truncated by `ToUint16`.
        let mut units = Vec::new();
        for given in super::super::array_proto::arguments_at(context, 0, [a0, a1, a2, a3]) {
            units.push(as_unit(context, given));
        }
        answer(context, &units)
    })
}

/// `String.fromCodePoint(…)` — characters, which may be two units each.
///
/// The pair with `fromCharCode`, and the reason both exist: `fromCharCode` is
/// given units and this is given characters, so `String.fromCodePoint(128512)`
/// is one emoji of length two where `String.fromCharCode(128512)` is one unit of
/// the wrong character.
extern "C" fn from_code_point(_e: u64, _this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    with_current(|context| {
        let mut units = Vec::new();
        // Same fix as `from_char_code`: past four, read the spilled vector
        // rather than the four convention slots alone.
        for given in super::super::array_proto::arguments_at(context, 0, [a0, a1, a2, a3]) {
            let number = super::super::operators::as_number(context, Value(given))
                .unwrap_or(f64::NAN);
            // A code point that is not a whole number in range is a `RangeError`.
            // The empty string is the same stated gap `repeat` takes for the same
            // reason — throwing cannot find a handler here — and it is bounded,
            // where letting the number through would build lone surrogates that
            // compare equal to nothing.
            if !(0.0..=1_114_111.0).contains(&number) || number.fract() != 0.0 {
                return answer(context, &[]);
            }
            let point = number as u32;
            if point < 0x10000 {
                units.push(point as u16);
            } else {
                let shifted = point - 0x10000;
                units.push(0xD800 + (shifted >> 10) as u16);
                units.push(0xDC00 + (shifted & 0x3FF) as u16);
            }
        }
        answer(context, &units)
    })
}

/// The receiver itself, when it is a string.
///
/// The receiver's own value rather than a fresh string built from its units:
/// interning already made every equal string one cell, and re-interning would be
/// a lookup and a copy to arrive back where it started. Written once because the
/// three methods that are the identity must not come to differ about what a
/// non-string receiver answers.
fn identity(this: u64) -> u64 {
    with_current(|context| match units_of(context, this) {
        Some(_) => this,
        None => nothing(context),
    })
}

/// The code point beginning at a position, joining a surrogate pair.
fn point_at(units: &[u16], at: usize) -> u32 {
    let first = units[at];
    let paired = (0xD800..0xDC00).contains(&first)
        && at + 1 < units.len()
        && (0xDC00..0xE000).contains(&units[at + 1]);
    if paired {
        0x10000 + ((u32::from(first) - 0xD800) << 10) + (u32::from(units[at + 1]) - 0xDC00)
    } else {
        u32::from(first)
    }
}

/// Both one-sided trims, which differ only in which end is eaten.
fn trimmed(this: u64, at_start: bool, at_end: bool) -> u64 {
    with_current(|context| {
        let Some(text) = text_of(context, this).and_then(|text| text.to_rust()) else {
            return nothing(context);
        };
        let mut view = text.as_str();
        if at_start {
            view = view.trim_start_matches(white_space);
        }
        if at_end {
            view = view.trim_end_matches(white_space);
        }
        let units: Vec<u16> = view.encode_utf16().collect();
        answer(context, &units)
    })
}

/// The specification's white space.
///
/// Rust's own set plus the zero-width no-break space, which the language counts
/// and `char::is_whitespace` does not. `basic::trim` spells the same predicate
/// inline; this is the rule written twice that the crate keeps refusing, and the
/// two should become one call the next time that file is opened — it is stated
/// here rather than fixed here because this change may not edit that file.
fn white_space(character: char) -> bool {
    character.is_whitespace() || character == '\u{feff}'
}

/// One argument as a code unit, the way `ToUint16` produces one.
fn as_unit(context: &super::Context, value: u64) -> u16 {
    let number = super::super::operators::as_number(context, Value(value)).unwrap_or(f64::NAN);
    if !number.is_finite() {
        // `NaN` and the infinities are zero under `ToUint16`, which is the
        // language rather than a fallback: the conversion truncates first, and
        // there is nothing to truncate.
        return 0;
    }
    (number.trunc() as i64).rem_euclid(65536) as u16
}

/// `s.isWellFormed()` — whether every surrogate is part of a pair.
///
/// A JavaScript string is a sequence of code units and may legally hold half a
/// pair; text handed to anything that speaks UTF-8 may not. That is the whole
/// reason the pair of methods exists, and it is why they cannot be written over
/// `char`s: converting first is what would hide the answer.
extern "C" fn is_well_formed(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let well = with_current(|context| {
        units_of(context, this).is_some_and(|units| lone(&units).is_none())
    });
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

/// `String.raw(strings, a, b, c)` — a tagged template's text, escapes unread.
///
/// The `raw` property of the first argument, joined with the substitutions
/// between. Three of them, because the convention carries four slots and the
/// strings array takes one — a template with more substitutions than that is
/// refused at the call site rather than losing them here.
extern "C" fn raw(_e: u64, _this: u64, strings: u64, a: u64, b: u64, c: u64) -> u64 {
    let key = with_current(|context| {
        context.intern_value(crate::text::Str::from_str("raw")).bits()
    });
    let parts = super::super::computed::get_indexed(strings, key);
    let (parts, absent) = with_current(|context| {
        let held = Value(parts)
            .as_slot()
            .and_then(|cell| context.elements_at(cell).cloned())
            .unwrap_or_default();
        (held, nothing(context))
    });

    let mut out: Vec<u16> = Vec::new();
    for (at, part) in parts.iter().enumerate() {
        with_current(|context| {
            if let Some(units) = arg_units(context, *part) {
                out.extend(units);
            }
        });
        // A substitution goes BETWEEN parts, so the last part has none after it
        // — which is what makes `String.raw` of a template with no substitutions
        // the template itself rather than a trailing `undefined`.
        let Some(value) = [a, b, c].get(at).copied() else {
            continue;
        };
        if at + 1 >= parts.len() || value == absent {
            continue;
        }
        with_current(|context| {
            if let Some(units) = arg_units(context, value) {
                out.extend(units);
            }
        });
    }
    with_current(|context| answer(context, &out))
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

    #[test]
    fn a_surrogate_pair_is_one_code_point_and_two_units() {
        // The corner that makes `codePointAt` a different method from
        // `charCodeAt` rather than a longer spelling of it.
        let units: Vec<u16> = "😀".encode_utf16().collect();
        assert_eq!(units.len(), 2);
        assert_eq!(point_at(&units, 0), 128_512);
        // At the second unit there is no pair to join, so the low surrogate
        // stands for itself — which is what the language answers too.
        assert_eq!(point_at(&units, 1), u32::from(units[1]));
    }

    #[test]
    fn a_lone_high_surrogate_at_the_end_is_not_read_past() {
        // A string may legally end mid-pair. Joining without the bounds check is
        // a read one past the end, which is the difference between a wrong
        // answer and a crash.
        let units = [0xD83Du16];
        assert_eq!(point_at(&units, 0), 0xD83D);
    }
}
