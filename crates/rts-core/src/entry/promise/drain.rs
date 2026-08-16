//! The turn: run every microtask, then report the rejections nobody looked at.
//!
//! # Why the host calls this and compiled code does not
//!
//! A turn is a host concept. Compiled code returns when the script's last
//! statement has run, and *then* the queued reactions run — so the loop belongs
//! where the script was entered, not inside anything the script can call. The
//! alternative, draining at the end of each entry point that settled something,
//! would run a reaction inside the program that scheduled it, which is exactly
//! the synchrony a promise is defined not to have.
//!
//! # Why the report is after the drain and not at settle time
//!
//! `const p = Promise.reject(1); p.catch(f);` attaches a handler *after* the
//! rejection. Reporting when a promise rejects flags it; reporting when the turn
//! ends does not. Getting this wrong produces a warning for correct code, which
//! teaches people to ignore the warning — the rule
//! [`crate::schedule::Settlements`] is built around, and this is its one caller.

use rts_cranelift::sched::Settlement;

use super::react::{Handler, Step, prepare};
use super::state;
use super::thenable;
use crate::entry::{Context, functions, with_current};
use crate::value::Value;

/// Runs every queued reaction, then reports unhandled rejections.
///
/// Called by the host after the compiled entry point returns. Runs to empty,
/// including reactions queued *by* reactions, which is what makes a chain of
/// settled promises resolve completely — the machine's ordering rule, unchanged
/// and not restated here.
pub fn drain_microtasks() {
    // Each half of the loop takes its own borrow and gives it back, because
    // `perform` calls user code and a borrow held across that call aborts the
    // process rather than failing anything.
    while let Some(waiter) = with_current(|context| context.promises.next()) {
        let Some(step) = with_current(|context| prepare(context, waiter)) else {
            continue;
        };
        perform(step);
    }
    report();
    // Where a finalizer runs, and the reason it runs HERE: this is the one
    // point every host in this repository already pumps, so neither of them had
    // to be taught anything. `super::super::finalize` explains why the sweep
    // cannot call them itself.
    super::super::finalize::drain();
}

/// Does what a prepared step says, with no borrow held while user code runs.
fn perform(step: Step) {
    let absent = super::undefined();
    match step {
        Step::Call {
            callee,
            argument,
            derived,
        } => {
            let produced = functions::call(callee, absent, argument, absent, absent, absent);
            // A handler that THREW rejects the derived promise with what it
            // threw. Without this it was RESOLVED with `undefined` — the value
            // `call` answers when it did not run — so `.catch()` after a
            // throwing `.then()` never fired and a failed chain reported
            // success. That is the specification inverted, and it is one of the
            // reasons the runtime was not allowed to raise until these checks
            // existed.
            if let Some(thrown) = super::super::throw::caught() {
                with_current(|context| state::reject(context, derived, thrown));
                return;
            }
            // Through `resolve` rather than a fulfilment: a handler answering a
            // promise makes the derived one adopt it, which is what
            // `p.then(() => q)` means and the reason a `then` chain flattens.
            with_current(|context| state::resolve(context, derived, produced));
        }
        Step::Pass {
            derived,
            settlement,
            value,
        } => with_current(|context| settle_through(context, derived, settlement, value)),
        // The one step that runs a compiled BODY rather than a callable. It
        // takes its own borrows and settles its own promise, which is why
        // nothing is done with an answer here: an async function's completion
        // is a settlement, not a value handed back to a caller.
        Step::Resume {
            frame,
            result,
            settlement,
            value,
        } => super::async_fn::resumed(frame, result, settlement, value),
        Step::Finally {
            callback,
            derived,
            settlement,
            value,
        } => {
            let produced = functions::call(callback, absent, absent, absent, absent, absent);
            // A `finally` that throws REPLACES the settlement it was passing
            // through, which is what the language says a `finally` does to the
            // value it was unwinding.
            if let Some(thrown) = super::super::throw::caught() {
                with_current(|context| state::reject(context, derived, thrown));
                return;
            }
            // And a `finally` that ANSWERS a promise or a thenable WAITS for it,
            // which is the half this used to drop: the answer was discarded and
            // the settlement handed on at once, so cleanup that returned a
            // promise ran concurrently with whatever came after `.finally`
            // rather than before it.
            //
            // `thenable::waited_on` is what decides, and it lives beside the one
            // definition of what a thenable IS rather than here — see it for
            // why, and for the measurement that says a callback answering an
            // ordinary value must cost NO extra microtask.
            with_current(|context| match thenable::waited_on(context, produced) {
                Some(inner) => state::react(
                    context,
                    inner,
                    Handler::Restore {
                        derived,
                        settlement,
                        value,
                    },
                ),
                // Nothing to wait for: the ORIGINAL settlement, not the
                // callback's answer. That is the whole difference between
                // `finally` and `then(f, f)`.
                None => settle_through(context, derived, settlement, value),
            });
        }
        Step::Adopt {
            thenable,
            then_fn,
            resolve_fn,
            reject_fn,
            promise,
        } => {
            // `Get(thenable, "then")`, which may be a GETTER — user code, and
            // the reason this read is here rather than where the step was
            // decided. `then_fn` is already in hand for the ordinary data
            // property, so nothing pays for the case it does not have.
            //
            // Through `objects::get_property` rather than a walk written here:
            // it is the crate's one property read, so a `then` behind a proxy
            // trap or an inherited getter answers the same way it does
            // everywhere else in the program.
            let then_fn = match then_fn {
                Some(found) => found,
                None => match then_key() {
                    Some(key) => super::super::objects::get_property(thenable, key),
                    None => absent,
                },
            };
            // The getter threw. The language rejects the promise being resolved
            // with what it threw, and this is the drain — the one native that
            // HANDLES rather than propagates, per rule 8 of the crate's README.
            if let Some(thrown) = super::super::throw::caught() {
                with_current(|context| state::reject(context, promise, thrown));
                return;
            }
            // A getter that answered something uncallable leaves an ordinary
            // object, which fulfils with itself. Decided here and not at queue
            // time because that is where the value first exists.
            if !with_current(|context| callable(context, then_fn)) {
                with_current(|context| {
                    state::settle(context, promise, Settlement::Fulfilled, thenable)
                });
                return;
            }
            functions::call(then_fn, thenable, resolve_fn, reject_fn, absent, absent);
            // `then` itself throwing rejects the promise too — unless it had
            // already settled it before throwing, which `PromiseTable::settle`
            // drops on its own rather than being guarded against here.
            if let Some(thrown) = super::super::throw::caught() {
                with_current(|context| state::reject(context, promise, thrown));
            }
        }
        Step::Collect {
            promise,
            settlement,
            values,
        } => {
            let array = super::array_of(values);
            with_current(|context| state::settle(context, promise, settlement, array));
        }
        Step::Finish {
            promise,
            settlement,
            value,
        } => with_current(|context| settle_through(context, promise, settlement, value)),
        Step::Aggregate { promise, reasons } => {
            let errors = super::array_of(reasons);
            with_current(|context| {
                let reason = super::group::aggregate(context, errors);
                state::reject(context, promise, reason);
            });
        }
    }
}

/// The key number `then` interns to, as an entry point spells a key.
///
/// `None` only for an index, which a name is not — so the answer is `Some` for
/// this name and the `Option` is the shared conversion's, not a case here.
fn then_key() -> Option<i64> {
    with_current(|context| {
        let key = context.well_known("then");
        crate::entry::objects::machine_key(key).map(|key| key.index() as i64)
    })
}

/// Whether a value can be called, which is what makes a `then` a `then`.
fn callable(context: &Context, value: u64) -> bool {
    Value(value)
        .as_slot()
        .and_then(|cell| context.callable_at(cell))
        .is_some()
}

/// Settles a promise the way another one settled.
///
/// The fulfilled side goes through `resolve`, so that a promise passed along a
/// chain is adopted rather than becoming the value of the next one — `Promise
/// .resolve(Promise.resolve(1)).then(v => v)` sees `1`, not a promise.
fn settle_through(context: &mut Context, promise: rts_cranelift::sched::PromiseId, settlement: Settlement, value: u64) {
    match settlement {
        Settlement::Fulfilled => state::resolve(context, promise, value),
        Settlement::Rejected => state::reject(context, promise, value),
    }
}

/// Every rejection nothing was waiting for when the turn ended.
fn report() {
    let unhandled = with_current(|context| context.promises.unhandled());
    for reason in unhandled {
        let described = with_current(|context| describe(context, reason));
        eprintln!("rts: unhandled promise rejection: {described}");
    }
}

/// Whatever text a rejection reason has **without running user code**.
///
/// The shape `entry::throw` settles on, and for its reason: `ToPrimitive`
/// on an object calls a `toString` an entry point cannot call, while `name` and
/// `message` are ordinary data properties — so `Promise.reject(new Error("boom"))`
/// reports `Error: boom` from two reads rather than from a call.
fn describe(context: &mut Context, reason: u64) -> String {
    let value = Value(reason);
    if let Some(text) =
        crate::entry::text::to_text(context, value).and_then(|text| text.to_rust())
    {
        return text;
    }
    value
        .as_slot()
        .and_then(|cell| crate::entry::error::joined(context, cell))
        .unwrap_or_else(|| "an object".to_owned())
}
