//! `channel.bindStore` / `unbindStore` / `runStores` — the
//! `AsyncLocalStorage` half of a `Channel`.
//!
//! # Why this is real now and was refused before
//!
//! It was refused because `AsyncLocalStorage` did not exist. It does:
//! [`crate::async_hooks`] builds one, with a working `run(store, callback)`.
//! Nothing here reimplements it — a second store stack beside that module's
//! would be two answers to "what is the current store", which is the
//! duplication `reuse-check` exists to stop. This calls the JS `run` method of
//! whatever store the program bound, so a program's own `AsyncLocalStorage`
//! subclass works for the same reason Node's does.
//!
//! # Why entering N stores needs a frame stack
//!
//! `store.run(value, callback)` takes a *callback*, and entering three stores
//! means three nested `run`s with the innermost finally calling the user's
//! `fn`. A closure per level is what JavaScript would use, and this crate
//! cannot mint one: `entry::make_callable` hands back a fixed code address
//! whose environment slot is `undefined` by construction.
//!
//! The alternative that works is the one a native always has — put the state
//! where the code can find it. [`FRAMES`] holds the plan; [`resume`] is one
//! callable that advances it. The nesting is strictly LIFO and strictly
//! synchronous (`run` returns before `runStores` does), which is exactly what
//! makes a stack the right shape rather than a map keyed by an identity a
//! native cannot read.
//!
//! # Not implemented, by name
//!
//! `runStores`' `...args` tail — the four argument slots are spent on
//! `context`, `fn` and `thisArg`, so `fn` is called with none. A store whose
//! `run` is not callable is **not recorded** by `bindStore` (and `unbindStore`
//! answers `false` for it): recording it would mean `runStores` silently
//! entering nothing under a name that says it entered something. Unwinding on
//! throw is not implemented either — a JS throw out of `fn` does not unwind
//! this Rust frame, so [`FRAMES`] is left holding a frame; see
//! [`crate::diagnostics_channel`]'s doc for the wall that causes it.

use super::{do_publish, elements_under, is_callable, member, set_elements_under};
use rts_core::entry;
use std::cell::RefCell;

/// One in-flight `runStores` call: the stores still to enter, the value each
/// enters with, and what to run once they all are.
struct Frame {
    /// The bound stores, in binding order.
    stores: Vec<u64>,
    /// What each store enters with — `transform(context)` or `context`.
    values: Vec<u64>,
    /// How many of them [`resume`] has already entered.
    entered: usize,
    /// The program's own function, run innermost.
    callback: u64,
    /// Its receiver.
    this_arg: u64,
}

thread_local! {
    /// The `runStores` calls currently on this thread's stack, outermost
    /// first. A thread-local rather than a global because a store is
    /// per-realm — the same reason `async_hooks`' own stack is one.
    static FRAMES: RefCell<Vec<Frame>> = const { RefCell::new(Vec::new()) };
}

/// `channel.bindStore(store, transform?)`.
///
/// Re-binding an already-bound store replaces its transform rather than
/// entering it twice, which is what Node's `Map`-keyed storage does.
pub(super) extern "C" fn bind_store(_e: u64, this: u64, store: u64, transform: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    if !is_callable(member(store, "run")) {
        return absent;
    }
    let mut stores = elements_under(this, "__stores__");
    let mut transforms = elements_under(this, "__transforms__");
    transforms.resize(stores.len(), absent);
    match stores.iter().position(|&held| entry::strict_equals(held, store)) {
        Some(at) => transforms[at] = transform,
        None => {
            stores.push(store);
            transforms.push(transform);
        }
    }
    set_elements_under(this, "__stores__", stores);
    set_elements_under(this, "__transforms__", transforms);
    absent
}

/// `channel.unbindStore(store)`.
pub(super) extern "C" fn unbind_store(_e: u64, this: u64, store: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let mut stores = elements_under(this, "__stores__");
    let mut transforms = elements_under(this, "__transforms__");
    transforms.resize(stores.len(), entry::undefined_value());
    let Some(at) = stores.iter().position(|&held| entry::strict_equals(held, store)) else {
        return entry::boolean_value(false);
    };
    stores.remove(at);
    transforms.remove(at);
    set_elements_under(this, "__stores__", stores);
    set_elements_under(this, "__transforms__", transforms);
    entry::boolean_value(true)
}

/// `channel.runStores(context, fn, thisArg?)` — publishes, enters every bound
/// store, runs `fn`, leaves them again.
pub(super) extern "C" fn run_stores(_e: u64, this: u64, context_val: u64, callback: u64, this_arg: u64, _d: u64) -> u64 {
    do_publish(this, context_val);
    let absent = entry::undefined_value();
    let stores = elements_under(this, "__stores__");
    let mut transforms = elements_under(this, "__transforms__");
    transforms.resize(stores.len(), absent);
    let values = transforms
        .iter()
        .map(|&transform| match is_callable(transform) {
            true => entry::call(transform, absent, context_val, absent, absent, absent),
            false => context_val,
        })
        .collect();
    FRAMES.with(|frames| {
        frames.borrow_mut().push(Frame {
            stores,
            values,
            entered: 0,
            callback,
            this_arg,
        })
    });
    let result = step();
    FRAMES.with(|frames| {
        frames.borrow_mut().pop();
    });
    result
}

/// What the innermost `store.run` calls, and what each level calls to reach
/// the next — one callable for every level, because what differs between two
/// levels is data rather than code.
extern "C" fn resume(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    step()
}

/// Enters the next store, or runs the program's function once none are left.
///
/// The borrow on [`FRAMES`] is taken and released BEFORE anything is called:
/// the call below re-enters this function through [`resume`], and a borrow
/// held across it would be a second `borrow_mut` in an `extern "C"` frame,
/// which cannot unwind and therefore aborts. The same rule that governs the
/// runtime's own borrow, for the same reason.
fn step() -> u64 {
    let next = FRAMES.with(|frames| {
        let mut frames = frames.borrow_mut();
        let frame = frames.last_mut()?;
        if frame.entered < frame.stores.len() {
            let at = frame.entered;
            frame.entered += 1;
            return Some((Some((frame.stores[at], frame.values[at])), frame.callback, frame.this_arg));
        }
        Some((None, frame.callback, frame.this_arg))
    });
    let absent = entry::undefined_value();
    let Some((enter, callback, this_arg)) = next else {
        return absent;
    };
    match enter {
        Some((store, value)) => {
            let run = member(store, "run");
            let inner = entry::with_runtime(|context| entry::make_callable(context, resume));
            entry::call(run, store, value, inner, absent, absent)
        }
        None => entry::call(callback, this_arg, absent, absent, absent, absent),
    }
}
