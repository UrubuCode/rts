//! The static methods that speak in **descriptors**, and `Object.create`.
//!
//! # Where the three flags live
//!
//! This section used to say they lived nowhere, and that recording them was a
//! change to what a shape IS. That was wrong about the place: an attribute is
//! per OBJECT per property — `defineProperty(o, "x", {enumerable: false})` says
//! nothing about the other objects sharing `o`'s shape — so the shape is exactly
//! where it must not go.
//!
//! They live beside the cell, in the accessor table's shape and for the accessor
//! table's reason. [`super::super::integrity`] holds them, records only
//! deviations, and makes a non-writable one stick the same way a freeze does.
//!
//! The two cases that used to be named as lies are now ordinary: an array's
//! `length` and a collection's `size` are non-enumerable properties, recorded
//! where each is written rather than as a name the enumeration knows to skip.
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
//! `configurable: false`. That is the object's answer folded onto the
//! property's own, which [`super::super::integrity::effective`] performs — and
//! it is asked by the validation `defineProperty` runs as well, so the
//! descriptor a program READS and the one a definition is CHECKED against
//! cannot come apart.

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
    ("getOwnPropertySymbols", get_own_property_symbols),
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
    let held = with_current(|context| match object_cell(context, object) {
        Some(cell) => super::super::integrity::is_frozen(context, cell),
        None => true,
    });
    Value::from_bool(held).bits()
}

/// `Object.isSealed(o)`.
extern "C" fn is_sealed(_e: u64, _this: u64, object: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let held = with_current(|context| match object_cell(context, object) {
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
        object_cell(context, object).is_some_and(|cell| context.integrity_at(cell).is_none())
    });
    Value::from_bool(open).bits()
}

/// The cell of a value that is an OBJECT, and `None` for anything else.
///
/// # Why `as_slot` is the wrong question here
///
/// Because a string primitive has a cell. Text lives in the region like every
/// other allocated thing, so `Value("a").as_slot()` is `Some` — and the three
/// integrity predicates read that as "an object, ask its record", found no
/// record, and answered that a string is extensible and not frozen.
///
/// The specification says the opposite and says it for a reason that is not a
/// special case: a non-object has nothing to write, so it is frozen and sealed
/// by construction, and nothing can be added to it, so it is not extensible.
/// `Object.isFrozen("a")` is `true` and `Object.isExtensible("a")` is `false`.
///
/// The distinction between "has a cell" and "is an object" already exists —
/// `primitive::is_object_in` is what answers it, and it is what a set operation
/// asks before deciding an argument is set-like. Asking it here too is what
/// keeps a string, a symbol and a boxed primitive from diverging in three
/// separate places.
fn object_cell(context: &super::super::Context, object: u64) -> Option<u32> {
    if !super::super::primitive::is_object_in(context, object) {
        return None;
    }
    Value(object).as_slot()
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
    let fresh = super::super::objects::object_new(0);
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
///
/// # Every descriptor is READ before any property is defined
///
/// Two passes, and the language requires it: `DefinePropertiesImpl` gathers the
/// whole list and only then applies it, so a set whose third descriptor throws
/// leaves the object exactly as it was. One pass defined the first two and then
/// let the exception out, which is a half-applied merge — the failure mode this
/// crate treats as worse than an outright refusal, because the object looks
/// like it was configured.
fn apply(object: u64, descriptors: u64) {
    let absent = with_current(|context| undefined_of(context));
    if descriptors == absent {
        return;
    }
    let names = super::super::array::own_keys(descriptors);
    let Some(names) = elements(names) else {
        return;
    };
    let mut gathered = Vec::with_capacity(names.len());
    for name in names {
        // Reading the entry runs a getter, and so does reading each of its six
        // fields. Rule 8 both times: a throw stops the gathering, and nothing
        // has been defined yet.
        let descriptor = super::super::computed::get_indexed(descriptors, name);
        if super::super::throw::in_flight() {
            return;
        }
        let Some(wanted) = super::descriptor::read(descriptor) else {
            return;
        };
        gathered.push((name, wanted));
    }
    for (name, wanted) in gathered {
        super::define_read(object, name, &wanted);
        if super::super::throw::in_flight() {
            return;
        }
    }
}

/// `Object.getOwnPropertyNames(o)`.
///
/// NOT the same answer `Object.keys` gives: this one reports a property whose
/// `enumerable` is false, which is the whole difference between the two and was
/// not expressible until attributes were recorded.
extern "C" fn get_own_property_names(
    _e: u64,
    _this: u64,
    object: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    super::super::array::own_names(object)
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

/// `Object.getOwnPropertySymbols(o)`.
///
/// Was missing entirely — every symbol key an object holds was reachable
/// from JavaScript, through `sym in obj` and `obj[sym]`, but there was no
/// way to ENUMERATE them, which is what this answers: the array `for (const
/// s of Object.getOwnPropertySymbols(o))` walks.
extern "C" fn get_own_property_symbols(
    _e: u64,
    _this: u64,
    object: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    own_symbols(object)
}

/// Every own symbol key of an object, as an array of symbol VALUES.
///
/// Named rather than left inside the method above because it is half of a
/// second operation: `[[OwnPropertyKeys]]` is the string keys followed by the
/// symbol ones, and `Reflect.ownKeys` is the only spelling that reports both.
/// A second walk written there would be a second answer to which keys are
/// symbols — and this crate's own encoding of one (a reserved name space rather
/// than a third `Key` variant) is exactly the fact two walks would eventually
/// disagree about.
pub(in crate::entry) fn own_symbols(object: u64) -> u64 {
    let symbols = with_current(|context| {
        super::super::array::symbol_keyed_with(context, object, false)
            .into_iter()
            .filter_map(|(key, _)| match key {
                Key::Name(named) => context.interner.text(named).and_then(|text| text.to_rust()),
                Key::Index(_) => None,
            })
            .filter_map(|text| super::super::symbol::value_of_key_text(context, &text))
            .collect::<Vec<u64>>()
    });
    super::super::array_proto::built(symbols)
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
    let made = super::super::objects::object_new(0);
    // Every own key, `own_names` rather than `own_keys`: a property made
    // non-enumerable through `defineProperty` is exactly the one this walk
    // must not skip — the ENTIRE reason to call `getOwnPropertyDescriptors`
    // over `Object.keys` is to see what the enumerable filter hides. Using
    // the enumerable list here made such a property vanish from its own
    // descriptor object rather than report `enumerable: false` on it.
    let names = super::super::array::own_names(object);
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

/// The descriptor for a key, asking a proxy's handler first.
///
/// The spelling every caller outside this module wants: `Reflect`, and the
/// enumerability filter `Object.keys` applies to a proxy. Separate from
/// [`describe_own`] because that one is what FORWARDING reaches — a handler
/// without the trap asks the target, and asking through this would ask the
/// proxy again.
pub(in crate::entry) fn describe_of(object: u64, name: u64) -> u64 {
    if let Some(named) =
        with_current(|context| super::super::computed::property_key(context, Value(name)))
        && let Some(answered) = super::super::proxy::describe(object, named)
    {
        return answered;
    }
    describe_own(object, name)
}

/// The descriptor an object has for a key, without asking a proxy.
///
/// One caller: a proxy whose handler does not trap the question and forwards it
/// to its target. Answers `undefined` for a key the object does not have, which
/// is the same distinction the public spelling makes.
pub(in crate::entry) fn describe_own(object: u64, name: u64) -> u64 {
    match descriptor(object, name) {
        Some(made) => made,
        None => with_current(|context| undefined_of(context)),
    }
}

/// The descriptor for one own key, or `None` if there is no such key.
///
/// An accessor answers `{get, set}` and a slot answers `{value, writable}`,
/// which is the distinction the language draws. This paragraph used to end
/// "and the only one this engine can draw, since the three flags are not
/// recorded" — they are, in [`super::super::integrity`], per object and per
/// property, which is what made `defineProperty` able to refuse.
fn descriptor(object: u64, name: u64) -> Option<u64> {
    // All three flags come from the property's own record folded together with
    // the object's integrity — either can refuse, so a frozen object reports a
    // property written the ordinary way as non-writable. `effective` is that
    // fold, shared with the validation `defineProperty` performs so the two
    // cannot disagree about what a sealed object's `configurable` is.
    let (found, attributes) = with_current(|context| {
        let cell = Value(object).as_slot()?;
        let key = super::key_for(context, name)?;
        // An ELEMENT is an own property with no shape slot — `super::super::array`'s
        // first page says why the store lives apart from the layout — so the walk
        // below never finds one, and `Object.getOwnPropertyDescriptor([1], "0")`
        // answered `undefined` for a property `0 in [1]` reports as present. Two
        // readable facts about one array that cannot both be true.
        if let Some(element) = element_state(context, cell, key) {
            return Some(element);
        }
        let Key::Name(key) = key else {
            return None;
        };
        let attributes = super::super::integrity::effective(context, cell, key);
        // The accessor table is asked first: an accessor is deliberately absent
        // from the layout, so a key that is one has no slot to find.
        if let Some(pair) = context.accessor_at(cell, key.index() as u32) {
            let absent = undefined_of(context);
            let found =
                Descriptor::Accessor(pair.0.unwrap_or(absent), pair.1.unwrap_or(absent));
            return Some((found, attributes));
        }
        // Own rather than inherited: a descriptor describes what the object
        // itself has, and reporting a prototype's property as own is what would
        // make a copy through `defineProperties` flatten a chain.
        let held = super::super::objects::own_property(context, cell, Key::Name(key))?;
        Some((Descriptor::Value(held.bits()), attributes))
    })?;

    let made = super::super::objects::object_new(0);
    match found {
        Descriptor::Accessor(getter, setter) => {
            put(made, "get", getter);
            put(made, "set", setter);
        }
        Descriptor::Value(held) => {
            put(made, "value", held);
            put(made, "writable", Value::from_bool(attributes.writable).bits());
        }
    }
    put(made, "enumerable", Value::from_bool(attributes.enumerable).bits());
    put(
        made,
        "configurable",
        Value::from_bool(attributes.configurable).bits(),
    );
    Some(made)
}

/// The descriptor an ARRAY's element is, when the key names one.
///
/// # Why the three flags are folded by hand here
///
/// `super::super::integrity::effective` takes a SHAPE key, and an element has
/// none — its attributes cannot be recorded per element, which this module's
/// sibling [`super::arrays`] names as the stated gap it leaves. So what an
/// element permits is what the OBJECT permits: `Object.freeze([1])` makes its
/// element non-writable and non-configurable, and nothing else can make one
/// deviate.
///
/// A HOLE is not a property. `Object.getOwnPropertyDescriptor([, 1], "0")` is
/// `undefined`, the same absence `0 in [, 1]` reports.
fn element_state(
    context: &mut super::super::Context,
    cell: u32,
    key: Key,
) -> Option<(Descriptor, super::super::integrity::Attributes)> {
    let at = match key {
        Key::Index(at) => at as usize,
        // Every computed key arrives as a NAME, index or not — see
        // `super::super::computed::property_key` — so the canonical spelling is
        // what decides, and `as_array_index` is the one answer to which strings
        // those are.
        Key::Name(named) => {
            let text = context.interner.text(named)?;
            crate::object::as_array_index(text)? as usize
        }
    };
    let held = *context.elements_at(cell)?.get(at)?;
    if super::super::array::is_hole(context, held) {
        return None;
    }
    let attributes = super::super::integrity::Attributes {
        writable: !super::super::integrity::refuses_write(context, cell),
        enumerable: true,
        configurable: !super::super::integrity::refuses_removal(context, cell),
    };
    Some((Descriptor::Value(held), attributes))
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
