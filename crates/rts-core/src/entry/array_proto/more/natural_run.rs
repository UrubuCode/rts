//! The order V8 and JavaScriptCore both produce, at the sizes where stating it
//! exactly is two steps rather than a whole TimSort.
//!
//! # Why the algorithm is observable at all
//!
//! It is not, for a comparator that is a real ordering: every stable sort
//! answers the same permutation for one, and the language requires stability.
//! It becomes observable the moment a comparator is INCONSISTENT — and the
//! commonest way to write one is by accident, because `NaN` counts as "equal"
//! (`SortCompare` turns it into `+0`) while the same comparator answers a real
//! order for every other pair. `[4,2,6,1,3,5].sort((x, y) => x === 1 ? NaN : x - y)`
//! is that shape, and the result is whatever the engine's algorithm does with a
//! comparison that contradicts itself.
//!
//! The specification permits any of them. What it does not permit is
//! disagreeing with the two engines a program is actually tested against, and
//! [`super::sorting::merge_sorted`] did: it answered a fully sorted array where
//! V8 and JavaScriptCore both answer `2,3,4,6,1,5`.
//!
//! # Why this is a file of its own
//!
//! Because it is a second ORDERING, not more of the first. `sorting.rs` holds
//! what a sort is — which elements take part, where `undefined` lands, when a
//! result is written back — and this holds one answer to how the ones that do
//! take part are arranged. Keeping them together took that file past this
//! crate's 500-line ceiling, and the split is along the seam the ceiling exists
//! to expose.

/// Below this many elements, the order is decided here. Above it, by
/// [`super::sorting::merge_sorted`].
///
/// **64 is V8's own number**, not one chosen here: `ComputeMinRunLength` answers
/// the whole length for anything shorter, so its TimSort finds exactly one run
/// and forces that run to cover the array — which is what [`sorted`] is.
/// Copying the threshold is what makes the two agree exactly rather than
/// approximately.
///
/// # What is given up above it, stated rather than left to be found
///
/// Matching a larger array would mean carrying TimSort's run stack, its
/// collapse invariants and its galloping mode — several hundred lines to decide
/// the order an INCONSISTENT comparator produces over a large array. A
/// consistent one is unaffected either way, because every stable sort agrees
/// about it. So the merge keeps the big arrays, at O(n log n) moves against the
/// insertion sort's O(n²).
pub(super) const LIMIT: usize = 64;

/// TimSort, at the sizes where TimSort is two steps.
///
/// `ComputeMinRunLength` answers the whole length for anything below [`LIMIT`],
/// so the march over the array finds one run and extends it to everything:
/// `CountAndMakeRun`, then `BinaryInsertionSort` over what the run did not
/// already order. There is no merge, no run stack and no collapse — which is
/// why this is a faithful copy rather than an approximation of one.
///
/// Takes the comparison as a closure for the reason
/// [`super::sorting::merge_sorted`] does: the order is the whole of what this
/// decides, and a decision that needs a running program to observe is one
/// nothing pins.
pub(super) fn sorted(
    mut values: Vec<u64>,
    compare: &mut impl FnMut(u64, u64) -> Option<f64>,
) -> Vec<u64> {
    if values.len() < 2 {
        return values;
    }
    let run = count_and_make_run(&mut values, compare);
    if run < values.len() {
        binary_insertion_sort(&mut values, run, compare);
    }
    values
}

/// The natural run at the front, reversed into place when it runs downhill.
///
/// A DESCENDING run is reversed rather than merged backwards, and it is
/// STRICTLY descending — `>= 0` ends it — because reversing a run containing
/// equal elements would swap them, and the language requires stability.
///
/// Answers the whole length when the comparator threw, which stops the sort
/// where it is: the caller writes nothing back, and asking at all is rule 8 of
/// this crate's README — a native that called user code asks before it believes
/// the answer.
fn count_and_make_run(
    values: &mut [u64],
    compare: &mut impl FnMut(u64, u64) -> Option<f64>,
) -> usize {
    let high = values.len();
    if high < 2 {
        return high;
    }
    let Some(order) = compare(values[1], values[0]) else {
        return high;
    };
    let descending = order < 0.0;
    let mut run = 2;
    let mut previous = values[1];
    for at in 2..high {
        let current = values[at];
        let Some(order) = compare(current, previous) else {
            break;
        };
        let ends = match descending {
            true => order >= 0.0,
            false => order < 0.0,
        };
        if ends {
            break;
        }
        previous = current;
        run += 1;
    }
    if descending {
        values[..run].reverse();
    }
    run
}

/// Binary insertion sort over `values[start..]`, into the ordered front.
///
/// # Why the search tests `< 0` and nothing else
///
/// Because that single test is what makes it stable AND what decides where an
/// "equal" — which includes every `NaN` the comparator answered — lands: the
/// pivot goes AFTER anything it does not strictly precede, so a run of equals
/// keeps the order it arrived in. Reading the test as `<= 0` would reverse
/// equal elements, which the language forbids.
///
/// Cannot lose or duplicate an element whatever the comparator says: `left` and
/// `right` start inside the array and each branch strictly shrinks the gap, and
/// the move is a rotation of one contiguous range. That is the same structural
/// guarantee [`super::sorting::merge_sorted`] documents, and it is why neither
/// of them is `slice::sort_by` — the standard sort is allowed to panic on input
/// that is not a total order, and a panic crossing an `extern "C"` frame ends
/// the process instead of the call.
fn binary_insertion_sort(
    values: &mut [u64],
    start: usize,
    compare: &mut impl FnMut(u64, u64) -> Option<f64>,
) {
    let high = values.len();
    // V8 begins one past `low` when the run is empty; `low` is zero here and a
    // run is never shorter than one, so the clamp states that rather than
    // leaving it as a case the caller has to remember.
    for i in start.max(1)..high {
        let pivot = values[i];
        let (mut left, mut right) = (0usize, i);
        while left < right {
            let mid = left + ((right - left) >> 1);
            let Some(order) = compare(pivot, values[mid]) else {
                return;
            };
            match order < 0.0 {
                true => right = mid,
                false => left = mid + 1,
            }
        }
        // `values[left..=i]` shifted one right with the pivot dropped at
        // `left`, which is exactly V8's copy loop and one call instead of a
        // loop that could be written off by one.
        values[left..=i].rotate_right(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_comparator_that_contradicts_itself_lands_where_v8_and_jsc_land() {
        // `[4,2,6,1,3,5].sort((x, y) => x === 1 ? NaN : x - y)`, which answers a
        // real order for every pair except the ones involving 1 — where `NaN`
        // becomes `+0` and says "equal". The permutation is
        // implementation-defined by the specification, and BOTH engines this
        // one is measured against answer `2,3,4,6,1,5`. The merge sort answered
        // `1,2,3,4,5,6`, which is the shape of divergence only an inconsistent
        // comparator can show.
        let mut compare = |a: u64, b: u64| -> Option<f64> {
            Some(match a == 1 {
                true => 0.0,
                false => a as f64 - b as f64,
            })
        };
        assert_eq!(sorted(vec![4, 2, 6, 1, 3, 5], &mut compare), vec![
            2, 3, 4, 6, 1, 5
        ]);
    }

    #[test]
    fn a_comparator_that_calls_everything_equal_moves_nothing() {
        // `sort(() => NaN)` — every comparison is `+0`, so the natural run
        // covers the whole array and there is nothing left to insert.
        // Stability makes that the input unchanged, which is what both engines
        // answer.
        let values: Vec<u64> = vec![4, 2, 6, 1, 3, 5];
        assert_eq!(sorted(values.clone(), &mut |_, _| Some(0.0)), values);
    }

    #[test]
    fn a_comparator_that_stops_answering_loses_no_element() {
        // `None` is a throw in flight. The insertion stops where it is — the
        // caller writes nothing back — and what it stopped on must still be a
        // permutation of the input, because the alternative is an array with a
        // duplicate in it and a value gone.
        let values: Vec<u64> = (0..32).collect();
        let mut asked = 0;
        let stopped = sorted(values.clone(), &mut |a, b| {
            asked += 1;
            (asked < 5).then(|| a as f64 - b as f64)
        });
        let mut seen = stopped;
        seen.sort_unstable();
        assert_eq!(seen, values);
    }

    #[test]
    fn a_descending_run_is_reversed_rather_than_merged() {
        // The first half of `CountAndMakeRun`: an array that runs downhill the
        // whole way is one run, reversed, and never reaches the insertion sort
        // at all.
        let ordered = sorted(vec![5, 4, 3, 2, 1], &mut |a, b| Some(a as f64 - b as f64));
        assert_eq!(ordered, vec![1, 2, 3, 4, 5]);
    }
}
