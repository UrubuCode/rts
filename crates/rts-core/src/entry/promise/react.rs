//! What a waiter identifier stands for, and what running it turns into.
//!
//! # Why the decision is a value
//!
//! Every reaction ends in a call to user code, and user code reaches the runtime
//! — so it must run with no borrow held, or the process aborts. Splitting that
//! into "decide" and "do" is what makes it safe, and making the decision a
//! **type** is what stops the two halves drifting back together: [`Step`] can be
//! computed inside a borrow and cannot be performed inside one, because
//! performing it needs entry points that take their own.
//!
//! The alternative was a comment on each reaction asking for care. This crate
//! has one of those per callback-taking method already, and they are correct;
//! here there are five paths into user code and one of them is reached only when
//! a program resolves a promise with a foreign thenable, which is the path a
//! reader would never test by hand.

use rts_cranelift::sched::{ContinuationId, PromiseId, Settlement};

use crate::entry::Context;
use crate::value::Value;

use super::settler;

/// Something waiting on a promise that is not a parked frame.
#[derive(Clone, Copy)]
pub(super) struct Reaction {
    /// The promise whose settlement runs it.
    ///
    /// `None` for the one job that waits on nothing: calling a foreign
    /// thenable's `then`, which is queued so that it happens in a microtask
    /// rather than inside the `resolve` that discovered it.
    pub(super) source: Option<PromiseId>,
    /// What running it does.
    pub(super) handler: Handler,
}

/// The four shapes a reaction has.
#[derive(Clone, Copy)]
pub(super) enum Handler {
    /// `.then(onFulfilled, onRejected)`, and everything spelled with it.
    ///
    /// Either handler may be absent, which is not an omission: `p.catch(f)` is
    /// `p.then(undefined, f)`, and a settlement with no handler for it passes
    /// through to the derived promise unchanged. That pass-through is also what
    /// makes adoption work with no separate mechanism.
    Js {
        /// Called with the value, when the source fulfils.
        on_fulfilled: u64,
        /// Called with the reason, when the source rejects.
        on_rejected: u64,
        /// The promise `.then` answered.
        derived: PromiseId,
    },
    /// `.finally(f)` — run either way, and pass the settlement on untouched.
    ///
    /// Its own shape rather than a `Js` with two natives, because those natives
    /// would have to close over the value they are passing through, and a native
    /// closes over one word — which [`super::settler::settler`] has already spent on
    /// which promise it settles.
    Finally {
        /// Called with no arguments, whichever way the source settled.
        callback: u64,
        /// The promise `.finally` answered.
        derived: PromiseId,
    },
    /// One element of a combinator.
    Member {
        /// Which combinator.
        group: usize,
        /// Which position in it, which is where the value goes.
        index: usize,
    },
    /// A foreign thenable's `then`, called once with a settler pair.
    Thenable {
        /// The object, which is the receiver.
        thenable: u64,
        /// Its `then`, already known to be callable — or `None` when it sits
        /// behind a getter and the drain has to read it first.
        ///
        /// `Option` rather than a fifth variant: the two differ only in whether
        /// one step has already happened, and the drain reads `then` in the same
        /// place either way. A variant would put the same three fields in two
        /// places and make `root_words` list them twice.
        then_fn: Option<u64>,
        /// The promise the settlers settle.
        promise: PromiseId,
    },
    /// A parked async frame, waiting for what it awaited.
    ///
    /// The variant [`super`] was documented as lacking: everything else here is
    /// a callable, and this is the one waiter that is a FRAME. It shares the
    /// identifier space and the queue with them, which is what makes "a `.then`
    /// and an `await` on one promise run in the order they attached" a fact
    /// rather than a coincidence.
    Frame {
        /// The frame owner whose body is parked — the cell [`super::async_fn`]
        /// made, which is the same object a `function*` call answers.
        frame: u32,
        /// The promise the async call handed its caller, settled when the body
        /// finishes or throws.
        result: PromiseId,
    },

    /// What `finally` does after its callback answered something to wait for.
    ///
    /// The callback ran, the value it produced was resolved into a promise of
    /// its own, and this waits on that: a fulfilment restores the settlement
    /// `finally` was passing through, and a rejection REPLACES it. That is the
    /// specification's own spelling — `PromiseResolve(C, result).then(() =>
    /// value)` — and it is the half the previous implementation dropped, which
    /// made `finally(() => thenable)` carry on without waiting.
    Restore {
        /// The promise `.finally` answered.
        derived: PromiseId,
        /// Which way the ORIGINAL source settled.
        settlement: Settlement,
        /// What it settled with.
        value: u64,
    },
}

/// What running a reaction does, decided under a borrow and performed without
/// one.
pub(super) enum Step {
    /// Call user code with the settled value, then resolve `derived` with what
    /// it answered.
    Call {
        /// The handler.
        callee: u64,
        /// The value or the reason, as the source settled.
        argument: u64,
        /// The promise that takes the result.
        derived: PromiseId,
    },
    /// No handler for this settlement: the derived promise takes it unchanged.
    Pass {
        /// The promise that takes it.
        derived: PromiseId,
        /// Which way.
        settlement: Settlement,
        /// With what.
        value: u64,
    },
    /// Call the callback `.finally` was given, then pass the settlement on.
    Finally {
        /// The callback, called with no arguments.
        callback: u64,
        /// The promise that takes the original settlement.
        derived: PromiseId,
        /// Which way the source settled.
        settlement: Settlement,
        /// With what.
        value: u64,
    },
    /// Call a foreign thenable's `then(resolve, reject)`.
    Adopt {
        /// The receiver.
        thenable: u64,
        /// Its `then`, or `None` when it still has to be read — which may run a
        /// getter, and therefore cannot happen where this step was decided.
        then_fn: Option<u64>,
        /// The settler for the fulfilled side.
        resolve_fn: u64,
        /// The settler for the rejected side.
        reject_fn: u64,
        /// The promise being resolved, which is what a `then` that throws — or
        /// a `then` that turns out not to be callable — has to settle.
        promise: PromiseId,
    },
    /// Re-enter a parked async frame with what it awaited.
    ///
    /// A step and not a call, because what runs is a compiled body rather than
    /// a callable — but it is in the same list for the same reason: it runs
    /// user code, so it cannot happen where it was decided.
    Resume {
        /// The frame owner.
        frame: u32,
        /// The promise the async call answered.
        result: PromiseId,
        /// Which way what it awaited settled — a rejection re-enters the body
        /// raising at the `await`.
        settlement: Settlement,
        /// The value or the reason.
        value: u64,
    },

    /// A combinator finished, and its answer is a list that becomes an array.
    Collect {
        /// The promise it answers.
        promise: PromiseId,
        /// Which way it settles.
        settlement: Settlement,
        /// The elements, in the order the input had them.
        values: Vec<u64>,
    },
    /// A combinator finished with a single value.
    Finish {
        /// The promise it answers.
        promise: PromiseId,
        /// Which way it settles.
        settlement: Settlement,
        /// With what.
        value: u64,
    },
    /// `Promise.any` ran out of promises: every one of them rejected.
    Aggregate {
        /// The promise it rejects.
        promise: PromiseId,
        /// Every reason, in input order.
        reasons: Vec<u64>,
    },
}

/// What running this waiter does, or `None` when there is nothing to do.
///
/// `None` covers a reaction already taken — which cannot happen, because a
/// waiter is queued once and taken when it runs — and a source that is somehow
/// not settled. Answering rather than asserting: this is reached from an
/// `extern "C"` frame, where a panic is an abort rather than a failed test.
pub(super) fn prepare(context: &mut Context, waiter: ContinuationId) -> Option<Step> {
    let reaction = context.promises.taken(waiter)?;
    match reaction.handler {
        Handler::Thenable {
            thenable,
            then_fn,
            promise,
        } => {
            let resolve_fn = settler::settler(context, promise, Settlement::Fulfilled);
            let reject_fn = settler::settler(context, promise, Settlement::Rejected);
            Some(Step::Adopt {
                thenable,
                then_fn,
                resolve_fn,
                reject_fn,
                promise,
            })
        }
        Handler::Js {
            on_fulfilled,
            on_rejected,
            derived,
        } => {
            let (settlement, value) = settled(context, reaction.source)?;
            let handler = match settlement {
                Settlement::Fulfilled => on_fulfilled,
                Settlement::Rejected => on_rejected,
            };
            match callable(context, handler) {
                true => Some(Step::Call {
                    callee: handler,
                    argument: value,
                    derived,
                }),
                false => Some(Step::Pass {
                    derived,
                    settlement,
                    value,
                }),
            }
        }
        Handler::Finally { callback, derived } => {
            let (settlement, value) = settled(context, reaction.source)?;
            match callable(context, callback) {
                true => Some(Step::Finally {
                    callback,
                    derived,
                    settlement,
                    value,
                }),
                // `p.finally(3)` is `p.then(3, 3)`, which passes through — the
                // language ignores a non-callable rather than throwing.
                false => Some(Step::Pass {
                    derived,
                    settlement,
                    value,
                }),
            }
        }
        Handler::Frame { frame, result } => {
            let (settlement, value) = settled(context, reaction.source)?;
            Some(Step::Resume {
                frame,
                result,
                settlement,
                value,
            })
        }
        Handler::Member { group, index } => {
            let (settlement, value) = settled(context, reaction.source)?;
            super::group::advance(context, group, index, settlement, value)
        }
        Handler::Restore {
            derived,
            settlement,
            value,
        } => {
            let (inner, reason) = settled(context, reaction.source)?;
            Some(match inner {
                // What the callback answered has settled, so `finally` hands on
                // the settlement it was carrying — untouched, which is the whole
                // difference between `finally` and `then(f, f)`.
                Settlement::Fulfilled => Step::Pass {
                    derived,
                    settlement,
                    value,
                },
                // Except when it REJECTED, which replaces the original outcome.
                // A `finally` whose cleanup fails reports the cleanup's failure,
                // not the one it was passing along.
                Settlement::Rejected => Step::Pass {
                    derived,
                    settlement: Settlement::Rejected,
                    value: reason,
                },
            })
        }
    }
}

/// How the promise a reaction waited on settled.
fn settled(context: &Context, source: Option<PromiseId>) -> Option<(Settlement, u64)> {
    context.promises.outcome(source?)
}

/// Whether a value can be called.
///
/// A handler that is not callable is ignored rather than refused, which is what
/// `p.then(null)` means: the settlement passes through to the derived promise.
fn callable(context: &Context, value: u64) -> bool {
    Value(value)
        .as_slot()
        .and_then(|cell| context.callable_at(cell))
        .is_some()
}
