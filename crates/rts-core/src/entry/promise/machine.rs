//! The three operations compiled code performs on a promise.
//!
//! # Reuse-check
//!
//! `rts-cranelift` already owns everything about what a promise IS —
//! `sched::PromiseTable`, `PromiseId`, `Settlement`, the wait sets and the
//! queues — and this module owns none of it, exactly as [`super`] says. What was
//! missing is not a mechanism but a *reachable symbol*: `RtEntry::PromiseNew`,
//! `PromiseSettle` and `PromiseAwait` are emitted by the machine's own lowering
//! of `Inst::PromiseNew` / `PromiseSettle` / `Await`, and
//! `rts-host`'s `machine_entry` answered a null pointer for all three. A
//! compiled `await` therefore called address zero.
//!
//! So this is the runtime half of a contract whose other three quarters already
//! existed. Nothing here re-derives a promise; every operation goes through
//! [`super::state`], which is what `Promise` itself goes through.
//!
//! # What `await` does here, and the divergence that is not one
//!
//! It drains. `rts-cranelift`'s own signature doc for `PromiseAwait` states the
//! contract this layer is meant to present: *"A frame that parks resumes here,
//! so from the caller's side this looks like a call that took a long time —
//! which is exactly what it should look like."* That is the design of the
//! boundary, not an approximation invented here.
//!
//! What IS a divergence, and it is a real one: a true `await` yields to the
//! caller, so statements after an async call run BEFORE the awaited value
//! arrives. Here the awaiting frame keeps the machine, so they run after. The
//! values a program computes are the same; the interleaving is not.
//!
//! Removing that needs `Inst::Suspend` lowered — `lower/body.rs` refuses it with
//! `Capability::Suspension` — over the frame transformation
//! `rts_cranelift::frame::transform` already implements and nothing yet calls.
//! That is the correct shape and it is named here rather than implied, because a
//! blocking `await` that nobody wrote down is how an engine acquires a
//! permanent one.
//!
//! # Not handled here, by name
//!
//! A THENABLE — a plain object carrying a callable `then` — is not recognised.
//! `await` over one should adopt it, and doing so means calling user code from
//! an entry point, which is the borrow rule this crate aborts on. It needs the
//! reaction machinery rather than a table lookup, and it is written down because
//! a program that awaits a thenable gets the OBJECT rather than what it settles
//! with, which is a wrong answer that runs.
//!
//! # Why a stalled `await` reports instead of hanging
//!
//! A promise nothing can settle is reachable: `await new Promise(() => {})`. The
//! choices were to spin forever, to answer `undefined`, or to say so. A hang
//! cannot be diagnosed from inside the program and `undefined` is a wrong answer
//! that runs, so this reports.

use rts_cranelift::sched::Settlement;

use super::super::Context;
use super::super::with_current;
use super::{register_promise, state};
use crate::value::Value;

/// `RtEntry::PromiseNew` — a fresh pending promise.
///
/// Registers the class first, for the reason [`super::settled`] gives: a program
/// that never writes the word `Promise` has triggered no registration, and a
/// promise built without one comes back with no prototype and therefore no
/// `.then` — so every reaction attached to it would be silently unreachable.
#[rtse::entry("rts_promise_new")]
pub fn promise_new() -> u64 {
    with_current(|context| match make_pending(context) {
        Some(value) => value,
        None => super::undefined_of(context),
    })
}

fn make_pending(context: &mut Context) -> Option<u64> {
    register_promise(context);
    let (cell, _) = state::fresh(context)?;
    Some(Value::from_slot(cell).bits())
}

/// `RtEntry::PromiseSettle` — settles one, making its waiters runnable.
///
/// `rejected` is an `i64` and not a `bool` because that is what the lowering
/// passes: it emits `iconst I64`. A Rust `bool` is one byte, and reading a word
/// where a byte was written takes the callee's leftover bits — the mistake that
/// once made `===` answer true for two different strings in release and false in
/// debug.
///
/// Fulfilment goes through `resolve` rather than `settle` for the reason
/// [`super::settled`] states: resolving with another promise ADOPTS it, and a
/// value that came from somewhere else must not turn a promise into a promise
/// for a promise. Rejection has no adoption to do.
#[rtse::entry("rts_promise_settle")]
pub fn promise_settle(promise: u64, value: u64, rejected: i64) {
    with_current(|context| {
        let Some(id) = id_of(context, promise) else {
            return;
        };
        match rejected != 0 {
            true => state::settle(context, id, Settlement::Rejected, value),
            false => state::resolve(context, id, value),
        }
    });
}

/// `RtEntry::PromiseAwait` — the value a promise settled with.
///
/// # Why the borrow is taken and released around every step
///
/// Draining runs user code — a `.then` reaction is a JavaScript function — and a
/// borrow held across that call aborts the process rather than failing. So each
/// half of the loop takes its own borrow and gives it back, which is the same
/// shape [`super::drain_microtasks`] is written in and for the same reason.
#[rtse::entry("rts_promise_await")]
pub fn promise_await(promise: u64) -> u64 {
    // `await 7` is `7`. Awaiting a value that is not a promise resolves to it
    // immediately, which is what the language says and what a program relying on
    // `await` over a maybe-promise depends on. Without this the lookup found no
    // promise and fell through to the stall path, so every `await` of a plain
    // value reported a deadlock that was not one.
    //
    // A thenable — an object with a callable `then` — is NOT handled here and is
    // named in the module doc: recognising one means calling user code from an
    // entry point, which is the borrow rule, and it needs the reaction machinery
    // rather than this lookup.
    // Said BEFORE the loop, and this is the part `notice` inside the loop cannot
    // do: from here on something is waiting on this promise, so a rejection that
    // arrives while the frame is parked is somebody's problem. The poll that
    // reads it comes one drain too late to say so — `try { await p } catch`
    // printed "unhandled promise rejection" for a rejection it then caught,
    // whenever the rejection came from a timer or a socket rather than being
    // there already.
    let parked = with_current(|context| {
        let id = id_of(context, promise)?;
        context.promises.awaiting(id);
        Some(id)
    });
    let Some(parked) = parked else {
        return promise;
    };
    let finish = |value: u64| {
        with_current(|context| context.promises.awaited(parked));
        value
    };
    loop {
        if let Some((settlement, value)) =
            with_current(|context| outcome_of_and_notice(context, promise))
        {
            return finish(match settlement {
                Settlement::Fulfilled => value,
                // A rejection crossing an `await` is a throw, which is what the
                // language says. It used to be uncatchable — `throw` ended the
                // program and a `try` around a call was refused — and it is not
                // any more: a throw leaves one frame and every call site asks,
                // so `try { await p } catch` reaches this one like any other.
                Settlement::Rejected => {
                    super::super::throw(0, value);
                    value
                }
            });
        }
        // Reactions first: the commonest promise is one another reaction
        // settles, and that costs nothing but a queue walk.
        super::drain_microtasks();
        if with_current(|context| outcome_of(context, promise).is_some()) {
            continue;
        }
        // Then the world: a promise settled by a timer or a socket needs the
        // loop to turn, and `pump_sources` is what turns it. Its answer is how
        // long the caller may wait — and this crate deliberately cannot sleep,
        // because its membership rule is availability and `std::thread::sleep`
        // is not on every target. So the waiting is the host's, handed down the
        // same way the evaluator is.
        match super::super::loops::pump_sources() {
            // Nothing OUTSTANDING — which is not the same as nothing having
            // happened. Pumping DELIVERS: a source fires the callbacks that came
            // due while it was being asked, so the very turn that empties the
            // timer table is the turn that ran the `resolve` this `await` is
            // waiting for. Reading the empty answer as "no future event can
            // settle it" reported that as a deadlock.
            //
            // That is exactly what `await new Promise(r => setTimeout(r, 5))`
            // does — the standard way to wait — and it ended the program with
            // "this promise cannot settle" while the promise had, in fact, just
            // settled. So the answer is checked before it is believed, the same
            // way it already is after `drain_microtasks` above; the callback may
            // also have queued a reaction, so that is drained first.
            None => {
                super::drain_microtasks();
                if with_current(|context| outcome_of(context, promise).is_some()) {
                    continue;
                }
                // Now it is a real deadlock: nothing is outstanding, nothing was
                // delivered, and nothing is queued. `await new Promise(() => {})`
                // is the shape that reaches this.
                stall(promise);
                return finish(super::super::undefined_value());
            }
            Some(wait) => match super::super::loops::rest_for(wait) {
                true => continue,
                // A host that installed no waiter cannot make time pass, so
                // looping would spin a core forever over a promise only time
                // can settle. Said out loud rather than spun on.
                false => {
                    stall(promise);
                    return finish(super::super::undefined_value());
                }
            },
        }
    }
}

/// Reports an `await` that can never finish.
fn stall(promise: u64) {
    let message = with_current(|context| {
        super::super::make_string(
            context,
            "await: this promise cannot settle — nothing is left to run that could settle it",
        )
    });
    super::super::throw(0, message);
    let _ = promise;
}

/// The promise identifier a value stands for.
fn id_of(context: &Context, value: u64) -> Option<rts_cranelift::sched::PromiseId> {
    let cell = Value(value).as_slot()?;
    context.promises.id_of(cell)
}

/// What it settled with, if it has.
fn outcome_of(context: &Context, value: u64) -> Option<(Settlement, u64)> {
    let id = id_of(context, value)?;
    context.promises.outcome(id)
}

/// The same, and marks the settlement as looked at.
///
/// [`promise_await`]'s own poll — the branch that actually answers — is the
/// only caller: the ones that only ask "has it settled yet, so I know whether
/// to keep polling" must not mark it noticed on a `None`, and the immutable
/// `outcome_of` above stays the right tool for that.
fn outcome_of_and_notice(context: &mut Context, value: u64) -> Option<(Settlement, u64)> {
    let id = id_of(context, value)?;
    let outcome = context.promises.outcome(id);
    if outcome.is_some() {
        context.promises.notice(id);
    }
    outcome
}
