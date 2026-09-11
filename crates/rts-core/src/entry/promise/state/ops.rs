//! Every operation over the machine that runs no user code.
//!
//! Split from [`super`] at the crate's 500-line ceiling, and along the seam the
//! module was already written around: [`super::Machine`] is the TABLES — what a
//! promise settled with, which reaction a waiter identifier stands for, which
//! cell is which promise — and these are the verbs over them. The rule that
//! holds both halves together is the one in the file header above: nothing here
//! calls user code, so every one of these may be reached from inside a borrow.

use rts_cranelift::sched::{Attachment, Delivery, PromiseId, Settlement};

use crate::entry::Context;
use crate::entry::objects::undefined_of;
use crate::text::Str;
use crate::value::Value;

use super::super::react::{Handler, Reaction};
use super::super::thenable;

/// A pending promise, with `Promise.prototype` on it.
pub(in crate::entry::promise) fn fresh(context: &mut Context) -> Option<(u32, PromiseId)> {
    let cell = crate::entry::native::plain(context)?;
    // Through the class's own registration rather than a field on the context,
    // for the reason `collections::fresh` records: what `.then` answers must
    // itself answer to the methods this module installed.
    if let Some(prototype) = crate::entry::class_support::prototype(context, "Promise") {
        context.set_prototype(cell, prototype);
    }
    let id = context.promises.create(cell);
    Some((cell, id))
}

/// The object a constructor writes into: the one `new` made, or one made here.
///
/// # Why a plain call is not refused
///
/// `Promise(f)` without `new` is a `TypeError` in the language, and raising one
/// here would end the program — `entry::throw` cannot find a handler in
/// a caller. The same tolerance `Error("x")` and `Map()` settle on, and it fails
/// where the program uses the result rather than at an arbitrary later point.
pub(in crate::entry::promise) fn built(context: &mut Context, this: u64) -> Option<(u32, PromiseId)> {
    match Value(this).as_slot() {
        // The cell `construct` already made, which carries the prototype of
        // whatever class was named — so `class Mine extends Promise {}` gives an
        // instance with `Mine.prototype` on it and this only records what it is.
        Some(cell) => Some((cell, context.promises.create(cell))),
        None => fresh(context),
    }
}

/// Settles a promise, waking whatever was waiting, and remembers the value.
///
/// Runs no user code: the waiters are queued, and the drain runs them. That is
/// the single most-tested property of a promise implementation —
/// `Promise.resolve(1).then(f)` must not call `f` before `then` returns — and it
/// is a property of the machine's queue rather than of care taken here.
pub(in crate::entry::promise) fn settle(context: &mut Context, id: PromiseId, settlement: Settlement, value: u64) {
    let machine = &mut context.promises;
    let delivery = machine
        .scheduler
        .settle(&mut machine.promises, id, settlement);
    // Whether anything was already waiting, which is what decides an unhandled
    // rejection. Taken from the machine's own answer rather than re-derived from
    // this module's tables — the second copy is the one that would come to
    // disagree.
    let had_waiters = delivery != Delivery::Nobody;
    machine
        .settlements
        .record(id, settlement, Value(value), had_waiters);
}

/// Resolves a promise, adopting whatever it was resolved with.
///
/// The three cases the language distinguishes, in the order it distinguishes
/// them: the promise itself, another promise or thenable, an ordinary value.
pub(in crate::entry::promise) fn resolve(context: &mut Context, id: PromiseId, value: u64) {
    if let Some(cell) = Value(value).as_slot() {
        if context.promises.id_of(cell) == Some(id) {
            // `resolve(p)` inside `new Promise(resolve => …)`. Nothing could
            // ever settle it, so the language rejects with a `TypeError` rather
            // than leaving a promise that hangs and says nothing.
            let reason = type_error(context, "Chaining cycle detected for promise");
            settle(context, id, Settlement::Rejected, reason);
            return;
        }
        // A native promise is NOT short-circuited here, and that used to be
        // the whole of the next twelve lines: adoption was an ordinary reaction
        // with no handlers, so `resolve(p, q)` settled one microtask after `q`
        // did. The specification enqueues a `NewPromiseResolveThenableJob` and
        // that job CALLS `q.then(resolve, reject)` — two ticks, not one, and a
        // promise is a thenable like any other. Falling through to the thenable
        // path below is what makes it two, because that path is the job.
        //
        // Measured against Bun 2026-09-11 on `claude2-adoption-tick-penalty`:
        // `new Promise(r => r(donor)).then(f)` runs `f` on tick 3, where the
        // short circuit ran it on tick 2 — one tick ahead of every runtime, and
        // observable to any program with two chains in flight. The short
        // circuit was the cheaper answer to a question the language does not
        // ask cheaply.
        let queued = match thenable::then_of(context, cell) {
            // A callable `then` already in hand. It is user code and is called
            // from a microtask, which is what the specification says and what
            // stops it from running inside this borrow.
            thenable::Then::Ready(then_fn) => Some(Some(then_fn)),
            // A `then` behind a GETTER. Reading it is itself user code, so even
            // the read waits for the microtask — see [`Then`] for why that is
            // not the divergence it looks like.
            thenable::Then::Deferred => Some(None),
            thenable::Then::Absent => None,
        };
        if let Some(then_fn) = queued {
            let waiter = context.promises.record(Reaction {
                source: None,
                handler: Handler::Thenable {
                    thenable: value,
                    then_fn,
                    promise: id,
                },
            });
            context.promises.scheduler.queues().wake(waiter);
            return;
        }
    }
    settle(context, id, Settlement::Fulfilled, value);
}

/// Rejects a promise with a reason.
pub(in crate::entry::promise) fn reject(context: &mut Context, id: PromiseId, reason: u64) {
    settle(context, id, Settlement::Rejected, reason);
}


/// Attaches a reaction to a promise, queueing it if the promise already settled.
///
/// Queued rather than run, even when the promise settled long ago: the ordering
/// must not depend on whether the handler was early or late, which is the one
/// thing a program can observe about a promise that it was never told it
/// depended on.
pub(in crate::entry::promise) fn react(context: &mut Context, source: PromiseId, handler: Handler) {
    let machine = &mut context.promises;
    let waiter = machine.record(Reaction {
        source: Some(source),
        handler,
    });
    if let Attachment::ReadyNow(_) = machine.promises.attach(source, waiter) {
        machine.scheduler.queues().wake(waiter);
    }
    // Something is waiting on it now, so a rejection it carries is somebody's
    // problem. `Promise.reject(x).catch(f)` attaches after the rejection and is
    // not an unhandled rejection — which is the whole reason the report waits
    // for the end of the turn.
    machine.settlements.noticed(source);
}

/// A `TypeError` with a message, made without running a constructor.
///
/// The class is registered first so that the object inherits the same prototype
/// `new TypeError("x")` gives — a program that catches one and reads `.name`
/// must not be able to tell where it came from.
pub(in crate::entry::promise) fn type_error(context: &mut Context, message: &str) -> u64 {
    crate::entry::error::register_type_error(context);
    let Some(cell) = crate::entry::native::plain(context) else {
        return undefined_of(context);
    };
    if let Some(prototype) = crate::entry::class_support::prototype(context, "TypeError") {
        context.set_prototype(cell, prototype);
    }
    let text = context.intern_value(Str::from_str(message)).bits();
    let key = context.well_known("message");
    crate::entry::objects::put(context, cell, key, text);
    // The one thing assembling an error by hand loses, and a program reads it:
    // `typeof e.stack` answered `"undefined"` here and `"string"` for every
    // `TypeError` the constructor made — a difference a program can see and
    // nothing justifies.
    //
    // BOTH halves, because they are the two the constructor does and either
    // alone is silent: `Error.prototype`'s `stack` accessor is installed at
    // CONSTRUCTION rather than at registration (see `error`'s own note on why),
    // so a program that never wrote `new Error` has none for the deferred
    // frames to be read through.
    crate::entry::error::install_stack_accessor(context);
    context.defer_stack(cell, "TypeError");
    Value::from_slot(cell).bits()
}
