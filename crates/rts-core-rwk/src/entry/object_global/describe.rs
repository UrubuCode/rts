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
//! # Why `freeze` and `seal` are absent rather than approximated
//!
//! They cannot be enforced from here. Compiled code emits `cached_set`, which
//! writes the slot a layout says a key is at without asking the runtime — so a
//! frozen object would be frozen only against the slow path, and `o.x = 1` in a
//! loop that had warmed its cache would go through. An `Object.freeze` that
//! silently does not freeze is a wrong program that runs, where its absence is a
//! `TypeError` a program can see.
//!
//! Enforcing it is a machine question: a guard on the store, which is
//! `rts-cranelift`'s to add. Recorded here because this is where a reader looks
//! for it.

use super::super::native::Native;
use super::super::objects::undefined_of;
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
];

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

    let made = super::super::objects::object_new();
    let truth = Value::from_bool(true).bits();
    match found {
        Descriptor::Accessor(getter, setter) => {
            put(made, "get", getter);
            put(made, "set", setter);
        }
        Descriptor::Value(held) => {
            put(made, "value", held);
            put(made, "writable", truth);
        }
    }
    put(made, "enumerable", truth);
    put(made, "configurable", truth);
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
