//! `Object`, and the static methods a program reaches it for.
//!
//! # Why these are here and not on a prototype
//!
//! `Object.keys(o)` is a static method, not an instance one — the language put
//! them on the constructor precisely so they keep working on an object whose
//! own prototype was replaced or removed. So they hang off the constructor,
//! which is an ordinary object with ordinary properties.
//!
//! # What `Object.prototype` is, and what it is not
//!
//! An object every `{}` should inherit from, and this makes one — but nothing
//! links a literal to it yet, because `object_new` allocates with no prototype
//! and giving it one there is a link per object rather than the substitution a
//! string gets. So `Object.prototype` exists, a program can put something on it,
//! and a plain literal will not see it. Named rather than quietly half-done.
//!
//! # Why `defineProperty` is only the accessor half
//!
//! A descriptor is an object with any of six fields, and four of them —
//! `writable`, `enumerable`, `configurable`, and the distinction between an own
//! and an inherited definition — are facts the shape tree does not record. So
//! this reads `get`, `set` and `value`, and silently ignores the rest, which is
//! the one place here that answers less than it appears to. Recording them needs
//! a per-property flag word, which is a change to what a shape IS.

use super::native::Native;
use super::objects::undefined_of;
use super::{Context, with_current};
use crate::text::Str;
use crate::value::Value;

/// What `Object` holds.
const STATICS: &[(&str, Native)] = &[
    ("keys", keys),
    ("values", values),
    ("getPrototypeOf", get_prototype_of),
    ("setPrototypeOf", set_prototype_of),
    ("defineProperty", define_property),
    ("assign", assign),
];

/// `Object` itself, as the value the name reads.
pub(super) fn constructor(context: &mut Context) -> u64 {
    let callable = super::native::callable(context, make);
    let Some(cell) = Value(callable).as_slot() else {
        return callable;
    };
    super::native::install(context, cell, STATICS);
    // A `prototype` property like any constructor's, so `Object.prototype.m = f`
    // has somewhere to land — and so `instanceof Object` has something to
    // compare against once literals link to it.
    // The one every plain object already inherits from, not a fresh object:
    // making a second here would put `Object.prototype.m = f` somewhere nothing
    // walks, which is a write that succeeds and a read that never finds it.
    if let Some(prototype) = super::object_proto::prototype_of(context) {
        let key = context.well_known("prototype");
        let value = Value::from_slot(prototype).bits();
        super::objects::put(context, cell, key, value);
    }
    callable
}

/// `Object()` and `new Object()` — a new empty object.
///
/// The argument form (`Object(5)`, which wraps) is not built: there are no
/// wrapper objects here, and answering the primitive unchanged would be a
/// different function wearing the same name.
extern "C" fn make(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let fresh = super::objects::object_new();
    with_current(|context| {
        // `class Tag extends Object { own() {} }` has to reach `Tag.prototype`,
        // which means the object this makes inherits from the class `new`
        // named rather than from nothing.
        if let Some(cell) = Value(fresh).as_slot() {
            let absent = undefined_of(context);
            let prototype = super::functions::prototype_for_new(context, absent);
            if prototype != absent {
                context.set_prototype(cell, prototype);
            }
        }
        fresh
    })
}

/// `Object.keys(o)` — the own enumerable keys, as an array of strings.
///
/// Exactly what `for (k in o)` walks, minus what it inherits — and the runtime
/// already answers that question for the loop, so this is the same call rather
/// than a second enumeration that would drift from it.
extern "C" fn keys(_e: u64, _this: u64, object: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    super::array::own_keys(object)
}

/// `Object.values(o)` — what those keys hold, in the same order.
///
/// Built from the keys rather than from a second walk of the layout, so the two
/// cannot disagree about order — which is the property `Object.keys` and
/// `Object.values` are useful in pairs for.
extern "C" fn values(_e: u64, _this: u64, object: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let names = super::array::own_keys(object);
    let found = with_current(|context| {
        let cell = Value(names).as_slot()?;
        Some(context.elements_at(cell)?.clone())
    });
    let Some(found) = found else {
        return names;
    };
    // Each read goes through the ordinary property path, so a key that names an
    // accessor runs its getter — which is what the language says and what a
    // direct read of the layout would have skipped.
    let read: Vec<u64> = found
        .into_iter()
        .map(|name| super::computed::get_indexed(object, name))
        .collect();
    let array = super::array::array_new(read.len() as i64);
    with_current(|context| {
        if let Some(cell) = Value(array).as_slot()
            && let Some(elements) = context.elements_at_mut(cell)
        {
            *elements = read;
        }
        array
    })
}

/// `Object.getPrototypeOf(o)`.
extern "C" fn get_prototype_of(_e: u64, _this: u64, object: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    super::chain::get_prototype(object)
}

/// `Object.setPrototypeOf(o, p)` — answers the object.
extern "C" fn set_prototype_of(
    _e: u64,
    _this: u64,
    object: u64,
    prototype: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    super::chain::set_prototype(object, prototype)
}

/// `Object.defineProperty(o, k, descriptor)`.
///
/// Reads `get`, `set` and `value` from the descriptor and ignores the rest —
/// see the module documentation for which four fields those are and why
/// recording them is a change to what a shape is rather than more of this.
extern "C" fn define_property(
    _e: u64,
    _this: u64,
    object: u64,
    name: u64,
    descriptor: u64,
    _a3: u64,
) -> u64 {
    let read = |field: &str| {
        let key = with_current(|context| {
            context.intern_value(Str::from_str(field)).bits()
        });
        super::computed::get_indexed(descriptor, key)
    };
    let getter = read("get");
    let setter = read("set");
    let value = read("value");

    let (key, absent) = with_current(|context| {
        let text = super::text::to_text(context, Value(name));
        let key = text.map(|text| {
            context.interner.intern(&text, &mut context.keys).index() as i64
        });
        (key, undefined_of(context))
    });
    let Some(key) = key else {
        return object;
    };

    if getter != absent {
        super::accessor::define_getter(object, key, getter);
    }
    if setter != absent {
        super::accessor::define_setter(object, key, setter);
    }
    // A descriptor with neither is a data property, and one with `value` absent
    // still creates it — `{}` as a descriptor defines the property as
    // `undefined`, which is what the language does.
    if getter == absent && setter == absent {
        super::objects::set_property(object, key, value);
    }
    object
}

/// `Object.assign(target, source)` — copies the own keys across.
///
/// One source rather than any number, because the arity a call carries is four
/// and the receiver takes one of them. A call with more is refused at the site
/// rather than losing its arguments here.
extern "C" fn assign(_e: u64, _this: u64, target: u64, source: u64, _a2: u64, _a3: u64) -> u64 {
    let names = super::array::own_keys(source);
    let found = with_current(|context| {
        let cell = Value(names).as_slot()?;
        Some(context.elements_at(cell)?.clone())
    });
    let Some(found) = found else {
        return target;
    };
    for name in found {
        // Through the ordinary paths on both sides, so a getter on the source
        // runs and a setter on the target runs — which is what `assign` does
        // and what a slot-to-slot copy would have skipped.
        let value = super::computed::get_indexed(source, name);
        super::computed::set_indexed(target, name, value);
    }
    target
}
