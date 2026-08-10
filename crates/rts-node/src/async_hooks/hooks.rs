//! `createHook` and the `AsyncHook` it answers, plus the dispatch [`super::resource`] fires.
//!
//! # What is honest about a hook that sees almost nothing
//!
//! Node's `init` fires for every async resource the process creates. Here it
//! fires for one: an explicit `new AsyncResource(...)`. The reason is in
//! [`super`]'s doc — the other call sites are in the timer and promise
//! machinery, which this crate does not own and cannot reach into.
//!
//! The alternative considered and rejected was refusing `createHook`
//! altogether. It loses because the four callbacks that CAN fire are exactly
//! the ones the reference doc's own test plan pins (one `init`, one `before`,
//! one `after`, one `destroy` across a construct/`runInAsyncScope`/
//! `emitDestroy` sequence), and a program that only instruments its own
//! `AsyncResource`s gets a correct answer. What it must not do is read
//! "nothing fired" as "nothing happened", which is why the coverage limit is
//! stated in the module doc rather than left to be discovered.
//!
//! # Why the registry holds only ENABLED hooks
//!
//! A boolean beside each hook would be a second place the enabled state lives —
//! the object is the first, since a program can hold it and call `.enable()`
//! again. Membership IS the state: `enable` inserts, `disable` removes, and
//! both are idempotent because both are set operations rather than toggles.

use rts_core::entry::Provided;
use std::cell::RefCell;

thread_local! {
    /// The enabled hooks, in enable order — per thread, because the resources
    /// they observe are per thread and a hook enabled on one thread has no
    /// callback frame to run on another.
    static ENABLED: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

/// The five callback fields a hook may carry, in Node's own spelling.
///
/// `promiseResolve` is copied onto the hook and never read — see [`super`]'s
/// refusal list. It is accepted rather than rejected so a program passing the
/// full set is not told its options object is malformed when the real answer is
/// that this engine has nowhere to fire it from.
const FIELDS: &[&str] = &["init", "before", "after", "destroy", "promiseResolve"];

/// What an `AsyncHook` can do.
const METHODS: &[(&str, Provided)] = &[("enable", enable), ("disable", disable)];

/// `async_hooks.createHook(options)` — answers a hook that is **not** enabled,
/// which is Node's contract.
///
/// A field that is present and not callable is dropped with a stderr line
/// rather than throwing the `TypeError` Node throws: nothing on this surface
/// raises a catchable exception. Dropping it silently was the other option and
/// it loses for the reason this whole module is written around — a tool would
/// then see a callback that never fires and read it as no activity.
pub(super) extern "C" fn create_hook(_e: u64, _this: u64, options: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let mut rejected: Vec<&str> = Vec::new();
    let hook = rts_core::entry::with_runtime(|context| {
        let prototype = rts_core::entry::make_prototype(context, "AsyncHook", METHODS);
        let hook = rts_core::entry::make_instance(context, prototype);
        let absent = rts_core::entry::undefined_in(context);
        for field in FIELDS {
            let given = rts_core::entry::get_member(context, options, field);
            if given == absent {
                continue;
            }
            match rts_core::entry::is_callable_in(context, given) {
                true => rts_core::entry::put_member(context, hook, field, given),
                false => rejected.push(field),
            }
        }
        hook
    });
    for field in rejected {
        eprintln!(
            "node:async_hooks: createHook ignored `{field}` — it is not a function (real Node throws a TypeError here; this engine cannot)"
        );
    }
    hook
}

/// `hook.enable()` — idempotent, and answers `this` for chaining.
extern "C" fn enable(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    ENABLED.with(|held| {
        let mut held = held.borrow_mut();
        if !held.contains(&this) {
            held.push(this);
        }
    });
    this
}

/// `hook.disable()` — idempotent, and answers `this` for chaining.
extern "C" fn disable(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    ENABLED.with(|held| held.borrow_mut().retain(|&hook| hook != this));
    this
}

/// The callbacks under one field name, across every enabled hook.
///
/// Collected inside ONE borrow and called outside it. Calling a hook callback
/// while the borrow is held would be a nested borrow the moment that callback
/// touched anything — an abort, not an exception, because an `extern "C"` frame
/// cannot unwind.
fn callbacks(field: &str) -> Vec<u64> {
    if ENABLED.with(|held| held.borrow().is_empty()) {
        return Vec::new();
    }
    rts_core::entry::with_runtime(|context| {
        let hooks = ENABLED.with(|held| held.borrow().clone());
        let mut found = Vec::new();
        for hook in hooks {
            let callback = rts_core::entry::get_member(context, hook, field);
            if rts_core::entry::is_callable_in(context, callback) {
                found.push(callback);
            }
        }
        found
    })
}

/// Fires `init(asyncId, type, triggerAsyncId, resource)`.
pub(super) fn fire_init(async_id: u64, type_label: &str, trigger: u64, resource: u64) {
    let found = callbacks("init");
    if found.is_empty() {
        return;
    }
    let (label, undefined) = rts_core::entry::with_runtime(|context| {
        (
            rts_core::entry::make_string(context, type_label),
            rts_core::entry::undefined_in(context),
        )
    });
    let id = rts_core::entry::make_number(async_id as f64);
    let trigger = rts_core::entry::make_number(trigger as f64);
    for callback in found {
        rts_core::entry::call(callback, undefined, id, label, trigger, resource);
    }
}

/// Fires one of the three callbacks whose only argument is an id.
pub(super) fn fire_id(field: &str, async_id: u64) {
    let found = callbacks(field);
    if found.is_empty() {
        return;
    }
    let undefined = rts_core::entry::undefined_value();
    let id = rts_core::entry::make_number(async_id as f64);
    for callback in found {
        rts_core::entry::call(callback, undefined, id, undefined, undefined, undefined);
    }
}
