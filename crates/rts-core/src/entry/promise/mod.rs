//! `Promise`, and the microtask queue its reactions run on.
//!
//! # Half of asynchrony, and the half that can land
//!
//! This said the compiler refused `async`, `await`, `yield` and `for await` **by
//! name**, so that nothing here was ever reached through one. All four run now,
//! and the sentence stood long enough that a reader could have believed the
//! whole `await` path was dead code.
//!
//! The SUSPENSION is built, and it needed nothing new in the machine — which is
//! what [`machine`] predicted: `rts_cranelift::frame` already parks and resumes
//! a frame three ways, and every generator goes through it. A plain `async
//! function`'s body is one of those frames now, `await` hands its promise out
//! at the suspension exactly as `yield` hands out a value, and [`async_fn`] is
//! the driver that re-enters it when the promise settles. So `f(); g()` runs
//! `g` before the half of `f` that follows its first `await`, which is the one
//! thing an async function decides.
//!
//! Two bodies still DRAIN instead: an `async function*`, whose frame is already
//! stepped by `next()`, and a module's top level, which the host drives.
//! [`machine`] says why in full, so there is one answer to it.
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

mod async_fn;
mod class;
mod machine;
mod combinators;
mod drain;
mod group;
mod react;
mod settler;
mod state;
mod thenable;

pub(in crate::entry) use class::{PROMISE_TYPES, register_promise};
pub use drain::drain_microtasks;
pub use async_fn::{ASYNC_START_ENTRY, async_start};
pub use machine::{promise_await, promise_new, promise_settle};
pub(in crate::entry) use state::Machine;

use super::objects::undefined_of;
use super::with_current;
use crate::value::Value;

/// A promise already resolved with a value — what `Promise.resolve(v)` is.
///
/// Here rather than only inside [`class`] because a second caller needs it and
/// is not a program: `import()` answers a promise for a namespace the runtime
/// already holds. Written once, so the two cannot come to disagree about the
/// two rules that are easy to get wrong — a promise is passed through as it
/// stands (`Promise.resolve(p) === p`), and a thenable is ADOPTED rather than
/// wrapped, which is why this goes through `state::resolve` and not through a
/// straight fulfilment.
pub(in crate::entry) fn resolved_with(value: u64) -> u64 {
    with_current(|context| {
        if let Some(cell) = Value(value).as_slot()
            && context.promises.id_of(cell).is_some()
        {
            return value;
        }
        let Some((cell, id)) = state::fresh(context) else {
            return undefined_of(context);
        };
        state::resolve(context, id, value);
        Value::from_slot(cell).bits()
    })
}

/// A promise already rejected with a reason — the pair of [`resolved_with`].
///
/// A reason is **not** adopted: `Promise.reject(p)` rejects with the promise
/// itself, which is the asymmetry a symmetrical implementation gets wrong
/// quietly.
pub(in crate::entry) fn rejected_with(reason: u64) -> u64 {
    with_current(|context| {
        let Some((cell, id)) = state::fresh(context) else {
            return undefined_of(context);
        };
        state::reject(context, id, reason);
        Value::from_slot(cell).bits()
    })
}

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
/// `iterate::iterate` is the crate's one walk, and it dispatches on
/// `Symbol.iterator` — so `Promise.all(aSet)`, `Promise.all(aGenerator)` and
/// `Promise.all(aMap.values())` wait for what they contain. This doc said the
/// opposite, and said it long after it had stopped being true: the walk answered
/// an EMPTY array for anything but an array or a string when it was written, and
/// the four combinators inherited that. Verified 2026-08-14 against Bun over all
/// four shapes.
///
/// What is still divergent is the walk's, not this: an iterable is drained to
/// exhaustion before any element is observed, so an infinite one does not
/// terminate. `entry::iterate` states it and is where it is fixed.
fn elements_of(iterable: u64) -> Vec<u64> {
    let array = super::iterate::iterate(iterable);
    with_current(|context| {
        Value(array)
            .as_slot()
            .and_then(|cell| context.elements_at(cell).cloned())
            .unwrap_or_default()
    })
}

/// A promise that is already settled, for a host answering an async-shaped API.
///
/// # Why a host needs this, and what it is NOT
///
/// `fs.promises.readFile` answers a promise. This engine has no asynchronous
/// file I/O — nothing here yields to an event loop — so the work happens before
/// the promise exists and the promise is handed over already settled.
///
/// That is a real divergence and it is the shape of it that matters: the CALL
/// blocks where Node's does not, so two reads do not overlap and a program that
/// starts several and awaits them all takes as long as doing them in order. What
/// is NOT wrong is the observable order of the reactions — a `.then` still runs
/// on the microtask drain rather than immediately, because settling a promise
/// goes through the machine's queue like any other settlement.
///
/// The alternative was refusing `fs.promises` entirely. It was rejected because
/// a promise-shaped answer over synchronous work is what every runtime's first
/// version of this was, and because the divergence is one of TIMING rather than
/// of value — a program gets the right bytes.
pub fn settled(context: &mut super::Context, value: u64, rejected: bool) -> u64 {
    // The class REGISTERED, not merely asked for. `state::fresh` links
    // `Promise.prototype` from what a registration recorded, and a program that
    // never writes the word `Promise` has never triggered one — so the promise
    // came back with no prototype and therefore no `.then`, and every reaction a
    // host-made promise carried was silently unreachable.
    //
    // Idempotent: a registration answers what it already made. That is what lets
    // this be a call rather than a question about whether one has happened.
    register_promise(context);
    let Some((cell, id)) = state::fresh(context) else {
        return super::objects::undefined_of(context);
    };
    match rejected {
        true => state::settle(context, id, rts_cranelift::sched::Settlement::Rejected, value),
        // Through `resolve` rather than `settle`, because resolving with
        // another promise adopts it — and a host answering a value it got from
        // somewhere else must not turn a promise into a promise for a promise.
        false => state::resolve(context, id, value),
    }
    crate::value::Value::from_slot(cell).bits()
}
