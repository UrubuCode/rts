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
    get_on(object, key, object)
}

/// The same read, with the receiver said rather than assumed.
///
/// `[[Get]]` carries a receiver distinct from the object holding the property in
/// two shapes this engine could not express: `Reflect.get(p, k, other)`, and a
/// proxy reached as a PROTOTYPE — where `child.x` must call the trap with
/// `child`, not with the proxy. Passing the proxy in both cases made a handler
/// that forwards with `Reflect.get(t, k, r)` run an inherited getter on the
/// wrong `this`, which is the one thing the third argument exists to decide.
pub(in crate::entry) fn get_on(object: u64, key: Key, receiver: u64) -> Option<u64> {
    // A PRIVATE class member is not a property, so no trap can forward it and
    // no target can answer it: a proxy has no private slots however faithfully
    // it wraps something that does. Here a private name IS a key — see
    // `symbol::is_private_key` for why — so the read reached the target and
    // answered its field, where every runtime raises a brand failure. Asked
    // before the handler is consulted, which is also the order: the
    // specification never reaches `[[Get]]` for a private name at all.
    if super::is_proxy(object)
        && let Some(spelled) = private_name(key)
    {
        throw::type_error(&format!(
            "Cannot read private member {spelled} from an object whose class did not declare it"
        ));
        return Some(absent());
    }
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
        receiver,
        absent(),
    );
    if throw::in_flight() {
        return Some(answered);
    }
    // A property the target froze in place reads what the target holds,
    // whatever the handler says: `writable: false` plus `configurable: false`
    // is the language promising a program that the value cannot change, and a
    // proxy is not allowed to be where that promise breaks.
    // §10.5.8 steps 9–10, and `own_state` may be a trap of its own when the
    // target is a proxy — so it is asked whether it threw before its answer is.
    let own = invariant::own_state(trap.target, key);
    if throw::in_flight() {
        return Some(absent());
    }
    if let Some(own) = own
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
            None if own.get.is_none() && answered != absent() => {
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
/// The write, answering whether the handler ACCEPTED it.
///
/// One function rather than the pair that used to be here — a `set` answering
/// the value beside a `set_verdict` answering the flag. `o.x = 1` is an
/// expression whose value is `1` however the store went, so the value spelling
/// carried no information its caller did not already have; and the callers that
/// dropped the flag are exactly the ones that had to act on it.
///
/// # The refusal, which is raised now
///
/// A `set` trap answering falsy is a `TypeError` in strict code, and every
/// module here is strict. The note that stood here said it could not be raised,
/// because "a compiled STORE does not ask whether a throw is in flight — only
/// calls do". That has not been true for some time: `SetProperty` keeps its
/// throw check, `runtime::raising::CANNOT_RAISE` does not list it, and a test
/// beside that list asserts it never will. So the error surfaces at the store
/// that caused it, which is what the note was protecting against.
///
/// Raised by the CALLER rather than here, because only the caller knows the
/// mode — `Reflect.set` answers `false` for the same verdict and must not
/// raise at all.
pub(in crate::entry) fn set_verdict(object: u64, key: Key, value: u64) -> Option<bool> {
    set_verdict_on(object, key, value, object)
}

/// The same write, with the receiver said rather than assumed — see
/// [`get_on`] for why the fourth argument of `[[Set]]` is not decoration.
///
/// The forwarding case is where it does the most work: `target.[[Set]](k, v,
/// proxy)` is an `OrdinarySet` whose receiver is the PROXY, so a data store
/// lands through the proxy's own `getOwnPropertyDescriptor` and
/// `defineProperty` traps rather than on the target. That is what a handler
/// logging its trap names observes, and writing the target directly — which is
/// what this did — made both traps silent.
pub(in crate::entry) fn set_verdict_on(
    object: u64,
    key: Key,
    value: u64,
    receiver: u64,
) -> Option<bool> {
    let trap = trap_for(object, "set")?;
    if trap.refused {
        return Some(false);
    }
    let Some(callee) = trap.callee else {
        // A target that is itself a proxy gets its own traps, for the reason
        // `forwarded_read` states — and the receiver travels with the forward.
        if let Some(answered) = set_verdict_on(trap.target, key, value, receiver) {
            return Some(answered);
        }
        if receiver != trap.target {
            return Some(crate::entry::ordinary::store_on(
                trap.target,
                key,
                value,
                receiver,
            ));
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
    let answered = functions::call(callee, trap.handler, trap.target, property, value, receiver);
    if throw::in_flight() {
        return Some(false);
    }
    let answered = primitives::to_boolean(answered);
    if !answered {
        return Some(false);
    }
    // §10.5.9 steps 9–10: only a non-configurable property of the target
    // constrains a store the trap reported as done.
    let own = invariant::own_state(trap.target, key);
    if throw::in_flight() {
        return Some(false);
    }
    let Some(own) = own.filter(|own| !own.configurable) else {
        return Some(true);
    };
    let refusal = match own.value {
        // 10.a: a frozen value may only be "written" with itself.
        Some(held) if !own.writable && !primitives::same_value(value, held) => Some(
            "exists in the proxy target as a non-configurable and non-writable data property \
             with a different value",
        ),
        // 10.b: an accessor with no setter accepts no store at all — the
        // handler reporting one is claiming an effect that cannot have happened.
        None if own.set.is_none() => Some(
            "exists in the proxy target as a non-configurable accessor property without a setter",
        ),
        _ => None,
    };
    if let Some(reason) = refusal {
        throw::type_error(&format!(
            "'set' on proxy: trap returned truthy for property '{}' which {reason}",
            spelled(key)
        ));
        return Some(false);
    }
    Some(true)
}

/// `handler.has(target, prop)`, or whether the target has it.
pub(in crate::entry) fn has(object: u64, key: Key) -> Option<bool> {
    let trap = trap_for(object, "has")?;
    if trap.refused {
        return Some(false);
    }
    let Some(callee) = trap.callee else {
        // Forwarded through the entry point, as `delete` below is: the target's
        // own `[[HasProperty]]`, a nested proxy and an ARRAY ELEMENT included.
        // It walked `objects::read_property`, which knows no element store, so
        // `0 in new Proxy([1], {})` was false and `Array.prototype.slice.call`
        // over such a proxy — which asks `HasProperty` per index — answered
        // nothing but holes.
        let property = property_of(key);
        return Some(crate::entry::computed::has_property(property, trap.target));
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
    //
    // §10.5.7 step 9, in its order: the descriptor, then its configurability,
    // and only then `IsExtensible` — each of which may be a trap of a proxy
    // target, so each is followed by the rule-8 question.
    if answered {
        return Some(true);
    }
    let own = invariant::own_state(trap.target, key);
    if throw::in_flight() {
        return Some(false);
    }
    let Some(own) = own else {
        return Some(false);
    };
    if !own.configurable {
        throw::type_error(&format!(
            "'has' on proxy: trap returned falsish for property '{}' which exists in the proxy \
             target as a non-configurable property",
            spelled(key)
        ));
        return Some(false);
    }
    let open = invariant::extensible(trap.target);
    if !open && !throw::in_flight() {
        throw::type_error(&format!(
            "'has' on proxy: trap returned falsish for property '{}' but the proxy target is \
             not extensible",
            spelled(key)
        ));
    }
    Some(false)
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
    //
    // §10.5.10 steps 10–13: absent in the target is fine, non-configurable is
    // not, and neither is a property of a target that refuses to shrink — its
    // key set is as fixed as the property would have been.
    let own = invariant::own_state(trap.target, key);
    if throw::in_flight() {
        return Some(false);
    }
    let Some(own) = own else {
        return Some(true);
    };
    if !own.configurable {
        throw::type_error(&format!(
            "'deleteProperty' on proxy: trap returned truthy for property '{}' which is \
             non-configurable in the proxy target",
            spelled(key)
        ));
        return Some(false);
    }
    let open = invariant::extensible(trap.target);
    if throw::in_flight() {
        return Some(false);
    }
    if !open {
        throw::type_error(&format!(
            "'deleteProperty' on proxy: trap returned truthy for property '{}' but the proxy \
             target is non-extensible",
            spelled(key)
        ));
        return Some(false);
    }
    Some(true)
}

/// The `#name` a key spells, when the key is a private class member's.
///
/// The `@@` is this crate's encoding and must not reach a program's `catch`;
/// what a reader wrote is the `#name` underneath it — the same unwrapping
/// `objects::looked_up` performs for the ordinary brand failure, and the message
/// is the same one for the same reason.
fn private_name(key: Key) -> Option<String> {
    with_current(|context| {
        let Key::Name(named) = key else {
            return None;
        };
        let text = context.interner.text(named)?;
        if !crate::entry::symbol::is_private_key(text) {
            return None;
        }
        let spelled = text.to_rust()?;
        Some(spelled.strip_prefix("@@").unwrap_or(&spelled).to_owned())
    })
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
