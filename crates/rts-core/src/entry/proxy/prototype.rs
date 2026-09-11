//! The four traps about the object ITSELF rather than about a property of it:
//! what it inherits from, and whether it still accepts new properties.
//!
//! They are together because their invariants are the same sentence twice. A
//! target that refuses to grow has frozen both facts — nothing can add to it and
//! nothing can relink it — so a handler answering either one differently is
//! contradicting something the program can read off the target directly.

use super::invariant;
use crate::entry::{Context, chain, functions, integrity, objects, primitives, throw};
use crate::object::Key;
use crate::value::Value;

/// The nearest proxy ABOVE an object, when it stands between that object and a
/// key — the other half of "what does this inherit from", asked from below.
///
/// # Why the chain walk cannot answer this itself
///
/// `accessor::resolve` runs inside the context borrow and a proxy answers by
/// running user code, so the walk cannot call a handler where it meets one. It
/// has no way to report one either: a fourth `Found` would have to be decided
/// about by every caller of that walk, and most of them are asking about a key
/// they already know is on the receiver.
///
/// So the question is asked separately, and the answer is EXACT rather than
/// approximate: the walk stops as soon as an ordinary level owns the key, so a
/// proxy further out never steals a property nearer in. Without it, a miss on a
/// child whose prototype is a proxy answered `undefined` — the cell standing
/// for a proxy holds no properties of its own, so the walk carried straight
/// past it to `Object.prototype` and no trap was ever reached.
///
/// # What a program with no proxy in it pays
///
/// One comparison. [`Context::any_proxy`] is false until `new Proxy` has run at
/// all, and this answers before touching the chain.
pub(in crate::entry) fn above(context: &mut Context, start: u32, key: Key) -> Option<u64> {
    if !context.any_proxy() {
        return None;
    }
    let Key::Name(machine) = key else {
        return None;
    };
    let number = machine.index() as u32;
    let mut cell = start;
    for _ in 0..objects::CHAIN_LIMIT {
        // The receiver itself is never the answer: a read ON a proxy is
        // intercepted before any chain is walked, and answering it here would
        // ask one handler twice for one operation.
        if cell != start && context.proxy_at(cell).is_some() {
            return Some(Value::from_slot(cell).bits());
        }
        if context.accessor_at(cell, number).is_some()
            || objects::own_property(context, cell, key).is_some()
        {
            return None;
        }
        cell = objects::inherited_from(context, cell)?;
    }
    None
}

/// `handler.getPrototypeOf(target)`, or the target's prototype.
pub(in crate::entry) fn prototype_of(object: u64) -> Option<u64> {
    let trap = super::trap_for(object, "getPrototypeOf")?;
    if trap.refused {
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
    // The result is an OBJECT or `null`, and nothing else. A handler answering
    // `1`, `"p"` or `undefined` was passed straight through, so a prototype
    // chain walk then started from a number — and `Object.getPrototypeOf(p)`
    // answered something no prototype can be. The specification refuses it
    // before any invariant is checked, which is also the order that matters:
    // the extensibility comparison below reads the answer as a prototype.
    let usable = crate::entry::with_current(|context| {
        crate::entry::objects::is_object(context, answered)
            || answered == Value::from_singleton(context.singletons.null).bits()
    });
    if !usable {
        throw::type_error(
            "'getPrototypeOf' on proxy: trap returned neither object nor null",
        );
        return Some(super::absent());
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
    if trap.refused {
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
    if trap.refused {
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
    if trap.refused {
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
