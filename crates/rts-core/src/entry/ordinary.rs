//! `OrdinarySet` with a RECEIVER that is not the object the property was found
//! on.
//!
//! # Why this is a module and not a line in the write path
//!
//! Because a substituted receiver turns one store into three operations on two
//! objects, and every one of them is observable. `Reflect.set(t, k, v, r)` and a
//! proxy's forwarded write both perform
//! `OrdinarySetWithOwnDescriptor(t, k, v, r, ownDesc)`, which — when the
//! property is a DATA property and `r` is not `t` — does not write `t` at all:
//! it asks `r` for its own descriptor of the key and then DEFINES the property
//! on `r`. When `r` is a proxy, those two steps are its
//! `getOwnPropertyDescriptor` and `defineProperty` traps, and a handler logging
//! its own trap names sees all three.
//!
//! The ordinary write path has none of that shape, and it must not grow it: a
//! receiver distinct from the target cannot arise from `o.x = 1`, so putting
//! this there would put a branch nothing ever takes in the hottest store in the
//! engine. The two callers that CAN say a receiver call this instead.
//!
//! # What is deliberately not here
//!
//! The recursion into the parent's `[[Set]]`. A target with no own property
//! delegates upward, and the walk this uses — `accessor::setter_for` — already
//! answers the accessor case over the whole chain, which is the half of that
//! recursion a program can observe. A parent that is itself a PROXY is
//! therefore not asked, and that is a stated gap rather than an oversight.

use super::{objects, primitives, throw, with_current};
use crate::object::Key;
use crate::value::Value;

/// `target.[[Set]](key, value, receiver)`, where `receiver` is not `target`.
///
/// Answers whether the store was accepted, which is what `Reflect.set` reports
/// and what a strict-mode assignment raises on.
pub(super) fn store_on(target: u64, key: Key, value: u64, receiver: u64) -> bool {
    // A proxy target answers with its handler's verdict, and the receiver is
    // passed through to the trap as the fourth argument it is entitled to.
    if let Some(answered) = super::proxy::set_verdict_on(target, key, value, receiver) {
        return answered;
    }
    // An accessor anywhere in the target's chain runs with the RECEIVER as
    // `this`, which is the whole reason a setter can be inherited at all.
    let setter = with_current(|context| {
        let cell = Value(target).as_slot()?;
        super::accessor::setter_for(context, cell, key)
    });
    if let Some(setter) = setter {
        let absent = with_current(|context| objects::undefined_of(context));
        super::functions::call(setter, receiver, value, absent, absent, absent);
        return !throw::in_flight();
    }
    // A DATA property — present or absent — is written on the receiver rather
    // than on the target, and through `[[GetOwnProperty]]`/`[[DefineOwnProperty]]`
    // so that a proxy receiver sees both of its traps.
    let Some(_) = Value(receiver).as_slot() else {
        return false;
    };
    let property = super::proxy::property_of(key);
    let existing = super::object_global::describe_of(receiver, property);
    // Rule 8: the descriptor may have come from a trap that threw.
    if throw::in_flight() {
        return false;
    }
    let held = with_current(|context| {
        let cell = Value(existing).as_slot()?;
        let read = |context: &mut super::Context, name: &str| {
            let named = context.well_known(name);
            objects::read_property(context, cell, named).map(|found| found.bits())
        };
        let getter = read(context, "get");
        let setter = read(context, "set");
        let writable = read(context, "writable");
        Some((getter, setter, writable))
    });
    if let Some((getter, setter, writable)) = held {
        // An accessor the receiver already has cannot be overwritten by a data
        // store, and neither can a property it declared read-only.
        let undefined = with_current(|context| objects::undefined_of(context));
        if getter.is_some_and(|found| found != undefined)
            || setter.is_some_and(|found| found != undefined)
        {
            return false;
        }
        if writable.is_some_and(|found| !primitives::to_boolean(found)) {
            return false;
        }
    }
    let descriptor = descriptor_for(value, held.is_none());
    super::reflect::defined(receiver, property, descriptor)
}

/// The descriptor a data store defines on the receiver.
///
/// `{ value }` alone when the receiver already had the property — the language
/// changes only the value and leaves the three attributes as they were — and
/// the full set when it did not, which is `CreateDataProperty`.
fn descriptor_for(value: u64, fresh: bool) -> u64 {
    let made = objects::object_new(0);
    with_current(|context| {
        let Some(cell) = Value(made).as_slot() else {
            return;
        };
        let key = context.well_known("value");
        objects::put(context, cell, key, value);
        if !fresh {
            return;
        }
        let yes = Value::from_bool(true).bits();
        for name in ["writable", "enumerable", "configurable"] {
            let key = context.well_known(name);
            objects::put(context, cell, key, yes);
        }
    });
    made
}
