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
/// # The forwarding case, and the divergence that used to be here
///
/// It answers EVERY own key of the target, strings and symbols, which is what
/// `[[OwnPropertyKeys]]` means. It used to answer the ENUMERABLE ones, on the
/// grounds that answering every key would put `length` into a spread of a
/// proxied array — and that reasoning was right about the symptom and wrong
/// about the level. `Object.keys` and a spread reach [`enumerable_keys`], which
/// filters by the descriptor per key and drops `length` because it is not
/// enumerable; the unfiltered spelling is what `Reflect.ownKeys` and
/// `getOwnPropertyNames` want.
///
/// What the filtered answer cost was worse than a missing hidden property: the
/// invariant below refuses a list that omits a key the target cannot lose, so
/// `JSON.stringify(new Proxy([1, 2], {}))` raised *"'ownKeys' on proxy: trap
/// result did not include 'length'"* — the engine refusing its own forward.
pub(in crate::entry) fn own_keys(object: u64) -> Option<u64> {
    let trap = super::trap_for(object, "ownKeys")?;
    if trap.refused {
        return Some(crate::entry::modules::make_array(Vec::new()));
    }
    let Some(callee) = trap.callee else {
        return Some(forwarded_keys(trap.target));
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

/// The target's own keys, in the order and the completeness
/// `[[OwnPropertyKeys]]` states: strings first, then symbols.
///
/// Two lists rather than one walk because that is how this crate already
/// answers the question — a symbol's key text is an internal encoding, so
/// `array::own_names` deliberately filters it out and
/// `object_global::own_symbols` is the one place that decodes it back. Writing
/// a third walk here would be a second answer to which of an object's keys are
/// symbols.
fn forwarded_keys(target: u64) -> u64 {
    let named = crate::entry::array::own_names(target);
    let symbols = crate::entry::object_global::own_symbols(target);
    let mut keys = with_current(|context| {
        Value(named)
            .as_slot()
            .and_then(|cell| context.elements_at(cell))
            .cloned()
            .unwrap_or_default()
    });
    let rest = with_current(|context| {
        Value(symbols)
            .as_slot()
            .and_then(|cell| context.elements_at(cell))
            .cloned()
            .unwrap_or_default()
    });
    keys.extend(rest);
    crate::entry::modules::make_array(keys)
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
    // §10.5.11 steps 11-23, in the specification's order: extensibility, the
    // target's keys, then one descriptor per key — every one of which may be a
    // trap of a proxy target, so each is followed by the rule-8 question.
    let open = invariant::extensible(target);
    if throw::in_flight() {
        return;
    }
    let own = invariant::own_keys_of(target);
    if throw::in_flight() {
        return;
    }
    let mut fixed = Vec::new();
    let mut loose = Vec::new();
    for (key, spelled) in own {
        let state = invariant::own_state(target, key);
        if throw::in_flight() {
            return;
        }
        match state {
            Some(state) if !state.configurable => fixed.push((key, spelled)),
            _ => loose.push((key, spelled)),
        }
    }
    if open && fixed.is_empty() {
        return;
    }
    let mut unchecked = reported;
    // A key the target cannot lose — or, once the target refuses to shrink, any
    // key it has — must be listed: the program has already been told the key
    // is there and cannot be told otherwise now.
    let required = match open {
        true => fixed,
        false => fixed.into_iter().chain(loose).collect(),
    };
    for (key, spelled) in &required {
        let Some(at) = unchecked.iter().position(|listed| listed == key) else {
            throw::type_error(&format!(
                "'ownKeys' on proxy: trap result did not include '{spelled}'"
            ));
            return;
        };
        unchecked.remove(at);
    }
    if !open && let Some(extra) = unchecked.first() {
        throw::type_error(&format!(
            "'ownKeys' on proxy: trap returned extra keys but proxy target is non-extensible: '{}'",
            super::spelled(*extra)
        ));
    }
}

/// Whether a key a trap listed is a SYMBOL, in either spelling it arrives in.
///
/// A handler that built the list itself answers real symbol values; one that
/// forwarded the target's own keys answers the reserved key TEXT, because that
/// is how a symbol-keyed property is filed. Both are the same key, and every
/// caller that splits a key list by kind has to recognise both — which is why
/// this is one function rather than the test written twice.
fn is_symbol_entry(entry: u64) -> bool {
    with_current(|context| {
        crate::entry::symbol::is_symbol(context, entry)
            || crate::entry::text::to_text(context, Value(entry))
                .is_some_and(|text| crate::entry::symbol::is_symbol_key(&text))
    })
}

/// The STRING half of what `ownKeys` reported — `Object.getOwnPropertyNames`.
///
/// `[[OwnPropertyKeys]]` on a proxy is the trap's list entire, symbols
/// included, and the two `Object` spellings each take one half of it. Handing
/// the whole list to `getOwnPropertyNames` put a symbol where a name belongs:
/// `Object.getOwnPropertyNames(p).join("|")` answered `a||b|`, the empty
/// spellings being symbols that have no string form.
pub(in crate::entry) fn own_names(object: u64) -> Option<u64> {
    own_half(object, false)
}

/// The SYMBOL half — `Object.getOwnPropertySymbols`, which read the proxy's
/// own empty cell and answered `[]` without running `ownKeys` at all.
pub(in crate::entry) fn own_symbols(object: u64) -> Option<u64> {
    own_half(object, true)
}

/// One half of the trap's list, in the trap's order.
///
/// A symbol arrives in either spelling [`is_symbol_entry`] names; the reserved
/// key text of a forwarded list is decoded back to the symbol VALUE, because
/// `getOwnPropertySymbols` answers symbols and not their filing names.
fn own_half(object: u64, symbols: bool) -> Option<u64> {
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
    let kept = keys
        .into_iter()
        .filter(|entry| is_symbol_entry(*entry) == symbols)
        .map(|entry| match symbols {
            false => entry,
            true => with_current(|context| {
                if crate::entry::symbol::is_symbol(context, entry) {
                    return entry;
                }
                crate::entry::text::to_text(context, Value(entry))
                    .and_then(|text| text.to_rust())
                    .and_then(|text| crate::entry::symbol::value_of_key_text(context, &text))
                    .unwrap_or(entry)
            }),
        })
        .collect();
    Some(crate::entry::modules::make_array(kept))
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
        // `Object.keys` never reports a symbol, whatever the trap listed — and
        // skipped BEFORE the descriptor is asked for, because the specification
        // only calls `[[GetOwnProperty]]` for the keys it could report.
        if is_symbol_entry(key) {
            continue;
        }
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
