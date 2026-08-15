//! The `resolve` and `reject` a promise hands out, and how one gets back to the
//! promise it settles.
//!
//! # Why these are their own module
//!
//! [`super::state`] is the promise machine — the tables, and the operations over
//! them that run no user code. These two are the opposite: they are JavaScript
//! **values**, made to be handed to an executor, to a foreign thenable's `then`,
//! or to whoever called `Promise.withResolvers`. Nothing about them is
//! bookkeeping, and they came out when `state` passed the crate's 500-line
//! ceiling, which was the moment the difference stopped being a matter of taste.

use rts_cranelift::sched::{PromiseId, Settlement};

use crate::entry::Context;
use crate::entry::objects::undefined_of;
use crate::value::Value;

use super::state;

/// A settler — the `resolve` or `reject` an executor is handed.
///
/// # A native cannot close over anything, so this closes over the environment
///
/// `native::callable` gives every native the environment
/// `undefined`, because a native closes over nothing. This one has to: two
/// promises' `resolve` functions are the same code and must settle different
/// promises.
///
/// The environment slot is where it goes. It is passed to the callee by
/// `functions::call` untouched, it is not reachable from
/// JavaScript — `callables` is private and `callable_at` answers only inside
/// this crate — and it costs nothing.
///
/// The alternative was a property on the callable's own cell, which a callable
/// being an object makes possible. It was rejected because it is **observable**:
/// `Object.keys(resolve)` would show it, a program could overwrite it, and it
/// would mint a property key for a fact no program named.
///
/// What crosses is the promise's INDEX, not a `PromiseId`: the machine keeps
/// that field private so nothing outside can invent one, and `Machine::at` is
/// the honest way back.
pub(super) fn settler(context: &mut Context, id: PromiseId, settlement: Settlement) -> u64 {
    let code = match settlement {
        Settlement::Fulfilled => resolve_native as crate::entry::native::Native,
        Settlement::Rejected => reject_native as crate::entry::native::Native,
    };
    let made = crate::entry::native::callable(context, code);
    if let Some(cell) = Value(made).as_slot() {
        context.mark_callable(cell, code as usize as u64, id.index() as u64);
    }
    made
}

/// `resolve(value)` as an executor receives it.
extern "C" fn resolve_native(
    environment: u64,
    _this: u64,
    a0: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    with(environment, |context, id| {
        state::resolve(context, id, a0)
    })
}

/// `reject(reason)` as an executor receives it.
extern "C" fn reject_native(
    environment: u64,
    _this: u64,
    a0: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    with(environment, |context, id| state::reject(context, id, a0))
}

/// The body both settlers share: find the promise, act, answer `undefined`.
fn with(environment: u64, body: impl FnOnce(&mut Context, PromiseId)) -> u64 {
    crate::entry::with_current(|context| {
        if let Some(id) = context.promises.at(environment as usize) {
            body(context, id);
        }
        undefined_of(context)
    })
}
