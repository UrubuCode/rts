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
];

/// What `String` itself holds.
///
/// Statics, because they build a string out of nothing — there is no receiver
/// for them to be a method on, which is exactly why the language put them on the
/// constructor rather than on the prototype.
pub(super) const STATICS: &[(&str, Native)] = &[
    ("fromCharCode", from_char_code),
    ("fromCodePoint", from_code_point),
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
        let order = match units.cmp(&against) {
            std::cmp::Ordering::Less => -1.0,
            std::cmp::Ordering::Equal => 0.0,
            std::cmp::Ordering::Greater => 1.0,
        };
        Value::from_f64(order).bits()
    })
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
        let mut units = Vec::new();
        for given in carried(context, [a0, a1, a2, a3]) {
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
        for given in carried(context, [a0, a1, a2, a3]) {
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

/// The arguments a call actually carried.
///
/// Trailing `undefined` is dropped, because the convention pads missing
/// arguments with it and a native cannot tell padding from an argument a program
/// wrote. The divergence, named: `String.fromCharCode(undefined)` is the empty
/// string rather than `"\0"`. The same price the array methods pay, and it
/// disappears with the argument vector rather than with more code here.
fn carried(context: &super::Context, given: [u64; 4]) -> Vec<u64> {
    let mut given = given.to_vec();
    while given.last().is_some_and(|last| absent(context, *last)) {
        given.pop();
    }
    given
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

#[cfg(test)]
mod tests {
    use super::*;

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
