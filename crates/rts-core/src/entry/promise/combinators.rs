//! `Promise.all`, `allSettled`, `race` and `any`.
//!
//! # What the argument can be
//!
//! Anything iterable. [`super::elements_of`] goes through `iterate`, which
//! dispatches on `Symbol.iterator`, so an array, a string, a `Set`, a `Map`'s
//! entries and a generator all wait for what they contain — verified against Bun
//! 2026-08-14 over all four.
//!
//! This paragraph used to say "an **array**, today", and it stayed there after
//! the walk grew the protocol: a reader had every reason to believe
//! `Promise.all(aSet)` fulfilled with `[]`. The reason it is worth a note rather
//! than a silent deletion is that the fix was never these four's to make — an
//! implementation that special-cased `Set` here would have been a second answer
//! to what an iterable is, and the right one arrived one level down.
//!
//! # Why an empty input is decided before anything is allocated
//!
//! Because the four answers differ, and three of them are immediate:
//! `all([])` and `allSettled([])` fulfil with `[]`, `any([])` rejects at once,
//! and `race([])` **stays pending for ever** — which is the specification and
//! not an oversight. Deciding it first also keeps the group machinery from
//! having to represent a count that starts at zero.

use rts_cranelift::sched::Settlement;

use super::group::{Group, Kind};
use super::react::Handler;
use super::state;
use crate::entry::objects::undefined_of;
use crate::entry::with_current;
use crate::value::Value;

/// The shared body of the four static combinators.
///
/// `this` is the constructor, exactly as `NewPromiseCapability(C)` reads it —
/// see [`super::capability`], including why a receiver this engine cannot use
/// falls back to the intrinsic instead of throwing.
pub(super) fn combine(this: u64, iterable: u64, kind: Kind) -> u64 {
    match super::capability::chosen(this) {
        Some(constructor) => foreign(this, constructor, iterable, kind),
        None => intrinsic(iterable, kind),
    }
}

/// The combinator on a constructor of the program's own.
///
/// # The protocol a program can count
///
/// `C.resolve` is read ONCE, before the walk, and called once per element —
/// which is what `GetPromiseResolve` and the loop after it say, and what a
/// program watching an accessor on `resolve` observes. Reading it per element
/// would be the same answer for every program that is not watching and a
/// different one for every program that is.
fn foreign(this: u64, constructor: u64, iterable: u64, kind: Kind) -> u64 {
    let Some((promise, result)) = super::capability::derive(constructor) else {
        return super::undefined();
    };
    let adopt = super::super::array_proto::species::property(this, "resolve");
    let callable = with_current(|context| {
        adopt.is_some_and(|value| {
            Value(value)
                .as_slot()
                .is_some_and(|cell| context.callable_at(cell).is_some())
        })
    });
    if !callable {
        // The capability REJECTS rather than the call throwing: the caller
        // already said where a failure goes by writing `.catch` on the answer.
        let reason = with_current(|context| state::type_error(context, "resolve is not a function"));
        with_current(|context| state::reject(context, result, reason));
        return promise;
    }
    let adopt = adopt.unwrap_or_else(super::undefined);
    let elements = super::elements_of(iterable);
    if let Some(reason) = crate::entry::throw::caught() {
        with_current(|context| state::reject(context, result, reason));
        return promise;
    }
    if elements.is_empty() {
        settle_empty(result, kind);
        return promise;
    }
    // `C.resolve(element)` for every element BEFORE any borrow, because each
    // call is user code. Rooted: until the group holds them they are named by a
    // `Vec` on the Rust heap, which no scan reaches.
    let mut adopted = crate::entry::rooted::Rooted::new();
    let absent = super::undefined();
    for element in elements {
        let answered = crate::entry::functions::call(adopt, this, element, absent, absent, absent);
        if let Some(reason) = crate::entry::throw::caught() {
            with_current(|context| state::reject(context, result, reason));
            return promise;
        }
        adopted.values().push(answered);
    }
    with_current(|context| {
        let adopted = adopted.take();
        let group = context
            .promises
            .open(Group::new(kind, result, vec![absent; adopted.len()]));
        for (index, element) in adopted.into_iter().enumerate() {
            let Some(source) = observed(context, element) else {
                let reason = state::type_error(context, "the heap is full");
                state::reject(context, result, reason);
                return promise;
            };
            state::react(context, source, Handler::Member { group, index });
        }
        promise
    })
}

/// The combinator on `Promise` itself, which is every program that never
/// subclassed one.
///
/// It reads no `resolve` and constructs nothing: the four answers are promises
/// of this module's own making, so the protocol above could only ever arrive at
/// what this already does — two property reads and a construction per element
/// slower.
fn intrinsic(iterable: u64, kind: Kind) -> u64 {
    // Before any borrow: `iterate` is an entry point and takes one of its own.
    let elements = super::elements_of(iterable);
    // Rule 8 of `crates/rts-core/README.md`, in its HANDLING form: walking the
    // argument runs user code — `Symbol.iterator`, `next`, a getter — and any of
    // it may throw. The specification says the combinator REJECTS in that case;
    // it never throws out of the call, because a caller writing
    // `Promise.all(x).catch(f)` has already said where the failure goes.
    //
    // Left in flight it was both wrong answers at once: the throw escaped
    // `Promise.all` synchronously AND the call answered `undefined`, so the
    // `.then` on the next line died with "Cannot read properties of undefined".
    // `caught` takes it, which is the right half here — this native is handling
    // the throw rather than propagating it.
    if let Some(reason) = crate::entry::throw::caught() {
        return super::rejected_with(reason);
    }
    if elements.is_empty() {
        return empty(kind);
    }
    // The array every collecting kind eventually answers is built through
    // `array_new`, which is an entry point — so it cannot be made from inside
    // the borrow below. It is made when the group finishes, in the drain.
    with_current(|context| {
        let absent = undefined_of(context);
        let Some((cell, result)) = state::fresh(context) else {
            return absent;
        };
        let group = context
            .promises
            .open(Group::new(kind, result, vec![absent; elements.len()]));
        for (index, element) in elements.into_iter().enumerate() {
            let Some(source) = observed(context, element) else {
                // The region is full, so the next allocation fails too and a
                // group short of one element would never reach zero — a promise
                // that hangs, which is the one outcome worse than a rejection.
                let reason = state::type_error(context, "the heap is full");
                state::reject(context, result, reason);
                return Value::from_slot(cell).bits();
            };
            state::react(context, source, Handler::Member { group, index });
        }
        Value::from_slot(cell).bits()
    })
}

/// The promise an element is observed through.
///
/// A promise is observed as it stands; anything else is wrapped in one that is
/// resolved with it — which is `Promise.resolve`'s rule, and reusing it is what
/// makes `Promise.all([1, thenable, p])` treat all three the same way.
fn observed(
    context: &mut crate::entry::Context,
    element: u64,
) -> Option<rts_cranelift::sched::PromiseId> {
    if let Some(cell) = Value(element).as_slot()
        && let Some(id) = context.promises.id_of(cell)
    {
        return Some(id);
    }
    let (_, id) = state::fresh(context)?;
    state::resolve(context, id, element);
    Some(id)
}

/// What each combinator answers for an input with nothing in it.
fn empty(kind: Kind) -> u64 {
    let made = with_current(state::fresh);
    let Some((cell, id)) = made else {
        return super::undefined();
    };
    settle_empty(id, kind);
    Value::from_slot(cell).bits()
}

/// The same four answers, written into a promise that already exists.
///
/// Split out because a capability's promise is built before the walk and cannot
/// be replaced by one made here: `Promise.all.call(C, [])` must still answer the
/// `C` its author's constructor produced.
fn settle_empty(id: rts_cranelift::sched::PromiseId, kind: Kind) {
    // Built before the borrow, because an array comes from an entry point —
    // `all` and `allSettled` fulfil with it, and `any` reports it as the
    // aggregate's (empty) `errors`.
    let collected = match kind {
        Kind::Race => None,
        _ => Some(super::array_of(Vec::new())),
    };
    with_current(|context| {
        match (kind, collected) {
            (Kind::All | Kind::AllSettled, Some(array)) => {
                state::settle(context, id, Settlement::Fulfilled, array);
            }
            (Kind::Any, Some(array)) => {
                let reason = super::group::aggregate(context, array);
                state::reject(context, id, reason);
            }
            // `Promise.race([])` never settles. Nothing to do is the answer, not
            // a case that was forgotten.
            (Kind::Race, _) => {}
            // Unreachable: every kind but `race` carries the array above.
            (_, None) => {}
        }
    })
}
