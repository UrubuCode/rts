//! Making an array from what a caller handed over, and deciding which class it
//! belongs to.
//!
//! # Why this is a file and not three functions in [`super`]
//!
//! Because the class question turned out to have a body. `Array(n)`, `Array.of`
//! and `Array.isArray` are each four lines and would have stayed where they
//! were; what did not fit is [`inherited_for_new`], which exists because an
//! array's prototype is SUBSTITUTED rather than linked and a subclass therefore
//! has to write one by hand. That put [`super`] over this crate's 500-line
//! ceiling, and the cohesive thing to move out is "what a construction
//! produces" rather than an arbitrary tail.

use super::super::with_current;
use super::{arguments_at, built};
use crate::value::Value;

/// `Array(n)` and `new Array(n)`.
///
/// One argument means two different things and the language decided which by
/// *type*, not by count: `Array(3)` is three empty slots and `Array("3")` is one
/// element. Getting that backwards makes `Array(x)` silently produce a different
/// array whenever `x` stops being a number, which is a wrong program that runs.
///
/// `new Array(n)` answers this array rather than the object `construct` made,
/// because a constructor returning an object wins — so nothing here has to know
/// whether it was called with `new`. What it DOES have to know is which class
/// `new` named, which is [`inherited_for_new`]'s question.
pub(super) extern "C" fn make(_e: u64, _this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let given = with_current(|context| arguments_at(context, 0, [a0, a1, a2, a3]));
    let made = if given.len() == 1
        && let Some(count) = Value(given[0]).numeric()
        && (0.0..4_294_967_295.0).contains(&count)
        && count.fract() == 0.0
    {
        super::super::array::array_new(count as i64)
    } else {
        built(given)
    };
    inherited_for_new(made);
    made
}

/// `Array.of(…)` — an array of exactly the arguments given.
///
/// The method that exists because `Array(3)` does not mean what it looks like.
pub(super) extern "C" fn of(_e: u64, _this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let given = with_current(|context| arguments_at(context, 0, [a0, a1, a2, a3]));
    built(given)
}

/// `Array.isArray(x)`.
///
/// Asks the side table, which is what being an array IS here — not the
/// prototype, because `Object.setPrototypeOf(a, null)` leaves `a` an array and a
/// chain walk would say otherwise. That is also why it survives
/// [`inherited_for_new`] unchanged: a subclass instance has a different
/// prototype and is still an array.
pub(super) extern "C" fn is_array(
    _e: u64,
    _this: u64,
    value: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    with_current(|context| {
        let held = Value(value)
            .as_slot()
            .is_some_and(|cell| context.elements_at(cell).is_some());
        Value::from_bool(held).bits()
    })
}

/// Links a constructed array to the prototype of the class `new` actually named.
///
/// # Why an array needs this when nothing else here does
///
/// Because an array's prototype is SUBSTITUTED rather than linked — [`super`]'s
/// own documentation says why every array shares one object and stores no link
/// — and substitution answers `Array.prototype` for every array there is. So
/// `class Fancy extends Array {}` produced instances whose chain never reached
/// `Fancy.prototype`: `new Fancy(1, 2)` was not `instanceof Fancy`, its
/// `constructor` resolved all the way to `Object`, and every method the subclass
/// declared was missing. A link is written only for the arrays that need one, so
/// the word per array that substitution exists to save is still saved by every
/// array a program actually writes.
///
/// # Why the target is checked against `Array.prototype` and not merely read
///
/// [`super::super::functions::prototype_for_new`] answers the target of whatever
/// construction is in progress, which for a bare `Array(3)` written INSIDE some
/// unrelated `new Thing()` is `Thing` — the array would inherit from
/// `Thing.prototype`. `regex` and `object_global` call the same helper and
/// accept exactly that, because `RegExp("a")` inside a constructor is
/// vanishingly rare; `Array(n)` is not, and this is the one place where the
/// accepted risk would be paid often enough to matter.
///
/// So the target's prototype has to REACH `Array.prototype` before it is
/// believed, which is the shape a subclass of `Array` has and no unrelated class
/// does. What the check still admits is a `class Fancy extends Array` whose own
/// constructor calls bare `Array(3)` — and there the answer it produces is the
/// one that class would have wanted anyway.
fn inherited_for_new(made: u64) {
    with_current(|context| {
        let Some(cell) = Value(made).as_slot() else {
            return;
        };
        let Some(own) = context
            .array_prototype
            .map(|prototype| Value::from_slot(prototype).bits())
        else {
            return;
        };
        let asked = super::super::functions::prototype_for_new(context, own);
        if asked == own {
            return;
        }
        let mut walked = Value(asked).as_slot();
        while let Some(step) = walked {
            if Value::from_slot(step).bits() == own {
                context.set_prototype(cell, asked);
                return;
            }
            walked = context.prototype_at(step).and_then(|up| Value(up).as_slot());
        }
    });
}
