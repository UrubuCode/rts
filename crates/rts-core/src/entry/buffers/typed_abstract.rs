//! `%TypedArray%.prototype` — the level the eight concrete prototypes inherit
//! from.
//!
//! # What already answered this, and what did not
//!
//! Asked before writing (`reuse-check`), and most of it was already here. The
//! question *"what does an instance of this class inherit from"* is answered by
//! `class_support::prototype`, keyed by the name a class declares; the question
//! *"is this value one of the eight typed arrays, and which"* is answered by
//! [`super::view_of`] plus [`super::typed_classes::named`], which is the one
//! form for it — a second predicate over the element kind is exactly the
//! "seven ways to ask is-this-an-array" family the rule exists to prevent. And
//! an accessor on a built-in prototype is `native::getter`, which already
//! spells the `get [Symbol.x]` name, the zero length and the setter-less pair.
//!
//! What nothing answered is *"what does a prototype that no class declares
//! inherit from"*. The nearest is `#[rtse::class(extends = path)]`, and it
//! differs because it reaches the parent through the parent's own
//! **registration function** — a Rust path to a declared class. `%TypedArray%`
//! is declared by nobody: it has no constructor to register and no global name
//! to read. So the object is built here and recorded under a name, which is the
//! same shape `object_proto` uses for `"Object.prototype"` rather than a new
//! mechanism.
//!
//! # Why a name in `classes` and not a field on `Context`
//!
//! Because of rule 10: a reference this crate holds is a reference the
//! collector is told about, and `roots::context_roots` already walks every
//! `Registered`'s `made` and `prototype`. A new field would be a new place to
//! be missing from that list — and this one is reachable from no JavaScript
//! value until a concrete prototype links to it, so a cell the collector did
//! not know about would be freed under the chain of all eight classes.
//!
//! # Why the tag is an ACCESSOR here and a data property there
//!
//! The specification puts exactly one `Symbol.toStringTag` on the typed arrays,
//! and it is a getter on this object that answers the receiver's class. Each
//! concrete prototype here carries its own data property with the same string,
//! which is what makes `Object.prototype.toString.call(t)` work, and that stays:
//! it shadows this getter for an instance, so nothing a program reads *through*
//! the property changes. What the getter adds is the question asked *about* it —
//! `Object.getOwnPropertyDescriptor(…, Symbol.toStringTag).get` — which real
//! libraries use as a typed-array test precisely because it answers `undefined`
//! instead of throwing for everything else.

use super::{Context, typed_classes};
use crate::entry::{class_support, native, objects, symbol, with_current};
use crate::text::Str;
use crate::value::Value;

/// The name `classes` remembers it under.
///
/// Not a name any program can read: `%TypedArray%` is how the specification
/// spells an intrinsic with no global binding, and a name no identifier can
/// spell cannot collide with a class a host declares.
const NAME: &str = "%TypedArray%.prototype";

/// The shared prototype, made once.
///
/// Lazily, like every other built-in prototype in this crate: a program that
/// never mentions a typed array should not spend the cells.
fn prototype_of(context: &mut Context) -> Option<u32> {
    if let Some(found) =
        class_support::prototype(context, NAME).and_then(|value| Value(value).as_slot())
    {
        return Some(found);
    }
    // Room for the ~35 shared methods, the geometry getters, the tag and
    // `constructor`: a prototype sized at birth keeps every method inline-cacheable.
    let cell = native::plain_with_room(context, 48)?;
    let value = Value::from_slot(cell).bits();
    // Recorded BEFORE anything else allocates, which is the order
    // `class_support::record` documents and the reason `string::prototype_of`
    // once recursed until the region ran out: installing interns, interning
    // allocates, and an allocation is one chain walk away from asking this
    // again. `made` and `prototype` are the same object because there is no
    // constructor value — nothing reads this name as a global.
    class_support::record(context, NAME, value, value, Some("rts-core::buffers"));
    if let Some(parent) = crate::entry::object_proto::prototype_of(context) {
        context.set_prototype(cell, Value::from_slot(parent).bits());
    }
    native::getter(context, cell, symbol::TO_STRING_TAG, to_string_tag as native::Native);
    native::getter(context, cell, "buffer", buffer_getter as native::Native);
    native::getter(context, cell, "byteLength", byte_length_getter as native::Native);
    native::getter(context, cell, "byteOffset", byte_offset_getter as native::Native);
    native::getter(context, cell, "length", length_getter as native::Native);

    // `%TypedArray%.prototype.toString` IS `Array.prototype.toString` (identity),
    // which joins through the receiver's own `join`.
    if let Some(array) = crate::entry::array_proto::prototype_of(context) {
        let key = context.well_known("toString");
        if let Some(found) = objects::read_property(context, array, key) {
            objects::put(context, cell, key, found.bits());
            native::hidden(context, cell, key);
        }
    }

    // `%TypedArray%` itself: an abstract constructor. The registry entry is
    // already rooted (made == prototype above), so the allocation below cannot
    // free the prototype; once the constructor exists the entry is updated.
    let ctor = native::callable(context, abstract_constructor as native::Native);
    native::name_of(context, ctor, "TypedArray");
    native::length_of(context, ctor, 0);
    if let Some(ctor_cell) = Value(ctor).as_slot() {
        let prototype_key = context.well_known("prototype");
        objects::put(context, ctor_cell, prototype_key, value);
        native::pinned(context, ctor_cell, prototype_key);
        if let Some(function) = crate::entry::function_proto::prototype_of(context) {
            context.set_prototype(ctor_cell, Value::from_slot(function).bits());
        }
        native::species(context, ctor);
        let constructor_key = context.well_known("constructor");
        objects::put(context, cell, constructor_key, ctor);
        native::hidden(context, cell, constructor_key);
    }
    if let Some(entry) = context.classes.iter_mut().find(|entry| entry.name == NAME) {
        entry.made = ctor;
    }
    Some(cell)
}

/// `%TypedArray%` called or constructed directly.
extern "C" fn abstract_constructor(
    _e: u64,
    _this: u64,
    _a0: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    crate::entry::throw::type_error("Abstract class TypedArray not directly constructable");
    with_current(|context| objects::undefined_of(context))
}

/// The own members of a concrete prototype that belong on `%TypedArray%.prototype`.
const SHARED_METHODS: &[&str] = &[
    "at", "copyWithin", "entries", "every", "fill", "filter", "find", "findIndex", "findLast",
    "findLastIndex", "forEach", "includes", "indexOf", "join", "keys", "lastIndexOf", "map",
    "reduce", "reduceRight", "reverse", "set", "slice", "some", "sort", "subarray", "toReversed",
    "toSorted", "toString", "values", "with", "get", "setAt", symbol::ITERATOR,
];

/// Moves one own data property from `from` to `to` (keeping the first holder's
/// value if `to` already has it) and removes it from `from`.
fn relocate(context: &mut Context, from: u32, to: u32, name: &str) {
    let key = context.well_known(name);
    let Some(machine) = objects::machine_key(key) else {
        return;
    };
    if objects::own_property(context, to, key).is_none()
        && let Some(held) = objects::own_property(context, from, key)
    {
        objects::put(context, to, key, held.bits());
        native::hidden(context, to, key);
    }
    crate::entry::computed::remove_own(context, from, machine);
}

/// Puts the shared prototype under one concrete class's prototype.
///
/// Called from each class's wrapper in [`super::typed_classes`] rather than from
/// the `#[rtse::class]` expansion, because the attribute can only name a parent
/// that is itself a declared class — see this module's header.
pub(in crate::entry) fn link(context: &mut Context, class: &'static str, made: u64) {
    let Some(concrete) =
        class_support::prototype(context, class).and_then(|value| Value(value).as_slot())
    else {
        return;
    };
    let Some(shared) = prototype_of(context) else {
        return;
    };
    context.set_prototype(concrete, Value::from_slot(shared).bits());
    for name in SHARED_METHODS {
        relocate(context, concrete, shared, name);
    }
    // The tag is an own data property on the concrete prototype only so the
    // shared accessor need not be consulted; the language has just the accessor.
    let tag = context.well_known(symbol::TO_STRING_TAG);
    if let Some(machine) = objects::machine_key(tag) {
        crate::entry::computed::remove_own(context, concrete, machine);
    }
    if let Some(ctor) = class_support::made(context, NAME).and_then(|value| Value(value).as_slot())
        && let Some(own) = Value(made).as_slot()
    {
        context.set_prototype(own, Value::from_slot(ctor).bits());
        relocate(context, own, ctor, "from");
        relocate(context, own, ctor, "of");
        let species = context.well_known(symbol::SPECIES);
        if let Some(machine) = objects::machine_key(species) {
            crate::entry::computed::remove_own(context, own, machine);
        }
    }
}

/// `get %TypedArray%.prototype[Symbol.toStringTag]`.
///
/// Answers `undefined` — never a throw — for a receiver that is not one of the
/// eight, which is measured behaviour rather than a reading of the text: that is
/// what makes the getter usable as a classification test, and it is how
/// `safe-stable-stringify` uses it. A `DataView` is the case that proves the
/// distinction is about the CLASS and not about being a view:
/// [`typed_classes::named`] answers `None` for `Kind::Raw`, which is the one
/// view kind that is not a typed array.
///
/// The borrow is held across the intern and nothing here calls user code, so
/// rule 4 has nothing to ask: `view_of` reads a side table and `named` is a
/// match over an element kind.
extern "C" fn to_string_tag(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let named = super::view_of(context, this).and_then(|view| typed_classes::named(view.kind));
        match named {
            Some(name) => context.intern_value(Str::from_str(name)).bits(),
            None => objects::undefined_of(context),
        }
    })
}

/// The four facts a typed array reports, as `get %TypedArray%.prototype.X`.
///
/// An instance here keeps them as own data slots (see [`super::attach`]) so
/// compiled code reads them without asking the runtime; the language makes them
/// accessors on this object. These getters are the language's half: they are
/// what `Object.getOwnPropertyDescriptor(%TypedArray%.prototype, "length").get`
/// answers, and what `.call(receiver)` runs. They read the receiver's own slot,
/// so a detached or resized buffer's stamped value is what comes back. A
/// receiver that is not one of the eight typed arrays is a `TypeError`, which is
/// the specification's and unlike the tag getter above.
fn reflect_fact(this: u64, name: &str) -> u64 {
    let fact = with_current(|context| {
        let cell = Value(this).as_slot()?;
        let view = context.view_at(cell)?;
        typed_classes::named(view.kind)?;
        let key = context.well_known(name);
        objects::own_property(context, cell, key).map(|held| held.bits())
    });
    match fact {
        Some(bits) => bits,
        None => {
            crate::entry::throw::type_error(&format!(
                "Receiver is not a typed array (get {name})"
            ));
            with_current(|context| objects::undefined_of(context))
        }
    }
}

extern "C" fn length_getter(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    reflect_fact(this, "length")
}
extern "C" fn byte_length_getter(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    reflect_fact(this, "byteLength")
}
extern "C" fn byte_offset_getter(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    reflect_fact(this, "byteOffset")
}
extern "C" fn buffer_getter(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    reflect_fact(this, "buffer")
}

fn dv_fact(this: u64, name: &str) -> u64 {
    // `buffer` stays readable after a detach; the two sizes throw.
    let detached = name != "buffer"
        && with_current(|context| {
            Value(this)
                .as_slot()
                .and_then(|cell| context.view_at(cell))
                .is_some_and(|view| {
                    matches!(view.kind, super::Kind::Raw) && context.buffer_detached(view.buffer)
                })
        });
    if detached {
        crate::entry::throw::type_error(&format!(
            "Cannot perform get DataView.prototype.{name} on a detached ArrayBuffer"
        ));
        return with_current(|context| objects::undefined_of(context));
    }
    let fact = with_current(|context| {
        let cell = Value(this).as_slot()?;
        let view = context.view_at(cell)?;
        if !matches!(view.kind, super::Kind::Raw) {
            return None;
        }
        let key = context.well_known(name);
        objects::own_property(context, cell, key).map(|held| held.bits())
    });
    match fact {
        Some(bits) => bits,
        None => {
            crate::entry::throw::type_error(&format!("Receiver is not a DataView (get {name})"));
            with_current(|context| objects::undefined_of(context))
        }
    }
}

extern "C" fn dv_byte_length(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    dv_fact(this, "byteLength")
}
extern "C" fn dv_byte_offset(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    dv_fact(this, "byteOffset")
}
extern "C" fn dv_buffer(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    dv_fact(this, "buffer")
}

/// `DataView.prototype`'s three accessors; see [`reflect_fact`] for why an
/// instance still keeps own slots.
pub(in crate::entry) fn dataview_accessors(context: &mut Context) {
    let Some(cell) =
        class_support::prototype(context, "DataView").and_then(|value| Value(value).as_slot())
    else {
        return;
    };
    native::getter(context, cell, "buffer", dv_buffer as native::Native);
    native::getter(context, cell, "byteLength", dv_byte_length as native::Native);
    native::getter(context, cell, "byteOffset", dv_byte_offset as native::Native);
}
