//! The four traps about the object ITSELF rather than about a property of it:
//! what it inherits from, and whether it still accepts new properties.
//!
//! They are together because their invariants are the same sentence twice. A
//! target that refuses to grow has frozen both facts — nothing can add to it and
//! nothing can relink it — so a handler answering either one differently is
//! contradicting something the program can read off the target directly.

use super::invariant;
use crate::entry::{chain, functions, integrity, primitives, throw};
use crate::value::Value;

/// `handler.getPrototypeOf(target)`, or the target's prototype.
pub(in crate::entry) fn prototype_of(object: u64) -> Option<u64> {
    let trap = super::trap_for(object, "getPrototypeOf")?;
    if trap.revoked {
        return Some(super::absent());
    }
    let Some(callee) = trap.callee else {
        return Some(chain::get_prototype(trap.target));
    };
    let absent = super::absent();
    let answered = functions::call(callee, trap.handler, trap.target, absent, absent, absent);
    if throw::in_flight() {
        return Some(answered);
    }
    // An extensible target may be relinked, so a handler naming a different
    // prototype is only describing something that could still become true. A
    // target that refuses to grow cannot be relinked at all, which makes its
    // prototype a fact rather than a current state.
    if invariant::extensible(trap.target) {
        return Some(answered);
    }
    let actual = chain::get_prototype(trap.target);
    if !primitives::same_value(answered, actual) {
        throw::type_error(
            "'getPrototypeOf' on proxy: proxy target is non-extensible but the trap did not \
             return its actual prototype",
        );
        return Some(actual);
    }
    Some(answered)
}

/// Whether a `setPrototypeOf` was accepted.
///
/// One function rather than two, where there used to be a pair — one answering
/// the object for `Object.setPrototypeOf` and one answering the verdict for
/// `Reflect.setPrototypeOf`. The pair was two lookups and two forwards for one
/// operation, and the object half never carried information: its caller already
/// had the object it was about to answer. `chain::set_prototype` turns the
/// verdict back into the object, which is the one place that conversion belongs.
pub(in crate::entry) fn set_prototype_verdict(object: u64, prototype: u64) -> Option<bool> {
    let trap = super::trap_for(object, "setPrototypeOf")?;
    if trap.revoked {
        return Some(false);
    }
    let Some(callee) = trap.callee else {
        return Some(chain::apply_prototype(trap.target, prototype));
    };
    let absent = super::absent();
    let answered = functions::call(callee, trap.handler, trap.target, prototype, absent, absent);
    if throw::in_flight() {
        return Some(false);
    }
    let answered = primitives::to_boolean(answered);
    if answered
        && !invariant::extensible(trap.target)
        && !primitives::same_value(prototype, chain::get_prototype(trap.target))
    {
        throw::type_error(
            "'setPrototypeOf' on proxy: trap returned truthy for setting a new prototype on the \
             non-extensible proxy target",
        );
        return Some(false);
    }
    Some(answered)
}

/// `handler.isExtensible(target)`, or the target's own answer.
///
/// The one trap whose result is pinned exactly: the specification requires it to
/// equal the target's own extensibility, so a handler cannot use it to say
/// anything at all. It exists so that `preventExtensions` through a proxy is
/// observable, not so that extensibility can be faked.
pub(in crate::entry) fn extensible(object: u64) -> Option<bool> {
    let trap = super::trap_for(object, "isExtensible")?;
    if trap.revoked {
        return Some(false);
    }
    let actual = invariant::extensible(trap.target);
    let Some(callee) = trap.callee else {
        return Some(actual);
    };
    let absent = super::absent();
    let answered = functions::call(callee, trap.handler, trap.target, absent, absent, absent);
    if throw::in_flight() {
        return Some(false);
    }
    if primitives::to_boolean(answered) != actual {
        throw::type_error(
            "'isExtensible' on proxy: trap result does not reflect extensibility of proxy target",
        );
    }
    Some(actual)
}

/// `handler.preventExtensions(target)`, or closing the target itself.
pub(in crate::entry) fn prevent_extensions(object: u64) -> Option<bool> {
    let trap = super::trap_for(object, "preventExtensions")?;
    if trap.revoked {
        return Some(false);
    }
    let Some(callee) = trap.callee else {
        integrity::restrict(trap.target, integrity::Integrity::Closed);
        return Some(Value(trap.target).as_slot().is_some());
    };
    let absent = super::absent();
    let answered = functions::call(callee, trap.handler, trap.target, absent, absent, absent);
    if throw::in_flight() {
        return Some(false);
    }
    let answered = primitives::to_boolean(answered);
    // Reporting success without the target actually closing would leave
    // `Object.isExtensible(proxy)` answering false and the target still
    // growable, which is the two-answers-to-one-question this whole layer is
    // about.
    if answered && invariant::extensible(trap.target) {
        throw::type_error(
            "'preventExtensions' on proxy: trap returned truthy but the proxy target is \
             extensible",
        );
        return Some(false);
    }
    Some(answered)
}
