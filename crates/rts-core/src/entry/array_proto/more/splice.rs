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
use super::nothing;
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
    // Converted BEFORE any borrow, like every other index argument in this
    // folder: `ToIntegerOrInfinity` may run a `valueOf`, and `Value::numeric`
    // answers `None` for anything that is not already a number — so
    // `a.splice("2", "1")` read both as zero instead of the string coerced,
    // and `a.splice(2)` removed nothing past that.
    let asked_start = super::super::numeric::integer_or_infinity(start);
    // The count too, and for the same reason: converting it inside the
    // borrow below panicked with a re-entrant `RefCell` for `a.splice("2", "1")`
    // — a string reaches `ToPrimitive`, which takes the context.
    let asked_count = super::super::numeric::integer_or_infinity(count);
    if super::super::super::throw::in_flight() {
        return nothing();
    }
    let removed = with_current(|context| {
        let (cell, mut elements) = staged(context, this)?;
        // How many arguments the SITE wrote, which is what decides the deletion
        // and cannot be read off the values: `a.splice()` deletes NOTHING and
        // `a.splice(0)` deletes everything from zero, yet both arrive here with
        // `start` and `count` padded to `undefined`. Comparing against
        // `undefined` answered "delete to the end" for both, so `a.splice()`
        // emptied the array — the most destructive possible reading of a call
        // that asks for nothing.
        let written = super::super::arguments_at(context, 0, [start, count, x, y]).len();
        let from = relative(asked_start, elements.len());
        let left = (elements.len() - from) as f64;
        let taken = if written == 0 {
            0
        } else if absent(context, count) {
            left as usize
        } else {
            asked_count.clamp(0.0, left) as usize
        };
        let removed: Vec<u64> = elements.drain(from..from + taken).collect();
        // Every item PAST the two controls, not merely the two slots `x` and
        // `y`: those carry only the first two, so `a.splice(1, 0, 'a', 'b',
        // 'c')` inserted two of the three and silently dropped the rest once a
        // call spilled past four arguments.
        let inserted = super::super::arguments_at(context, 2, [start, count, x, y]);
        elements.splice(from..from, inserted);
        store(context, cell, elements);
        Some(removed)
    });
    match removed {
        // `ArraySpeciesCreate`: the REMOVED elements go in whatever the
        // receiver's species names, which is what `source.splice(…) instanceof
        // Removed` asks. The receiver itself is untouched and stays its own
        // class — the two are different objects, and only one of them is made
        // here.
        Some(removed) => super::super::species::collected(this, removed),
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
    // Converted BEFORE any borrow, for the reason every index argument in this
    // folder is: `Value::numeric` answers `None` for anything not already a
    // number, so `a.toSpliced("1")` read the start as zero.
    let asked_start = super::super::numeric::integer_or_infinity(start);
    // The count too, and for the same reason: converting it inside the
    // borrow below panicked with a re-entrant `RefCell` for `a.splice("2", "1")`
    // — a string reaches `ToPrimitive`, which takes the context.
    let asked_count = super::super::numeric::integer_or_infinity(count);
    if super::super::super::throw::in_flight() {
        return nothing();
    }
    let spliced = with_current(|context| {
        // Every hole materialised, the same rule `with` and `toReversed` follow:
        // a copying method reads its source with `Get`, so the result has an own
        // `undefined` where the source had nothing.
        let (_, elements) = staged(context, this)?;
        let mut elements: Vec<u64> = elements
            .iter()
            .map(|held| super::super::super::array::visible(context, *held))
            .collect();
        let from = relative(asked_start, elements.len());
        let left = (elements.len() - from) as f64;
        // No `start` at all removes NOTHING — `a.toSpliced()` is a plain copy —
        // where a `start` given without a `count` removes to the end. The two
        // read identically once padded to `undefined`, so the argument COUNT is
        // what tells them apart; collapsing both into "count is absent" made
        // `a.toSpliced()` delete everything, the same wrong reading `splice`'s
        // own comment already names for itself.
        let taken = if absent(context, start) {
            0
        } else if absent(context, count) {
            left as usize
        } else {
            asked_count.clamp(0.0, left) as usize
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
/// Out of range is a `RangeError`, raised before `ArrayCreate` runs. It used to
/// answer `undefined` instead, named here as a stated gap rather than solved —
/// which is a wrong program that keeps running: `[1,2].with(5, 0)` produced a
/// copy whose next use silently read `undefined`, instead of failing at the
/// call that asked for an index the array does not have.
pub(super) extern "C" fn with(_e: u64, this: u64, index: u64, value: u64, _a2: u64, _a3: u64) -> u64 {
    // `ToIntegerOrInfinity`, outside every borrow: it may run a `valueOf`.
    let asked = super::super::numeric::integer_or_infinity(index);
    // `seen` and not `snapshot`: the ES2023 copying methods read the source with
    // `Get`, so a HOLE becomes an own `undefined` in the copy. `[1, , 3].with(0, 9)`
    // has three own indices, and carrying the hole across made it have two.
    let Some(mut elements) = super::seen(this) else {
        return nothing();
    };
    let at = if asked < 0.0 {
        elements.len() as f64 + asked
    } else {
        asked
    };
    if !(0.0..elements.len() as f64).contains(&at) {
        super::super::super::throw::range_error("Invalid index");
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
