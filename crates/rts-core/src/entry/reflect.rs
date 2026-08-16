//! `Reflect` — the operations the language performs, as callable functions.
//!
//! # Why this is thin on purpose
//!
//! Every member here is one of the runtime's own entry points with a JavaScript
//! name in front of it. That is what `Reflect` **is**: ECMA-262 defines each
//! method as the internal method the corresponding syntax invokes, so a second
//! implementation would be a second answer to a question the engine already
//! answers — and the two would disagree the first time one of them was fixed.
//!
//! `Reflect.get(o, k)` and `o[k]` therefore reach the same function, and a
//! getter runs in both because the path is shared rather than duplicated.
//!
//! # Where the verdicts come from
//!
//! Four members answer a boolean rather than a value, and three of them used to
//! invent it: `set`, `isExtensible` and `preventExtensions` each answered `true`
//! for any object, on the stated grounds that nothing recorded a refusal. That
//! stopped being true when [`super::integrity`] began recording one — an
//! object's level and a property's three attributes — and an invented `true`
//! beside a recorded `false` is the two-answers-to-one-question this crate
//! keeps refusing. Every verdict here is now read from what that module holds,
//! or from a proxy handler's own answer.
//!
//! # What is deliberately incomplete
//!
//! `Reflect.set(target, key, value, receiver)` ignores its fourth argument, and
//! `Reflect.get(target, key, receiver)` its third. A receiver distinct from the
//! target changes which object a setter's `this` is, which is a capability of
//! the property path rather than of this wrapper.
//!
//! `Reflect.construct(target, args, newTarget)` reads its third argument for
//! the PROTOTYPE and not for `new.target` — see [`Reflect::construct`] for
//! exactly which half is missing and where the rest of it belongs.

use super::{Context, with_current};
use crate::object::Key;
use crate::value::Value;

/// Whether a store to this key of this cell would land.
///
/// # Why this is a second function rather than `put`'s answer
///
/// It no longer is one. This asked [`super::integrity`] itself while `put`
/// refused silently, and said so — "it belongs beside `put`, and moves there the
/// day that function answers a verdict". That day arrived with the strict-mode
/// throw: [`super::objects::resolve_store`] decides every refusal a write can
/// meet, so this reads the verdict rather than re-deriving it.
///
/// What stays here is the DIFFERENCE between the two callers. `o.x = 1` throws
/// on a refusal because a module is strict; `Reflect.set` and a proxy's
/// forwarded write answer `false` and throw nothing, which is why they ask for a
/// verdict instead of performing the write and being unwound by it.
pub(in crate::entry) fn write_lands(context: &mut Context, cell: u32, key: Key) -> bool {
    !matches!(
        super::objects::resolve_store(context, cell, key),
        super::objects::Store::Refused(_)
    )
}

/// An array's elements, the borrow ending here.
///
/// The array itself stays named by a local of the caller's frame across the
/// allocation that follows, so nothing is ever reachable only from this vector —
/// which the conservative stack scan could not see.
fn elements_of(array: u64) -> Vec<u64> {
    with_current(|context| {
        Value(array)
            .as_slot()
            .and_then(|cell| context.elements_at(cell))
            .cloned()
            .unwrap_or_default()
    })
}

/// `Reflect`.
#[rtse::class("Reflect", namespace, tag)]
impl Reflect {
    /// `Reflect.get(target, key)` — the same read `target[key]` performs.
    fn get(target: u64, key: u64) -> u64 {
        super::computed::get_indexed(target, key)
    }

    /// `Reflect.set(target, key, value)` — answers whether it was written.
    ///
    /// A proxy answers with its handler's own `set` verdict, and an ordinary
    /// object with whether anything refused the store: `Object.freeze` and
    /// `writable: false` are both recorded, and both mean `false` here.
    fn set(target: u64, key: u64, value: u64) -> bool {
        if let Some(named) =
            with_current(|context| super::computed::property_key(context, Value(key)))
            && let Some(answered) = super::proxy::set_verdict(target, named, value)
        {
            return answered;
        }
        let lands = with_current(|context| {
            let (Some(cell), Some(named)) = (
                Value(target).as_slot(),
                super::computed::property_key(context, Value(key)),
            ) else {
                return false;
            };
            write_lands(context, cell, named)
        });
        // Only when it lands. The write path THROWS on a refusal now — a module
        // is strict — and `Reflect.set` is the operation whose entire purpose is
        // to answer that question instead of raising it, so performing the write
        // to find out would end the program it was asked to inform.
        if lands {
            super::computed::set_indexed(target, key, value);
        }
        lands
    }

    /// `Reflect.has(target, key)` — the same question `key in target` asks.
    fn has(target: u64, key: u64) -> bool {
        super::computed::has_property(key, target)
    }

    /// `Reflect.deleteProperty(target, key)`.
    ///
    /// The quiet spelling: `delete o.x` raises on a refusal because a module is
    /// strict, and this is the operation whose whole purpose is to report the
    /// refusal instead — the same split `set` above already makes.
    fn delete_property(target: u64, key: u64) -> bool {
        super::computed::delete_own(target, key)
    }

    /// `Reflect.ownKeys(target)`.
    ///
    /// The same enumeration `Object.keys` and `for-in` walk, so the three cannot
    /// disagree about order — which is the property array-index-first ordering
    /// exists to guarantee.
    ///
    /// # Why the symbols are a second list rather than a second walk
    ///
    /// `[[OwnPropertyKeys]]` is three groups in one order: integer indices in
    /// numeric order, then strings in insertion order, then **symbols** in
    /// insertion order. `own_names` answers the first two — its keys are TEXT,
    /// and a symbol's key text is this crate's own encoding rather than a name
    /// a program can spell (see `super::symbol`), so it deliberately filters
    /// them out. `Object.getOwnPropertySymbols` already undoes that encoding
    /// for the third group, and asking it here is what keeps one answer to
    /// "which of an object's keys are symbols" instead of two.
    ///
    /// Without this the operation whose whole job is to see everything was the
    /// one operation that could not see a `[sym]: v` property at all.
    fn own_keys(target: u64) -> u64 {
        // EVERY own key, not the enumerable ones: `Reflect.ownKeys` is
        // `[[OwnPropertyKeys]]`, and `Object.keys` is the filtered spelling.
        // Answering the filtered list here made a property defined with
        // `{ value: 1 }` — which is not enumerable — invisible to the operation
        // whose whole job is to see everything.
        let named = super::array::own_names(target);
        // A proxy has already answered through its handler, and its `ownKeys`
        // trap reports symbols itself — the cell standing for it holds no
        // properties of its own, so this contributes nothing there rather than
        // needing to be asked about.
        let symbols = super::object_global::own_symbols(target);
        let mut keys = elements_of(named);
        keys.extend(elements_of(symbols));
        super::array_proto::built(keys)
    }

    /// `Reflect.getPrototypeOf(target)`.
    fn get_prototype_of(target: u64) -> u64 {
        super::chain::get_prototype(target)
    }

    /// `Reflect.setPrototypeOf(target, prototype)`.
    ///
    /// The verdict `Object.setPrototypeOf` throws on: a cycle, and a target that
    /// refuses to grow. `super::chain::apply_prototype` is the one place either
    /// is decided, and this is the spelling that reports rather than raises.
    fn set_prototype_of(target: u64, prototype: u64) -> bool {
        super::chain::apply_prototype(target, prototype)
    }

    /// `Reflect.apply(target, thisArgument, argumentList)`.
    ///
    /// The vector spelling, not the four-slot one: a list is what the caller
    /// has, and `call_with_args` is the path that carries any number.
    ///
    /// The list is array-LIKE, and unlike `Function.prototype.apply` there is
    /// no spelling of it that means "no arguments": `Reflect.apply(f, o)` is a
    /// `TypeError` where `f.apply(o)` is a call with none.
    fn apply(target: u64, receiver: u64, arguments: u64) -> u64 {
        let Some(list) = super::functions::list_from_array_like(arguments) else {
            super::throw::type_error("CreateListFromArrayLike called on non-object");
            return with_current(|context| super::objects::undefined_of(context));
        };
        super::functions::call_with_args(target, receiver, list)
    }

    /// `Reflect.construct(target, argumentList, newTarget)`.
    ///
    /// # What the third argument does here, and what it does not
    ///
    /// It decides what the produced object INHERITS FROM, which is what a
    /// program reaches for it for: `Reflect.construct(Base, [x], Derived)`
    /// answers something `instanceof Derived`. It does **not** decide what
    /// `new.target` reads inside `target`'s body, which stays `target`.
    ///
    /// The two are one act in the specification and two here, because the object
    /// is allocated by `functions::construct_with_args` from the target stack it
    /// pushes itself — so a different prototype can only be applied after the
    /// fact. Making them one again means a `newTarget` parameter on that entry
    /// point, which is where the whole of `[[Construct]]` already lives; this
    /// wrapper cannot spell it without writing a second `[[Construct]]` beside
    /// it, and a second one is how the two come to disagree about what a derived
    /// constructor allocates.
    ///
    /// The divergence, named: a constructor that branches on `new.target`
    /// branches on the wrong value. Answering the wrong PROTOTYPE — which is
    /// what dropping the argument did — is the larger of the two wrongs, because
    /// every `instanceof` on the result reads it.
    fn construct(target: u64, arguments: u64, new_target: u64) -> u64 {
        let Some(list) = super::functions::list_from_array_like(arguments) else {
            super::throw::type_error("CreateListFromArrayLike called on non-object");
            return with_current(|context| super::objects::undefined_of(context));
        };
        let produced = super::functions::construct_with_args(target, list);
        // Rule 8 of this crate's README: the constructor is user code, and a
        // relink applied to the `undefined` a throw leaves behind would be work
        // done under an exception that is already on its way out.
        if super::throw::in_flight() {
            return produced;
        }
        if new_target == target || Value(new_target).as_slot().is_none() {
            return produced;
        }
        let key = with_current(|context| context.well_known_text("prototype"));
        let prototype = super::computed::get_indexed(new_target, key);
        if Value(prototype).as_slot().is_some() {
            super::chain::apply_prototype(produced, prototype);
        }
        produced
    }

    /// `Reflect.defineProperty(target, key, descriptor)`.
    ///
    /// Answers whether it was accepted, which is what a proxy handler returning
    /// `false` says and what distinguishes this from `Object.defineProperty` —
    /// that one answers the object.
    fn define_property(target: u64, key: u64, descriptor: u64) -> bool {
        if let Some(named) =
            with_current(|context| super::computed::property_key(context, Value(key)))
            && let Some(answered) = super::proxy::define(target, named, descriptor)
        {
            return answered;
        }
        super::object_global::define(target, key, descriptor);
        Value(target).as_slot().is_some()
    }

    /// `Reflect.getOwnPropertyDescriptor(target, key)`.
    ///
    /// The descriptor this engine can state, which `super::object_global` says
    /// the limits of — and a proxy handler's own answer when it traps the
    /// question, since that one is whatever the handler returns.
    fn get_own_property_descriptor(target: u64, key: u64) -> u64 {
        super::object_global::describe_of(target, key)
    }

    /// `Reflect.isExtensible(target)`.
    ///
    /// What `super::integrity` recorded, which is the same answer
    /// `Object.isExtensible` gives — the two are one question and must not be
    /// two. A proxy asks its handler, whose answer the specification pins to the
    /// target's own.
    fn is_extensible(target: u64) -> bool {
        if let Some(answered) = super::proxy::extensible(target) {
            return answered;
        }
        with_current(|context| {
            Value(target)
                .as_slot()
                .is_some_and(|cell| context.integrity_at(cell).is_none())
        })
    }

    /// `Reflect.preventExtensions(target)`.
    ///
    /// The real thing, through the module that makes it stick: a closed object
    /// refuses new properties at the store resolver, not only at the slow path.
    /// This answered `true` without doing anything for as long as nothing here
    /// could close an object — which was a promise no write honoured, and
    /// `Object.preventExtensions` has honoured it since `integrity` landed.
    fn prevent_extensions(target: u64) -> bool {
        if let Some(answered) = super::proxy::prevent_extensions(target) {
            return answered;
        }
        super::integrity::restrict(target, super::integrity::Integrity::Closed);
        Value(target).as_slot().is_some()
    }
}
