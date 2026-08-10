//! Splicing a range, and the copying forms of the same operation.
//!
//! Split out of [`super`], which had crossed the 500-line ceiling. These four
//! are the cohesive piece: each takes a RANGE and answers an array, and the two
//! pairs — `splice`/`toSpliced`, `with`/`copyWithin` — differ only in whether
//! they write the receiver back. Keeping a pair apart is how one of them comes
//! to learn that a negative start counts from the end and the other does not.

use super::super::super::objects::undefined_of;
use super::super::super::string::{absent, relative};
use super::super::super::with_current;
use super::super::{built, staged, store};
use super::{nothing, snapshot};
use crate::value::Value;

/// `a.splice(start, count, x, y)` — removes, inserts, answers what it removed.
///
/// Two insertions, because the four argument slots are spent on the two controls
/// and what is left. The rest are refused at the call rather than dropped here —
/// see `super::super::functions::ARGUMENT_SLOTS`.
///
/// An absent count removes to the end, which is not the same as a count of zero:
/// `a.splice(1)` empties the tail and `a.splice(1, 0, x)` is a pure insertion.
/// A version defaulting the count to zero looks harmless and silently turns
/// every truncation in a program into nothing at all.
pub(super) extern "C" fn splice(_e: u64, this: u64, start: u64, count: u64, x: u64, y: u64) -> u64 {
    let removed = with_current(|context| {
        let (cell, mut elements) = staged(context, this)?;
        let from = relative(Value(start).numeric().unwrap_or(0.0), elements.len());
        let left = (elements.len() - from) as f64;
        let taken = if absent(context, count) {
            left as usize
        } else {
            Value(count).numeric().unwrap_or(0.0).clamp(0.0, left) as usize
        };
        let removed: Vec<u64> = elements.drain(from..from + taken).collect();
        let inserted: Vec<u64> = [x, y]
            .into_iter()
            .filter(|given| !absent(context, *given))
            .collect();
        elements.splice(from..from, inserted);
        store(context, cell, elements);
        Some(removed)
    });
    match removed {
        Some(removed) => built(removed),
        None => nothing(),
    }
}

/// `a.toSpliced(start, count, ...items)` — a copy, where `splice` mutates.
///
/// The insertions past the two controls read the spilled vector the same way
/// `Math.max` and `a.push` do, rather than the fourth slot alone: that slot held
/// only the first inserted item, so `[1,2,3].toSpliced(1, 0, 'a', 'b')` answered
/// `[1,'a',2,3]` — the second insertion was never read back, not merely
/// truncated.
///
/// The receiver is left alone, which is the whole distinction — and the reason
/// this is not `splice` on a copy: `splice` writes `length` back through
/// `store`, and doing that to a copy is the version that works until someone
/// passes the same array twice.
pub(super) extern "C" fn to_spliced(_e: u64, this: u64, start: u64, count: u64, x: u64, a3: u64) -> u64 {
    let spliced = with_current(|context| {
        let (_, mut elements) = staged(context, this)?;
        let from = relative(Value(start).numeric().unwrap_or(0.0), elements.len());
        let left = (elements.len() - from) as f64;
        // An absent count removes to the end, for the reason `splice` records:
        // defaulting it to zero turns every truncation into nothing at all.
        let taken = if absent(context, count) {
            left as usize
        } else {
            Value(count).numeric().unwrap_or(0.0).clamp(0.0, left) as usize
        };
        elements.drain(from..from + taken);
        let inserted = super::super::arguments_at(context, 2, [start, count, x, a3]);
        elements.splice(from..from, inserted);
        Some(elements)
    });
    match spliced {
        Some(elements) => built(elements),
        None => nothing(),
    }
}

/// `a.with(i, v)` — a copy with one element replaced.
///
/// Out of range answers `undefined`, where the language throws a `RangeError` —
/// the stated gap every operation here has while a throw cannot find a handler.
/// Answering the unchanged copy was rejected: that is a wrong program that
/// keeps running, where this one fails at the next use of the result.
pub(super) extern "C" fn with(_e: u64, this: u64, index: u64, value: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(mut elements) = snapshot(this) else {
        return nothing();
    };
    let asked = Value(index).numeric().unwrap_or(f64::NAN);
    let at = if asked < 0.0 {
        elements.len() as f64 + asked
    } else {
        asked
    };
    if at.is_nan() || at < 0.0 || at >= elements.len() as f64 {
        return nothing();
    }
    elements[at as usize] = value;
    built(elements)
}

/// `a.copyWithin(target, from, to)` — in place, answering the receiver.
///
/// The copy reads out of the snapshot, so overlapping ranges see the elements as
/// they were. That is what the specification requires and the corner an in-place
/// loop gets wrong when the target lands inside the source.
pub(super) extern "C" fn copy_within(_e: u64, this: u64, target: u64, from: u64, to: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some((cell, mut elements)) = staged(context, this) else {
            return undefined_of(context);
        };
        let count = elements.len();
        let at = relative(Value(target).numeric().unwrap_or(0.0), count);
        let start = relative(Value(from).numeric().unwrap_or(0.0), count);
        let end = if absent(context, to) {
            count
        } else {
            relative(Value(to).numeric().unwrap_or(0.0), count)
        };
        let source: Vec<u64> = elements[start..end.max(start)].to_vec();
        for (offset, held) in source.into_iter().enumerate() {
            if at + offset >= count {
                break;
            }
            elements[at + offset] = held;
        }
        store(context, cell, elements);
        this
    })
}
