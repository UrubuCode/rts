//! What the constructor, `Date.UTC` and the seven setters compute — from the
//! arguments the call *actually carried*, not from the four the convention has
//! room for.
//!
//! # Why this file exists at all
//!
//! A compiled call carries a receiver and four argument slots, so
//! `new Date(y, m, d, h, min, s, ms)` could not be *declared*: three of its
//! parameters had nowhere to arrive. What shipped instead stopped at
//! `(year, month)` and built the first of that month at midnight — so
//! `new Date(2024, 0, 15, 10, 30, 45, 123)` answered `2024-01-01T00:00:00Z`,
//! and `Date.UTC` answered the same, and every setter past the third argument
//! was silently dropped.
//!
//! The vector was already there. [`crate::entry::array_proto::arguments_at`]
//! answers what a call carried — the four slots when there was no vector, the
//! whole thing when a caller built one — and `push`, `unshift`, `concat` and
//! `Math.max` had already been taught to ask it. `Date` had not. This file is
//! that question asked once, for fourteen members.
//!
//! # Why the setters are one function and not seven
//!
//! `setFullYear`, `setMonth`, `setDate`, `setHours`, `setMinutes`,
//! `setSeconds` and `setMilliseconds` differ in exactly two numbers: where in
//! the seven fields they start writing, and how many they are allowed to
//! write. Seven bodies would be seven chances to disagree about what happens
//! to the fields a call did not reach — and the language's answer is the same
//! for all of them, so it is written once.

use super::civil::{from_parts, full_year};
use super::support::{commit, fields_of, time_of};
use super::with_current;
use crate::entry::class_support::to_number;

/// The arguments a call to one of these members actually carried.
///
/// Takes the four slots because most calls have no vector and the slots *are*
/// the arguments then; the vector wins when a caller built one, which is what
/// makes the seven-argument constructor reach seven fields.
pub(super) fn given(slots: [u64; 4]) -> Vec<u64> {
    with_current(|context| crate::entry::array_proto::arguments_at(context, 0, slots))
}

/// One positional field of a construction, or its default when the call did not
/// carry it.
///
/// The defaults are the specification's: month zero, day one, and zero for
/// everything the clock counts.
fn component(given: &[u64], at: usize, default: f64) -> f64 {
    given.get(at).map_or(default, |value| to_number(*value))
}

/// `new Date(y, m, d, h, min, s, ms)` and `Date.UTC(...)` — the same arithmetic,
/// because everything here is UTC and the module documentation says why.
///
/// The caller has already established that at least one argument was carried;
/// `Date.UTC()` with none answers `NaN` rather than the epoch, and that
/// distinction belongs to the member rather than here.
pub(super) fn constructed(given: &[u64]) -> f64 {
    from_parts(
        full_year(to_number(given[0])),
        component(given, 1, 0.0),
        component(given, 2, 1.0),
        component(given, 3, 0.0),
        component(given, 4, 0.0),
        component(given, 5, 0.0),
        component(given, 6, 0.0),
    )
}

/// A setter: overwrite the fields from `first` on with what the call carried,
/// keep every field it did not reach, and store the result.
///
/// `arity` is how many fields this member is allowed to write — three for
/// `setFullYear`, four for `setHours` — so an extra argument is ignored rather
/// than rolling into the next field down, which is what `setFullYear(y, m, d, 9)`
/// would otherwise do to the hour.
///
/// `revives` is whether this member may rebuild an INVALID date. Only
/// `setFullYear`/`setUTCFullYear` may — the specification reads their `t` as
/// `+0` where every other setter returns `NaN` — and it is a parameter rather
/// than "is `first` zero" because that coincidence is not the rule.
///
/// Both halves were missing, and each was a date that came back valid when the
/// language says it does not: `new Date(NaN).setUTCMonth(5, 10)` rebuilt from
/// the EPOCH, because `fields_of` falls back to it, so an invalid date was
/// repairable by any setter at all.
pub(super) fn written(this: u64, first: usize, arity: usize, given: &[u64], revives: bool) -> f64 {
    // Read before any coercion runs: `to_number` can call user code, and the
    // fields it would then observe are the ones this call is replacing.
    let existing = fields_of(this);
    let current = time_of(this);
    let mut parts = [
        existing.year as f64,
        existing.month as f64,
        existing.day as f64,
        existing.hour as f64,
        existing.minute as f64,
        existing.second as f64,
        existing.milli as f64,
    ];
    for at in 0..arity {
        let Some(field) = parts.get_mut(first + at) else {
            break;
        };
        match given.get(at) {
            Some(value) => *field = to_number(*value),
            // The FIRST field is the member's declared parameter and is read
            // whether or not the call carried it — `ToNumber(undefined)` is
            // `NaN`, so `d.setUTCMonth()` invalidates the date. It used to
            // leave every field alone and answer the date unchanged, which is
            // a call that silently did nothing.
            None if at == 0 => *field = f64::NAN,
            None => break,
        }
    }
    // AFTER the coercions, because they are observable: the specification reads
    // every argument through `ToNumber` — running a `valueOf` the program
    // wrote — and only then asks whether `t` was `NaN`.
    if current.is_nan() && !revives {
        return commit(this, f64::NAN);
    }
    let stored = from_parts(parts[0], parts[1], parts[2], parts[3], parts[4], parts[5], parts[6]);
    commit(this, stored)
}
