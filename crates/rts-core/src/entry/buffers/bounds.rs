//! How an argument becomes a position: clamping, relative indices, and the one
//! question every member of this module asks before it takes a borrow.
//!
//! # Why these left [`super`]
//!
//! Size, and what size stood for. Every one of them is a rule about the
//! *arguments* a view's methods take rather than about bytes, views or classes —
//! which is the rest of that module — and it was reached as `super::range` from
//! seven files that never look at anything else in it. Splitting on that line
//! leaves the module documentation about memory and puts the argument rules
//! where their tests already were.
//!
//! They are re-exported from [`super`] rather than renamed at the call sites,
//! because `super::range` is what the members read as and the move is not a
//! change any of them should have to notice.

use super::{Context, with_current};
use crate::entry::objects::undefined_of;

/// A length argument as a count of bytes.
///
/// Negative, fractional and non-finite all become zero rather than an error: the
/// language throws a `RangeError` for a negative one, which this engine cannot
/// raise where a handler could catch it — the same stated gap every refusal in
/// this layer settles on.
pub(in crate::entry) fn as_count(length: f64) -> usize {
    match length.is_finite() && length > 0.0 {
        true => length.trunc() as usize,
        false => 0,
    }
}

/// The language's `ToIndex`: `None` where it throws a `RangeError`.
///
/// Beside [`as_count`] rather than replacing it, because the two disagree on
/// purpose: `as_count` settles a bad length on zero for the callers that have
/// not been taught to raise, and this refuses it for the ones that have. Nothing
/// else in the crate computed `ToIndex` — `array_buffer.rs` and
/// `bigint_class.rs` each test only the sign inline — so this is the first
/// shared form, not a second one. `NaN` is index zero; `-0.5` truncates to
/// zero and is accepted; infinities and anything past 2^53 - 1 are refused.
pub(in crate::entry) fn to_index(number: f64) -> Option<usize> {
    if number.is_nan() {
        return Some(0);
    }
    let whole = number.trunc();
    match (0.0..=9_007_199_254_740_991.0).contains(&whole) {
        true => Some(whole as usize),
        false => None,
    }
}

/// `undefined`, from outside a borrow.
pub(in crate::entry) fn undefined() -> u64 {
    with_current(|context: &mut Context| undefined_of(context))
}

/// An argument that may have been left off, as a number.
///
/// **Called at the top of a member body, never inside a borrow.** Both halves
/// take one of their own, and a second borrow of the `RefCell` panics inside an
/// `extern "C"` frame that cannot unwind — so the process aborts rather than
/// failing a test.
///
/// `None` for an absent argument rather than `NaN`, because the two mean opposite
/// things at a range boundary: `t.slice(0)` ends at the length and
/// `t.slice(0, NaN)` ends at zero. Coercing first and testing for `NaN` would
/// merge them.
pub(in crate::entry) fn optional_number(value: u64) -> Option<f64> {
    if value == undefined() {
        return None;
    }
    Some(crate::entry::class_support::to_number(value))
}

/// A `[begin, end)` pair over `count` items, with the language's clamping.
///
/// A negative index counts from the end, everything is clamped into range, and an
/// empty range comes back as `begin == end` rather than as `None` — the callers
/// all want to produce something empty rather than to refuse.
pub(in crate::entry) fn range(count: usize, begin: Option<f64>, end: Option<f64>) -> (usize, usize) {
    let first = relative(begin.unwrap_or(0.0), count);
    let last = match end {
        Some(number) => relative(number, count),
        None => count,
    };
    (first, last.max(first))
}

/// One relative index, resolved against a length.
fn relative(index: f64, count: usize) -> usize {
    if !index.is_finite() {
        // ToIntegerOrInfinity: -Infinity is before the start, +Infinity past the
        // end, and NaN is zero.
        return match (index.is_nan(), index > 0.0) {
            (true, _) => 0,
            (false, true) => count,
            (false, false) => 0,
        };
    }
    let index = index.trunc();
    match index < 0.0 {
        true => ((count as f64) + index).max(0.0) as usize,
        false => (index as usize).min(count),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_negative_bound_counts_from_the_end_and_never_past_the_start() {
        assert_eq!(range(8, Some(-3.0), None), (5, 8));
        assert_eq!(range(8, Some(-99.0), Some(2.0)), (0, 2));
        // An inverted range is empty, not reversed: `t.slice(5, 2)` answers
        // nothing rather than three elements backwards.
        assert_eq!(range(8, Some(5.0), Some(2.0)), (5, 5));
    }

    #[test]
    fn dataview_to_index_refuses_negative_and_infinite_but_reads_nan_as_zero() {
        assert_eq!(to_index(f64::NAN), Some(0));
        assert_eq!(to_index(-0.5), Some(0));
        assert_eq!(to_index(1.9), Some(1));
        assert_eq!(to_index(-1.0), None);
        assert_eq!(to_index(f64::INFINITY), None);
        assert_eq!(to_index(9_007_199_254_740_992.0), None);
    }

    #[test]
    fn an_absent_end_is_the_length_and_an_explicit_nan_is_zero() {
        // The distinction `optional_number` exists to keep: these two are not
        // the same call, and a coercion that answered `NaN` for both would make
        // `t.slice(0)` empty.
        assert_eq!(range(4, Some(0.0), None), (0, 4));
        assert_eq!(range(4, Some(0.0), Some(f64::NAN)), (0, 0));
    }

    #[test]
    fn a_run_that_reaches_past_the_end_is_shortened_and_not_refused() {
        // What `copyWithin` computes over these two answers: the specification's
        // `min(final - from, len - to)`, and the reason the pair above clamps
        // rather than answering `None`.
        let count = 5;
        let (to, _) = range(count, Some(3.0), None);
        let (from, last) = range(count, Some(0.0), None);
        assert_eq!((last - from).min(count - to), 2);
    }
}
