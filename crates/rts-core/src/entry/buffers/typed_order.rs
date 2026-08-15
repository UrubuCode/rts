//! Moving and ordering a view's elements **in place**.
//!
//! # Why a view sorts its own bytes and never a copy
//!
//! `t.sort()` is defined over the receiver's elements, so a `subarray` sorted
//! reorders the bytes its parent shares — `a.subarray(1, 4).sort()` changes
//! `a`. That is the property the whole module is shaped around
//! ([`super`] states it), and it is why nothing here allocates a buffer:
//! the ordering is decided over values and written back through the same
//! window every other member writes through.
//!
//! `Array.prototype.sort` cannot be borrowed for this even where the ordering
//! is identical, because an array's elements are a slot vector and a view's are
//! a byte range. What IS shared is the shape of the answer, and where this file
//! repeats one it says so.
//!
//! # Why these two are not in [`super::typed`]
//!
//! Because that file is at the crate's 500-line ceiling, and because the split
//! falls on a real line: every member there writes at a position its caller
//! named, and these two decide the position themselves. The one member in the
//! family that calls **user code** — a comparator — is therefore in the file
//! whose documentation is about that, rather than inside a module that states
//! it never calls out.

use core::cmp::Ordering;

use super::element::Kind;
use super::with_current;
use crate::value::Value;

/// `t.copyWithin(target, start, end)` — elements moved within the same view.
///
/// A byte move rather than an element loop: the range is contiguous and the
/// element width divides it exactly, so `copy_within` on the slice is the same
/// answer with the overlap already handled. Copying element by element was the
/// rejected alternative and it is the one that gets overlap wrong in the
/// direction nothing tests — a forward copy over a target below the source
/// reads bytes it has already written.
pub(in crate::entry) fn copy_within(this: u64, target: u64, start: u64, end: u64) -> u64 {
    // Before the borrow: each of these takes one of its own.
    let target = super::optional_number(target);
    let start = super::optional_number(start);
    let end = super::optional_number(end);
    with_current(|context| {
        let Some(view) = super::view_of(context, this) else {
            return this;
        };
        let count = view.count();
        let (to, _) = super::range(count, target, None);
        let (from, last) = super::range(count, start, end);
        // The specification's `min(final - from, len - to)`: a run that would
        // reach past the end is shortened rather than refused, which is what
        // makes `t.copyWithin(3, 0)` copy what fits and stop.
        let moved = (last - from).min(count - to);
        let size = view.kind.size();
        if moved > 0
            && let Some(bytes) = super::window_mut(context, &view)
        {
            bytes.copy_within(from * size..(from + moved) * size, to * size);
        }
        this
    })
}

/// `t.sort(compare?)` — in place, answering the receiver.
///
/// # Why the default is NUMERIC where an array's is by text
///
/// `[10, 9].sort()` is `[10, 9]` on an array — the language compares the
/// strings — and `new Uint8Array([10, 9]).sort()` is `[9, 10]`. The two methods
/// genuinely disagree, because a typed array's elements are numbers and cannot
/// be anything else, so there is no coercion for a text comparison to be
/// consistent about. Sharing `Array.prototype`'s default would be wrong for
/// every two-digit element, which is exactly the class of test that never
/// notices.
pub(in crate::entry) fn sort(this: u64, comparator: u64) -> u64 {
    match calls(comparator) {
        true => by_comparator(this, comparator),
        // Told apart before anything is read, because the two paths differ in
        // what they even gather: the default orders WORDS and never leaves this
        // file, where a comparator has to be handed values.
        false => by_value(this),
    }
}

/// The default order: ascending, with no call into the program.
///
/// Ordered by the number an element **reads as** rather than by its stored
/// word, which is the mistake that looks right for unsigned kinds and puts every
/// negative `Int8` last. The two sixty-four-bit kinds are the exception and take
/// the word directly: at that width the bits *are* the value, and which class is
/// reading decides only whether the top one means a sign.
fn by_value(this: u64) -> u64 {
    with_current(|context| {
        let Some(view) = super::view_of(context, this) else {
            return this;
        };
        let kind = view.kind;
        let size = kind.size();
        let count = view.count();
        let Some(bytes) = super::window_mut(context, &view) else {
            return this;
        };
        let ordered: Vec<u64> = match kind.is_bigint() {
            true => {
                let mut words: Vec<u64> = (0..count)
                    .filter_map(|at| super::element::word_at(bytes, at * size, kind, true))
                    .collect();
                match kind.is_signed() {
                    true => words.sort_by(|a, b| (*a as i64).cmp(&(*b as i64))),
                    false => words.sort_unstable(),
                }
                words
            }
            false => {
                // The word travels beside the number so the write-back is the
                // bytes that were read, not the bytes a conversion would
                // rebuild: a `Float32` `NaN` keeps the payload it held.
                let mut keyed: Vec<(f64, u64)> = (0..count)
                    .filter_map(|at| {
                        let number = super::element::read(bytes, at * size, kind, true)?;
                        let word = super::element::word_at(bytes, at * size, kind, true)?;
                        Some((number, word))
                    })
                    .collect();
                // The standard sort, and only here: [`numeric`] is a genuine
                // total order over what a view can hold, so nothing a program
                // does can provoke it. That is the whole difference from the
                // comparator path.
                keyed.sort_by(|a, b| numeric(a.0, b.0));
                keyed.into_iter().map(|(_, word)| word).collect()
            }
        };
        write_back(bytes, kind, &ordered);
        this
    })
}

/// The order two numeric elements sort in.
///
/// `NaN` last whatever its sign bit says, and `-0` before `+0`. Those two are
/// the entire difference from `f64::total_cmp` alone, which orders a negative
/// `NaN` before `-Infinity` — a position the language never answers and one a
/// program can produce, because a `Float64Array` holds whatever bit pattern was
/// written into it.
fn numeric(a: f64, b: f64) -> Ordering {
    match (a.is_nan(), b.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => a.total_cmp(&b),
    }
}

/// The order a comparator decides, with the elements as the values it sees.
///
/// Three stages for the reason [`super::typed`] states about borrows: the
/// comparator is user code, and calling it while this holds the context's
/// `RefCell` is not a wrong answer but a re-entrant borrow, which aborts a
/// process that cannot unwind out of an `extern "C"` frame.
fn by_comparator(this: u64, comparator: u64) -> u64 {
    let Some(values) = with_current(|context| {
        super::view_of(context, this).map(|view| super::typed::elements(context, &view))
    }) else {
        return this;
    };
    let ordered = merged(values, &mut |a, b| precedes(comparator, a, b));
    // A comparator that threw decided an order it never finished. Writing it
    // back would make a failed sort permanent, so the view is left as it was —
    // the discipline rule 8 of this crate's README states, and the same answer
    // `Array.prototype.sort` gives.
    if crate::entry::throw::in_flight() {
        return this;
    }
    with_current(|context| {
        // Read again rather than carried across the calls: a comparator may
        // have detached the buffer or replaced what this cell views, and the
        // window that was gathered from no longer says where to write.
        let Some(view) = super::view_of(context, this) else {
            return this;
        };
        let kind = view.kind;
        let words: Vec<u64> = ordered
            .iter()
            .map(|value| super::typed::word_of(context, *value, kind))
            .collect();
        if let Some(bytes) = super::window_mut(context, &view) {
            write_back(bytes, kind, &words);
        }
        this
    })
}

/// The ordered words, from the first element on.
///
/// One function because both paths end the same way, and the position is the
/// index in the run rather than where the word came from — which is what makes
/// this a sort rather than a permutation applied twice.
fn write_back(bytes: &mut [u8], kind: Kind, words: &[u64]) {
    let size = kind.size();
    for (at, word) in words.iter().enumerate() {
        super::element::write_word(bytes, at * size, kind, *word, true);
    }
}

/// Whether a value is something to call at all.
///
/// `t.sort(undefined)` is `t.sort()`, and so is `t.sort(3)` here — the language
/// throws a `TypeError` for the second, which this layer cannot raise where a
/// handler could catch it. The stated gap every refusal in this module settles
/// on.
fn calls(value: u64) -> bool {
    with_current(|context| {
        Value(value)
            .as_slot()
            .is_some_and(|cell| context.callable_at(cell).is_some())
    })
}

/// A stable bottom-up merge, driven by a comparison that may lie.
///
/// # Why not `slice::sort_by`
///
/// Because the comparison is **user code**, and the standard sort documents
/// that it may panic when what it is given is not a total order. A panic
/// crossing an `extern "C"` frame cannot unwind, so it is not an exception — it
/// ends the process. A merge makes that structural rather than defended: every
/// pass moves each element from the input to the output exactly once, whatever
/// the comparison answers, so no answer can lose or duplicate one.
///
/// # Why this is a second copy, and what removes it
///
/// `array_proto::more::sorting` has the same merge, and this is the duplication
/// this crate's rule 2 exists to refuse. It is written twice today only because
/// `more` is a private module of `array_proto`, so nothing outside that subtree
/// can name what is inside it — a visibility fact, not a design difference. The
/// fix is to lift the merge and the comparator contract to a module both can
/// reach, which is an edit to a file this change does not own. Until then the
/// two must stay identical in the one respect a program can observe: a tie
/// takes from the LEFT run, which is what stability means and what the language
/// requires.
fn merged(mut values: Vec<u64>, before: &mut impl FnMut(u64, u64) -> bool) -> Vec<u64> {
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
                let take_left = if left >= middle {
                    false
                } else if right >= end {
                    true
                } else {
                    before(values[left], values[right])
                };
                match take_left {
                    true => {
                        buffer.push(values[left]);
                        left += 1;
                    }
                    false => {
                        buffer.push(values[right]);
                        right += 1;
                    }
                }
            }
            at = end;
        }
        core::mem::swap(&mut values, &mut buffer);
        width *= 2;
    }
    values
}

/// Whether the comparator says `a` comes before `b`, or with it.
///
/// A positive answer is the only one that moves `b` first, so `NaN` — what a
/// comparator answers for values it was not written for, and what `ToNumber`
/// gives for an object — counts as equal and leaves the order stable.
fn precedes(comparator: u64, a: u64, b: u64) -> bool {
    let absent = super::undefined();
    // Outside every borrow: this is the call the three-stage shape exists for.
    let answered = crate::entry::functions::call(comparator, absent, a, b, absent, absent);
    !(crate::entry::class_support::to_number(answered) > 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nan_sorts_last_whichever_sign_bit_it_carries() {
        // The position `f64::total_cmp` alone gets wrong, and one a program can
        // reach: a `Float64Array` holds whatever bit pattern was written, so a
        // negative `NaN` is an element and not a theoretical value.
        let negative = f64::from_bits(0xfff8_0000_0000_0000);
        assert!(negative.is_nan());
        let mut values = vec![1.0, negative, -1.0, f64::NAN];
        values.sort_by(|a, b| numeric(*a, *b));
        assert_eq!(values[0], -1.0);
        assert_eq!(values[1], 1.0);
        assert!(values[2].is_nan() && values[3].is_nan());
    }

    #[test]
    fn negative_zero_sorts_before_positive_zero() {
        let mut values = vec![0.0f64, -0.0f64];
        values.sort_by(|a, b| numeric(*a, *b));
        assert!(values[0].is_sign_negative());
    }

    #[test]
    fn an_inconsistent_comparison_loses_no_element_and_duplicates_none() {
        // "Before" both ways round, which no total order is — the input the
        // standard sort is documented to be allowed to panic on, and a panic
        // here is an abort rather than a failed test.
        let values: Vec<u64> = (0..64).collect();
        let mut sorted = merged(values.clone(), &mut |_, _| true);
        sorted.sort_unstable();
        assert_eq!(sorted, values);
    }

    #[test]
    fn elements_a_comparator_calls_equal_keep_their_order() {
        let values: Vec<u64> = vec![9, 4, 7, 1];
        assert_eq!(merged(values.clone(), &mut |_, _| true), values);
    }
}
