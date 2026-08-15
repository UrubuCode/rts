//! `[[DefineOwnProperty]]` — the one property operation that can REFUSE.
//!
//! # Why this is not `put` with three extra flags
//!
//! An assignment asks "may I write here?" and gets one answer. A definition asks
//! six questions, and five of them are about what the property currently IS:
//! a non-configurable property may not change kind, may not change its
//! enumerability, and — when it is also non-writable — may not change its value.
//! `ValidateAndApplyPropertyDescriptor` is that comparison, and until it existed
//! `Object.defineProperty` accepted every redefinition, which made
//! `configurable: false` a flag a program could read back and nothing enforced.
//!
//! # ABSENT is not `undefined`, and that is the whole shape of this file
//!
//! `{value: 1}` leaves `writable` out, and the language reads that as `false`
//! for a NEW property and as "unchanged" for an existing one. `{value: undefined}`
//! states a value. A descriptor read as six values with defaults cannot tell
//! those apart, so every field here is an `Option` and the reading asks
//! `HasProperty` before it asks for the value.
//!
//! That distinction is what `Object.defineProperty(a, "length", {writable: false})`
//! turns on: the array's `length` must keep the number it holds, and a reader
//! that filled the absent `value` with `undefined` set the array's length to
//! `undefined` instead.
//!
//! # The refusal is a throw here and a verdict in `Reflect`
//!
//! [`apply`] answers a [`Verdict`] rather than throwing, because
//! `Reflect.defineProperty` reports the same refusal as `false`. The throw is
//! [`super::define`]'s, one level up, which is the only difference between the
//! two spellings of one operation.

use super::super::integrity::{self, Attributes};
use super::super::objects::undefined_of;
use super::super::{Context, with_current};
use crate::object::Key;
use crate::value::Value;
use rts_cranelift::shape::Key as ShapeKey;

/// A descriptor as the language reads one: six fields, every one optional.
///
/// Readable by [`super::arrays`], which is the other half of one operation: an
/// array's `length` and its indices are defined from the same six fields, and a
/// second reading of a descriptor is where the two would come to disagree about
/// what an absent `value` means.
pub(in crate::entry) struct Descriptor {
    pub(super) value: Option<u64>,
    pub(super) get: Option<u64>,
    pub(super) set: Option<u64>,
    writable: Option<bool>,
    enumerable: Option<bool>,
    configurable: Option<bool>,
}

/// What [`apply`] decided, so that the two spellings can report it differently.
pub(in crate::entry) enum Verdict {
    /// The property is now what the descriptor said.
    Done,
    /// The definition was refused — a non-configurable property, or growth on a
    /// non-extensible object.
    Refused,
    /// The receiver was not an object, which is a different error message.
    NotObject,
    /// An array's `length` was given something that is not one — a `RangeError`
    /// rather than a refusal, which is why it is a fourth verdict and not
    /// `Refused` with a different message.
    BadLength,
}

/// `ToPropertyDescriptor` — reads the fields that are PRESENT.
///
/// `None` when the descriptor is not an object (already thrown), or when
/// reading one of its fields left a throw behind. Rule 8: a field may be a
/// getter, and a getter is user code — carrying on with the `undefined` a
/// failed call answers would define a property out of an exception.
pub(in crate::entry) fn read(descriptor: u64) -> Option<Descriptor> {
    if !with_current(|context| super::super::primitive::is_object_in(context, descriptor)) {
        super::super::throw::type_error("Property description must be an object");
        return None;
    }
    // The specification's order, and it is observable: a descriptor whose
    // fields are getters logs them in exactly this sequence.
    let enumerable = field(descriptor, "enumerable")?;
    let configurable = field(descriptor, "configurable")?;
    let value = field(descriptor, "value")?;
    let writable = field(descriptor, "writable")?;
    let get = field(descriptor, "get")?;
    let set = field(descriptor, "set")?;
    if (get.is_some() || set.is_some()) && (value.is_some() || writable.is_some()) {
        super::super::throw::type_error(
            "Invalid property descriptor. Cannot both specify accessors and a value or writable attribute",
        );
        return None;
    }
    Some(Descriptor {
        value,
        get,
        set,
        writable: writable.map(|held| super::super::primitives::to_boolean(held)),
        enumerable: enumerable.map(|held| super::super::primitives::to_boolean(held)),
        configurable: configurable.map(|held| super::super::primitives::to_boolean(held)),
    })
}

/// One field, asked for only when the descriptor HAS it.
///
/// The outer `None` is a throw in flight; the inner one is a field that is not
/// there. Two absences with different meanings, which is why they are two
/// layers rather than one.
fn field(descriptor: u64, name: &str) -> Option<Option<u64>> {
    // `well_known_text` rather than `intern_value`: the second allocates a cell
    // per call, and six of them per `defineProperty` is six cells a collection
    // then has to walk to learn they are garbage.
    let key = with_current(|context| context.well_known_text(name));
    // `HasProperty`, not `hasOwn`: a descriptor built by `Object.create(base)`
    // inherits its fields, and the language reads those.
    let present = super::super::computed::has_property(key, descriptor);
    if super::super::throw::in_flight() {
        return None;
    }
    if !present {
        return Some(None);
    }
    let held = super::super::computed::get_indexed(descriptor, key);
    if super::super::throw::in_flight() {
        return None;
    }
    Some(Some(held))
}

/// The descriptor a cell already has for a key, if it has one at all.
enum Current {
    /// A slot, already read, and what it permits.
    Data { value: u64, attributes: Attributes },
    /// A pair, either half of which may be absent.
    Accessor {
        get: Option<u64>,
        set: Option<u64>,
        attributes: Attributes,
    },
}

impl Current {
    fn attributes(&self) -> Attributes {
        match self {
            Current::Data { attributes, .. } | Current::Accessor { attributes, .. } => *attributes,
        }
    }
}

/// Defines one property, or refuses.
pub(in crate::entry) fn apply(object: u64, name: u64, wanted: &Descriptor) -> Verdict {
    // An array's `length` and its indices are not shape slots, so the walk below
    // would define a property describing a store it never touched. See
    // [`super::arrays`] for the four facts that came apart when it did.
    if let Some(verdict) = super::arrays::define(object, name, wanted) {
        return verdict;
    }
    with_current(|context| {
        let Some(cell) = Value(object).as_slot() else {
            return Verdict::NotObject;
        };
        let Some(Key::Name(key)) = super::key_for(context, name) else {
            return Verdict::Refused;
        };
        let current = own_of(context, cell, key);
        let extensible = !integrity::refuses_growth(context, cell);
        if !permitted(context, current.as_ref(), wanted, extensible) {
            return Verdict::Refused;
        }
        write(context, cell, key, current.as_ref(), wanted);
        Verdict::Done
    })
}

/// What the cell has for the key now.
///
/// The accessor table first, for the reason [`super::super::accessor`]'s first
/// page gives: an accessor is deliberately outside the layout, so a walk of the
/// shape alone would report it as absent and let a redefinition create a data
/// property beside the getter it was replacing.
fn own_of(context: &mut Context, cell: u32, key: ShapeKey) -> Option<Current> {
    let attributes = integrity::effective(context, cell, key);
    if let Some((get, set)) = context.accessor_at(cell, key.index() as u32) {
        return Some(Current::Accessor {
            get,
            set,
            attributes,
        });
    }
    let held = super::super::objects::own_property(context, cell, Key::Name(key))?;
    Some(Current::Data {
        value: held.bits(),
        attributes,
    })
}

/// `ValidateAndApplyPropertyDescriptor`, asked before anything is written.
///
/// Every branch here answers "does this change something the property said it
/// would not change?" — which is why a descriptor naming only `enumerable: true`
/// on an already-enumerable non-configurable property is accepted: it asks for
/// nothing.
fn permitted(
    context: &Context,
    current: Option<&Current>,
    wanted: &Descriptor,
    extensible: bool,
) -> bool {
    let Some(current) = current else {
        // A property the object does not have is growth, and growth is exactly
        // what `preventExtensions` refuses.
        return extensible;
    };
    let held = current.attributes();
    if held.configurable {
        return true;
    }
    if wanted.configurable == Some(true) {
        return false;
    }
    if let Some(asked) = wanted.enumerable
        && asked != held.enumerable
    {
        return false;
    }
    let accessor_shaped = wanted.get.is_some() || wanted.set.is_some();
    let data_shaped = wanted.value.is_some() || wanted.writable.is_some();
    if !accessor_shaped && !data_shaped {
        return true;
    }
    match current {
        Current::Accessor { get, set, .. } => {
            if !accessor_shaped {
                return false;
            }
            let absent = undefined_of(context);
            // Function identity, not `SameValue` over a general value: a
            // descriptor may only restate the halves this property already has,
            // and two distinct closures with the same body are two functions.
            let unchanged = |asked: Option<u64>, held: Option<u64>| {
                asked.is_none_or(|value| value == held.unwrap_or(absent))
            };
            unchanged(wanted.get, *get) && unchanged(wanted.set, *set)
        }
        Current::Data { value, attributes } => {
            if accessor_shaped {
                return false;
            }
            if attributes.writable {
                return true;
            }
            if wanted.writable == Some(true) {
                return false;
            }
            wanted.value.is_none_or(|asked| {
                crate::value::same_value(Value(asked), Value(*value), |a, b| {
                    context.same_text(a, b)
                })
            })
        }
    }
}

/// The definition, once it is known to be allowed.
fn write(
    context: &mut Context,
    cell: u32,
    key: ShapeKey,
    current: Option<&Current>,
    wanted: &Descriptor,
) {
    let accessor = becomes_accessor(current, wanted);
    let attributes = merged(current, wanted, accessor);
    // Cleared BEFORE the value lands and recorded after, because a non-writable
    // record is what the ordinary store path refuses — leaving the old one in
    // place would make a redefinition refuse its own value.
    integrity::clear_attributes(context, cell, key);
    if accessor {
        // The key leaves the layout when it was a data property, or the object
        // reports it twice — once from the shape and once from the accessor
        // table — and a warmed read finds the slot the getter replaced.
        if matches!(current, Some(Current::Data { .. })) {
            super::super::computed::remove_own(context, cell, key);
        }
        let (get, set) = match current {
            Some(Current::Accessor { get, set, .. }) => (*get, *set),
            _ => (None, None),
        };
        let absent = undefined_of(context);
        // `{get: undefined}` STATES that there is no getter; leaving the field
        // out keeps whichever half the property already had.
        let pick = |asked: Option<u64>, held: Option<u64>| match asked {
            Some(value) if value == absent => None,
            Some(value) => Some(value),
            None => held,
        };
        context.set_accessor(
            cell,
            key.index() as u32,
            pick(wanted.get, get),
            pick(wanted.set, set),
        );
    } else {
        if matches!(current, Some(Current::Accessor { .. })) {
            super::super::computed::remove_own(context, cell, key);
        }
        let held = match current {
            Some(Current::Data { value, .. }) => Some(*value),
            _ => None,
        };
        // A descriptor with no `value` keeps what is there, and gives a NEW
        // property `undefined` — `Object.defineProperty(o, "x", {})` creates it.
        let value = wanted
            .value
            .or(held)
            .unwrap_or_else(|| undefined_of(context));
        // `put` rather than `set_property`: a definition never runs a setter,
        // and `set_property` is the assignment path that looks for one.
        super::super::objects::put(context, cell, Key::Name(key), value);
    }
    // Only a deviation is recorded — see [`Attributes::default`] for why the
    // common case must not pay for a table entry.
    if attributes != Attributes::default() {
        integrity::set_attributes(context, cell, key, attributes);
    }
}

/// Whether the property ends up as a pair of functions.
///
/// A descriptor naming neither kind — `{enumerable: true}` — leaves the kind
/// alone, which is the only reason this is not one boolean read off `wanted`.
fn becomes_accessor(current: Option<&Current>, wanted: &Descriptor) -> bool {
    if wanted.get.is_some() || wanted.set.is_some() {
        return true;
    }
    if wanted.value.is_some() || wanted.writable.is_some() {
        return false;
    }
    matches!(current, Some(Current::Accessor { .. }))
}

/// The three flags after the descriptor is folded onto what is there.
///
/// A field the descriptor leaves out keeps the current value, and defaults to
/// `false` for a property that did not exist — which is the difference between
/// `Object.defineProperty` and an assignment, and the reason programs reach for
/// it.
fn merged(current: Option<&Current>, wanted: &Descriptor, accessor: bool) -> Attributes {
    let (writable, enumerable, configurable) = match current {
        Some(Current::Data { attributes, .. }) => (
            attributes.writable,
            attributes.enumerable,
            attributes.configurable,
        ),
        // An accessor has no `writable`, so a conversion back to a data
        // property starts from `false` rather than from whatever was recorded
        // beside the pair.
        Some(Current::Accessor { attributes, .. }) => {
            (false, attributes.enumerable, attributes.configurable)
        }
        None => (false, false, false),
    };
    Attributes {
        // An accessor is written through its setter, never through the slot, so
        // the store path must not be told to refuse the key — a recorded
        // `writable: false` would retype the cell for a refusal that can never
        // apply.
        writable: if accessor {
            true
        } else {
            wanted.writable.unwrap_or(writable)
        },
        enumerable: wanted.enumerable.unwrap_or(enumerable),
        configurable: wanted.configurable.unwrap_or(configurable),
    }
}
