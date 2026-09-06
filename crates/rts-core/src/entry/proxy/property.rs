//! The four traps a PROPERTY access reaches: `get`, `set`, `has` and
//! `deleteProperty`.
//!
//! They are together because they are the operations a compiled site performs
//! by name — `o.x`, `o.x = 1`, `"x" in o`, `delete o.x` — and because each of
//! them ends in the same question: what does the target itself already say about
//! this key? [`super::invariant`] is where that question is asked once for all
//! four.

use super::{absent, invariant, property_of, spelled, trap_for};
use crate::entry::{functions, objects, primitives, throw, with_current};
use crate::object::Key;
use crate::value::Value;

/// `handler.get(target, prop, receiver)`, or the target's own answer.
pub(in crate::entry) fn get(object: u64, key: Key) -> Option<u64> {
    let trap = trap_for(object, "get")?;
    if trap.refused {
        return Some(absent());
    }
    let Some(callee) = trap.callee else {
        return Some(forwarded_read(trap.target, key));
    };
    let property = property_of(key);
    let answered = functions::call(
        callee,
        trap.handler,
        trap.target,
        property,
        object,
        absent(),
    );
    if throw::in_flight() {
        return Some(answered);
    }
    // A property the target froze in place reads what the target holds,
    // whatever the handler says: `writable: false` plus `configurable: false`
    // is the language promising a program that the value cannot change, and a
    // proxy is not allowed to be where that promise breaks.
    if let Some(own) = invariant::own_state(trap.target, key)
        && !own.configurable
    {
        match own.value {
            Some(held) if !own.writable && !primitives::same_value(answered, held) => {
                throw::type_error(&format!(
                    "'get' on proxy: property '{}' is a read-only and non-configurable data \
                     property on the proxy target but the proxy did not return its actual value",
                    spelled(key)
                ));
            }
            None if !own.getter && answered != absent() => {
                throw::type_error(&format!(
                    "'get' on proxy: property '{}' is a non-configurable accessor property on \
                     the proxy target and does not have a getter function",
                    spelled(key)
                ));
            }
            _ => {}
        }
    }
    Some(answered)
}

/// `handler.set(target, prop, value, receiver)`, or a write to the target.
///
/// Answers the value, because an assignment is an expression — the trap's own
/// answer is a success flag the specification only reads in strict mode, and
/// reporting it as the assignment's value would make `p.x = 1` evaluate to
/// `true`. [`set_verdict`] is where that flag survives, for the one caller that
/// reports it.
pub(in crate::entry) fn set(object: u64, key: Key, value: u64) -> Option<u64> {
    set_verdict(object, key, value)?;
    Some(value)
}

/// The same write, answering whether it was accepted.
///
/// Beside [`set`] rather than replacing it, and the pair is one function with
/// two readings rather than two implementations: `o.x = 1` is an expression
/// whose value is `1` however the store went, and `Reflect.set(o, "x", 1)` is a
/// question about whether it went. Writing the second as its own trap call
/// would run the handler twice for one operation.
///
/// # The refusal this deliberately does not raise
///
/// A `set` trap answering falsy is a `TypeError` in strict code, and every
/// module here is strict. It is not raised because a compiled STORE does not
/// ask whether a throw is in flight — only calls do — so the error would
/// surface at whatever call came next, naming a line that had nothing to do
/// with it. That is worse than the missing throw: a wrong location is a
/// diagnostic that costs time rather than saving it. It comes back when the
/// emitter checks after a store.
pub(in crate::entry) fn set_verdict(object: u64, key: Key, value: u64) -> Option<bool> {
    let trap = trap_for(object, "set")?;
    if trap.refused {
        return Some(false);
    }
    let Some(callee) = trap.callee else {
        // A target that is itself a proxy gets its own traps, for the reason
        // `forwarded_read` states.
        if let Some(answered) = set_verdict(trap.target, key, value) {
            return Some(answered);
        }
        return Some(with_current(|context| {
            let Some(cell) = Value(trap.target).as_slot() else {
                return false;
            };
            // Asked BEFORE the write, because `put` refuses silently and
            // afterwards there is nothing to read back that distinguishes a
            // refusal from a store of the same value.
            let lands = crate::entry::reflect::write_lands(context, cell, key);
            objects::put(context, cell, key, value);
            lands
        }));
    };
    let property = property_of(key);
    let answered = functions::call(callee, trap.handler, trap.target, property, value, object);
    if throw::in_flight() {
        return Some(false);
    }
    let answered = primitives::to_boolean(answered);
    if answered
        && let Some(own) = invariant::own_state(trap.target, key)
        && !own.configurable
        && !own.writable
        && own
            .value
            .is_some_and(|held| !primitives::same_value(value, held))
    {
        throw::type_error(&format!(
            "'set' on proxy: trap returned truthy for property '{}' which exists in the proxy \
             target as a non-configurable and non-writable data property with a different value",
            spelled(key)
        ));
        return Some(false);
    }
    Some(answered)
}

/// `handler.has(target, prop)`, or whether the target has it.
pub(in crate::entry) fn has(object: u64, key: Key) -> Option<bool> {
    let trap = trap_for(object, "has")?;
    if trap.refused {
        return Some(false);
    }
    let Some(callee) = trap.callee else {
        return Some(match has(trap.target, key) {
            Some(answered) => answered,
            None => with_current(|context| {
                Value(trap.target)
                    .as_slot()
                    .and_then(|cell| objects::read_property(context, cell, key))
                    .is_some()
            }),
        });
    };
    let property = property_of(key);
    let answered = functions::call(
        callee,
        trap.handler,
        trap.target,
        property,
        absent(),
        absent(),
    );
    if throw::in_flight() {
        return Some(false);
    }
    let answered = primitives::to_boolean(answered);
    // Hiding is the lie this one can tell: a property the target cannot lose —
    // because it is non-configurable, or because the target refuses to shrink —
    // is one `in` has already reported, so a handler denying it now would make
    // the same question answer twice.
    if !answered
        && let Some(own) = invariant::own_state(trap.target, key)
        && (!own.configurable || !invariant::extensible(trap.target))
    {
        throw::type_error(&format!(
            "'has' on proxy: trap returned falsish for property '{}' which exists in the proxy \
             target as a non-configurable property",
            spelled(key)
        ));
    }
    Some(answered)
}

/// `handler.deleteProperty(target, prop)`, or a delete on the target.
pub(in crate::entry) fn delete(object: u64, key: Key) -> Option<bool> {
    let trap = trap_for(object, "deleteProperty")?;
    if trap.refused {
        return Some(false);
    }
    let Some(callee) = trap.callee else {
        // Forwarded through the entry point rather than reimplemented: deleting
        // rebuilds the shape without the key, and a second spelling of that is
        // a second answer to where a property lives.
        let property = property_of(key);
        return Some(crate::entry::computed::delete_property(
            trap.target,
            property,
        ));
    };
    let property = property_of(key);
    let answered = functions::call(
        callee,
        trap.handler,
        trap.target,
        property,
        absent(),
        absent(),
    );
    if throw::in_flight() {
        return Some(false);
    }
    if !primitives::to_boolean(answered) {
        return Some(false);
    }
    // A trap may refuse a delete the target would have allowed. What it may not
    // do is REPORT one the target refuses: `delete o.x` answering true while
    // `o.x` is still there is the one outcome no program can recover from.
    if let Some(own) = invariant::own_state(trap.target, key)
        && !own.configurable
    {
        throw::type_error(&format!(
            "'deleteProperty' on proxy: trap returned truthy for property '{}' which is \
             non-configurable in the proxy target",
            spelled(key)
        ));
        return Some(false);
    }
    Some(true)
}

/// Reads through to the target, for a handler that does not trap the read.
///
/// The target may itself be a proxy — `new Proxy(new Proxy(x, inner), {})` is
/// what a wrapper around a wrapper is — so this asks the traps again before
/// reading a shape. Without it the innermost handler was skipped and the read
/// answered `undefined`, which is what a chain of three proxies reported.
fn forwarded_read(target: u64, key: Key) -> u64 {
    if let Some(answered) = get(target, key) {
        return answered;
    }
    let found = with_current(|context| {
        let cell = Value(target).as_slot()?;
        // An INDEX first, and only from the element vector, because an array's
        // elements are not shape properties and `read_property` therefore cannot
        // see them. Without this, `new Proxy([10, 20], {})[0]` answered
        // `undefined` where node answers 10 — and only for a handler with no
        // `get` trap, since a handler that HAS one reaches the target through
        // `Reflect.get` and never arrives here. `p.length` worked throughout,
        // which is what made the shape of the defect hard to see: `length` IS a
        // property and the indices are not.
        //
        // `visible` and not the raw word: a hole reads as `undefined` rather
        // than as whatever the vector holds for one, exactly as an ordinary
        // indexed read of the target would answer.
        //
        // The key arrives as `Key::Name` even for `p[0]`, which is the part that
        // is easy to get wrong: `computed::property_key` interns every string
        // and never mints a `Key::Index`, so matching on that variant matches
        // nothing. The text is asked back and `as_array_index` decides, which is
        // the same idiom `accessor::resolve` and `object_global::arrays` use and
        // the same CANONICAL rule — `p["01"]` and `p["1.0"]` stay ordinary
        // properties, exactly as they are on the target itself.
        if let Key::Name(named) = key
            && context.elements_at(cell).is_some()
            && let Some(index) = context
                .interner
                .text(named)
                .and_then(crate::object::as_array_index)
            && let Some(held) = context
                .elements_at(cell)
                .and_then(|elements| elements.get(index as usize))
                .copied()
        {
            return Some(super::super::array::visible(context, held));
        }
        objects::read_property(context, cell, key).map(|value| value.bits())
    });
    match found {
        Some(value) => value,
        None => absent(),
    }
}
