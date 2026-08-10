//! `Object.prototype` — what every plain object inherits from.
//!
//! # Why this is a namespace rather than a class
//!
//! `#[rtse::class("X")]` makes a constructor, a prototype, and the `prototype`
//! and `constructor` link between them. `Object` already has a constructor —
//! [`super::object_global::constructor`], with the six statics on it — so
//! declaring one here would make a **second** `Object` callable, and the global
//! name would read whichever registration ran first. The namespace flavour makes
//! exactly one object, installs the members on it, and records it as its own
//! prototype — the shape this needs: an object other things inherit from, whose
//! `constructor` property is wired from the other side, in
//! [`super::object_global::constructor`], once `Object` itself exists to point
//! it at.
//!
//! The registry name is therefore `"Object.prototype"` and not `"Object"` —
//! deliberately a string no `#[rtse::class]` can declare, so the day `Object`
//! becomes a declared class the two cannot collide silently.
//!
//! # Why the members are the attribute rather than a `NATIVES` list
//!
//! Every one of them is receiver-plus-at-most-one-argument answering a `bool` or
//! a value, which is exactly the wrapper the attribute writes — and the wrapper
//! it writes takes each argument's borrow separately and drops it before the
//! body runs. Writing them by hand as `extern "C"` would be six copies of that
//! discipline, and `toLocaleString` calls user code, so it is the one member
//! where getting it wrong aborts the process rather than answering wrong.
//!
//! # What is NOT here, and why each is absent rather than guessed
//!
//! `__proto__` — an accessor pair on this object, which is a real thing to
//! install and a different mechanism from a method; `Object.getPrototypeOf`
//! already answers the question.
//!
//! `__defineGetter__` and friends — annex-B spellings of what
//! `Object.defineProperty` does.

use super::objects::undefined_of;
use super::{Context, with_current};
use crate::object::Key;
use crate::text::Str;
use crate::value::Value;

/// The methods a plain object inherits.
#[rtse::class("Object.prototype", namespace)]
impl ObjectPrototype {
    /// `o.hasOwnProperty(k)` — the object's own properties, with no walk.
    ///
    /// Own-ness is the entire question, so this asks [`super::objects::own_property`]
    /// rather than [`super::objects::read_property`]: the second walks the chain,
    /// which would make `({}).hasOwnProperty("toString")` answer true and invert
    /// the one distinction the method exists for.
    ///
    /// An accessor is asked for separately because it is not in the layout —
    /// `accessor_at` is where a getter defined on this cell lives, and a version
    /// that only consulted the shape would answer false for a property
    /// `Object.defineProperty` had just defined.
    fn has_own_property(this: u64, key: u64) -> bool {
        owns(this, key)
    }

    /// `p.isPrototypeOf(v)` — whether the receiver is anywhere in what `v`
    /// inherits from.
    ///
    /// Bounded by [`super::objects::CHAIN_LIMIT`], for the reason the property
    /// walk is: a prototype is a value a program can set, and a walk that
    /// trusted it would hang on a cycle rather than answer — which is worse than
    /// a wrong answer, because nothing reports it.
    ///
    /// The step is [`super::objects::inherited_from`] rather than `prototype_at`,
    /// which is the one place this differs from `instanceof`: a string, an array
    /// and a callable have no stored link, and asking only for the stored one
    /// would make `Array.prototype.isPrototypeOf([])` answer false about a chain
    /// every property read walks.
    ///
    /// The value itself is not compared. `p.isPrototypeOf(p)` is false — the
    /// receiver has to be a *proper* ancestor, which is why the first thing the
    /// loop does is step.
    fn is_prototype_of(this: u64, value: u64) -> bool {
        with_current(|context| {
            let (Some(receiver), Some(mut cell)) = (Value(this).as_slot(), Value(value).as_slot())
            else {
                return false;
            };
            for _ in 0..super::objects::CHAIN_LIMIT {
                let Some(next) = super::objects::inherited_from(context, cell) else {
                    return false;
                };
                if next == receiver {
                    return true;
                }
                cell = next;
            }
            false
        })
    }

    /// `o.propertyIsEnumerable(k)`.
    ///
    /// Own-ness first — an inherited or absent key is `false` before the
    /// `enumerable` flag is even asked about, same as `hasOwnProperty` — and
    /// then the flag [`super::integrity`] tracks: **not** every own property is
    /// enumerable. `set_length` marks an array's `"length"` non-enumerable at
    /// the one place it is written, and `Object.defineProperty` can mark any
    /// property so, both recorded in the same side table `owns` never
    /// consults. Answering plain own-ness here — as this used to — made
    /// `[1,2,3].propertyIsEnumerable("length")` answer `true`, which is `for-in`
    /// and `Object.keys` disagreeing with the one method whose entire job is to
    /// ask them the question directly.
    fn property_is_enumerable(this: u64, key: u64) -> bool {
        with_current(|context| {
            let Some(cell) = Value(this).as_slot() else {
                return false;
            };
            // An array element: own storage with no shape key, and this engine
            // never marks one non-enumerable, so presence is the whole answer.
            if let Some(at) = super::array::as_index(context, Value(key))
                && let Some(elements) = context.elements_at(cell)
            {
                return elements
                    .get(at)
                    .is_some_and(|&held| !super::array::is_hole(context, held));
            }
            let Some(key) = own_key(context, key) else {
                return false;
            };
            if super::objects::own_property(context, cell, key).is_some() {
                return match key {
                    Key::Name(machine) => super::integrity::enumerable(context, cell, machine),
                    // A symbol key has no attribute table entry today, so it
                    // carries the default every property starts with.
                    _ => true,
                };
            }
            let Key::Name(machine) = key else {
                return false;
            };
            // An accessor `Object.defineProperty` installed: not in the shape,
            // so `own_property` missed it, but it is still an own key.
            context.accessor_at(cell, machine.index() as u32).is_some()
        })
    }

    /// `o.toString()` — `"[object " + tag + "]"`, the specification's own
    /// dispatch on what `this` IS rather than a fixed string.
    ///
    /// It answered `"[object Object]"` unconditionally, so
    /// `Object.prototype.toString.call([1,2])` — the idiom a real program uses
    /// to ask "what is this, really" past a `toString` an intervening
    /// prototype may have overridden — was wrong for everything but a plain
    /// object.
    ///
    /// `Symbol.toStringTag` is not read: there are no symbols in this engine
    /// yet, so there is no property a class could have put one on, and the
    /// fallback the specification defines for its absence is exactly the
    /// per-kind table below.
    fn to_string(this: u64) -> u64 {
        with_current(|context| {
            let tag = object_tag(context, this);
            context
                .intern_value(Str::from_str(&format!("[object {tag}]")))
                .bits()
        })
    }

    /// `o.valueOf()` — the receiver, unchanged.
    ///
    /// The identity that makes `ToPrimitive` fall through to `toString` for an
    /// ordinary object: `valueOf` answering an object is what the protocol reads
    /// as "not a primitive, try the other one".
    fn value_of(this: u64) -> u64 {
        this
    }

    /// `o.toLocaleString()` — the receiver's own `toString`, called.
    ///
    /// Delegating rather than duplicating, because that is what the
    /// specification says this method *is*: it exists so a class overriding
    /// `toString` gets a locale form for free. Answering `"[object Object]"`
    /// directly would be the same text today and would stop being the same the
    /// first time anything overrode `toString`.
    ///
    /// # Why this is three statements
    ///
    /// The lookup is user-visible — the receiver's `toString` may be a getter,
    /// and the method itself is user code whose first act may be to call the
    /// runtime. Both go through entry points that take their own borrow, so
    /// neither may run inside one: the name is interned in a borrow that ends,
    /// then the read, then the call.
    fn to_locale_string(this: u64) -> u64 {
        let (name, absent) = with_current(|context| {
            (
                context.intern_value(Str::from_str("toString")).bits(),
                undefined_of(context),
            )
        });
        let method = super::computed::get_indexed(this, name);
        if method == absent {
            return absent;
        }
        super::functions::call(method, this, absent, absent, absent, absent)
    }
}

/// What every plain object inherits from, made once.
///
/// Lazily, like every other built-in prototype here: a program that never reads
/// an inherited member should not spend the cells for six methods.
///
/// The recording happens inside `register_object_prototype`, and it happens
/// **before** the members are installed — installing interns names, interning
/// allocates, and an allocation is one chain walk away from asking this function
/// again. That order is the attribute's, not this function's, which is the point
/// of the trap living in the expansion: the first version of
/// `string::prototype_of` got it wrong and recursed until the region ran out.
pub(super) fn prototype_of(context: &mut Context) -> Option<u32> {
    if super::class_support::made(context, "Object.prototype").is_none() {
        register_object_prototype(context);
    }
    Value(super::class_support::prototype(context, "Object.prototype")?).as_slot()
}

/// What `Object.prototype.toString` answers between `[object ` and `]`.
///
/// The specification's own table, read off what `this` already tells the
/// runtime rather than a class name nothing here stores: an array is
/// answered by the side table [`super::array_proto`] keys on, a callable by
/// the one [`super::function_proto`] keys on, a boxed primitive by the table
/// the wrapper-object work put beside the cell, and `Date` by the property
/// its own module already uses in place of an internal slot — see
/// [`super::date`]'s module documentation for why that property exists.
fn object_tag(context: &mut Context, this: u64) -> &'static str {
    if this == Value::from_singleton(context.singletons.undefined).bits() {
        return "Undefined";
    }
    if this == Value::from_singleton(context.singletons.null).bits() {
        return "Null";
    }
    if Value(this)
        .as_slot()
        .is_some_and(|cell| context.elements_at(cell).is_some())
    {
        return "Array";
    }
    if Value(this)
        .as_slot()
        .is_some_and(|cell| context.callable_at(cell).is_some())
    {
        return "Function";
    }
    if Value(this).as_bool().is_some() {
        return "Boolean";
    }
    if Value(this).numeric().is_some() {
        return "Number";
    }
    let Some(cell) = Value(this).as_slot() else {
        return "Object";
    };
    if context.text_at(cell).is_some() {
        return "String";
    }
    if context.regexp_at(cell).is_some() {
        return "RegExp";
    }
    let time_key = Key::Name(
        context
            .interner
            .intern(&Str::from_str(super::date::TIME), &mut context.keys),
    );
    if super::objects::own_property(context, cell, time_key).is_some() {
        return "Date";
    }
    if extends_class(context, cell, "Error") {
        return "Error";
    }
    if let Some(boxed) = context.boxed_at(cell) {
        if Value(boxed).as_bool().is_some() {
            return "Boolean";
        }
        if Value(boxed).numeric().is_some() {
            return "Number";
        }
        return "String";
    }
    "Object"
}

/// Whether `cell`'s prototype chain — including itself — reaches the
/// prototype a registered class recorded under `name`.
///
/// Written for `Error`: `new TypeError()`'s own link is `TypeError.prototype`,
/// whose link is `Error.prototype`, and the specification tags every one of
/// its subclasses `"Error"` rather than by the subclass's own name.
fn extends_class(context: &mut Context, mut cell: u32, name: &str) -> bool {
    let Some(target) = super::class_support::prototype(context, name).and_then(|value| Value(value).as_slot())
    else {
        return false;
    };
    for _ in 0..super::objects::CHAIN_LIMIT {
        if cell == target {
            return true;
        }
        let Some(next) = super::objects::inherited_from(context, cell) else {
            return false;
        };
        cell = next;
    }
    false
}

/// Whether a cell has a key of its own, in every storage a cell has one in.
///
/// Written once because two members ask it and they must not drift: the whole
/// content of `propertyIsEnumerable`'s documentation is that it answers the same
/// as `hasOwnProperty` here, and two bodies is where that stops being true.
fn owns(this: u64, key: u64) -> bool {
    with_current(|context| {
        let Some(cell) = Value(this).as_slot() else {
            return false;
        };
        // An array element is own storage that no shape records, so the index
        // question is asked before the key is turned into text —
        // `ToPropertyKey` would turn `0` into `"0"` and lose it.
        if let Some(at) = super::array::as_index(context, Value(key))
            && let Some(elements) = context.elements_at(cell)
        {
            // O par exato do teste em `computed.rs` — o comentário lá diz que os
            // dois têm de responder o mesmo, e um buraco não é uma propriedade
            // própria.
            return elements
                .get(at)
                .is_some_and(|&held| !super::array::is_hole(context, held));
        }
        let Some(key) = own_key(context, key) else {
            return false;
        };
        if super::objects::own_property(context, cell, key).is_some() {
            return true;
        }
        let Key::Name(machine) = key else {
            return false;
        };
        context.accessor_at(cell, machine.index() as u32).is_some()
    })
}

/// The key a value names, for the two members that take one.
///
/// `None` for an object, whose `ToPropertyKey` runs a `toString` — user code
/// this borrow cannot call. `hasOwnProperty({})` therefore answers false, which
/// is the same stated boundary every coercion in this crate stops at.
fn own_key(context: &mut Context, key: u64) -> Option<Key> {
    let text = super::text::to_text(context, Value(key))?;
    Some(Key::Name(context.interner.intern(&text, &mut context.keys)))
}
