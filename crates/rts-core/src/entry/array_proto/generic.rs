//! The generic arm of the `Array.prototype` methods: what they do when the
//! receiver is NOT a real array.
//!
//! # Why a second arm exists at all
//!
//! ES2025 §23.1.3 defines every one of these methods over
//! `O = ToObject(this)` and `LengthOfArrayLike(O)`, and then over `HasProperty`,
//! `Get`, `Set(…, true)` and `DeletePropertyOrThrow` per index. None of that
//! mentions an element vector. So `Array.prototype.indexOf.call({length: 3, 1:
//! "b"}, "b")` is `1`, `Array.prototype.push.call(like, x)` writes `like[len]`
//! and `like.length`, and a `Proxy` over an array sees one trap per step. Every
//! method answered `undefined` for such a receiver except `slice`, `join` and
//! `at`, and those three read through `objects::read_property` — which asks no
//! proxy — so `slice.call(new Proxy([1, 2], {}))` answered `undefined` too.
//!
//! # Why it is the SECOND arm and never the first
//!
//! The cheap arm first, the general one after — `docs/codegen/entry-tax.md`
//! part five is the class this avoids. A real array keeps answering from its
//! vector inside one borrow; this file is reached only when [`super::staged`]
//! has already said "no vector here". Routing a dense array through this path
//! would be correct and would cost a property lookup plus an interning per
//! element, for a question the `Vec` answers from its header.
//!
//! # What this reuses rather than restates
//!
//! Nothing here reads a property itself. `computed::get_indexed`,
//! `set_indexed`, `has_property` and `delete_property` are the language's
//! `Get`, `Set`, `HasProperty` and `DeletePropertyOrThrow`, proxy traps and
//! accessors included; [`super::numeric`] is `ToIntegerOrInfinity` and
//! `ToLength`. Every call below happens OUTSIDE a borrow of the context,
//! because each of them may run user code.

use super::super::computed::{delete_property, get_indexed, has_property, set_indexed};
use super::super::objects::{is_object, nullish, undefined_of};
use super::super::rooted::Rooted;
use super::super::{throw, with_current};
use crate::value::Value;

/// `ToObject(this)`, as far as these methods need it: `undefined` and `null`
/// are the `TypeError` the specification raises before `length` is read.
///
/// A primitive that is neither stays as it is. `get_indexed` already reads a
/// string's characters and `length` off the primitive, and a number or a
/// boolean has a `length` of `undefined`, which `ToLength` makes zero — the
/// same answers a wrapper object would give, without allocating one.
pub(super) fn object(this: u64, method: &str) -> Option<u64> {
    let refused = with_current(|context| nullish(context, this));
    if let Some(named) = refused {
        throw::type_error(&format!("Array.prototype.{method} called on {named}"));
        return None;
    }
    Some(this)
}

/// `LengthOfArrayLike(O)` — `ToLength(Get(O, "length"))`. `None` on a throw.
pub(super) fn length(object: u64) -> Option<usize> {
    let key = with_current(|context| context.well_known_text("length"));
    let claimed = get_indexed(object, key);
    if throw::in_flight() {
        return None;
    }
    let count = super::numeric::length(claimed);
    (!throw::in_flight()).then_some(count)
}

/// The property key an index names, as a number: `get_indexed` and the rest
/// perform `ToPropertyKey` themselves, and a proxy is handed the canonical
/// string exactly as it would be for `o[i]`.
fn key(index: usize) -> u64 {
    Value::from_f64(index as f64).bits()
}

/// `HasProperty(O, ToString(index))`. `None` on a throw.
///
/// A primitive receiver answers by range: a string's characters are its only
/// indices, and a number or a boolean has a length of zero, so this is never
/// asked of one. `has_property` itself refuses a primitive — that is `in` —
/// which is why it is not asked.
///
/// The key is the index's TEXT, not the number the other three take: asked
/// with a number, `has_property` answered `false` for every index of a `Proxy`
/// over an array, and `slice.call(proxy)` came back all holes.
/// [`super::iterate::existing`] asks the same way for the same reason.
pub(super) fn has(object: u64, index: usize) -> Option<bool> {
    if !with_current(|context| is_object(context, object)) {
        return Some(true);
    }
    let named = with_current(|context| {
        context.intern_value(crate::text::Str::from_str(&index.to_string())).bits()
    });
    let answer = has_property(named, object);
    (!throw::in_flight()).then_some(answer)
}

/// `Get(O, ToString(index))`. `None` on a throw.
pub(super) fn get(object: u64, index: usize) -> Option<u64> {
    let answer = get_indexed(object, key(index));
    (!throw::in_flight()).then_some(answer)
}

/// Whether the receiver is a string PRIMITIVE, whose indices and `length` the
/// wrapper `ToObject` makes are read-only: `Set(…, true)` on one is a
/// `TypeError`, so `Array.prototype.reverse.call("abc")` throws. A number or a
/// boolean's wrapper accepts the write and is discarded, which is what writing
/// nothing does.
fn read_only(object: u64) -> bool {
    let text = with_current(|context| {
        Value(object).as_slot().is_some_and(|cell| context.is_text_at(cell))
    });
    if text {
        throw::type_error("Cannot assign to read only property of a string");
    }
    text
}

/// `Set(O, ToString(index), value, true)` — a refusal raises. `None` on a throw.
fn set(object: u64, index: usize, value: u64) -> Option<()> {
    if read_only(object) {
        return None;
    }
    set_indexed(object, key(index), value, 0);
    (!throw::in_flight()).then_some(())
}

/// `Set(O, "length", count, true)`.
fn set_length(object: u64, count: usize) -> Option<()> {
    if read_only(object) {
        return None;
    }
    let key = with_current(|context| context.well_known_text("length"));
    set_indexed(object, key, Value::from_f64(count as f64).bits(), 0);
    (!throw::in_flight()).then_some(())
}

/// `DeletePropertyOrThrow(O, ToString(index))`.
fn delete(object: u64, index: usize) -> Option<()> {
    if read_only(object) {
        return None;
    }
    delete_property(object, key(index));
    (!throw::in_flight()).then_some(())
}

fn undefined() -> u64 {
    with_current(|context| undefined_of(context))
}

fn number(value: f64) -> u64 {
    Value::from_f64(value).bits()
}

/// The positions `start..end`, each `Get` only where `HasProperty` says there
/// is one — a missing index stays a hole in what comes back, which is the
/// difference `slice` makes visible through `in`.
///
/// ROOTED: every read may run a getter or a trap, and those allocate; what was
/// read so far lives in a `Vec` no scan reaches otherwise.
pub(super) fn gathered(object: u64, start: usize, end: usize) -> Option<Vec<u64>> {
    let mut found = Rooted::new();
    for index in start..end {
        let held = match has(object, index)? {
            true => get(object, index)?,
            false => with_current(|context| super::super::array::hole_of(context)),
        };
        found.values().push(held);
    }
    Some(found.take())
}

/// Every position `0..count` read with `Get` alone, no `HasProperty` — what
/// `join` does, where a hole and `undefined` both print as nothing.
pub(super) fn read_all(object: u64, count: usize) -> Option<Vec<u64>> {
    let mut found = Rooted::new();
    for index in 0..count {
        let held = get(object, index)?;
        found.values().push(held);
    }
    Some(found.take())
}

/// §23.1.3.17 `indexOf` and §23.1.3.20 `lastIndexOf` over an array-like, with
/// `start` already the first position to look at and the direction given.
/// Strict equality, and only positions that EXIST — a hole is skipped.
pub(super) fn search(object: u64, sought: u64, positions: impl Iterator<Item = usize>) -> u64 {
    for index in positions {
        let Some(present) = has(object, index) else {
            return undefined();
        };
        if !present {
            continue;
        }
        let Some(held) = get(object, index) else {
            return undefined();
        };
        let equal = with_current(|context| {
            crate::value::strict_equals(Value(held), Value(sought), |a, b| context.same_text(a, b))
        });
        if equal {
            return number(index as f64);
        }
    }
    number(-1.0)
}

/// §23.1.3.16 `includes` — `SameValueZero`, and EVERY position, holes read as
/// `undefined`: no `HasProperty` at all.
pub(super) fn includes(object: u64, sought: u64, start: usize, count: usize) -> u64 {
    for index in start..count {
        let Some(held) = get(object, index) else {
            return undefined();
        };
        let equal = with_current(|context| {
            crate::value::same_value_zero(Value(held), Value(sought), |a, b| context.same_text(a, b))
        });
        if equal {
            return Value::from_bool(true).bits();
        }
    }
    Value::from_bool(false).bits()
}

/// §23.1.3.23 `push`: `Set` each argument at `len + i`, then `length`.
pub(super) fn push(object: u64, values: &[u64]) -> u64 {
    let Some(count) = length(object) else {
        return undefined();
    };
    // Step 4: a length that would pass 2^53 - 1 is refused before any write.
    if (count + values.len()) as f64 > 9_007_199_254_740_991.0 {
        throw::type_error("Pushing past the maximum array length");
        return undefined();
    }
    for (at, value) in values.iter().enumerate() {
        if set(object, count + at, *value).is_none() {
            return undefined();
        }
    }
    let grown = count + values.len();
    match set_length(object, grown) {
        Some(()) => number(grown as f64),
        None => undefined(),
    }
}

/// §23.1.3.22 `pop`: the last element, deleted, and `length` written even when
/// it was already zero.
pub(super) fn pop(object: u64) -> u64 {
    let Some(count) = length(object) else {
        return undefined();
    };
    if count == 0 {
        let _ = set_length(object, 0);
        return undefined();
    }
    let last = count - 1;
    let Some(taken) = get(object, last) else {
        return undefined();
    };
    if delete(object, last).is_none() || set_length(object, last).is_none() {
        return undefined();
    }
    taken
}

/// Moves position `from` to `to` the way `shift`, `unshift` and `reverse`
/// all do: `Set` when the source exists, `DeletePropertyOrThrow` when it does
/// not — so a hole travels as a hole.
fn moved(object: u64, from: usize, to: usize) -> Option<()> {
    match has(object, from)? {
        true => set(object, to, get(object, from)?),
        false => delete(object, to),
    }
}

/// §23.1.3.27 `shift`.
pub(super) fn shift(object: u64) -> u64 {
    let Some(count) = length(object) else {
        return undefined();
    };
    if count == 0 {
        let _ = set_length(object, 0);
        return undefined();
    }
    let Some(first) = get(object, 0) else {
        return undefined();
    };
    for index in 1..count {
        if moved(object, index, index - 1).is_none() {
            return undefined();
        }
    }
    if delete(object, count - 1).is_none() || set_length(object, count - 1).is_none() {
        return undefined();
    }
    first
}

/// §23.1.3.34 `unshift`: the existing positions move up from the END, so no
/// element is overwritten before it has been read.
pub(super) fn unshift(object: u64, values: &[u64]) -> u64 {
    let Some(count) = length(object) else {
        return undefined();
    };
    let added = values.len();
    if added > 0 {
        if (count + added) as f64 > 9_007_199_254_740_991.0 {
            throw::type_error("Unshifting past the maximum array length");
            return undefined();
        }
        for index in (0..count).rev() {
            if moved(object, index, index + added).is_none() {
                return undefined();
            }
        }
        for (at, value) in values.iter().enumerate() {
            if set(object, at, *value).is_none() {
                return undefined();
            }
        }
    }
    match set_length(object, count + added) {
        Some(()) => number((count + added) as f64),
        None => undefined(),
    }
}

/// §23.1.3.26 `reverse`: pairs from both ends, each of the four
/// present/absent combinations written as the specification lists them, so a
/// hole moves to its mirror position rather than becoming `undefined`.
pub(super) fn reverse(object: u64) -> Option<()> {
    let count = length(object)?;
    let middle = count / 2;
    for lower in 0..middle {
        let upper = count - lower - 1;
        let low_exists = has(object, lower)?;
        let low = if low_exists { get(object, lower)? } else { 0 };
        let high_exists = has(object, upper)?;
        let high = if high_exists { get(object, upper)? } else { 0 };
        match (low_exists, high_exists) {
            (true, true) => {
                set(object, lower, high)?;
                set(object, upper, low)?;
            }
            (false, true) => {
                set(object, lower, high)?;
                delete(object, upper)?;
            }
            (true, false) => {
                delete(object, lower)?;
                set(object, upper, low)?;
            }
            (false, false) => {}
        }
    }
    Some(())
}

/// §23.1.3.7 `fill`, over positions already resolved against the length.
pub(super) fn fill(object: u64, value: u64, start: usize, end: usize) -> Option<()> {
    for index in start..end {
        set(object, index, value)?;
    }
    Some(())
}
