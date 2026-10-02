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
    // Two slots: the tag accessor, and room for `constructor` should the
    // remaining divergence below ever be closed.
    let cell = native::plain_with_room(context, 2)?;
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
    Some(cell)
}

/// Puts the shared prototype under one concrete class's prototype.
///
/// Called from each class's wrapper in [`super::typed_classes`] rather than from
/// the `#[rtse::class]` expansion, because the attribute can only name a parent
/// that is itself a declared class — see this module's header.
pub(in crate::entry) fn link(context: &mut Context, class: &'static str) {
    let Some(concrete) =
        class_support::prototype(context, class).and_then(|value| Value(value).as_slot())
    else {
        return;
    };
    let Some(shared) = prototype_of(context) else {
        return;
    };
    context.set_prototype(concrete, Value::from_slot(shared).bits());
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
