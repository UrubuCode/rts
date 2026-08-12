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
//! is built from units. That is a copy per call, which was called "a real cost
//! and the honest starting point" here, with the note that indexing the two
//! layouts a `Str` can have belonged "after something measures it".
//!
//! **Something measured it.** For the three methods that read ONE unit —
//! `charAt`, `charCodeAt`, `at` — the copy is the whole cost, and it makes a
//! scan quadratic: 10 000 characters took 0.86 s, 20 000 took 3.17 s, 40 000
//! took 14.4 s, 80 000 took 63.1 s. Four times per doubling, and the suite file
//! that scans 100 000 never finished at all.
//!
//! Those three now use `super::indexed`, which is `Str::unit_at` — constant time
//! for both layouts, and it already existed. The methods that genuinely walk the
//! whole string still take the copy, because they read all of it anyway.

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
    ("toLocaleUpperCase", to_upper_case),
    ("toLocaleLowerCase", to_lower_case),
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
        let Some(length) = super::length_of(context, this) else {
            return nothing(context);
        };
        let at = Value(index).numeric().unwrap_or(0.0);
        if at < 0.0 || at >= length as f64 {
            return answer(context, &[]);
        }
        match super::indexed(context, this, at as usize) {
            Some(unit) => answer(context, &[unit]),
            None => answer(context, &[]),
        }
    })
}

/// `s.charCodeAt(i)` — the code unit as a number.
extern "C" fn char_code_at(_e: u64, this: u64, index: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some(length) = super::length_of(context, this) else {
            return nothing(context);
        };
        let at = Value(index).numeric().unwrap_or(0.0);
        if at < 0.0 || at >= length as f64 {
            // `NaN`, not `undefined`. A program comparing the result to a number
            // gets false either way; one doing arithmetic with it gets `NaN`
            // where `undefined` would also give `NaN` — but `typeof` tells them
            // apart, and the language says which it is.
            return Value::from_f64(f64::NAN).bits();
        }
        match super::indexed(context, this, at as usize) {
            Some(unit) => Value::from_f64(f64::from(unit)).bits(),
            None => Value::from_f64(f64::NAN).bits(),
        }
    })
}

/// `s.at(i)` — like the index, but negative counts from the end.
extern "C" fn at(_e: u64, this: u64, index: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some(length) = super::length_of(context, this) else {
            return nothing(context);
        };
        let asked = Value(index).numeric().unwrap_or(0.0);
        let at = if asked < 0.0 {
            length as f64 + asked
        } else {
            asked
        };
        // Out of range is `undefined` here and the empty string for `charAt`.
        // Not a nicety: `at` was added to the language precisely because the
        // older method could not say "there is nothing there".
        if at < 0.0 || at >= length as f64 {
            return nothing(context);
        }
        match super::indexed(context, this, at as usize) {
            Some(unit) => answer(context, &[unit]),
            None => nothing(context),
        }
    })
}

/// `s.indexOf(t, from)` — where `t` first occurs, or -1.
extern "C" fn index_of(_e: u64, this: u64, search: u64, from: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        // The narrow path first, and it is the common one: both sides ASCII, so
        // there is nothing to widen and nothing to copy. `units_of` builds a
        // fresh `Vec<u16>` of the WHOLE receiver on every call and throws away
        // the byte layout this crate keeps precisely so it would not have to —
        // a 256-character haystack allocated 512 bytes and widened 256 times to
        // answer where four characters were.
        //
        // Answered from borrowed slices, so neither side allocates at all.
        if let Some(text) = context.text_at(Value(this).as_slot().unwrap_or(u32::MAX))
            && let Some(hay) = text.narrow()
            && let Some(pattern) = super::text_of(context, search)
            && let Some(pin) = pattern.narrow().map(<[u8]>::to_vec)
        {
            let start = relative(Value(from).numeric().unwrap_or(0.0).max(0.0), hay.len());
            let found = find_bytes(hay, &pin, start);
            return Value::from_f64(found.map_or(-1.0, |at| at as f64)).bits();
        }
        let (Some(units), Some(needle)) = (units_of(context, this), arg_units(context, search))
        else {
            return nothing(context);
        };
        let start = relative(Value(from).numeric().unwrap_or(0.0).max(0.0), units.len());
        let found = find(&units, &needle, start);
        Value::from_f64(found.map_or(-1.0, |at| at as f64)).bits()
    })
}

/// `s.lastIndexOf(t, from)` — where it last occurs at or before `from`, or -1.
///
/// # Why this scans backwards rather than forwards keeping the last hit
///
/// It used to walk forwards with `from = at + 1` until `find` answered `None`,
/// and against an EMPTY needle that never happens: `find` answers `Some(from)`
/// for every position, so `from` grew without bound and the program hung — at
/// full CPU, printing nothing. `"abc".lastIndexOf("")` is `3` in the language,
/// and it was the one input that made this loop forever.
///
/// Backwards has no such case: the range is bounded before the search starts,
/// so an empty needle matches at `start` immediately and every other needle
/// stops at the first hit, which is the last one.
extern "C" fn last_index_of(_e: u64, this: u64, search: u64, from: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let (Some(units), Some(needle)) = (units_of(context, this), arg_units(context, search))
        else {
            return nothing(context);
        };
        // `undefined` and `NaN` both mean +infinity here, which is the whole
        // string — and that is NOT the same rule `indexOf` has, where an absent
        // position means 0. The asymmetry is the specification's: one searches
        // forwards from a lower bound, the other backwards from an upper one.
        let limit = match Value(from).numeric() {
            Some(number) if !number.is_nan() => relative(number.max(0.0), units.len()),
            _ => units.len(),
        };
        // The last position a match could START at: past `len - needle.len()`
        // there is not enough string left for one.
        let last = match units.len().checked_sub(needle.len()) {
            Some(last) => last.min(limit),
            None => return Value::from_f64(-1.0).bits(),
        };
        let found = (0..=last)
            .rev()
            .find(|&at| units[at..at + needle.len()] == needle[..]);
        Value::from_f64(found.map_or(-1.0, |at| at as f64)).bits()
    })
}

/// `s.includes(t, from)`.
///
/// The position was accepted and discarded, so `"abc".includes("a", 1)` was
/// `true` — an answer that is wrong in the direction a search is trusted in.
extern "C" fn includes(_e: u64, this: u64, search: u64, from: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let (Some(units), Some(needle)) = (units_of(context, this), arg_units(context, search))
        else {
            return nothing(context);
        };
        let start = relative(Value(from).numeric().unwrap_or(0.0).max(0.0), units.len());
        Value::from_bool(find(&units, &needle, start).is_some()).bits()
    })
}

/// `s.startsWith(t, from)` — whether `t` is there AT `from`.
///
/// Not "somewhere at or after `from`": the position moves where the comparison
/// happens, it does not start a search. Discarding it made
/// `"abc".startsWith("b", 1)` answer false, which is the opposite error to the
/// one `includes` had — both directions, from one dropped argument.
extern "C" fn starts_with(_e: u64, this: u64, search: u64, from: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let (Some(units), Some(needle)) = (units_of(context, this), arg_units(context, search))
        else {
            return nothing(context);
        };
        let start = relative(Value(from).numeric().unwrap_or(0.0).max(0.0), units.len());
        Value::from_bool(units[start..].starts_with(&needle)).bits()
    })
}

/// `s.endsWith(t, end)` — whether `t` ends the string considered to END at
/// `end`.
///
/// The position is an END rather than a start, which is why this cannot share
/// the clamp with `startsWith` above: `"abc".endsWith("b", 2)` is true, because
/// the string considered is `"ab"`. It was discarded, so that answered false.
extern "C" fn ends_with(_e: u64, this: u64, search: u64, end: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let (Some(units), Some(needle)) = (units_of(context, this), arg_units(context, search))
        else {
            return nothing(context);
        };
        let end = match Value(end).numeric() {
            Some(number) if !number.is_nan() => relative(number.max(0.0), units.len()),
            // Absent means the whole string, not zero — the opposite default to
            // `includes`, and for the same reason `lastIndexOf` differs from
            // `indexOf`: this one measures from the far end.
            _ => units.len(),
        };
        Value::from_bool(units[..end].ends_with(&needle)).bits()
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
///
/// `toLocaleUpperCase` and `toLocaleLowerCase` are installed as these same two
/// functions rather than as wrappers. They differ from them in exactly three
/// locales — Turkish, Azeri and Lithuanian dotted `i` — and this crate carries
/// no locale data, so a separate body would be the same mapping under a second
/// name and one more place for the two to drift. The divergence is the locale
/// data, not the dispatch.
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
    let asked = Value(count).numeric().unwrap_or(0.0);
    // A negative count, or one that never terminates, is a `RangeError` —
    // raising is possible now that rule 8's discipline is in place (`repeat`
    // calls no user code, so there is nothing to check first). Raised OUTSIDE
    // the borrow below: `range_error` takes the context's own `RefCell`
    // borrow to build the error object, and taking it a second time while the
    // first is still held is the nested borrow this crate's whole entry layer
    // is arranged to make unreachable — an `extern "C"` frame cannot unwind
    // the panic that follows, so it aborted the process instead of throwing.
    if asked < 0.0 || asked.is_infinite() {
        super::super::throw::range_error("Invalid count value");
        return with_current(|context| nothing(context));
    }
    with_current(|context| {
        let Some(units) = units_of(context, this) else {
            return nothing(context);
        };
        // An absurdly large but finite count is still bounded rather than
        // thrown for: the specification has no upper bound, but multiplying by
        // a number a program chose would let a script exhaust memory, and that
        // is a stated gap this change does not close.
        if asked > 4096.0 {
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
/// The same question over bytes, which is what both sides are whenever the text
/// is ASCII — and it nearly always is.
///
/// Separate from [`find`] rather than generic over the unit type, because the
/// one rule they must agree about is the empty needle and a shared body would
/// hide it. Both answer `from` clamped to the length, and both have a test.
fn find_bytes(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(from.min(haystack.len()));
    }
    if needle.len() > haystack.len() || from > haystack.len() - needle.len() {
        return None;
    }
    // `memchr` is already in the tree through `regex`, and its `memmem` finder
    // is a two-way search with a SIMD prefilter where the naive window compare
    // below is O(n*m). It is NOT used here yet, and the reason is worth stating
    // rather than leaving as an omission: the win this commit measures is the
    // allocation and the widening, not the search, and adding a dependency edge
    // in the same change would make the two impossible to tell apart.
    haystack[from..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|at| at + from)
}

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
