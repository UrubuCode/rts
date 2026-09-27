//! `at` and `lastIndexOf`: the two methods here that read positions of the
//! receiver by index and have a generic arm beside the dense one.
//!
//! Apart from `mod.rs` because that file was past the crate's 500-line ceiling
//! before either grew its generic arm; see [`super::super::generic`] for why the
//! arm exists and why it is always the second one asked.

use super::super::super::objects::undefined_of;
use super::super::super::with_current;
use super::nothing;
use crate::value::Value;

/// `a.at(i)` — negative counts from the end.
///
/// The index is converted BEFORE the borrow, and that is the whole reason this
/// is two statements: `ToIntegerOrInfinity` reaches a `valueOf` the program
/// wrote, and calling one inside `with_current` re-enters the `RefCell`. See
/// [`super::super::numeric`] for the three answers the conversion changes.
pub(super) extern "C" fn at(_e: u64, this: u64, index: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let dense = with_current(|context| super::super::borrowed(context, this).is_some());
    if dense {
        let asked = super::super::numeric::integer_or_infinity(index);
        let answer = with_current(|context| {
            let elements = super::super::borrowed(context, this)?;
            // Out of range is `undefined` rather than clamped, which is the
            // whole reason `at` was added beside indexing: `a.at(-1)` must be the
            // last element and `a.at(-99)` must be nothing, not the first.
            let Some(at) = resolved(asked, elements.len()) else {
                return Some(undefined_of(context));
            };
            // `visible`: `[,1].at(0)` is `undefined`, not the hole marker.
            Some(super::super::super::array::visible(context, elements[at]))
        });
        if let Some(answer) = answer {
            return answer;
        }
    }
    // Generic, over `LengthOfArrayLike(ToObject(this))` and then ONE `Get`, so
    // `Array.prototype.at.call({ length: 3, 2: "c" }, -1)` is `"c"` and a
    // `Proxy` is asked through its traps. Length before the index converts,
    // which is the specification's order.
    let Some(object) = super::super::generic::object(this, "at") else {
        return nothing();
    };
    let Some(count) = super::super::generic::length(object) else {
        return nothing();
    };
    let asked = super::super::numeric::integer_or_infinity(index);
    if super::super::super::throw::in_flight() {
        return nothing();
    }
    match resolved(asked, count) {
        Some(at) => super::super::generic::get(object, at).unwrap_or_else(nothing),
        None => nothing(),
    }
}

/// A relative index resolved against a length, or `None` out of range.
fn resolved(asked: f64, count: usize) -> Option<usize> {
    let at = if asked < 0.0 { count as f64 + asked } else { asked };
    (at >= 0.0 && at < count as f64).then_some(at as usize)
}

/// `a.lastIndexOf(x, from)` — strict equality, from the end.
///
/// Strict, like `indexOf` and unlike `includes`: `[NaN].lastIndexOf(NaN)` is -1.
///
/// Not `indexOf`'s start, because `relative` clamps a negative index to zero
/// — right for a forward search, where `indexOf(x, -99)` scans everything, and
/// wrong here: `lastIndexOf(x, -99)` searches NOTHING, and clamping would make it
/// find an element at position 0 the caller asked it to look past.
pub(super) extern "C" fn last_index_of(_e: u64, this: u64, search: u64, from: u64, _a2: u64, _a3: u64) -> u64 {
    let dense = with_current(|context| super::super::borrowed(context, this).is_some());
    let generic = match dense {
        true => None,
        false => {
            let Some(object) = super::super::generic::object(this, "lastIndexOf") else {
                return nothing();
            };
            let Some(count) = super::super::generic::length(object) else {
                return nothing();
            };
            Some((object, count))
        }
    };
    // `ToIntegerOrInfinity`, OUTSIDE the borrow: it may run a `valueOf`, and
    // `NaN` is ZERO — it read `Value::numeric()`, kept `NaN`, and `f64::min`
    // drops a `NaN`, so `lastIndexOf(x, NaN)` searched the whole array where the
    // language searches position 0 alone.
    //
    // PRESENT is what the specification asks, not "not undefined":
    // `lastIndexOf(x, undefined)` converts `undefined` to 0 and searches one
    // position, where `lastIndexOf(x)` searches all of them. The count the call
    // site wrote is what tells the two apart — see `reduce`.
    let given = with_current(|context| {
        super::super::arguments_at(context, 0, [search, from, from, from]).len() >= 2
    });
    let asked = given.then(|| super::super::numeric::integer_or_infinity(from));
    if super::super::super::throw::in_flight() {
        return nothing();
    }
    if let Some((object, count)) = generic {
        return match last_end(asked, count) {
            Some(end) => super::super::generic::search(object, search, (0..end).rev()),
            None => Value::from_f64(-1.0).bits(),
        };
    }
    with_current(|context| {
        let Some(elements) = super::super::borrowed(context, this) else {
            return undefined_of(context);
        };
        let Some(end) = last_end(asked, elements.len()) else {
            return Value::from_f64(-1.0).bits();
        };
        let at = elements[..end].iter().rposition(|held| {
            !super::super::super::array::is_hole(context, *held)
                && crate::value::strict_equals(Value(*held), Value(search), |a, b| {
                    context.same_text(a, b)
                })
        });
        Value::from_f64(at.map_or(-1.0, |at| at as f64)).bits()
    })
}

/// One past the last position `lastIndexOf` may look at, or `None` for none.
///
/// Absent searches everything; negative counts from the end and may leave
/// nothing; past the end is clamped to the last position — the mirror of the
/// negative case being empty. Inclusive: `lastIndexOf(x, 2)` may answer 2.
fn last_end(asked: Option<f64>, count: usize) -> Option<usize> {
    let Some(asked) = asked else {
        return Some(count);
    };
    let at = match asked < 0.0 {
        true => count as f64 + asked,
        false => asked.min(count as f64 - 1.0),
    };
    (at >= 0.0).then(|| at as usize + 1)
}
