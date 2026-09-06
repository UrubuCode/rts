//! The traps that speak in KEYS and DESCRIPTORS.
//!
//! `ownKeys`, `getOwnPropertyDescriptor` and `defineProperty` are together
//! because they are one question at three grains — what the object has — and
//! because `Object.keys` over a proxy needs two of them at once: the list, and
//! then a descriptor per key to know which of the listed ones are enumerable.

use super::invariant;
use crate::entry::{objects, primitives, throw, with_current};
use crate::object::Key;
use crate::value::Value;

/// `handler.ownKeys(target)`, or the target's own keys.
///
/// What `Object.keys`, `for`-`in`, `JSON.stringify` and spread all reach. The
/// forwarding case is the target's own keys and not the proxy's, which have
/// never existed: a proxy cell holds no properties at all.
///
/// # The divergence in the forwarding case, named
///
/// It answers the target's ENUMERABLE own keys, where `[[OwnPropertyKeys]]`
/// answers every one. `Reflect.ownKeys(new Proxy(o, {}))` therefore misses a
/// property defined with `enumerable: false`. The reason is that
/// `array::own_keys` and `array::own_names` — the filtered and unfiltered
/// spellings — both reach this one function, so it has a single answer to give
/// two callers that want different ones, and the other choice is worse:
/// answering every key would put `length` into a spread of a proxied array.
/// The fix is one level up, where the two spellings already differ.
pub(in crate::entry) fn own_keys(object: u64) -> Option<u64> {
    let trap = super::trap_for(object, "ownKeys")?;
    if trap.refused {
        return Some(crate::entry::modules::make_array(Vec::new()));
    }
    let Some(callee) = trap.callee else {
        return Some(crate::entry::array::own_keys(trap.target));
    };
    let absent = super::absent();
    let listed =
        crate::entry::functions::call(callee, trap.handler, trap.target, absent, absent, absent);
    if throw::in_flight() {
        return Some(listed);
    }
    checked_keys(trap.target, listed);
    Some(listed)
}

/// The refusals a key list has to survive.
///
/// Three of them now. Two are facts a program can already have observed through
/// the target: a key it cannot lose must still be listed, and a target that
/// refuses to grow or shrink must be listed exactly.
///
/// The third is about the LIST, and it came first in the specification for a
/// reason that showed here: `CreateListFromArrayLike(trapResult, «String,
/// Symbol»)` refuses a result that is not a list at all, and refuses any entry
/// that is neither a string nor a symbol. Neither was checked, so a handler
/// answering `1` produced an empty walk and one answering `[1]` produced a key
/// spelled `"1"` — a property name invented by the engine out of a value the
/// program never named.
fn checked_keys(target: u64, listed: u64) {
    // A LIST, which is what `CreateListFromArrayLike` demands before it reads
    // anything. Answering nothing for a non-list is what made
    // `Reflect.ownKeys(new Proxy({}, { ownKeys: () => 1 }))` an empty array.
    let is_list = with_current(|context| {
        Value(listed)
            .as_slot()
            .is_some_and(|cell| context.elements_at(cell).is_some())
    });
    if !is_list {
        throw::type_error("'ownKeys' on proxy: trap result must be a list of property keys");
        return;
    }
    let entries = with_current(|context| {
        Value(listed)
            .as_slot()
            .and_then(|cell| context.elements_at(cell))
            .cloned()
            .unwrap_or_default()
    });
    // Every entry a String or a Symbol. `property_key` would happily turn `1`
    // into `"1"`, which is the conversion `CreateListFromArrayLike` exists to
    // refuse: a key list is not coerced, it is validated.
    for entry in &entries {
        let acceptable = with_current(|context| {
            crate::entry::symbol::is_symbol(context, *entry)
                || Value(*entry)
                    .as_slot()
                    .is_some_and(|cell| context.text_at(cell).is_some())
        });
        if !acceptable {
            throw::type_error(
                "'ownKeys' on proxy: trap result contains an entry that is neither a string \
                 nor a symbol",
            );
            return;
        }
    }
    let reported: Vec<Key> = with_current(|context| {
        entries
            .into_iter()
            .filter_map(|value| crate::entry::computed::property_key(context, Value(value)))
            .collect()
    });
    for (at, key) in reported.iter().enumerate() {
        if reported[..at].contains(key) {
            throw::type_error(&format!(
                "'ownKeys' on proxy: trap returned duplicate entries for property '{}'",
                super::spelled(*key)
            ));
            return;
        }
    }
    let Some(own) = invariant::own_keys_of(target) else {
        return;
    };
    let open = invariant::extensible(target);
    for (key, spelled) in &own {
        if reported.contains(key) {
            continue;
        }
        // A key the target cannot lose, or one it cannot lose because the
        // target itself refuses to shrink. Either way the program has already
        // been told the key is there and cannot be told otherwise now.
        let fixed =
            !open || invariant::own_state(target, *key).is_some_and(|state| !state.configurable);
        if fixed {
            throw::type_error(&format!(
                "'ownKeys' on proxy: trap result did not include '{spelled}'"
            ));
            return;
        }
    }
    if !open
        && let Some(extra) = reported
            .iter()
            .find(|key| !own.iter().any(|(at, _)| at == *key))
    {
        throw::type_error(&format!(
            "'ownKeys' on proxy: trap returned extra keys but proxy target is non-extensible: \
             '{}'",
            super::spelled(*extra)
        ));
    }
}

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
        // shape is read. The same forwarding `super::forwarded_read` does, and
        // written the same way rather than through `describe_of`: that one
        // starts by asking whether its argument is a proxy, which is the
        // question this line has already answered.
        if let Some(answered) = describe(trap.target, key) {
            return Some(answered);
        }
        return Some(crate::entry::object_global::describe_own(
            trap.target,
            property,
        ));
    };
    let absent = super::absent();
    let answered =
        crate::entry::functions::call(callee, trap.handler, trap.target, property, absent, absent);
    if throw::in_flight() {
        return Some(answered);
    }
    checked_descriptor(trap.target, key, answered);
    if throw::in_flight() {
        return Some(super::absent());
    }
    Some(completed(answered))
}

/// `FromPropertyDescriptor(CompletePropertyDescriptor(ToPropertyDescriptor(r)))`
/// — the round trip the specification performs on a trap's answer.
///
/// The handler's own object was handed to the program unchanged, and that is two
/// divergences at once. A descriptor the trap INVENTED came back incomplete:
/// `{ value: 1, configurable: true }` reached the caller with no `writable` and
/// no `enumerable` at all, so `d.writable` read `undefined` where every runtime
/// reads `false` — and `undefined` is falsy, so a program branching on it agreed
/// by accident and one printing it did not. The second is identity: the object a
/// program gets back is a fresh one, and a handler that keeps a reference to what
/// it returned cannot watch the caller mutate it.
///
/// `undefined` passes through, which is `FromPropertyDescriptor`'s own first
/// step and the answer for a key the handler does not claim.
fn completed(answered: u64) -> u64 {
    if answered == super::absent() {
        return answered;
    }
    let Some(read) = crate::entry::object_global::descriptor_read(answered) else {
        return super::absent();
    };
    crate::entry::object_global::descriptor_object(&read)
}

/// The refusals a descriptor has to survive.
fn checked_descriptor(target: u64, key: Key, answered: u64) {
    // WHAT it is, before what it CLAIMS. The specification refuses a trap
    // result that is neither an object nor `undefined` in its own step, ahead
    // of every invariant — and taking the invariants first reported a number as
    // "non-configurability for a property that is configurable in the target",
    // which is a true sentence about the wrong mistake and points the reader at
    // the target instead of at the handler.
    let shaped = answered == super::absent()
        || crate::entry::with_current(|context| {
            crate::entry::primitive::is_object_in(context, answered)
        });
    if !shaped {
        throw::type_error(
            "'getOwnPropertyDescriptor' on proxy: trap must answer an object or undefined",
        );
        return;
    }
    let held = invariant::own_state(target, key);
    if answered == super::absent() {
        let hidden = match &held {
            None => false,
            Some(state) => !state.configurable || !invariant::extensible(target),
        };
        if hidden {
            throw::type_error(&format!(
                "'getOwnPropertyDescriptor' on proxy: trap returned undefined for property '{}' \
                 which is non-configurable in the proxy target",
                super::spelled(key)
            ));
        }
        return;
    }
    // The other direction, and it is the one that matters more: a handler
    // INVENTING a non-configurable property makes every later operation on that
    // key checkable against a fact the target never recorded.
    // Read under the borrow, coerced outside it: `to_boolean` takes a borrow of
    // its own, and a second one inside an `extern "C"` frame aborts the process
    // rather than unwinding.
    let field = with_current(|context| {
        let named = context.well_known("configurable");
        Value(answered)
            .as_slot()
            .and_then(|cell| objects::read_property(context, cell, named))
            .map(|found| found.bits())
    });
    // A descriptor with no `configurable` field means false, which is what
    // `Object.defineProperty` already reads it as.
    let claimed = field.is_some_and(|found| primitives::to_boolean(found));
    // And the direction that was missing, which is the one a program uses to
    // TRUST a key: a property the target says can never be redefined must not be
    // reported as redefinable. `Object.getOwnPropertyDescriptor(p, "c")`
    // answered `configurable: true` for a `c` the target had pinned, so a caller
    // that checked the descriptor before redefining was told yes and then
    // refused by the target.
    if claimed && held.as_ref().is_some_and(|state| !state.configurable) {
        throw::type_error(&format!(
            "'getOwnPropertyDescriptor' on proxy: trap reported property '{}' as configurable \
             which is non-configurable in the proxy target",
            super::spelled(key)
        ));
        return;
    }
    if !claimed && held.is_none_or(|state| state.configurable) {
        throw::type_error(&format!(
            "'getOwnPropertyDescriptor' on proxy: trap reported non-configurability for property \
             '{}' which is either non-existent or configurable in the proxy target",
            super::spelled(key)
        ));
    }
}

/// `handler.defineProperty(target, prop, descriptor)`, or a define on the
/// target.
///
/// Answers whether it was accepted, which is what `Reflect.defineProperty`
/// reports and what a trap returning `false` means. The forwarding case answers
/// true whenever it reached an object, the same thing `Reflect.set` can
/// establish and for the same reason: a refusal would be a non-configurable
/// property, and that is recorded per object rather than per key.
pub(in crate::entry) fn define(object: u64, key: Key, descriptor: u64) -> Option<bool> {
    let trap = super::trap_for(object, "defineProperty")?;
    if trap.refused {
        return Some(false);
    }
    let property = super::property_of(key);
    let Some(callee) = trap.callee else {
        // A target that is itself a proxy gets its own traps first, the same
        // forwarding every absent trap here does.
        if let Some(answered) = define(trap.target, key, descriptor) {
            return Some(answered);
        }
        crate::entry::object_global::define(trap.target, property, descriptor);
        return Some(Value(trap.target).as_slot().is_some());
    };
    let answered = crate::entry::functions::call(
        callee,
        trap.handler,
        trap.target,
        property,
        descriptor,
        descriptor,
    );
    if throw::in_flight() {
        return Some(false);
    }
    Some(primitives::to_boolean(answered))
}

/// The keys `Object.keys` reports for a proxy: its own keys, filtered.
///
/// `Reflect.ownKeys` answers what the trap said and nothing else, and
/// `Object.keys` answers only the ENUMERABLE ones — so it asks
/// `[[GetOwnProperty]]` per key, which on a proxy is the
/// `getOwnPropertyDescriptor` trap or a forward to the target. A key the trap
/// invented that the target does not have has no descriptor, so it is not
/// enumerable, so it is dropped: `ownKeys: () => ["only"]` over `{a, b}` gives
/// `Object.keys` an empty list, which is what every other engine answers.
///
/// This is the one place the two spellings of enumeration legitimately differ,
/// and the difference is the filter rather than a second walk.
pub(in crate::entry) fn enumerable_keys(object: u64) -> Option<u64> {
    let listed = own_keys(object)?;
    if throw::in_flight() {
        return Some(listed);
    }
    let keys: Vec<u64> = with_current(|context| {
        Value(listed)
            .as_slot()
            .and_then(|cell| context.elements_at(cell))
            .cloned()
            .unwrap_or_default()
    });

    let mut kept = Vec::with_capacity(keys.len());
    for key in keys {
        // Through the public spelling, so a proxy whose target is a proxy is
        // asked again — the forwarding every absent trap does.
        let described = crate::entry::object_global::describe_of(object, key);
        // Rule 8: the descriptor came from a trap, so it may not have come at
        // all. Carrying on would filter by a field read off `undefined`.
        if throw::in_flight() {
            return Some(listed);
        }
        let enumerable = with_current(|context| {
            let named = context.well_known("enumerable");
            Value(described)
                .as_slot()
                .and_then(|cell| objects::read_property(context, cell, named))
                .map(|found| found.bits())
        });
        if let Some(enumerable) = enumerable
            && primitives::to_boolean(enumerable)
        {
            kept.push(key);
        }
    }
    Some(crate::entry::modules::make_array(kept))
}
