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

use super::react::{Step, prepare};
use super::state;
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
        Step::Finally {
            callback,
            derived,
            settlement,
            value,
        } => {
            functions::call(callback, absent, absent, absent, absent, absent);
            // The original settlement, not the callback's answer. That is the
            // whole difference between `finally` and `then(f, f)`.
            with_current(|context| settle_through(context, derived, settlement, value));
        }
        Step::Adopt {
            thenable,
            then_fn,
            resolve_fn,
            reject_fn,
        } => {
            functions::call(then_fn, thenable, resolve_fn, reject_fn, absent, absent);
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
