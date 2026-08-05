//! `Promise`, and the microtask queue its reactions run on.
//!
//! # Half of asynchrony, and the half that can land
//!
//! The compiler refuses `async`, `await`, `yield` and `for await` **by name**.
//! So nothing here is ever reached through `await`: a promise exists in a
//! program that writes `new Promise(executor)`, `.then`, `.catch`, `.finally`
//! and the combinators, and that is the whole surface.
//!
//! The other half is a frame transformation in `rts-codegen` over
//! [`rts_cranelift::frame`] — an `await` is a suspension point, and resuming one
//! is a jump to a [`rts_cranelift::frame::ResumeLabel`] inside a parked frame.
//! It is not blocked on anything in this module: what lands here is the object
//! and the ordering, and the transformation lands against the same queue.
//!
//! # The decision: the machine drives this, and one thing does not fit
//!
//! [`rts_cranelift::sched`] owns the state machine, the waiter lists, the wake
//! sets and the queues, and this module owns none of them. `PromiseTable`
//! decides what settling does, `Queues` decides the order, `Scheduler::settle`
//! joins the two. Writing a second queue here would be a second answer to "what
//! runs next", and the ordering is the one property of a promise implementation
//! that programs depend on without being told they do.
//!
//! What does **not** fit is [`rts_cranelift::sched::Scheduler::park`]. It takes
//! a [`rts_cranelift::sched::Continuation`], which is a parked frame plus a
//! resume label — and a `.then` callback is not a parked frame. It is an
//! ordinary callable, with no frame, no label, and nothing to resume into.
//! Inventing a label for it would be a number no `frame` transformation issued
//! and nothing could ever jump to.
//!
//! So this module attaches with [`rts_cranelift::sched::PromiseTable::attach`]
//! and wakes with [`rts_cranelift::sched::Queues::wake`] directly, and keeps a
//! side table from `ContinuationId` to the *reaction* that identifier stands
//! for. `Continuation` and `ResumeLabel` are exactly what the `await` half will
//! use, unchanged — the two kinds of waiter share one identifier space and one
//! queue, which is what makes their relative ordering a fact rather than a
//! coincidence.
//!
//! # Why adoption is not `PromiseTable::adopt`
//!
//! `adopt` settles the adopting promises **transitively inside the machine**,
//! and the machine carries no value. This layer would learn only that something
//! settled, not which promises did — so [`crate::schedule::Settlements`] would
//! hold nothing for exactly the promises a program then reads. Adoption is
//! instead an ordinary reaction with no handlers, which is also what the
//! specification says it is: resolving with a thenable *calls its `then`*, and
//! the result is observable through the queue rather than instantaneous.
//!
//! The cycle detection `adopt` offers goes with it. `resolve(p, p)` is refused
//! here directly, with the `TypeError` the language raises. A longer cycle —
//! `p` resolved with a promise that is later resolved with `p` — stays pending
//! for ever, which is what every engine does with it and what the language says
//! happens.
//!
//! # The borrow rule this module is written around
//!
//! [`with_current`] holds a `RefCell` borrow for the length of its body, a
//! second borrow panics, and an `extern "C"` frame cannot unwind — so the
//! process **aborts**. An executor, a reaction and a combinator all run user
//! code, which is more places than anywhere else in this crate. Every one of
//! them is written as: decide under a borrow, drop it, call, re-borrow to
//! store. [`react::Step`] is that shape made into a type — the decision is a
//! value, so the call cannot accidentally happen where the decision was made.
//!
//! Unlike `entry::collections`, nothing here has to move a table out with
//! `std::mem::take`: the machine and the context are never both needed for
//! longer than a copy, so every operation reads one, then the other, in
//! sequence.

mod class;
mod combinators;
mod drain;
mod group;
mod react;
mod state;

pub(in crate::entry) use class::register_promise;
pub use drain::drain_microtasks;
pub(in crate::entry) use state::Machine;

use super::objects::undefined_of;
use super::with_current;
use crate::value::Value;

/// `undefined`, from outside a borrow.
fn undefined() -> u64 {
    with_current(|context| undefined_of(context))
}

/// An array holding these values.
///
/// Through `array::array_new`, which is an entry point and takes a
/// borrow of its own — so this must be called with none held. The same helper
/// `collections::array_of` is, and for the same reason: nothing else in
/// this crate may register a cell as an array.
fn array_of(values: Vec<u64>) -> u64 {
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

/// The elements an iterable yields.
///
/// `iterate::iterate` answers an array for an array and for a string,
/// and an EMPTY array for anything else. So `Promise.all(anArray)` works and
/// `Promise.all(aSet)`, `Promise.all(aGenerator)` or `Promise.all(aMap.values())`
/// fulfil immediately with `[]` rather than waiting for anything — the stated
/// gap of the iteration protocol, which has no `Symbol.iterator` dispatch,
/// showing up here rather than a defect in these combinators.
fn elements_of(iterable: u64) -> Vec<u64> {
    let array = super::iterate::iterate(iterable);
    with_current(|context| {
        Value(array)
            .as_slot()
            .and_then(|cell| context.elements_at(cell).cloned())
            .unwrap_or_default()
    })
}
