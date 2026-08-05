//! The string methods that do not take a pattern.
//!
//! # Why every one of these counts code units
//!
//! Because a JavaScript string is a sequence of UTF-16 code units, and every
//! index in this file is a position in that sequence. Working in bytes would
//! make `"é".slice(1)` cut a character in half and `"😀".length` disagree with
//! `"😀".charAt(1)`; working in `char`s would make both of those *look* right
//! and answer differently from every other engine.
//!
//! So the receiver becomes `Vec<u16>` at the top of each method and the answer
//! is built from units. That is a copy per call, which is a real cost and the
//! honest starting point: the alternative is indexing into the two layouts a
//! `Str` can have from seventeen places, and a rope-aware version of that
//! belongs after something measures it.

use super::super::with_current;
use super::super::native::Native;
use super::{absent, answer, arg_units, nothing, relative, text_of, units_of};
use crate::value::Value;

/// What a string's prototype holds, apart from the pattern methods.
pub(super) const NATIVES: &[(&str, Native)] = &[
    ("charAt", char_at),
    ("charCodeAt", char_code_at),
    ("at", at),
    ("indexOf", index_of),
    ("lastIndexOf", last_index_of),
    ("includes", includes),
    ("startsWith", starts_with),
    ("endsWith", ends_with),
    ("slice", slice),
    ("substring", substring),
    ("toUpperCase", to_upper_case),
    ("toLowerCase", to_lower_case),
    ("trim", trim),
    ("repeat", repeat),
    ("concat", concat),
    ("padStart", pad_start),
    ("padEnd", pad_end),
];

/// `s.charAt(i)`.
///
/// Out of range is the **empty string**, where `s[i]` is `undefined`. That
/// difference is the whole reason both spellings exist, and an implementation
/// that answered the same for both would make `s.charAt(9) === ""` false.
extern "C" fn char_at(_e: u64, this: u64, index: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some(units) = units_of(context, this) else {
            return nothing(context);
        };
        let at = Value(index).numeric().unwrap_or(0.0);
        if at < 0.0 || at >= units.len() as f64 {
            return answer(context, &[]);
        }
        answer(context, &units[at as usize..at as usize + 1])
    })
}

/// `s.charCodeAt(i)` — the code unit as a number.
extern "C" fn char_code_at(_e: u64, this: u64, index: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some(units) = units_of(context, this) else {
            return nothing(context);
        };
        let at = Value(index).numeric().unwrap_or(0.0);
        if at < 0.0 || at >= units.len() as f64 {
            // `NaN`, not `undefined`. A program comparing the result to a number
            // gets false either way; one doing arithmetic with it gets `NaN`
            // where `undefined` would also give `NaN` — but `typeof` tells them
            // apart, and the language says which it is.
            return Value::from_f64(f64::NAN).bits();
        }
        Value::from_f64(f64::from(units[at as usize])).bits()
    })
}

/// `s.at(i)` — like the index, but negative counts from the end.
extern "C" fn at(_e: u64, this: u64, index: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some(units) = units_of(context, this) else {
            return nothing(context);
        };
        let asked = Value(index).numeric().unwrap_or(0.0);
        let at = if asked < 0.0 {
            units.len() as f64 + asked
        } else {
            asked
        };
        // Out of range is `undefined` here and the empty string for `charAt`.
        // Not a nicety: `at` was added to the language precisely because the
        // older method could not say "there is nothing there".
        if at < 0.0 || at >= units.len() as f64 {
            return nothing(context);
        }
        answer(context, &units[at as usize..at as usize + 1])
    })
}

/// `s.indexOf(t, from)` — where `t` first occurs, or -1.
extern "C" fn index_of(_e: u64, this: u64, search: u64, from: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let (Some(units), Some(needle)) = (units_of(context, this), arg_units(context, search))
        else {
            return nothing(context);
        };
        let start = relative(Value(from).numeric().unwrap_or(0.0).max(0.0), units.len());
        let found = find(&units, &needle, start);
        Value::from_f64(found.map_or(-1.0, |at| at as f64)).bits()
    })
}

/// `s.lastIndexOf(t)` — where it last occurs, or -1.
extern "C" fn last_index_of(_e: u64, this: u64, search: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let (Some(units), Some(needle)) = (units_of(context, this), arg_units(context, search))
        else {
            return nothing(context);
        };
        let mut found = None;
        let mut from = 0;
        while let Some(at) = find(&units, &needle, from) {
            found = Some(at);
            from = at + 1;
        }
        Value::from_f64(found.map_or(-1.0, |at| at as f64)).bits()
    })
}

/// `s.includes(t)`.
extern "C" fn includes(_e: u64, this: u64, search: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let (Some(units), Some(needle)) = (units_of(context, this), arg_units(context, search))
        else {
            return nothing(context);
        };
        Value::from_bool(find(&units, &needle, 0).is_some()).bits()
    })
}

/// `s.startsWith(t)`.
extern "C" fn starts_with(_e: u64, this: u64, search: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let (Some(units), Some(needle)) = (units_of(context, this), arg_units(context, search))
        else {
            return nothing(context);
        };
        Value::from_bool(units.starts_with(&needle)).bits()
    })
}

/// `s.endsWith(t)`.
extern "C" fn ends_with(_e: u64, this: u64, search: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let (Some(units), Some(needle)) = (units_of(context, this), arg_units(context, search))
        else {
            return nothing(context);
        };
        Value::from_bool(units.ends_with(&needle)).bits()
    })
}

/// `s.slice(from, to)` — negative counts from the end.
extern "C" fn slice(_e: u64, this: u64, from: u64, to: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some(units) = units_of(context, this) else {
            return nothing(context);
        };
        let start = relative(Value(from).numeric().unwrap_or(0.0), units.len());
        let end = if absent(context, to) {
            units.len()
        } else {
            relative(Value(to).numeric().unwrap_or(0.0), units.len())
        };
        // Crossed rather than swapped. `"abc".slice(2, 1)` is empty, where
        // `substring` swaps and answers "b" — the one difference between the two
        // methods, and the reason both are here.
        if start >= end {
            return answer(context, &[]);
        }
        answer(context, &units[start..end])
    })
}

/// `s.substring(from, to)` — negative clamps to zero, and the two swap.
extern "C" fn substring(_e: u64, this: u64, from: u64, to: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some(units) = units_of(context, this) else {
            return nothing(context);
        };
        let length = units.len() as f64;
        let clamp = |value: f64| value.clamp(0.0, length) as usize;
        let start = clamp(Value(from).numeric().unwrap_or(0.0).max(0.0));
        let end = if absent(context, to) {
            units.len()
        } else {
            clamp(Value(to).numeric().unwrap_or(0.0).max(0.0))
        };
        let (start, end) = if start > end { (end, start) } else { (start, end) };
        answer(context, &units[start..end])
    })
}

/// `s.toUpperCase()`.
///
/// Through Rust's own case mapping, which is the full Unicode one — so `"ß"`
/// uppercases to `"SS"` and the result is longer than the receiver. That is
/// what the language says too, and it is why this converts through text rather
/// than mapping units in place.
extern "C" fn to_upper_case(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    mapped(this, str::to_uppercase)
}

/// `s.toLowerCase()`.
extern "C" fn to_lower_case(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    mapped(this, str::to_lowercase)
}

/// `s.trim()`.
///
/// The specification's white space, which is not `char::is_whitespace`: it also
/// includes the zero-width no-break space and excludes nothing Rust includes.
/// Close enough is not a specification, so the one difference is written out.
extern "C" fn trim(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    mapped(this, |text| {
        text.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
            .to_string()
    })
}

/// `s.repeat(n)`.
extern "C" fn repeat(_e: u64, this: u64, count: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some(units) = units_of(context, this) else {
            return nothing(context);
        };
        let asked = Value(count).numeric().unwrap_or(0.0);
        // A negative or absurd count is a `RangeError`. Answering the empty
        // string is the stated gap every operation here has while throwing
        // cannot find a handler — and it is bounded, where multiplying by a
        // number a program chose would let a script exhaust memory.
        if !(0.0..=4096.0).contains(&asked) {
            return answer(context, &[]);
        }
        let mut out = Vec::with_capacity(units.len() * asked as usize);
        for _ in 0..asked as usize {
            out.extend_from_slice(&units);
        }
        answer(context, &out)
    })
}

/// `s.concat(t, …)` — up to the three arguments a call carries beside the
/// receiver.
///
/// The rest of them are refused at the call rather than dropped here, which is
/// the fixed arity stated where it is paid. See `runtime::ARGUMENT_SLOTS`.
extern "C" fn concat(_e: u64, this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    with_current(|context| {
        let Some(mut units) = units_of(context, this) else {
            return nothing(context);
        };
        for argument in [a0, a1, a2, a3] {
            if absent(context, argument) {
                continue;
            }
            let Some(more) = arg_units(context, argument) else {
                return nothing(context);
            };
            units.extend_from_slice(&more);
        }
        answer(context, &units)
    })
}

/// `s.padStart(n, fill)`.
extern "C" fn pad_start(_e: u64, this: u64, width: u64, fill: u64, _a2: u64, _a3: u64) -> u64 {
    padded(this, width, fill, true)
}

/// `s.padEnd(n, fill)`.
extern "C" fn pad_end(_e: u64, this: u64, width: u64, fill: u64, _a2: u64, _a3: u64) -> u64 {
    padded(this, width, fill, false)
}

/// Both padding methods, which differ only in which end grows.
fn padded(this: u64, width: u64, fill: u64, at_start: bool) -> u64 {
    with_current(|context| {
        let Some(units) = units_of(context, this) else {
            return nothing(context);
        };
        let wanted = Value(width).numeric().unwrap_or(0.0);
        if !(0.0..=4096.0).contains(&wanted) || wanted as usize <= units.len() {
            return answer(context, &units);
        }
        let filler = if absent(context, fill) {
            vec![u16::from(b' ')]
        } else {
            match arg_units(context, fill) {
                // An empty fill pads with nothing, which the specification says
                // leaves the string alone rather than looping forever.
                Some(units) if units.is_empty() => return answer(context, &units),
                Some(units) => units,
                None => return nothing(context),
            }
        };
        let missing = wanted as usize - units.len();
        let mut pad = Vec::with_capacity(missing);
        while pad.len() < missing {
            pad.push(filler[pad.len() % filler.len()]);
        }
        let mut out = Vec::with_capacity(wanted as usize);
        if at_start {
            out.extend_from_slice(&pad);
            out.extend_from_slice(&units);
        } else {
            out.extend_from_slice(&units);
            out.extend_from_slice(&pad);
        }
        answer(context, &out)
    })
}

/// A method that is a function of the receiver's text.
///
/// The three that convert through Rust's own text operations rather than
/// indexing units, because case mapping and white space are Unicode tables
/// rather than positions.
fn mapped(this: u64, body: impl FnOnce(&str) -> String) -> u64 {
    with_current(|context| {
        let Some(text) = text_of(context, this).and_then(|text| text.to_rust()) else {
            return nothing(context);
        };
        let produced = body(&text);
        let units: Vec<u16> = produced.encode_utf16().collect();
        answer(context, &units)
    })
}

/// Where a sequence of units first occurs at or after a position.
///
/// Written once because five methods search, and five copies is where one of
/// them would disagree about the empty needle — which occurs at every position,
/// including the end.
fn find(haystack: &[u16], needle: &[u16], from: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(from.min(haystack.len()));
    }
    if needle.len() > haystack.len() {
        return None;
    }
    (from..=haystack.len() - needle.len()).find(|&at| &haystack[at..at + needle.len()] == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_needle_occurs_everywhere_including_the_end() {
        // `"abc".indexOf("")` is 0 and `"abc".indexOf("", 9)` is 3, not -1. The
        // corner that a search written as a loop over windows gets wrong,
        // because there are no windows of width zero.
        let haystack: Vec<u16> = "abc".encode_utf16().collect();
        assert_eq!(find(&haystack, &[], 0), Some(0));
        assert_eq!(find(&haystack, &[], 9), Some(3));
    }

    #[test]
    fn a_needle_longer_than_the_haystack_is_absent_rather_than_a_panic() {
        let haystack: Vec<u16> = "ab".encode_utf16().collect();
        let needle: Vec<u16> = "abcd".encode_utf16().collect();
        assert_eq!(find(&haystack, &needle, 0), None);
    }
}
