//! Whether a `for`-`of` over a string may walk its code points as a list.
//!
//! # Why a list is the same walk
//!
//! A string's primordial iterator already builds the list: `String.prototype
//! [Symbol.iterator]` answers `list_iterator::over(iterate(s))`, every code point
//! made before the first `next()`. So walking that list by index is the protocol
//! with the wrapping taken off: the same elements, made at the same moment, in the
//! same order, and a string cannot change under the walk. What the wrapping costs
//! is a call to `next` and a `{ value, done }` record per character, which
//! `bench/analytic.ts` `for-of chars 16` measured as 432 ns per character through the
//! MIR stage, which steps, against 181 through the running emitter, which walks a list
//! whenever the source is a string at all.
//!
//! # And when it is not the same walk
//!
//! The running emitter's `typeof src === "string"` asks too little: a program that
//! replaced `String.prototype[Symbol.iterator]`, or the list iterator's `next`, or
//! put a `return` on the chain above it, is owed the protocol it wrote. So this asks
//! the three facts [`super::pattern`] asks of an array, of the CURRENT state, and
//! answers `undefined` where any fails -- which sends the loop to the stepping it
//! was going to do anyway.

use crate::value::Value;

use super::current::with_current;
use super::{Context, objects, symbol};

/// The code points of `source` as an array, where walking them is
/// indistinguishable from stepping the string's iterator; `undefined` otherwise.
#[rtse::entry]
pub fn text_walk(source: u64) -> u64 {
    let walkable = with_current(|context| {
        let is_text = Value(source)
            .as_slot()
            .is_some_and(|cell| context.text_at(cell).is_some());
        is_text && iterator_is_primordial(context) && step_is_primordial(context)
    });
    match walkable {
        // Outside the borrow: `iterate` takes its own.
        true => super::iterate::iterate(source),
        false => with_current(|context| objects::undefined_of(context)),
    }
}

/// `String.prototype[Symbol.iterator]` is still the method the prototype was built
/// with -- an own DATA property holding that native. A getter in its place is not.
fn iterator_is_primordial(context: &mut Context) -> bool {
    let Some(prototype) = super::string::prototype_of(context) else {
        return false;
    };
    let key = context.well_known(symbol::ITERATOR);
    if let crate::object::Key::Name(machine) = key
        && context.accessor_at(prototype, machine.index() as u32).is_some()
    {
        return false;
    }
    objects::own_property(context, prototype, key)
        .is_some_and(|found| super::string::is_iterator_method(context, found.bits()))
}

/// The list iterator still steps the way it was built to, and nothing above it
/// has a `return` that abandoning the walk would owe a call to.
fn step_is_primordial(context: &mut Context) -> bool {
    let Some(cell) = context.list_cursor_prototype else {
        // No list iterator has ever been made, so none has had its `next`
        // replaced -- the same reading `pattern` gives the array cursor.
        return true;
    };
    let next = context.well_known("next");
    match (objects::own_property(context, cell, next), context.list_cursor_next) {
        (Some(found), Some(installed)) if found.bits() == installed => {}
        _ => return false,
    }
    !super::pattern::carries_return(context, cell) && !super::pattern::iterator_carries_return(context)
}
