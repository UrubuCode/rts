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
/// Because `objects::put` has none. It refuses silently — a frozen object, a
/// non-writable property, and a new property on a closed one all return without
/// writing and without saying so — which is exactly right for `o.x = 1`, an
/// expression whose value is `1` either way, and leaves `Reflect.set` and a
/// proxy's forwarded write with nothing to report.
///
/// So the two refusals are asked here, from [`super::integrity`] — the one place
/// that records them, so this composes facts rather than restating them. It
/// belongs beside `put`, and moves there the day that function answers a
/// verdict; until then this is the only spelling, which is what keeps
/// `Reflect.set` and `proxy::set_verdict` from growing one each.
pub(in crate::entry) fn write_lands(context: &mut Context, cell: u32, key: Key) -> bool {
    if let Some(named) = super::objects::machine_key(key)
        && super::integrity::refuses_key_write(context, cell, named)
    {
        return false;
    }
    if !super::integrity::refuses_growth(context, cell) {
        return true;
    }
    // A closed object still accepts a write to something it already has, and one
    // through a setter it inherits — `preventExtensions` refuses new properties
    // and nothing else.
    super::objects::own_property(context, cell, key).is_some()
        || super::accessor::setter_for(context, cell, key).is_some()
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
        super::computed::set_indexed(target, key, value);
        lands
    }

    /// `Reflect.has(target, key)` — the same question `key in target` asks.
    fn has(target: u64, key: u64) -> bool {
        super::computed::has_property(key, target)
    }

    /// `Reflect.deleteProperty(target, key)`.
    fn delete_property(target: u64, key: u64) -> bool {
        super::computed::delete_property(target, key)
    }

    /// `Reflect.ownKeys(target)`.
    ///
    /// The same enumeration `Object.keys` and `for-in` walk, so the three cannot
    /// disagree about order — which is the property array-index-first ordering
    /// exists to guarantee.
    fn own_keys(target: u64) -> u64 {
        // EVERY own key, not the enumerable ones: `Reflect.ownKeys` is
        // `[[OwnPropertyKeys]]`, and `Object.keys` is the filtered spelling.
        // Answering the filtered list here made a property defined with
        // `{ value: 1 }` — which is not enumerable — invisible to the operation
        // whose whole job is to see everything.
        super::array::own_names(target)
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
    fn apply(target: u64, receiver: u64, arguments: u64) -> u64 {
        super::functions::call_with_args(target, receiver, arguments)
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
        let produced = super::functions::construct_with_args(target, arguments);
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
