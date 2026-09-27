//! `indexOf`, `includes`, `slice`, `reverse` and `fill`: the methods that walk a
//! range of positions and have both a dense arm and a generic one.
//!
//! # Why the two arms, and in this order
//!
//! Each method asks the element vector FIRST, inside one borrow, and only a
//! receiver with no vector — an array-like, a string, a `Proxy` — reaches
//! [`super::generic`]. That order is the point: the specification's algorithm is
//! written over `HasProperty`/`Get`/`Set` per index, and for a real dense array
//! every one of those answers what the vector already says, so running them
//! would be the conversion-before-dispatch class `docs/codegen/entry-tax.md`
//! part five describes — correct, and a property lookup per element.
//!
//! # Why the numeric arguments are converted before any borrow
//!
//! `ToIntegerOrInfinity` may run a `valueOf`, which is user code. The previous
//! forms read `Value::numeric().unwrap_or(0.0)` inside the borrow instead, so
//! `a.indexOf(x, "1")` searched from zero and `a.fill(v, {valueOf(){return 1}})`
//! filled everything. [`argument`] is the cheap arm for the common case — a
//! plain double answers with no borrow at all — and the conversion after it.

use super::super::objects::undefined_of;
use super::super::string::{absent, relative};
use super::super::{throw, with_current};
use super::{borrowed, generic, numeric, species, staged, store};
use crate::value::Value;

/// A position argument: `None` when it was not given (each method has its own
/// default for that), otherwise `ToIntegerOrInfinity` of it.
///
/// A double answers without a borrow; everything else is converted OUTSIDE one.
fn argument(value: u64) -> Option<f64> {
    if let Some(number) = Value(value).as_f64() {
        return Some(if number.is_nan() { 0.0 } else { number.trunc() });
    }
    if with_current(|context| absent(context, value)) {
        return None;
    }
    Some(numeric::integer_or_infinity(value))
}

fn nothing() -> u64 {
    with_current(|context| undefined_of(context))
}

/// What the FIRST borrow of `indexOf`/`includes` found.
///
/// The ordinary call — a dense array, and a `from` that is absent or already a
/// number — is answered inside that one borrow, which is the cost these two
/// had before the generic arm existed. Only a `from` that needs converting
/// (it may run a `valueOf`) or a receiver with no vector costs more.
enum First {
    Answered(u64),
    /// Dense, but `from` must be converted outside a borrow first.
    Convert,
    /// No element vector: the generic arm.
    Generic,
}

/// `from` read WITHOUT converting: `Some(0.0)` for absent, the truncated number
/// for a double, `None` for anything whose conversion may run user code.
fn inert(context: &super::super::Context, value: u64) -> Option<f64> {
    if absent(context, value) {
        return Some(0.0);
    }
    let number = Value(value).as_f64()?;
    Some(if number.is_nan() { 0.0 } else { number.trunc() })
}

/// The dense `indexOf`: strict equality from `start`, holes skipped — `indexOf`
/// asks `HasProperty`, and a hole has none. Borrowed, never copied: nothing
/// here calls user code, and copying was the whole cost of the answer.
fn index_in(context: &super::super::Context, elements: &[u64], search: u64, asked: f64) -> u64 {
    let start = relative(asked, elements.len());
    let at = elements
        .iter()
        .skip(start)
        .position(|held| {
            !super::super::array::is_hole(context, *held)
                && crate::value::strict_equals(Value(*held), Value(search), |a, b| {
                    context.same_text(a, b)
                })
        })
        // Offset back by what was skipped, or it is a position in the TAIL.
        .map(|at| at + start);
    Value::from_f64(at.map_or(-1.0, |at| at as f64)).bits()
}

/// The dense `includes`: `SameValueZero`, and a hole is SEEN as `undefined` —
/// it walks `0..length`, not the keys that exist, so `[,1].includes(undefined)`
/// is `true`. Where the search starts and what it sees are separate questions.
fn includes_in(context: &super::super::Context, elements: &[u64], search: u64, asked: f64) -> u64 {
    let start = relative(asked, elements.len());
    let found = elements.iter().skip(start).any(|held| {
        let held = super::super::array::visible(context, *held);
        crate::value::same_value_zero(Value(held), Value(search), |a, b| context.same_text(a, b))
    });
    Value::from_bool(found).bits()
}

/// The dense arm shared by `indexOf` and `includes`: one borrow when `from`
/// needs nothing, two when it must convert. `None` sends the caller generic.
fn dense_search(
    this: u64,
    from: u64,
    answer: fn(&super::super::Context, &[u64], u64, f64) -> u64,
    search: u64,
) -> Option<u64> {
    let first = with_current(|context| {
        let Some(elements) = borrowed(context, this) else {
            return First::Generic;
        };
        match inert(context, from) {
            Some(asked) => First::Answered(answer(context, elements, search, asked)),
            None => First::Convert,
        }
    });
    match first {
        First::Answered(answered) => Some(answered),
        First::Generic => None,
        First::Convert => {
            let asked = numeric::integer_or_infinity(from);
            if throw::in_flight() {
                return Some(nothing());
            }
            // The conversion ran user code, which may have emptied the vector
            // but cannot have removed it; `None` (generic) is the honest arm if
            // it somehow did.
            with_current(|context| {
                let elements = borrowed(context, this)?;
                Some(answer(context, elements, search, asked))
            })
        }
    }
}

/// `a.indexOf(x, from)` — where `x` first is at or after `from`, or -1.
///
/// Strict equality, which is what the language says and why this is not
/// `includes` with a different answer: `[NaN].indexOf(NaN)` is -1 and
/// `[NaN].includes(NaN)` is true. One shared implementation would have to pick
/// one of those, and either choice is wrong half the time.
///
/// The generic arm reads `length` BEFORE converting `fromIndex`, which is the
/// specification's order and observable through a `length` getter; the dense
/// arm may convert first, because a vector's length runs no code.
pub(super) extern "C" fn index_of(
    _e: u64,
    this: u64,
    search: u64,
    from: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    if let Some(answer) = dense_search(this, from, index_in, search) {
        return answer;
    }
    let Some(object) = generic::object(this, "indexOf") else {
        return nothing();
    };
    let Some(count) = generic::length(object) else {
        return nothing();
    };
    if count == 0 {
        return Value::from_f64(-1.0).bits();
    }
    let asked = argument(from).unwrap_or(0.0);
    if throw::in_flight() {
        return nothing();
    }
    generic::search(object, search, relative(asked, count)..count)
}

/// `a.includes(x, from)` — `SameValueZero`, so `NaN` finds itself.
pub(super) extern "C" fn includes(
    _e: u64,
    this: u64,
    search: u64,
    from: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    if let Some(answer) = dense_search(this, from, includes_in, search) {
        return answer;
    }
    let Some(object) = generic::object(this, "includes") else {
        return nothing();
    };
    let Some(count) = generic::length(object) else {
        return nothing();
    };
    if count == 0 {
        return Value::from_bool(false).bits();
    }
    let asked = argument(from).unwrap_or(0.0);
    if throw::in_flight() {
        return nothing();
    }
    generic::includes(object, search, relative(asked, count), count)
}

/// `a.slice(from, to)` — a new array, negative counting from the end.
///
/// `slice` is GENERIC — defined over `LengthOfArrayLike(ToObject(this))` — and
/// the oldest idiom in JavaScript is exactly the generic use:
/// `Array.prototype.slice.call(arguments, 1)`. The generic arm reads through
/// `HasProperty`/`Get`, so a `Proxy` over an array is asked through its traps
/// and a length it lies about is the length used; it read `read_property`
/// before, which asks no proxy, and `slice.call(proxy)` answered `undefined`.
///
/// The result is `ArraySpeciesCreate` — for anything that is not an array
/// (a proxy included, as far as this runtime's `IsArray` goes) a plain `Array`
/// that inherits `Array.prototype`, so `.join` on it exists.
pub(super) extern "C" fn slice(_e: u64, this: u64, from: u64, to: u64, _a2: u64, _a3: u64) -> u64 {
    let dense = with_current(|context| borrowed(context, this).is_some());
    if !dense {
        return slice_generic(this, from, to);
    }
    // Converted OUTSIDE every borrow, because either bound may run a `valueOf`:
    // `a.slice(true)`, `a.slice("2")` and `a.slice({ valueOf() { return 2 } })`
    // all silently started at zero while this read `Value::numeric()`.
    let asked_start = argument(from).unwrap_or(0.0);
    let asked_end = argument(to);
    if throw::in_flight() {
        return nothing();
    }
    let taken = with_current(|context| {
        let (_, elements) = staged(context, this)?;
        let start = relative(asked_start, elements.len());
        let end = asked_end.map_or(elements.len(), |asked| relative(asked, elements.len()));
        // Crossed rather than swapped, the same as the string method:
        // `[1,2,3].slice(2, 1)` is empty.
        Some(if start >= end {
            Vec::new()
        } else {
            elements[start..end].to_vec()
        })
    });
    match taken {
        Some(taken) => species::collected(this, taken),
        None => nothing(),
    }
}

/// §23.1.3.28 over a receiver with no element vector, in the specification's
/// order: `ToObject`, `LengthOfArrayLike`, then the two bounds, then the reads.
fn slice_generic(this: u64, from: u64, to: u64) -> u64 {
    let Some(object) = generic::object(this, "slice") else {
        return nothing();
    };
    let Some(count) = generic::length(object) else {
        return nothing();
    };
    let asked_start = argument(from).unwrap_or(0.0);
    let asked_end = argument(to);
    if throw::in_flight() {
        return nothing();
    }
    let start = relative(asked_start, count);
    let end = asked_end.map_or(count, |asked| relative(asked, count));
    let Some(taken) = generic::gathered(object, start, end.max(start)) else {
        return nothing();
    };
    species::collected(this, taken)
}

/// `a.reverse()` — in place, answering the receiver.
///
/// In place and not a copy, because the language says so and programs rely on
/// it: `b = a.reverse()` leaves `a` reversed too. The generic arm writes each
/// mirrored pair back through `Set`/`DeletePropertyOrThrow`, so a hole in an
/// array-like moves to its mirror position instead of becoming `undefined`.
pub(super) extern "C" fn reverse(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let dense = with_current(|context| {
        let (cell, mut elements) = staged(context, this)?;
        // A frozen, sealed or non-extensible array takes the generic arm, whose
        // `Set` and `DeletePropertyOrThrow` raise the refusal; the vector
        // store below would write straight past it.
        if context.integrity_at(cell).is_some() {
            return None;
        }
        elements.reverse();
        store(context, cell, elements);
        Some(this)
    });
    if let Some(answer) = dense {
        return answer;
    }
    let Some(object) = generic::object(this, "reverse") else {
        return nothing();
    };
    match generic::reverse(object) {
        Some(()) => object,
        None => nothing(),
    }
}

/// `a.fill(v, from, to)` — in place, answering the receiver.
pub(super) extern "C" fn fill(_e: u64, this: u64, value: u64, from: u64, to: u64, _a3: u64) -> u64 {
    // A restricted array goes generic for the reason `reverse` gives:
    // `Object.freeze([1, 2]).fill(0)` is a `TypeError`, and it stored.
    let dense = with_current(|context| {
        let cell = Value(this).as_slot()?;
        context.elements_at(cell)?;
        context.integrity_at(cell).is_none().then_some(())
    });
    if dense.is_some() {
        let asked_start = argument(from).unwrap_or(0.0);
        let asked_end = argument(to);
        if throw::in_flight() {
            return nothing();
        }
        let answer = with_current(|context| {
            let (cell, mut elements) = staged(context, this)?;
            let start = relative(asked_start, elements.len());
            let end = asked_end.map_or(elements.len(), |asked| relative(asked, elements.len()));
            for slot in elements.iter_mut().take(end).skip(start) {
                *slot = value;
            }
            store(context, cell, elements);
            Some(this)
        });
        if let Some(answer) = answer {
            return answer;
        }
    }
    let Some(object) = generic::object(this, "fill") else {
        return nothing();
    };
    let Some(count) = generic::length(object) else {
        return nothing();
    };
    let asked_start = argument(from).unwrap_or(0.0);
    let asked_end = argument(to);
    if throw::in_flight() {
        return nothing();
    }
    let start = relative(asked_start, count);
    let end = asked_end.map_or(count, |asked| relative(asked, count));
    match generic::fill(object, value, start, end) {
        Some(()) => object,
        None => nothing(),
    }
}
