//! The five string methods that look for a literal sequence of units.
//!
//! # Why they are apart from the rest of the basic methods
//!
//! They were in `basic.rs`, which reached 596 lines against this crate's
//! 500-line ceiling. These five are the cohesive half: every one of them asks
//! the same question — where does this run of code units occur — and every one
//! of them has the same two paths, a byte comparison when both sides are narrow
//! and a unit comparison when either is wide. The measurements that decided
//! those paths are per method and are recorded on each.
//!
//! # Why the empty needle is stated once per path and tested twice
//!
//! Because it occurs at EVERY position, including one past the end, and a
//! search written as a loop over windows has no window of width zero. Both
//! [`find`] and [`find_bytes`] answer `from` clamped to the length, and a shared
//! generic body would have hidden that rule rather than stated it.

use super::super::native::Native;
use super::super::with_current;
use super::{arg_units, nothing, relative, units_of};
use crate::value::Value;

/// What a string's prototype holds that searches for literal text.
pub(super) const NATIVES: &[(&str, Native)] = &[
    ("indexOf", index_of),
    ("lastIndexOf", last_index_of),
    ("includes", includes),
    ("startsWith", starts_with),
    ("endsWith", ends_with),
];

/// `s.indexOf(t, from)` — where `t` first occurs, or -1.
extern "C" fn index_of(_e: u64, this: u64, search: u64, from: u64, _a2: u64, _a3: u64) -> u64 {
    // `ToString(RequireObjectCoercible(this))`, before any borrow — see
    // `super::coerce_receiver`.
    let Some(this) = super::coerce_receiver(this) else {
        return super::refused();
    };
    // `ToString(argument)`, outside the borrow for the reason
    // `super::text_arg` states.
    let Some(search) = super::text_arg(search) else {
        return super::refused();
    };
    // ToIntegerOrInfinity must run outside the borrow so arrays and objects can
    // perform their ordinary ToPrimitive conversion instead of becoming NaN.
    let from = super::integer_outside(from);
    with_current(|context| {
        // The narrow path first, and it is the common one: both sides ASCII, so
        // there is nothing to widen and nothing to copy. `units_of` builds a
        // fresh `Vec<u16>` of the WHOLE receiver on every call and throws away
        // the byte layout this crate keeps precisely so it would not have to —
        // a 256-character haystack allocated 512 bytes and widened 256 times to
        // answer where four characters were.
        //
        // Answered from borrowed slices, so neither side allocates at all.
        if let Some((hay, pin)) = narrow_pair(context, this, search) {
            let start = relative(from.max(0.0), hay.len());
            let found = find_bytes(hay, pin, start);
            return Value::from_f64(found.map_or(-1.0, |at| at as f64)).bits();
        }
        let (Some(units), Some(needle)) = (units_of(context, this), arg_units(context, search))
        else {
            return nothing(context);
        };
        let start = relative(from.max(0.0), units.len());
        let found = find(&units, &needle, start);
        Value::from_f64(found.map_or(-1.0, |at| at as f64)).bits()
    })
}

/// `s.lastIndexOf(t, from)` — where it last occurs at or before `from`, or -1.
///
/// # Why this one keeps the wide copy, when its four neighbours do not
///
/// It was given the narrow path and MEASURED SLOWER: 886.7 -> 990.9 ms over 2e6
/// calls on a 256-unit haystack, release, 2026-08-12, two binaries alternated
/// three times. Correct — nine corner cases against node agreed exactly — and
/// slower, so it was reverted.
///
/// The reason is not established, and saying so is the point: the same change
/// bought -58% on `indexOf`, -57% on `includes` and -61% on `startsWith` and
/// `endsWith`, so "borrowing beats copying" is not a rule that holds here
/// without measuring. A plausible story is that a backwards scan from a bound
/// finds its match in a few steps, so the copy it avoids never dominated — but
/// that is a story, and the number is the fact.
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
    // `ToString(RequireObjectCoercible(this))`, before any borrow — see
    // `super::coerce_receiver`.
    let Some(this) = super::coerce_receiver(this) else {
        return super::refused();
    };
    // `ToString(argument)`, outside the borrow for the reason
    // `super::text_arg` states.
    let Some(search) = super::text_arg(search) else {
        return super::refused();
    };
    // Unlike `integer_arg`, this keeps NaN visible: String.lastIndexOf treats
    // it like an omitted upper bound, and the conversion still belongs outside
    // the runtime borrow so object offsets can run their primitive hooks.
    let from = super::super::class_support::to_number(from);
    with_current(|context| {
        let (Some(units), Some(needle)) = (units_of(context, this), arg_units(context, search))
        else {
            return nothing(context);
        };
        // `undefined` and `NaN` both mean +infinity here, which is the whole
        // string — and that is NOT the same rule `indexOf` has, where an absent
        // position means 0. The asymmetry is the specification's: one searches
        // forwards from a lower bound, the other backwards from an upper one.
        //
        // `from` was converted before this borrow. Keep NaN as the far end;
        // otherwise truncate and clamp the requested upper bound.
        let limit = if from.is_nan() {
            units.len()
        } else {
            relative(from.trunc().max(0.0), units.len())
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
    // `ToString(RequireObjectCoercible(this))`, before any borrow — see
    // `super::coerce_receiver`.
    let Some(this) = super::coerce_receiver(this) else {
        return super::refused();
    };
    // `IsRegExp` FIRST, before the argument is read as text — the language
    // refuses the brand rather than stringifying it. See `super::is_regexp`.
    if super::is_regexp(search) {
        super::super::throw::type_error(
            "First argument to String.prototype method must not be a regular expression",
        );
        return super::refused();
    }
    // `ToString(argument)`, outside the borrow for the reason
    // `super::text_arg` states.
    let Some(search) = super::text_arg(search) else {
        return super::refused();
    };
    with_current(|context| {
        if let Some((hay, pin)) = narrow_pair(context, this, search) {
            let start = relative(super::integer_arg(context, from).max(0.0), hay.len());
            return Value::from_bool(find_bytes(hay, pin, start).is_some()).bits();
        }
        let (Some(units), Some(needle)) = (units_of(context, this), arg_units(context, search))
        else {
            return nothing(context);
        };
        let start = relative(super::integer_arg(context, from).max(0.0), units.len());
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
    // `ToString(RequireObjectCoercible(this))`, before any borrow — see
    // `super::coerce_receiver`.
    let Some(this) = super::coerce_receiver(this) else {
        return super::refused();
    };
    // `IsRegExp` FIRST, before the argument is read as text — the language
    // refuses the brand rather than stringifying it. See `super::is_regexp`.
    if super::is_regexp(search) {
        super::super::throw::type_error(
            "First argument to String.prototype method must not be a regular expression",
        );
        return super::refused();
    }
    // `ToString(argument)`, outside the borrow for the reason
    // `super::text_arg` states.
    let Some(search) = super::text_arg(search) else {
        return super::refused();
    };
    with_current(|context| {
        // Narrow on both sides is a slice comparison and nothing else — no
        // widening, no copy of the receiver. The wide path below is unchanged.
        if let Some((hay, pin)) = narrow_pair(context, this, search) {
            let start = relative(super::integer_arg(context, from).max(0.0), hay.len());
            return Value::from_bool(hay[start..].starts_with(pin)).bits();
        }
        let (Some(units), Some(needle)) = (units_of(context, this), arg_units(context, search))
        else {
            return nothing(context);
        };
        let start = relative(super::integer_arg(context, from).max(0.0), units.len());
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
    // `ToString(RequireObjectCoercible(this))`, before any borrow — see
    // `super::coerce_receiver`.
    let Some(this) = super::coerce_receiver(this) else {
        return super::refused();
    };
    // `IsRegExp` FIRST, before the argument is read as text — the language
    // refuses the brand rather than stringifying it. See `super::is_regexp`.
    if super::is_regexp(search) {
        super::super::throw::type_error(
            "First argument to String.prototype method must not be a regular expression",
        );
        return super::refused();
    }
    // `ToString(argument)`, outside the borrow for the reason
    // `super::text_arg` states.
    let Some(search) = super::text_arg(search) else {
        return super::refused();
    };
    with_current(|context| {
        if let Some((hay, pin)) = narrow_pair(context, this, search) {
            // The default is the WHOLE string and not zero, which is the
            // opposite of `includes` — this one measures from the far end. The
            // question is whether the argument was WRITTEN, not what it
            // converts to: `endsWith("a", NaN)` considers the empty string and
            // is false, where the absent form considers all of it.
            let stop = match super::absent(context, end) {
                true => hay.len(),
                false => relative(super::integer_arg(context, end).max(0.0), hay.len()),
            };
            return Value::from_bool(hay[..stop].ends_with(pin)).bits();
        }
        let (Some(units), Some(needle)) = (units_of(context, this), arg_units(context, search))
        else {
            return nothing(context);
        };
        let end = match super::absent(context, end) {
            // Absent means the whole string, not zero — the opposite default to
            // `includes`, and for the same reason `lastIndexOf` differs from
            // `indexOf`: this one measures from the far end.
            true => units.len(),
            false => relative(super::integer_arg(context, end).max(0.0), units.len()),
        };
        Value::from_bool(units[..end].ends_with(&needle)).bits()
    })
}

/// Both sides as BYTES, when both are narrow.
///
/// The shape `index_of` grew first and four more searches want: a receiver and
/// an argument that are both ASCII need no widening and no copy of the
/// receiver, which is what `units_of` does on every call — a 256-character
/// haystack allocating 512 bytes and widening 256 units to answer where four
/// characters are.
///
/// `text_arg` has already converted any dynamic argument into a bare string
/// value, so both sides can be borrowed directly from the context. This avoids
/// cloning the short needle on every call while retaining the same conversion
/// boundary for numbers, wrappers, symbols and objects.
///
/// `None` when either side is wide, which sends the caller to the path it
/// already had.
fn narrow_pair(context: &super::Context, this: u64, search: u64) -> Option<(&[u8], &[u8])> {
    let pattern = context.text_at(Value(search).as_slot()?)?;
    let text = context.text_at(Value(this).as_slot()?)?;
    Some((text.narrow()?, pattern.narrow()?))
}

/// Where a run of bytes first occurs at or after a position, which is what both
/// sides are whenever the text is ASCII — and it nearly always is.
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
    // Two-way with a SIMD prefilter, where the window compare this replaces was
    // O(n*m). Landed as its own change, after the allocation win was measured
    // separately, so the two are not confused for each other.
    memchr::memmem::find(&haystack[from..], needle).map(|at| at + from)
}

/// The same question over code units, for text either side of which is wide.
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
        assert_eq!(find_bytes(b"abc", b"", 0), Some(0));
        assert_eq!(find_bytes(b"abc", b"", 9), Some(3));
    }

    #[test]
    fn a_needle_longer_than_the_haystack_is_absent_rather_than_a_panic() {
        let haystack: Vec<u16> = "ab".encode_utf16().collect();
        let needle: Vec<u16> = "abcd".encode_utf16().collect();
        assert_eq!(find(&haystack, &needle, 0), None);
        assert_eq!(find_bytes(b"ab", b"abcd", 0), None);
    }
}
