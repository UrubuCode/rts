//! The static methods that speak in **descriptors**, and `Object.create`.
//!
//! # Why a descriptor this engine answers is mostly `true`
//!
//! `writable`, `enumerable` and `configurable` are not recorded anywhere: a
//! shape holds a key, a slot and a representation, and nothing else. So a
//! descriptor built here reports all three as `true`, which is what a property
//! written by a program actually is — and it is a **lie for the two cases the
//! engine already treats differently**: an array's `length` and a collection's
//! `size` are non-enumerable in the language and ordinary properties here.
//!
//! That is the same gap [`super`]'s `defineProperty` records from the writing
//! side, and it is one fact with two faces rather than two problems: recording
//! the flags is a change to what a shape IS, and both halves land the day it
//! happens.
//!
//! # `freeze` and `seal` are enforced, and what it took
//!
//! This section used to say they were absent because compiled code writes the
//! slot a layout names without asking the runtime — so a flag only the slow path
//! read would freeze an object against every site that had not warmed up yet and
//! against none that had.
//!
//! That is still true of a flag. What makes the freeze real is in
//! [`super::super::integrity`]: freezing gives the cell a new type, so every
//! warmed site misses, and a store's miss asks `rts_cache_resolve_store`, which
//! refuses. The machine grew `RtEntry::CacheResolveStore` for the second half.
//!
//! A descriptor on a frozen object therefore reports `writable: false` and
//! `configurable: false` — the two flags that ARE recorded, per object rather
//! than per property.

use super::super::native::Native;
use super::super::objects::undefined_of;
use super::super::integrity::Integrity;
use super::super::with_current;
use crate::object::Key;
use crate::value::Value;

/// What `Object` holds beyond the eight in [`super`].
pub(super) const STATICS: &[(&str, Native)] = &[
    ("create", create),
    ("getOwnPropertyNames", get_own_property_names),
    ("getOwnPropertyDescriptor", get_own_property_descriptor),
    ("getOwnPropertyDescriptors", get_own_property_descriptors),
    ("defineProperties", define_properties),
    ("freeze", freeze),
    ("seal", seal),
    ("preventExtensions", prevent_extensions),
    ("isFrozen", is_frozen),
    ("isSealed", is_sealed),
    ("isExtensible", is_extensible),
];

/// `Object.freeze(o)` — answers the object, which is what makes it chain.
extern "C" fn freeze(_e: u64, _this: u64, object: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    super::super::integrity::restrict(object, Integrity::Frozen)
}

/// `Object.seal(o)` — no properties added or removed, the rest still writable.
extern "C" fn seal(_e: u64, _this: u64, object: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    super::super::integrity::restrict(object, Integrity::Sealed)
}

/// `Object.preventExtensions(o)` — no properties added. Nothing else changes.
extern "C" fn prevent_extensions(
    _e: u64,
    _this: u64,
    object: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    super::super::integrity::restrict(object, Integrity::Closed)
}

/// `Object.isFrozen(o)`.
///
/// A primitive is frozen, which the specification says and which falls out of
/// there being nothing to write rather than out of a special case.
extern "C" fn is_frozen(_e: u64, _this: u64, object: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let held = with_current(|context| match Value(object).as_slot() {
        Some(cell) => super::super::integrity::is_frozen(context, cell),
        None => true,
    });
    Value::from_bool(held).bits()
}

/// `Object.isSealed(o)`.
extern "C" fn is_sealed(_e: u64, _this: u64, object: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let held = with_current(|context| match Value(object).as_slot() {
        Some(cell) => super::super::integrity::is_sealed(context, cell),
        None => true,
    });
    Value::from_bool(held).bits()
}

/// `Object.isExtensible(o)`.
///
/// The one of the three whose answer for a primitive is `false` rather than
/// `true`: a primitive cannot be frozen further and cannot be extended either.
extern "C" fn is_extensible(_e: u64, _this: u64, object: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let open = with_current(|context| {
        Value(object)
            .as_slot()
            .is_some_and(|cell| context.integrity_at(cell).is_none())
    });
    Value::from_bool(open).bits()
}

/// `Object.create(proto, descriptors?)`.
///
/// `null` as the prototype is the point of the function — an object inheriting
/// nothing — so it is passed through rather than treated as absent.
extern "C" fn create(
    _e: u64,
    _this: u64,
    prototype: u64,
    descriptors: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let fresh = super::super::objects::object_new();
    with_current(|context| {
        if let Some(cell) = Value(fresh).as_slot() {
            context.set_prototype(cell, prototype);
        }
    });
    // Outside the borrow: defining a property may run a setter on the way, and
    // reading a descriptor field goes through the ordinary property path.
    apply(fresh, descriptors);
    fresh
}

/// `Object.defineProperties(o, descriptors)` — answers the object.
extern "C" fn define_properties(
    _e: u64,
    _this: u64,
    object: u64,
    descriptors: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    apply(object, descriptors);
    object
}

/// Every descriptor in a map of them, defined on the object.
///
/// Shared so that `create` and `defineProperties` cannot disagree about what a
/// descriptor means — which is the whole reason the second exists as a name
/// rather than as a loop each caller writes.
fn apply(object: u64, descriptors: u64) {
    let absent = with_current(|context| undefined_of(context));
    if descriptors == absent {
        return;
    }
    let names = super::super::array::own_keys(descriptors);
    let Some(names) = elements(names) else {
        return;
    };
    for name in names {
        let descriptor = super::super::computed::get_indexed(descriptors, name);
        super::define(object, name, descriptor);
    }
}

/// `Object.getOwnPropertyNames(o)`.
///
/// The same answer `Object.keys` gives, because there are no non-enumerable
/// properties to tell apart — see the module documentation for the one place
/// that is observably wrong.
extern "C" fn get_own_property_names(
    _e: u64,
    _this: u64,
    object: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    super::super::array::own_keys(object)
}

/// `Object.getOwnPropertyDescriptor(o, k)`.
///
/// `undefined` for a key the object does not have, which is what distinguishes
/// it from one holding `undefined` — the same distinction `in` exists for.
extern "C" fn get_own_property_descriptor(
    _e: u64,
    _this: u64,
    object: u64,
    name: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    match descriptor(object, name) {
        Some(made) => made,
        None => with_current(|context| undefined_of(context)),
    }
}

/// `Object.getOwnPropertyDescriptors(o)` — one object holding all of them.
extern "C" fn get_own_property_descriptors(
    _e: u64,
    _this: u64,
    object: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let made = super::super::objects::object_new();
    let names = super::super::array::own_keys(object);
    let Some(names) = elements(names) else {
        return made;
    };
    for name in names {
        if let Some(built) = descriptor(object, name) {
            super::super::computed::set_indexed(made, name, built);
        }
    }
    made
}

/// The descriptor for one own key, or `None` if there is no such key.
///
/// An accessor answers `{get, set}` and a slot answers `{value, writable}`,
/// which is the distinction the language draws — and the only one this engine
/// can draw, since the three flags are not recorded.
fn descriptor(object: u64, name: u64) -> Option<u64> {
    let found = with_current(|context| {
        let cell = Value(object).as_slot()?;
        let key = super::key_for(context, name)?;
        // The accessor table is asked first: an accessor is deliberately absent
        // from the layout, so a key that is one has no slot to find.
        if let Key::Name(named) = key
            && let Some(pair) = context.accessor_at(cell, named.index() as u32)
        {
            let absent = undefined_of(context);
            return Some(Descriptor::Accessor(
                pair.0.unwrap_or(absent),
                pair.1.unwrap_or(absent),
            ));
        }
        // Own rather than inherited: a descriptor describes what the object
        // itself has, and reporting a prototype's property as own is what would
        // make a copy through `defineProperties` flatten a chain.
        let held = super::super::objects::own_property(context, cell, key)?;
        Some(Descriptor::Value(held.bits()))
    })?;

    // The two flags that ARE recorded, and they are recorded for the object
    // rather than for the property — which is the whole of what integrity is.
    let (writable, configurable) = with_current(|context| {
        let Some(cell) = Value(object).as_slot() else {
            return (true, true);
        };
        (
            !super::super::integrity::refuses_write(context, cell),
            !super::super::integrity::refuses_removal(context, cell),
        )
    });
    let made = super::super::objects::object_new();
    let truth = Value::from_bool(true).bits();
    match found {
        Descriptor::Accessor(getter, setter) => {
            put(made, "get", getter);
            put(made, "set", setter);
        }
        Descriptor::Value(held) => {
            put(made, "value", held);
            put(made, "writable", Value::from_bool(writable).bits());
        }
    }
    put(made, "enumerable", truth);
    put(made, "configurable", Value::from_bool(configurable).bits());
    Some(made)
}

/// What an own key turned out to be.
enum Descriptor {
    /// A pair of functions, either of which may be `undefined`.
    Accessor(u64, u64),
    /// A slot, already read.
    Value(u64),
}

/// One field of a descriptor being built, by a name the runtime knows.
fn put(object: u64, name: &str, value: u64) {
    with_current(|context| {
        if let Some(cell) = Value(object).as_slot() {
            let key = context.well_known(name);
            super::super::objects::put(context, cell, key, value);
        }
    });
}

/// An array's elements, the borrow ending here.
fn elements(array: u64) -> Option<Vec<u64>> {
    with_current(|context| {
        let cell = Value(array).as_slot()?;
        Some(context.elements_at(cell)?.clone())
    })
}
