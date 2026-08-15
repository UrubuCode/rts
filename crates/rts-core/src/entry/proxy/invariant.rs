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
//! there is what every other question in this crate does.
//!
//! # The one target this does not check
//!
//! A target that is itself a proxy. Asking it would run the INNER handler a
//! second time for one operation — a `get` on `new Proxy(new Proxy(x, a), b)`
//! would call an `a.getOwnPropertyDescriptor` that neither program wrote — and
//! the inner proxy applies these same checks to its own target when the forward
//! reaches it. So a chain is checked link by link rather than end to end.

use crate::entry::{Context, integrity, objects, with_current};
use crate::object::Key;
use crate::value::Value;

/// What one own property of a target permits.
pub(super) struct Own {
    /// Whether it may be removed or redefined.
    pub(super) configurable: bool,
    /// Whether a store lands. Always false for an accessor, which has no slot.
    pub(super) writable: bool,
    /// What a DATA property holds — `None` when the key is an accessor.
    pub(super) value: Option<u64>,
    /// Whether an accessor has a getter at all. A non-configurable one with
    /// none must read `undefined`, whatever a `get` trap says.
    pub(super) getter: bool,
}

/// The target's own property, when the target is an ordinary object.
///
/// `None` both for a key it does not have and for a target that is itself a
/// proxy — see this module's last section for why those two answer alike.
pub(super) fn own_state(target: u64, key: Key) -> Option<Own> {
    with_current(|context| own_state_in(context, target, key))
}

/// [`own_state`] from a context already in hand.
fn own_state_in(context: &mut Context, target: u64, key: Key) -> Option<Own> {
    let cell = Value(target).as_slot()?;
    if context.proxy_at(cell).is_some() {
        return None;
    }
    let machine = objects::machine_key(key);
    // The accessor table first, for the reason `object_global::describe` states:
    // an accessor is deliberately absent from the layout, so a key that is one
    // has no slot for `own_property` to find.
    if let Some(named) = machine
        && let Some((getter, _)) = context.accessor_at(cell, named.index() as u32)
    {
        return Some(Own {
            configurable: !integrity::refuses_key_removal(context, cell, named),
            writable: false,
            value: None,
            getter: getter.is_some(),
        });
    }
    let held = objects::own_property(context, cell, key)?;
    let (writable, configurable) = match machine {
        Some(named) => (
            !integrity::refuses_key_write(context, cell, named),
            !integrity::refuses_key_removal(context, cell, named),
        ),
        // An index has no machine key and therefore no recorded attributes, so
        // it permits what an array's own storage permits. Stated rather than
        // defaulted silently: `objects::machine_key` already names indexed
        // storage as the boundary, and inventing a refusal here would make an
        // element of a proxied array unwritable for a reason nothing recorded.
        None => (true, true),
    };
    Some(Own {
        configurable,
        writable,
        value: Some(held.bits()),
        getter: false,
    })
}

/// Whether the target accepts new properties — `Object.isExtensible`'s answer.
///
/// A target that is a proxy answers `true` here, which is the same boundary
/// [`own_state`] draws and for the same reason: its own extensibility is its own
/// handler's question, asked when the forward reaches it.
pub(super) fn extensible(target: u64) -> bool {
    with_current(|context| {
        Value(target)
            .as_slot()
            .is_some_and(|cell| context.integrity_at(cell).is_none())
    })
}

/// The target's own keys, as the key the runtime compares and the text a
/// message names.
///
/// `None` for a target that is a proxy, the boundary [`own_state`] draws.
/// Both halves are answered at once because the caller needs both and interning
/// the text twice would be two lookups for one key.
pub(super) fn own_keys_of(target: u64) -> Option<Vec<(Key, String)>> {
    with_current(|context| {
        let cell = Value(target).as_slot()?;
        if context.proxy_at(cell).is_some() {
            return None;
        }
        // Every own key, not the enumerable ones: an invariant is about what the
        // target HAS, and `enumerable: false` hides a property from a walk
        // without making it any less present.
        let texts = crate::entry::array::key_texts(context, target, false);
        Some(
            texts
                .into_iter()
                .filter_map(|text| {
                    let spelled = text.to_rust()?;
                    let key = Key::Name(context.interner.intern(&text, &mut context.keys));
                    Some((key, spelled))
                })
                .collect(),
        )
    })
}
