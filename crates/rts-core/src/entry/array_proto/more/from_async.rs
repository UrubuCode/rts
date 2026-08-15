//! `Array.fromAsync(items, mapFn, thisArg)` — [`super::from`] over values that
//! arrive later.
//!
//! # Reuse-check: what was searched, and what answers it
//!
//! - **`rts-cranelift`** — `src/sched/` owns what a promise IS and the order its
//!   reactions run in, and it is reached rather than restated: the waiting here
//!   is [`crate::entry::promise::promise_await`], which is the same entry point a
//!   compiled `await` calls. Nothing in this file inspects a `PromiseId`, a
//!   `Settlement` or a queue.
//! - **`crate::entry::iterate`** — the crate's ONE walk over anything iterable
//!   (an array, a string, a typed array, a collection, a `Symbol.iterator`), and
//!   the synchronous half of this is that call and nothing else. A second walk
//!   here would be a second answer to "what is iterable".
//! - **[`super::from`]** — its `array_like` test is called rather than copied,
//!   which is the point of it being `pub(super)`: `Array.fromAsync({length: 2})`
//!   and `Array.from({length: 2})` must agree about what an array-like is, and
//!   two spellings of that test is where one of them learns about
//!   `Symbol.iterator` and the other does not.
//!
//! # The divergence, and it is [`crate::entry::promise::machine`]'s
//!
//! `await` here DRAINS rather than suspending — the awaiting frame keeps the
//! machine — so this function runs to completion before it answers, and the
//! promise it hands back is already settled. A program that starts two
//! `Array.fromAsync` calls and awaits both gets the right elements in the right
//! order and no overlap between them. That is the engine's `await`, stated here
//! because a reader of this file would otherwise take it for this function's
//! choice; when `Inst::Suspend` lands, this changes with every other `await` and
//! not on its own.
//!
//! # Why a throw becomes a REJECTION rather than propagating
//!
//! Rule 8 of the crate README says a native that calls user code asks whether it
//! threw; it does not say what the answer must be. Every other caller here
//! PROPAGATES — it returns early and the compiled call site re-raises. This one
//! HANDLES, the way the promise drain does, because `Array.fromAsync` answers a
//! promise: `Array.fromAsync(bad()).catch(f)` runs `f` in every other runtime,
//! and a synchronous throw would run nothing and take the program with it.
//! [`crate::entry::throw::caught`] is the taking form, and it exists for exactly
//! this pair of cases.
//!
//! # What is NOT implemented, by name
//!
//! `IteratorClose`. A mapper that throws stops the walk and rejects, and the
//! iterator's `return()` is never called — the same limit [`super::from`] states
//! for the synchronous half, and for the same reason: the shared walk answers an
//! array rather than a cursor, so there is no open iterator left to close.

use super::super::super::rooted::Rooted;
use super::super::super::{functions, modules, promise, symbol, throw, with_current};
use super::super::{built, like};
use super::{calls, nothing};
use crate::value::Value;

/// `Array.fromAsync(items, mapFn, thisArg)`.
pub(in crate::entry) extern "C" fn from_async(
    _e: u64,
    _this: u64,
    items: u64,
    mapper: u64,
    receiver: u64,
    _a3: u64,
) -> u64 {
    let produced = gathered(items, mapper, receiver);
    // Asked BEFORE the array is built, and taken rather than looked at — see the
    // module doc for why this is the one shape here that handles instead of
    // propagating.
    if let Some(reason) = throw::caught() {
        return with_current(|context| promise::settled(context, reason, true));
    }
    let array = built(produced.take());
    with_current(|context| promise::settled(context, array, false))
}

/// Every element, awaited and mapped, in order.
///
/// Answers a [`Rooted`] rather than a `Vec` because the caller allocates an
/// array out of it: until the array exists and holds them, these values are
/// named only by a buffer on the Rust heap, which no scan of ours reaches. Same
/// hole `map` had, same mechanism — see [`crate::entry::rooted`].
///
/// A throw at any step stops the walk and is left in flight for [`from_async`]
/// to take.
fn gathered(items: u64, mapper: u64, receiver: u64) -> Rooted {
    let mut produced = Rooted::new();
    match async_steps(items) {
        Some(iterator) => drive(iterator, mapper, receiver, &mut produced),
        None => walk(items, mapper, receiver, &mut produced),
    }
    produced
}

/// The iterator an object's `Symbol.asyncIterator` answers, if it declares one.
///
/// `None` covers both "declares none" and "declared something that is not
/// callable", because the synchronous fallback is the right answer to each: an
/// array has no `Symbol.asyncIterator` and is still perfectly good input.
///
/// # Why the method is read as data
///
/// [`crate::entry::iterate::protocol`] draws the same boundary for
/// `Symbol.iterator`, `next` and `done`: a getter on one of them is user code,
/// and this read happens under a borrow. A real async iterable declares a
/// method.
fn async_steps(items: u64) -> Option<u64> {
    let method = with_current(|context| {
        let name = format!("{}asyncIterator", symbol::PREFIX);
        modules::get_member(context, items, &name)
    });
    if !calls(method) {
        return None;
    }
    let absent = nothing();
    let iterator = functions::call(method, items, absent, absent, absent, absent);
    if throw::in_flight() {
        return None;
    }
    let next = with_current(|context| modules::get_member(context, iterator, "next"));
    calls(next).then_some(iterator)
}

/// Steps a real async iterator to exhaustion.
///
/// Each `next()` answers a promise of `{done, value}`; the STEP is awaited and
/// the value is not, which is the specification's asymmetry rather than an
/// omission — an async iterator has already resolved whatever it yields, and
/// awaiting again would unwrap a promise a generator deliberately yielded as a
/// value.
///
/// Every call is outside a borrow: `next`, the awaiting drain, and the two
/// property reads on what it answered are all user code or reach it.
fn drive(iterator: u64, mapper: u64, receiver: u64, produced: &mut Rooted) {
    let absent = nothing();
    let next = with_current(|context| modules::get_member(context, iterator, "next"));
    loop {
        let step = functions::call(next, iterator, absent, absent, absent, absent);
        if throw::in_flight() {
            return;
        }
        let step = promise::promise_await(step);
        if throw::in_flight() {
            return;
        }
        // `done` before `value`, the order the specification states — an
        // iterator whose `done` getter has a side effect observes it first.
        let (done, value) = with_current(|context| {
            (
                modules::get_member(context, step, "done"),
                modules::get_member(context, step, "value"),
            )
        });
        if super::super::super::primitives::to_boolean(done) {
            return;
        }
        let index = produced.len();
        let Some(held) = mapped(value, mapper, receiver, index) else {
            return;
        };
        produced.values().push(held);
    }
}

/// The synchronous sources, every element of which is AWAITED.
///
/// That is what makes `Array.fromAsync([Promise.resolve("a")])` answer `["a"]`
/// where `Array.from` of the same answers the promise: the specification wraps a
/// sync iterator in an async one, and awaiting each value is the whole of what
/// that wrapper does here.
///
/// The elements are rooted while they are walked, and not merely on the way out:
/// awaiting DRAINS, a drain runs reactions, and a reaction allocates — so a list
/// held in a bare `Vec` between two awaits is invisible to a collection that
/// happens in the second one.
fn walk(items: u64, mapper: u64, receiver: u64, produced: &mut Rooted) {
    let source = match super::from::array_like(items) {
        true => like::values_of(items),
        false => {
            let array = super::super::super::iterate::iterate(items);
            with_current(|context| {
                Value(array)
                    .as_slot()
                    .and_then(|cell| context.elements_at(cell).cloned())
                    .unwrap_or_default()
            })
        }
    };
    if throw::in_flight() {
        return;
    }
    let source = Rooted::with(source);
    for index in 0..source.len() {
        let value = promise::promise_await(source.as_slice()[index]);
        if throw::in_flight() {
            return;
        }
        let Some(held) = mapped(value, mapper, receiver, index) else {
            return;
        };
        produced.values().push(held);
    }
}

/// One element through `mapFn`, awaited.
///
/// `None` means a throw is in flight and the walk must stop — rule 8's shape,
/// the same `Option` [`crate::entry::collections::invoke`] answers, so a caller
/// has to decide rather than inherit `undefined` as though it were the mapper's
/// answer.
///
/// The mapper is called with `(value, index)` and no third argument: the
/// specification hands it those two, where `Array.prototype.map` also passes the
/// array. There is no array yet.
fn mapped(value: u64, mapper: u64, receiver: u64, index: usize) -> Option<u64> {
    if !calls(mapper) {
        return Some(value);
    }
    let absent = nothing();
    let at = Value::from_f64(index as f64).bits();
    let answered = functions::call(mapper, receiver, value, at, absent, absent);
    if throw::in_flight() {
        return None;
    }
    // Awaited, which is what makes an `async` mapper work: the specification
    // awaits the mapper's result before it becomes an element.
    let held = promise::promise_await(answered);
    match throw::in_flight() {
        true => None,
        false => Some(held),
    }
}
