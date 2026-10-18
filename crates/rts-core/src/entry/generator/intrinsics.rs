//! `%GeneratorFunction%`, `%AsyncFunction%`, `%AsyncGeneratorFunction%` and the
//! three `*.prototype` objects between them and the functions they describe.
//!
//! # What was asked of this, and what already answered it
//!
//! "What does this function inherit from" is `closure_new`'s question, and it
//! left every function on `Function.prototype`. The shape to copy is
//! `buffers::typed_abstract` (`%TypedArray%`): an intrinsic nobody can name as a
//! global, built lazily, recorded in `classes` under a name no identifier can
//! spell so the collector's existing walk over `Registered` roots it.
//!
//! # Shape (checked against Node 25, 2026-10-18)
//!
//! ```text
//! g                    -> %GeneratorFunction.prototype%   (a plain object, proto Function.prototype)
//!   .constructor       -> %GeneratorFunction%             {writable: false, configurable: true}
//!   [@@toStringTag]    -> "GeneratorFunction"             {writable: false, configurable: true}
//!   .prototype         -> %Generator%                     {writable: false, configurable: true}
//! %Generator%.constructor -> %GeneratorFunction.prototype% {writable: false, configurable: true}
//! %GeneratorFunction%  proto = Function, name, length 1, .prototype pinned
//! ```
//!
//! An async function has no `.prototype` link (there is no async "Generator"
//! prototype), the other two follow the table above with `%AsyncGenerator%`.
//!
//! # What this does NOT do
//!
//! Calling or constructing the constructors throws: it would have to compile
//! source text at run time, which `Function(...)` cannot do here either.

use super::super::{class_support, native, objects, symbol};
use super::Context;
use crate::value::Value;

/// Which of the three a closure is, as the emitter recorded it in the top bits
/// of the arity (see `emit/function.rs`): 1 generator, 2 async, 3 async generator.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::entry) enum Kind {
    Generator,
    Async,
    AsyncGenerator,
}

/// Mask of the arity bits that are `f.length`; the rest carry the kind.
pub(in crate::entry) const ARITY_MASK: u32 = 0x0FFF_FFFF;

impl Kind {
    pub(in crate::entry) fn of_arity(arity: u32) -> Option<Kind> {
        match arity >> 28 {
            1 => Some(Kind::Generator),
            2 => Some(Kind::Async),
            3 => Some(Kind::AsyncGenerator),
            _ => None,
        }
    }

    /// The registry name of the prototype object: not spellable by a program.
    fn registry(self) -> &'static str {
        match self {
            Kind::Generator => "%GeneratorFunction.prototype%",
            Kind::Async => "%AsyncFunction.prototype%",
            Kind::AsyncGenerator => "%AsyncGeneratorFunction.prototype%",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Kind::Generator => "GeneratorFunction",
            Kind::Async => "AsyncFunction",
            Kind::AsyncGenerator => "AsyncGeneratorFunction",
        }
    }

    fn native(self) -> native::Native {
        match self {
            Kind::Generator => generator_function as native::Native,
            Kind::Async => async_function as native::Native,
            Kind::AsyncGenerator => async_generator_function as native::Native,
        }
    }
}

/// Gives a freshly made closure the `[[Prototype]]` its kind calls for.
pub(in crate::entry) fn link_function(context: &mut Context, function: u32, kind: Kind) {
    if let Some(prototype) = prototype_of(context, kind) {
        context.set_prototype(function, Value::from_slot(prototype).bits());
    }
}

/// The `*.prototype` intrinsic for `kind`, made once.
fn prototype_of(context: &mut Context, kind: Kind) -> Option<u32> {
    let name = kind.registry();
    if let Some(found) =
        class_support::prototype(context, name).and_then(|value| Value(value).as_slot())
    {
        return Some(found);
    }
    let cell = native::plain(context)?;
    let value = Value::from_slot(cell).bits();
    // Recorded BEFORE anything else allocates (`class_support::record`'s own
    // rule); `made` is the prototype until the constructor exists.
    class_support::record(context, name, value, value, Some("rts-core::generator"));
    if let Some(function) = crate::entry::function_proto::prototype_of(context) {
        context.set_prototype(cell, Value::from_slot(function).bits());
    }

    let ctor = native::callable(context, kind.native());
    if let Some(entry) = context.classes.iter_mut().find(|entry| entry.name == name) {
        entry.made = ctor;
    }
    // `length` before `name`: the order Node lists them.
    native::length_of(context, ctor, 1);
    native::name_of(context, ctor, kind.label());
    let Some(ctor_cell) = Value(ctor).as_slot() else {
        return Some(cell);
    };
    let prototype_key = context.well_known("prototype");
    objects::put(context, ctor_cell, prototype_key, value);
    native::pinned(context, ctor_cell, prototype_key);
    // `Object.getPrototypeOf(%GeneratorFunction%) === Function`.
    if let Some(function) = class_support::made(context, "Function") {
        context.set_prototype(ctor_cell, function);
    }

    let constructor_key = context.well_known("constructor");

    // `%GeneratorFunction.prototype%.prototype` is `%Generator%`, and
    // `%Generator%.constructor` points back. Written first: Node lists
    // `prototype` before `constructor`.
    let generator = match kind {
        Kind::Generator => super::generator_prototype(context),
        Kind::AsyncGenerator => super::async_generator_prototype(context),
        Kind::Async => None,
    };
    if let Some(generator) = generator {
        objects::put(context, cell, prototype_key, generator);
        native::tagged(context, cell, prototype_key);
        if let Some(generator_cell) = Value(generator).as_slot() {
            objects::put(context, generator_cell, constructor_key, value);
            native::tagged(context, generator_cell, constructor_key);
        }
    }

    objects::put(context, cell, constructor_key, ctor);
    native::tagged(context, cell, constructor_key);

    let tag_key = context.well_known(symbol::TO_STRING_TAG);
    let tag = context
        .intern_value(crate::text::Str::from_str(kind.label()))
        .bits();
    objects::put(context, cell, tag_key, tag);
    native::tagged(context, cell, tag_key);
    Some(cell)
}

fn refuse(label: &str) -> u64 {
    crate::entry::throw::type_error(&format!(
        "{label} constructor is not supported: this engine cannot compile source text at run time"
    ));
    crate::entry::with_current(|context| objects::undefined_of(context))
}

extern "C" fn generator_function(_e: u64, _t: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    refuse("GeneratorFunction")
}

extern "C" fn async_function(_e: u64, _t: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    refuse("AsyncFunction")
}

extern "C" fn async_generator_function(_e: u64, _t: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    refuse("AsyncGeneratorFunction")
}
