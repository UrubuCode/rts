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
    if let Some(answered) = super::proxy::prototype_of(object) {
        return answered;
    }
    with_current(|context| {
        let Some(cell) = Value(object).as_slot() else {
            return undefined_of(context);
        };
        match context.prototype_at(cell) {
            Some(found) => found,
            // No link of its own — the same question a property miss asks,
            // which `objects::inherited_from` already answers for every kind
            // of cell (callable, text, array, plain object) including the
            // `Object.prototype`-is-root termination. Answering `undefined`
            // here unconditionally, as this used to, was a second and
            // disagreeing answer to that question: it substituted nothing for
            // a no-extends class's `.prototype` (which never runs
            // `set_prototype`) where property lookup already walked it to
            // `Object.prototype`, and a chain read with
            // `Object.getPrototypeOf` dead-ended one turn short of where a
            // property read on the same object would have found it.
            None => match super::objects::inherited_from(context, cell) {
                Some(proto_cell) => Value::from_slot(proto_cell).bits(),
                // `inherited_from` also answers `None` for the cell that IS
                // the root — `Object.prototype`'s own chain ends here, which
                // is `null`, not "no link recorded".
                None => Value::from_singleton(context.singletons.null).bits(),
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
    if let Some(answered) = super::proxy::set_prototype_of(object, prototype) {
        return answered;
    }
    with_current(|context| {
        if let Some(cell) = Value(object).as_slot() {
            context.set_prototype(cell, prototype);
        }
        object
    })
}
