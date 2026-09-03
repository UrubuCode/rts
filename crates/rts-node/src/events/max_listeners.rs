//! The listener-count ceiling: `getMaxListeners`/`setMaxListeners`, instance
//! and static, plus the mutable `EventEmitter.defaultMaxListeners` every
//! instance falls back to.

use rts_core::entry;
use std::sync::atomic::{AtomicU64, Ordering};

/// `EventEmitter.defaultMaxListeners` — read by [`get_max_listeners`] when an
/// instance never called `.setMaxListeners()`, written by
/// [`static_set_max_listeners`] when called with no target.
static DEFAULT_MAX_LISTENERS: AtomicU64 = AtomicU64::new(10.0f64.to_bits());

/// `events.getMaxListeners(emitter)` — the module-level spelling of the
/// instance query. EventTarget is a separate ambient surface and remains
/// outside this module; EventEmitter-shaped objects use the shared query.
pub(super) extern "C" fn static_get_max_listeners(_e: u64, _this: u64, emitter: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    get_max_listeners(0, emitter, 0, 0, 0, 0)
}

/// `emitter.getMaxListeners()` — the explicit `setMaxListeners()` value if
/// one was set, else [`DEFAULT_MAX_LISTENERS`].
pub(super) extern "C" fn get_max_listeners(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let stored = entry::with_runtime(|context| entry::get_member(context, this, "__maxListeners__"));
    if stored == absent {
        entry::make_number(f64::from_bits(DEFAULT_MAX_LISTENERS.load(Ordering::Relaxed)))
    } else {
        stored
    }
}

/// `emitter.setMaxListeners(n)` — records the validated limit. Warning
/// emission is still outside this module's current process-hook surface.
pub(super) extern "C" fn set_max_listeners(_e: u64, this: u64, n: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(number) = valid_max_listeners(n) else {
        return entry::undefined_value();
    };
    let value = entry::make_number(number);
    entry::with_runtime(|context| {
        entry::put_member(context, this, "__maxListeners__", value);
    });
    this
}

/// Getter for the mutable module-level `defaultMaxListeners` property.
pub(super) extern "C" fn default_get(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::make_number(f64::from_bits(DEFAULT_MAX_LISTENERS.load(Ordering::Relaxed)))
}

/// Setter for `events.defaultMaxListeners`, with the same non-negative numeric
/// boundary used by Node's listener-limit API.
pub(super) extern "C" fn default_set(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(number) = entry::number_of(value) else {
        entry::invalid_arg_type("defaultMaxListeners", "number", value);
        return entry::undefined_value();
    };
    if number.is_nan() || number < 0.0 {
        entry::out_of_range("defaultMaxListeners", ">= 0", value);
        return entry::undefined_value();
    }
    DEFAULT_MAX_LISTENERS.store(number.to_bits(), Ordering::Relaxed);
    entry::undefined_value()
}

/// Validate the numeric listener limit once for both instance and static
/// setters. `Infinity` is a valid Node value; only NaN and negative numbers
/// are rejected.
fn valid_max_listeners(value: u64) -> Option<f64> {
    let Some(number) = entry::number_of(value) else {
        entry::invalid_arg_type("n", "number", value);
        return None;
    };
    if number.is_nan() || number < 0.0 {
        entry::out_of_range("n", ">= 0", value);
        return None;
    }
    Some(number)
}

/// `events.setMaxListeners(n, target?)` — with no target, changes
/// [`DEFAULT_MAX_LISTENERS`]; the native ABI exposes three target slots, so the
/// implementation accepts the corresponding variadic prefix and validates all
/// targets before writing any of them.
pub(super) extern "C" fn static_set_max_listeners(_e: u64, _this: u64, n: u64, target: u64, target_b: u64, target_c: u64) -> u64 {
    let absent = entry::undefined_value();
    let Some(number) = valid_max_listeners(n) else {
        return absent;
    };
    let targets = [target, target_b, target_c];
    if targets.iter().all(|&one| one == absent) {
        DEFAULT_MAX_LISTENERS.store(number.to_bits(), Ordering::Relaxed);
        return absent;
    }
    let mut valid_targets = Vec::new();
    for one in targets {
        if one == absent {
            continue;
        }
        let is_object = entry::with_runtime(|context| entry::is_object(context, one));
        if !is_object {
            entry::invalid_arg_type("eventTargets", "EventEmitter or EventTarget", one);
            return absent;
        }
        valid_targets.push(one);
    }
    let value = entry::make_number(number);
    for one in valid_targets {
        set_max_listeners(0, one, value, 0, 0, 0);
    }
    absent
}
