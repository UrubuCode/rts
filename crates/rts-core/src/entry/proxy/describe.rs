//! The two traps that speak in DESCRIPTORS: `getOwnPropertyDescriptor` and
//! `defineProperty`.
//!
//! Split out of [`super::keys`] when both grew the full invariant checks of
//! ES2025 §10.5.5 and §10.5.6, because the two are one question asked in two
//! directions — "what is this property?" and "make it this" — and they share
//! the one comparison that answers whether a descriptor is compatible with what
//! the target holds. That comparison is `object_global::compatible`, the check
//! `Object.defineProperty` itself makes; it is called here rather than
//! restated.

use super::invariant::{self, Own};
use crate::entry::object_global::{self, Descriptor};
use crate::entry::{functions, throw, with_current};
use crate::object::Key;

/// `handler.getOwnPropertyDescriptor(target, prop)`, or the target's own.
pub(in crate::entry) fn describe(object: u64, key: Key) -> Option<u64> {
    let trap = super::trap_for(object, "getOwnPropertyDescriptor")?;
    if trap.refused {
        return Some(super::absent());
    }
    let property = super::property_of(key);
    let Some(callee) = trap.callee else {
        // The target may be a proxy of its own — `new Proxy(new Proxy(x, inner),
        // {})` has to reach `inner` — so the traps are asked again before a
        // shape is read. Not through `describe_of`: that one starts by asking
        // whether its argument is a proxy, which this line has already answered.
        if let Some(answered) = describe(trap.target, key) {
            return Some(answered);
        }
        return Some(object_global::describe_own(trap.target, property));
    };
    let absent = super::absent();
    let answered = functions::call(callee, trap.handler, trap.target, property, absent, absent);
    if throw::in_flight() {
        return Some(absent);
    }
    let Some(read) = checked_descriptor(trap.target, key, answered) else {
        return Some(absent);
    };
    // `FromPropertyDescriptor(CompletePropertyDescriptor(…))`: the program gets
    // a FRESH, complete object — never the handler's own, which could be
    // missing `writable` or be watched for mutation by the handler.
    Some(object_global::descriptor_object(&read))
}

/// §10.5.5 steps 8–17: the refusals a descriptor has to survive.
///
/// Answers the descriptor as read — `None` for `undefined` and for every
/// refusal, which the caller tells apart by asking `throw::in_flight()`. Read
/// once and handed back, so a handler's descriptor with getters on it runs each
/// getter once, which is what `ToPropertyDescriptor` does.
fn checked_descriptor(target: u64, key: Key, answered: u64) -> Option<Descriptor> {
    // WHAT it is, before what it CLAIMS (step 8): a number answered here was
    // reported as a lie about configurability, a true sentence about the wrong
    // mistake that pointed the reader at the target instead of at the handler.
    let undefined = super::absent();
    let shaped = answered == undefined
        || with_current(|context| crate::entry::primitive::is_object_in(context, answered));
    if !shaped {
        throw::type_error(
            "'getOwnPropertyDescriptor' on proxy: trap must answer an object or undefined",
        );
        return None;
    }
    // Step 9 — a trap of its own when the target is a proxy.
    let held = invariant::own_state(target, key);
    if throw::in_flight() {
        return None;
    }
    if answered == undefined {
        // Step 10: hiding is allowed only for what the target could still lose.
        let Some(held) = held else {
            return None;
        };
        if !held.configurable {
            refuse(key, "which is non-configurable in the proxy target");
            return None;
        }
        let open = invariant::extensible(target);
        if !open && !throw::in_flight() {
            refuse(key, "which exists in the non-extensible proxy target");
        }
        return None;
    }
    // Steps 11–13.
    let open = invariant::extensible(target);
    if throw::in_flight() {
        return None;
    }
    let read = object_global::descriptor_read(answered)?;
    let complete = completed(&read, undefined);
    // Step 14: nothing a definition of the same descriptor would refuse.
    let existing = held.as_ref().map(Own::existing);
    if !object_global::compatible(open, &complete, existing.as_ref()) {
        throw::type_error(&format!(
            "'getOwnPropertyDescriptor' on proxy: trap returned descriptor for property '{}' \
             that is incompatible with the existing property in the proxy target",
            super::spelled(key)
        ));
        return None;
    }
    // Step 15: `configurable: false` is a promise, and only a property the
    // target itself pinned may make it — with the same `writable`.
    if complete.configurable == Some(false) {
        match &held {
            Some(held) if !held.configurable => {
                if complete.writable == Some(false) && held.writable {
                    refuse(
                        key,
                        "as non-configurable and non-writable while it is writable in the \
                         proxy target",
                    );
                    return None;
                }
            }
            _ => {
                refuse(
                    key,
                    "as non-configurable while it is either non-existent or configurable in the \
                     proxy target",
                );
                return None;
            }
        }
    }
    Some(read)
}

/// The `TypeError` of a `getOwnPropertyDescriptor` invariant, named by key.
fn refuse(key: Key, reason: &str) {
    throw::type_error(&format!(
        "'getOwnPropertyDescriptor' on proxy: trap reported property '{}' {reason}",
        super::spelled(key)
    ));
}

/// `CompletePropertyDescriptor` over a record: the defaults a comparison needs.
fn completed(read: &Descriptor, undefined: u64) -> Descriptor {
    let accessor = read.get.is_some() || read.set.is_some();
    Descriptor {
        value: (!accessor).then(|| read.value.unwrap_or(undefined)),
        writable: (!accessor).then(|| read.writable.unwrap_or(false)),
        get: accessor.then(|| read.get.unwrap_or(undefined)),
        set: accessor.then(|| read.set.unwrap_or(undefined)),
        enumerable: Some(read.enumerable.unwrap_or(false)),
        configurable: Some(read.configurable.unwrap_or(false)),
    }
}

/// `handler.defineProperty(target, prop, descriptor)`, or a define on the
/// target.
///
/// Answers whether it was accepted — `Reflect.defineProperty`'s answer; the
/// `Object` spelling turns `false` into its own `TypeError`.
///
/// # What the handler is handed
///
/// `FromPropertyDescriptor(ToPropertyDescriptor(desc))` (§10.5.6 step 7, with
/// the conversion done by the caller in the specification's text): a fresh
/// object with the PRESENT fields normalised, so `{enumerable: 1}` arrives as
/// `true` and a getter on the program's descriptor has already run once. The
/// program's own object used to be passed through, which a handler printing
/// `d.enumerable` saw as `1`.
///
/// # Why the forward REPORTS instead of raising
///
/// Without a trap the operation is `target.[[DefineOwnProperty]]`, which
/// answers a boolean. It was forwarded through `Object.defineProperty`'s raising
/// spelling, so an `OrdinarySet` through a proxy onto a non-extensible target —
/// which must answer `false` — ended the program with
/// `TypeError: Cannot redefine property`.
pub(in crate::entry) fn define(object: u64, key: Key, descriptor: u64) -> Option<bool> {
    if !super::is_proxy(object) {
        return None;
    }
    // Read BEFORE the trap is looked up, which is the specification's order:
    // `ToPropertyDescriptor` belongs to the caller of `[[DefineOwnProperty]]`.
    let Some(read) = object_global::descriptor_read(descriptor) else {
        return Some(false);
    };
    let normalised = object_global::descriptor_present(&read);
    let trap = super::trap_for(object, "defineProperty")?;
    if trap.refused {
        return Some(false);
    }
    let property = super::property_of(key);
    let Some(callee) = trap.callee else {
        if let Some(answered) = define(trap.target, key, normalised) {
            return Some(answered);
        }
        return Some(object_global::define_reported(
            trap.target,
            property,
            normalised,
        ));
    };
    let absent = super::absent();
    let answered = functions::call(callee, trap.handler, trap.target, property, normalised, absent);
    if throw::in_flight() {
        return Some(false);
    }
    if !crate::entry::primitives::to_boolean(answered) {
        return Some(false);
    }
    checked_definition(trap.target, key, &read);
    Some(!throw::in_flight())
}

/// §10.5.6 steps 12–17: what a handler reporting a definition as done may not
/// have claimed.
fn checked_definition(target: u64, key: Key, wanted: &Descriptor) {
    let held = invariant::own_state(target, key);
    if throw::in_flight() {
        return;
    }
    let open = invariant::extensible(target);
    if throw::in_flight() {
        return;
    }
    let pinning = wanted.configurable == Some(false);
    let refuse = |reason: &str| {
        throw::type_error(&format!(
            "'defineProperty' on proxy: trap returned truthy for defining property '{}' {reason}",
            super::spelled(key)
        ));
    };
    let Some(held) = held else {
        // Step 15: nothing may appear on a closed target, and nothing absent
        // from the target may be reported as pinned.
        if !open {
            refuse("which cannot be added to the non-extensible proxy target");
        } else if pinning {
            refuse("as non-configurable, which does not exist on the proxy target");
        }
        return;
    };
    // Step 16.
    if !object_global::compatible(open, wanted, Some(&held.existing())) {
        refuse("which is incompatible with the existing property in the proxy target");
        return;
    }
    if pinning && held.configurable {
        refuse("as non-configurable, which is configurable in the proxy target");
        return;
    }
    if held.value.is_some() && !held.configurable && held.writable && wanted.writable == Some(false)
    {
        refuse("as non-writable, which is writable and non-configurable in the proxy target");
    }
}

/// `HasOwnProperty(proxy, key)` — `Object.hasOwn` and `hasOwnProperty` on a
/// proxy: its `getOwnPropertyDescriptor` trap, and `undefined` means no.
///
/// Both read the cell's own storage, and a proxy cell has none, so both
/// answered `false` without the handler ever being asked — which a handler
/// logging `gopd:a` for `Object.hasOwn(p, "a")` shows. `None` for a value that
/// is not a proxy; a trap that threw answers `false` with the throw in flight.
pub(in crate::entry) fn owns(object: u64, name: u64) -> Option<bool> {
    if !super::is_proxy(object) {
        return None;
    }
    let described = object_global::describe_of(object, name);
    Some(!throw::in_flight() && described != super::absent())
}
