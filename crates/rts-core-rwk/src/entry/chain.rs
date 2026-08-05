//! Reading and writing what an object inherits from.
//!
//! # Why the language needs these and `new` did not
//!
//! `new F()` links a prototype without either of them: it reads `F.prototype` as
//! an ordinary property and the runtime does the linking, because both halves
//! happen inside one operation. A class is the case where they come apart —
//! `class B extends A {}` links `B.prototype` to `A.prototype` and `B` to `A` at
//! *definition* time, from a value the program computed, with no construction
//! anywhere near it.
//!
//! # Why this is not `Object.getPrototypeOf`
//!
//! It is the same operation, and that method is how a program will reach it once
//! there is an `Object` to hang it on. These exist first because the compiler
//! needs them before any program does: a class body is lowered into them.

use super::objects::undefined_of;
use super::with_current;
use crate::value::Value;

/// What an object inherits from — `null` when the chain ends there, `undefined`
/// when it was never given one.
///
/// The two are different and the difference is load-bearing: a class extending
/// `null` produces instances whose chain genuinely stops, where an object that
/// was never linked has one that was never started. Collapsing them would make
/// `class X extends null {}` indistinguishable from a plain object.
#[rtse::entry]
pub fn get_prototype(object: u64) -> u64 {
    with_current(|context| {
        let Some(cell) = Value(object).as_slot() else {
            return undefined_of(context);
        };
        match context.prototype_at(cell) {
            Some(found) => found,
            // A string, which inherits from the shared prototype without a link
            // of its own. Answered here rather than left absent so that
            // `super.x` inside a method on a string reaches the same object the
            // ordinary chain walk would.
            None => match context.text_at(cell).is_some() {
                true => match super::string::prototype_of(context) {
                    Some(prototype) => Value::from_slot(prototype).bits(),
                    None => undefined_of(context),
                },
                false => undefined_of(context),
            },
        }
    })
}

/// Links an object to what it inherits from, and answers the object.
///
/// Answers the object rather than nothing so that a lowering can chain — which
/// is what a class definition does three times in a row.
#[rtse::entry]
pub fn set_prototype(object: u64, prototype: u64) -> u64 {
    with_current(|context| {
        if let Some(cell) = Value(object).as_slot() {
            context.set_prototype(cell, prototype);
        }
        object
    })
}
