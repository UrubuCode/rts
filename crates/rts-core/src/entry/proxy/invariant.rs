//! What the target really says, against what the handler answered.
//!
//! # Why a layer at all
//!
//! A trap is user code, and user code may lie. The language allows most of the
//! lies — that is what a proxy is for — but not the ones a program can already
//! have relied on without the proxy: a property the target declared
//! non-configurable, a prototype an object that refuses to grow can no longer
//! change. Those facts are observable through the target itself, so a handler
//! contradicting one makes two answers to a question the language promised had
//! exactly one.
//!
//! Every refusal here is a `TypeError` the program can catch, raised through
//! [`crate::entry::throw`] so that `e instanceof TypeError` holds — the same
//! error object the rest of this crate raises, not a second shape invented for
//! proxies.
//!
//! # Why the flags are read beside the cell rather than through a descriptor
//!
//! `object_global::describe_of` answers the same three flags and allocates a
//! descriptor object to say so. That object would be built on every `get`,
//! which is the operation a proxy performs most, and thrown away unread. The
//! flags live in [`crate::entry::integrity`], beside the cell, and reading them
//! there is what every other question in this crate does — for an ORDINARY
//! target. A proxy target has no flags beside its cell; see the next section.
//!
//! # A target that is itself a proxy is ASKED, through its own traps
//!
//! The specification reads the target through its internal methods —
//! `target.[[GetOwnProperty]](P)`, `IsExtensible(target)`,
//! `target.[[OwnPropertyKeys]]()` — and on a proxy target each of those is a
//! trap. This module used to stop at such a target and answer "no own
//! property", on the grounds that asking would run the inner handler a second
//! time. That second run is not an invention: it is what every engine performs,
//! and `claude2-proxy-nested-target-depth` logs it —
//! `get3:a,get2:a,get1:a,gopd1:a,gopd2:a,gopd1:a,gopd1:a` — where the check
//! stopping short logged only the three `get`s. Stopping short also let a chain
//! lie: an outer handler could contradict a non-configurable property the inner
//! proxy reports, because nothing compared the two.
//!
//! So every function here may run user code, and **every caller asks
//! `throw::in_flight()` after it** before trusting the answer (rule 8). An
//! ordinary target still pays nothing beyond reading flags beside its cell.

use crate::entry::{Context, integrity, objects, throw, with_current};
use crate::object::Key;
use crate::value::Value;

/// What one own property of a target permits.
pub(super) struct Own {
    /// Whether it may be removed or redefined.
    pub(super) configurable: bool,
    /// Whether a store lands. Always false for an accessor, which has no slot.
    pub(super) writable: bool,
    /// Whether an enumeration reports it.
    pub(super) enumerable: bool,
    /// What a DATA property holds — `None` when the key is an accessor.
    pub(super) value: Option<u64>,
    /// An accessor's getter. A non-configurable one with none must read
    /// `undefined`, whatever a `get` trap says (§10.5.8 step 10.b).
    pub(super) get: Option<u64>,
    /// An accessor's setter. A non-configurable one with none refuses every
    /// store, whatever a `set` trap says (§10.5.9 step 10.b).
    pub(super) set: Option<u64>,
}

impl Own {
    /// The record `IsCompatiblePropertyDescriptor` compares against.
    pub(super) fn existing(&self) -> crate::entry::object_global::Existing {
        crate::entry::object_global::Existing {
            value: self.value,
            get: self.get,
            set: self.set,
            attributes: integrity::Attributes {
                writable: self.writable,
                enumerable: self.enumerable,
                configurable: self.configurable,
            },
        }
    }
}

/// `target.[[GetOwnProperty]](key)`, as the flags an invariant compares.
///
/// `None` for a key the target does not have — and also when a proxy target's
/// trap threw, which the caller tells apart by asking `throw::in_flight()`.
pub(super) fn own_state(target: u64, key: Key) -> Option<Own> {
    if super::is_proxy(target) {
        return described(target, key);
    }
    with_current(|context| own_state_in(context, target, key))
}

/// A proxy target's own property, read from the descriptor its trap answers.
fn described(target: u64, key: Key) -> Option<Own> {
    let answered = super::describe::describe(target, key)?;
    if throw::in_flight() || answered == super::absent() {
        return None;
    }
    // Already complete: `describe` puts a trap's answer through
    // `CompletePropertyDescriptor`, so every field read below is present.
    let read = crate::entry::object_global::descriptor_read(answered)?;
    let undefined = super::absent();
    let accessor = read.get.is_some() || read.set.is_some();
    let half = |held: Option<u64>| held.filter(|value| *value != undefined);
    Some(Own {
        configurable: read.configurable.unwrap_or(false),
        writable: read.writable.unwrap_or(false),
        enumerable: read.enumerable.unwrap_or(false),
        value: (!accessor).then(|| read.value.unwrap_or(undefined)),
        get: half(read.get),
        set: half(read.set),
    })
}

/// [`own_state`] for an ordinary target, from a context already in hand.
fn own_state_in(context: &mut Context, target: u64, key: Key) -> Option<Own> {
    let cell = Value(target).as_slot()?;
    let machine = objects::machine_key(key);
    // The accessor table first, for the reason `object_global::describe` states:
    // an accessor is deliberately absent from the layout, so a key that is one
    // has no slot for `own_property` to find.
    if let Some(named) = machine
        && let Some((get, set)) = context.accessor_at(cell, named.index() as u32)
    {
        return Some(Own {
            configurable: !integrity::refuses_key_removal(context, cell, named),
            writable: false,
            enumerable: integrity::effective(context, cell, named).enumerable,
            value: None,
            get,
            set,
        });
    }
    let held = objects::own_property(context, cell, key)?;
    let (writable, enumerable, configurable) = match machine {
        Some(named) => (
            !integrity::refuses_key_write(context, cell, named),
            integrity::effective(context, cell, named).enumerable,
            !integrity::refuses_key_removal(context, cell, named),
        ),
        // An index has no machine key and therefore no recorded attributes, so
        // it permits what an array's own storage permits. Stated rather than
        // defaulted silently: `objects::machine_key` already names indexed
        // storage as the boundary, and inventing a refusal here would make an
        // element of a proxied array unwritable for a reason nothing recorded.
        None => (true, true, true),
    };
    Some(Own {
        configurable,
        writable,
        enumerable,
        value: Some(held.bits()),
        get: None,
        set: None,
    })
}

/// `IsExtensible(target)`.
///
/// A proxy target answers through its `isExtensible` trap; `false` when that
/// threw, which the caller tells apart by asking `throw::in_flight()`.
pub(super) fn extensible(target: u64) -> bool {
    if let Some(answered) = super::prototype::extensible(target) {
        return answered;
    }
    with_current(|context| {
        Value(target)
            .as_slot()
            .is_some_and(|cell| context.integrity_at(cell).is_none())
    })
}

/// `target.[[OwnPropertyKeys]]()`, as the key the runtime compares and the
/// text a message names.
///
/// Both halves are answered at once because the caller needs both and interning
/// the text twice would be two lookups for one key. A proxy target answers
/// through its `ownKeys` trap, and the list is empty when that threw.
pub(super) fn own_keys_of(target: u64) -> Vec<(Key, String)> {
    if super::is_proxy(target) {
        let Some(listed) = super::keys::own_keys(target) else {
            return Vec::new();
        };
        if throw::in_flight() {
            return Vec::new();
        }
        let entries = with_current(|context| {
            Value(listed)
                .as_slot()
                .and_then(|cell| context.elements_at(cell))
                .cloned()
                .unwrap_or_default()
        });
        let keys: Vec<Key> = with_current(|context| {
            entries
                .into_iter()
                .filter_map(|entry| crate::entry::computed::property_key(context, Value(entry)))
                .collect()
        });
        return keys.into_iter().map(|key| (key, super::spelled(key))).collect();
    }
    with_current(|context| {
        // Every own key, not the enumerable ones: an invariant is about what the
        // target HAS, and `enumerable: false` hides a property from a walk
        // without making it any less present. Symbols too — a non-configurable
        // symbol-keyed property is as fixed as a named one, and leaving them out
        // let an `ownKeys` trap drop one unchecked.
        let texts = crate::entry::array::key_texts(context, target, false);
        let mut own: Vec<(Key, String)> = texts
            .into_iter()
            .filter_map(|text| {
                let spelled = text.to_rust()?;
                let key = Key::Name(context.interner.intern(&text, &mut context.keys));
                Some((key, spelled))
            })
            .collect();
        for (key, _) in crate::entry::array::symbol_keyed_with(context, target, false) {
            let spelled = match key {
                Key::Name(named) => context
                    .interner
                    .text(named)
                    .and_then(crate::text::Str::to_rust)
                    .unwrap_or_default(),
                Key::Index(index) => index.to_string(),
            };
            own.push((key, spelled));
        }
        own
    })
}
