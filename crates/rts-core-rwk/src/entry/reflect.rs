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
//! # What is deliberately absent
//!
//! `Reflect.getOwnPropertyDescriptor` and `Reflect.defineProperty`'s full
//! descriptor. A descriptor is an object with six fields and four of them —
//! `writable`, `enumerable`, `configurable`, and own-versus-inherited — are facts
//! the shape tree does not record, which is the same gap
//! [`super::object_global`] names for `Object.defineProperty`. Answering a
//! descriptor with three of six fields invented would be worse than not
//! answering: a program branching on `writable` would branch on a guess.
//!
//! `Reflect.construct(target, args, newTarget)` ignores its third argument. The
//! target stack carries one class per construction and taking a different
//! prototype from a fourth value is a capability, not a wrapper.

use super::with_current;


/// `Reflect`.
#[rtse::class("Reflect", namespace)]
impl Reflect {
    /// `Reflect.get(target, key)` — the same read `target[key]` performs.
    fn get(target: u64, key: u64) -> u64 {
        super::computed::get_indexed(target, key)
    }

    /// `Reflect.set(target, key, value)` — answers whether it was written.
    ///
    /// True whenever the write reached an object, which is what this engine can
    /// establish: a refusal is a non-writable property or a frozen object, and
    /// neither is recorded. Named rather than answered as a guess.
    fn set(target: u64, key: u64, value: u64) -> bool {
        super::computed::set_indexed(target, key, value);
        crate::value::Value(target).as_slot().is_some()
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
    fn set_prototype_of(target: u64, prototype: u64) -> bool {
        // A proxy answers with its handler's own verdict: a `setPrototypeOf`
        // trap returning `false` is a refusal, and reporting `true` because the
        // call reached an object would be this function inventing the answer.
        if let Some(answered) = super::proxy::set_prototype_verdict(target, prototype) {
            return answered;
        }
        super::chain::set_prototype(target, prototype);
        crate::value::Value(target).as_slot().is_some()
    }

    /// `Reflect.apply(target, thisArgument, argumentList)`.
    ///
    /// The vector spelling, not the four-slot one: a list is what the caller
    /// has, and `call_with_args` is the path that carries any number.
    fn apply(target: u64, receiver: u64, arguments: u64) -> u64 {
        super::functions::call_with_args(target, receiver, arguments)
    }

    /// `Reflect.construct(target, argumentList)`.
    fn construct(target: u64, arguments: u64) -> u64 {
        super::functions::construct_with_args(target, arguments)
    }

    /// `Reflect.defineProperty(target, key, descriptor)`.
    ///
    /// Answers whether it was accepted, which is what a proxy handler returning
    /// `false` says and what distinguishes this from `Object.defineProperty` —
    /// that one answers the object.
    fn define_property(target: u64, key: u64, descriptor: u64) -> bool {
        if let Some(named) = with_current(|context| {
            super::computed::property_key(context, crate::value::Value(key))
        })
            && let Some(answered) = super::proxy::define(target, named, descriptor)
        {
            return answered;
        }
        super::object_global::define(target, key, descriptor);
        crate::value::Value(target).as_slot().is_some()
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
    /// Always true for an object, because nothing here can make one otherwise:
    /// `Object.preventExtensions` is not built, and a flag no operation reads
    /// would be a property this answers about and nothing enforces.
    fn is_extensible(target: u64) -> bool {
        crate::value::Value(target).as_slot().is_some()
    }
}
