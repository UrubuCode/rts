//! Turning something iterable into the array a loop can walk.
//!
//! # Why this materialises rather than stepping
//!
//! The specification's iterator is a pair of calls per element: `next()`
//! answering an object with `done` and `value`. Expressing that here is two
//! property reads and a call for every pass of every `for-of` in the program,
//! and the object it reads them from is allocated per element.
//!
//! So this answers the elements **as an array**, and `for-of` becomes the
//! ordinary indexed loop `for-in` already reduces to — which buys `break`,
//! `continue`, labels and a fresh binding per pass without any of them being
//! written a second time.
//!
//! # What that costs, stated
//!
//! An iterable that is infinite or lazy cannot be walked this way, and one whose
//! side effects are meant to be interleaved with the body has them all up front.
//! Neither is reachable today — a generator is refused by name and no object can
//! declare `Symbol.iterator`, because there are no symbols — so the divergence is
//! recorded against the day one of those arrives rather than pretended away.
//!
//! # Why a string iterates by code POINT
//!
//! `for (const c of "😀")` yields one element, not two, where `"😀".length` is 2
//! and `"😀"[0]` is half a surrogate pair. That difference is the whole reason
//! the language grew `for-of` over strings, so getting it wrong here would make
//! the construct pointless.

use super::with_current;
use crate::text::Str;
use crate::value::Value;

/// The elements of an iterable, as an array.
///
/// An array is answered unchanged in content but **copied**, because the loop
/// walks what it is given and a body that pushes to the original must not walk
/// its own additions forever.
///
/// Anything that is not an array or a string answers an empty array, where the
/// language throws a `TypeError`. The same stated gap every operation here has
/// while a throw cannot find a handler in a caller — and it fails as a loop that
/// runs zero times, which is visible, rather than as a wrong element.
#[rtse::entry]
pub fn iterate(value: u64) -> u64 {
    // Two shapes, because one of them still has to be turned into values and
    // interning needs the context mutably — which the borrow that read the
    // elements is holding.
    let found = with_current(|context| {
        let Some(cell) = Value(value).as_slot() else {
            return Found::Nothing;
        };
        if let Some(elements) = context.elements_at(cell) {
            return Found::Values(elements.clone());
        }
        match context.text_at(cell) {
            Some(text) => Found::Text(code_points(text)),
            None => Found::Nothing,
        }
    });

    let values = match found {
        Found::Values(values) => values,
        Found::Nothing => Vec::new(),
        // Interned here, outside the borrow above.
        Found::Text(points) => with_current(|context| {
            points
                .into_iter()
                .map(|units| context.intern_value(Str::from_utf16(&units)).bits())
                .collect()
        }),
    };

    let array = super::array::array_new(values.len() as i64);
    with_current(|context| {
        if let Some(cell) = Value(array).as_slot()
            && let Some(elements) = context.elements_at_mut(cell)
        {
            *elements = values;
        }
        array
    })
}

/// What an iterable turned out to be.
enum Found {
    /// Elements already, from an array.
    Values(Vec<u64>),
    /// Code points that still have to become strings.
    Text(Vec<Vec<u16>>),
    /// Not something this engine iterates.
    Nothing,
}

/// A string's code points, as the units each is spelled with.
///
/// # Why this is not one element per unit
///
/// Because a surrogate pair is one character and two units. Splitting by unit
/// would make `for (const c of "😀")` run twice and hand the body half a
/// character each time — text that is not well formed and compares equal to
/// nothing. That difference is the whole reason the language grew `for-of` over
/// strings.
fn code_points(text: &Str) -> Vec<Vec<u16>> {
    let units: Vec<u16> = text.units().collect();
    let mut points = Vec::new();
    let mut at = 0;
    while at < units.len() {
        let wide = (0xD800..0xDC00).contains(&units[at])
            && at + 1 < units.len()
            && (0xDC00..0xE000).contains(&units[at + 1]);
        let span = if wide { 2 } else { 1 };
        points.push(units[at..at + span].to_vec());
        at += span;
    }
    points
}

/// Appends one value to an array, and answers the array.
///
/// Its own operation rather than a property write at a computed index: the index
/// is the current length, which the compiler does not know when a spread earlier
/// in the same literal contributed an unknown number of elements.
#[rtse::entry]
pub fn array_append(array: u64, value: u64) -> u64 {
    with_current(|context| {
        if let Some(cell) = Value(array).as_slot()
            && let Some(elements) = context.elements_at_mut(cell)
        {
            elements.push(value);
            let count = elements.len();
            super::array::set_length(context, cell, count);
        }
        array
    })
}

/// Appends everything an iterable yields, and answers the array.
///
/// What `...xs` is, wherever it is written. One operation rather than a loop the
/// compiler emits, because the count is not known while compiling and the loop
/// would be the same three instructions at every spread in the program.
#[rtse::entry]
pub fn array_append_all(array: u64, iterable: u64) -> u64 {
    let produced = iterate(iterable);
    with_current(|context| {
        let (Some(target), Some(source)) = (Value(array).as_slot(), Value(produced).as_slot())
        else {
            return array;
        };
        let Some(more) = context.elements_at(source).cloned() else {
            return array;
        };
        if let Some(elements) = context.elements_at_mut(target) {
            elements.extend(more);
            let count = elements.len();
            super::array::set_length(context, target, count);
        }
        array
    })
}

