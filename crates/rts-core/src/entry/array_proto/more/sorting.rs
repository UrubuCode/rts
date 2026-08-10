//! Putting an array in order, with a comparator that may be user code.
//!
//! # Why this is a file rather than two more entries in [`super`]
//!
//! Because `sort` is the method here whose *argument* is longer than its code.
//! Two things about it are wrong in ways that pass a test suite:
//!
//! - The default comparison is by **string**, so `[10, 9, 1].sort()` is
//!   `[1, 10, 9]`. A numeric default agrees with the language for every array of
//!   single digits — which is what almost every hand-written test contains — and
//!   diverges on the first two-digit number a real program sorts.
//! - The comparator is a call into user code, from inside a method that is
//!   holding the elements. Doing that under the `RefCell` borrow is not a wrong
//!   answer but a re-entrant borrow, and an `extern "C"` frame cannot unwind, so
//!   it ends the process.
//!
//! Both are stated where the code that answers them is, and the two-stage shape
//! [`super::super::iterate`] documents is what the second one is answered with:
//! collect, drop the borrow, call, re-borrow to store.

use super::super::super::objects::undefined_of;
use super::super::super::{Context, functions, with_current};
use super::super::{built, store};
use super::{calls, nothing, snapshot};
use crate::value::Value;

/// `a.sort(f)` — in place, answering the receiver.
///
/// The elements are written back from the snapshot the ordering worked on, so a
/// comparator that also mutates the receiver loses those mutations. Named: the
/// specification leaves the result implementation-defined when the comparator
/// mutates, and the alternative is re-reading the store between comparisons —
/// which is a borrow held across a call into user code.
pub(super) extern "C" fn sort(
    _e: u64,
    this: u64,
    comparator: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let Some(elements) = snapshot(this) else {
        return nothing();
    };
    let ordered = ordered(elements, comparator);
    // A comparator that threw leaves an order it never decided. Writing it back
    // would make a failed sort permanent — the array reordered by a comparison
    // that stopped answering — so the receiver is left as it was.
    if super::super::super::throw::in_flight() {
        return this;
    }
    with_current(|context| {
        if let Some(cell) = Value(this).as_slot() {
            store(context, cell, ordered);
        }
        this
    })
}

/// `a.toSorted(f)` — the same order, in a new array.
///
/// One ordering with two destinations, which is why the two are here together:
/// a second implementation is where one of them would learn that `undefined`
/// sorts last and the other would not.
pub(super) extern "C" fn to_sorted(
    _e: u64,
    this: u64,
    comparator: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    match snapshot(this) {
        Some(elements) => built(ordered(elements, comparator)),
        None => nothing(),
    }
}

/// The elements in order, `undefined` last.
///
/// `undefined` never reaches a comparator and always lands at the end, which the
/// specification states separately from the ordering for exactly this reason: a
/// comparator written for numbers answers `NaN` for it, and a sort driven by
/// that scatters it through the result.
fn ordered(elements: Vec<u64>, comparator: u64) -> Vec<u64> {
    let (mut present, missing) = with_current(|context| {
        let absent = undefined_of(context);
        let present: Vec<u64> = elements
            .iter()
            .copied()
            .filter(|held| *held != absent)
            .collect();
        let missing = vec![absent; elements.len() - present.len()];
        (present, missing)
    });
    if calls(comparator) {
        present = merge_sorted(present, &mut |a, b| precedes(comparator, a, b));
    } else {
        let mut keyed: Vec<(Vec<u16>, u64)> = with_current(|context| {
            present
                .iter()
                .map(|held| (sort_key(context, *held), *held))
                .collect()
        });
        // The standard sort, and only here: these keys are a genuine total
        // order, so nothing a program does can provoke it. That is the whole
        // difference from the branch above.
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        present = keyed.into_iter().map(|(_, held)| held).collect();
    }
    present.extend(missing);
    present
}

/// A stable bottom-up merge sort, driven by a comparison that may lie.
///
/// # Why not `slice::sort_by`
///
/// Because the comparison is **user code**, and the standard sort documents that
/// it may panic when what it is given is not a total order. A panic crossing an
/// `extern "C"` frame cannot unwind, so it is not an exception — it aborts the
/// process. A program whose comparator answers inconsistently must produce a
/// nonsensical order, not a dead runtime.
///
/// A merge is the shape that makes that structural rather than defended: each
/// pass moves every element from the input to the output exactly once, whatever
/// the comparison says, so no answer it can give loses or duplicates one. The
/// only thing an inconsistent comparator can change is the order — which is what
/// the specification also permits.
fn merge_sorted(mut values: Vec<u64>, before: &mut impl FnMut(u64, u64) -> bool) -> Vec<u64> {
    let count = values.len();
    let mut buffer: Vec<u64> = Vec::with_capacity(count);
    let mut width = 1;
    while width < count {
        buffer.clear();
        let mut at = 0;
        while at < count {
            let middle = (at + width).min(count);
            let end = (at + 2 * width).min(count);
            let (mut left, mut right) = (at, middle);
            while left < middle || right < end {
                // A tie takes from the left run, which is what makes this
                // stable — and stability is observable rather than a nicety:
                // the language requires it, so two elements a comparator calls
                // equal must come out in the order they went in.
                let take_left = if left >= middle {
                    false
                } else if right >= end {
                    true
                } else {
                    before(values[left], values[right])
                };
                if take_left {
                    buffer.push(values[left]);
                    left += 1;
                } else {
                    buffer.push(values[right]);
                    right += 1;
                }
            }
            at = end;
        }
        std::mem::swap(&mut values, &mut buffer);
        width *= 2;
    }
    values
}

/// Whether the comparator says `a` comes before `b`, or with it.
///
/// A positive answer is the only one that moves `b` first. `NaN` — which is what
/// a comparator answers for values it was not written for, and what `ToNumber`
/// gives for an object — therefore counts as equal, which the specification says
/// explicitly and which leaves the order stable instead of arbitrary.
fn precedes(comparator: u64, a: u64, b: u64) -> bool {
    let receiver = nothing();
    // Outside every borrow: this is the call the two-stage shape exists for.
    let answered = functions::call(comparator, receiver, a, b, receiver, receiver);
    // A comparator that threw answers `undefined`, whose `ToNumber` is `NaN`,
    // which reads as "equal" here — so the merge finishes rather than hanging,
    // in an order decided partly by a comparator that stopped running. That is
    // inside what the specification permits for an inconsistent comparator,
    // and it is why the throw is caught one level up instead: `ordered`'s
    // callers do not STORE a result they got while unwinding.
    !(super::super::super::class_support::to_number(answered) > 0.0)
}

/// What an element sorts as when no comparator was given.
///
/// The text, **as code units**, because the comparison the language specifies is
/// over UTF-16 order rather than UTF-8 bytes. The two agree for everything in
/// the basic plane and disagree above it, which is where a `String` key would
/// quietly order an emoji before an ordinary character.
///
/// An object sorts as the empty string: its `toString` is a call and this runs
/// under a borrow — the same boundary `super::super::join` stops at, stated for
/// the same reason.
fn sort_key(context: &Context, value: u64) -> Vec<u16> {
    super::super::super::text::to_text(context, Value(value))
        .map(|text| text.units().collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_inconsistent_comparison_loses_no_element_and_duplicates_none() {
        // "Before" both ways round, which no total order is. The input the
        // standard sort is documented to be allowed to panic on — and a panic
        // here is an abort, not a failed test.
        let values: Vec<u64> = (0..64).collect();
        let sorted = merge_sorted(values.clone(), &mut |_, _| true);
        let mut seen = sorted;
        seen.sort_unstable();
        assert_eq!(seen, values);
    }

    #[test]
    fn elements_a_comparator_calls_equal_keep_their_order() {
        // Stability, which the language requires. A comparator answering
        // "equal" for every pair must therefore answer the input unchanged.
        let values: Vec<u64> = vec![9, 4, 7, 1];
        let sorted = merge_sorted(values.clone(), &mut |_, _| true);
        assert_eq!(sorted, values);
    }

    #[test]
    fn the_default_order_is_by_text_and_not_by_number() {
        // `[10, 9, 1].sort()` is `[1, 10, 9]`. Pinned over the keys rather than
        // over the method, because the method needs a running context — and the
        // key is where the decision actually is.
        let keyed = |text: &str| -> Vec<u16> { text.encode_utf16().collect() };
        let mut keys = vec![keyed("10"), keyed("9"), keyed("1")];
        keys.sort();
        assert_eq!(keys, vec![keyed("1"), keyed("10"), keyed("9")]);
    }
}
