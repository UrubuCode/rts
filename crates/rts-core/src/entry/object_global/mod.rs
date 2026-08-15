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
//! # `defineProperty` is the whole operation now, and it lives next door
//!
//! This section used to say it read `get`, `set` and `value` and ignored the
//! other three, because "the shape tree does not record them". The place was
//! wrong rather than the fact: an attribute is per OBJECT per property, so it
//! belongs beside the cell — [`super::integrity`] holds it — and the shape is
//! exactly where it must not go.
//!
//! What was left after that was a definition that never REFUSED, which made
//! `configurable: false` a flag a program could read back and nothing enforced.
//! [`define`] is `ValidateAndApplyPropertyDescriptor`, in [`descriptor`] rather
//! than here because this file is at its ceiling and the validation is the
//! larger half of the operation.

mod descriptor;
mod describe;
pub(in crate::entry) use describe::{describe_of, describe_own};

use super::native::Native;
use super::objects::undefined_of;
use super::{Context, with_current};
use crate::object::Key;
use crate::text::Str;
use crate::value::Value;

/// What `Object` holds, apart from the five in [`describe`].
///
/// With arity: these are read as values often enough — `Object.assign` forwarded
/// into a reduce, `Object.keys` passed to `.map` — that `.length` has to be
/// right rather than merely present, and installed through
/// [`super::native::install_with_arity`] rather than [`super::native::install`]
/// for exactly that reason.
const STATICS: &[(&str, Native, u32)] = &[
    ("keys", keys, 1),
    ("values", values, 1),
    ("entries", entries, 1),
    ("fromEntries", from_entries, 1),
    ("hasOwn", has_own, 2),
    ("is", is, 2),
    ("getPrototypeOf", get_prototype_of, 1),
    ("setPrototypeOf", set_prototype_of, 2),
    ("defineProperty", define_property, 3),
    ("assign", assign, 2),
    ("groupBy", group_by, 2),
];

/// `Object` itself, as the value the name reads.
pub(super) fn constructor(context: &mut Context) -> u64 {
    let callable = super::native::callable(context, make);
    let Some(cell) = Value(callable).as_slot() else {
        return callable;
    };
    // `Object.name` and `.prototype.constructor.name` both read this — the
    // constructor itself is a single `native::callable`, not something
    // `install`'s per-entry naming reaches.
    let name_key = context.well_known("name");
    let name_value = context.intern_value(Str::from_str("Object")).bits();
    super::objects::put(context, cell, name_key, name_value);
    super::native::install_with_arity(context, cell, STATICS);
    super::native::install(context, cell, describe::STATICS);
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
        // The other half of the same link: `Object.prototype.constructor`
        // answers `Object` — a real property the specification requires
        // (`class Tag extends Object {}` and every plain object's `.constructor`
        // read it), not something `Object.prototype`'s own module can leave
        // absent as "no constructor of its own": it has one, this is it.
        let constructor_key = context.well_known("constructor");
        super::objects::put(context, prototype, constructor_key, callable);
    }
    callable
}

/// `Object()` and `new Object()` — a new empty object; `Object(x)` — `x`
/// unchanged if it is already an object, otherwise a boxed wrapper.
///
/// This used to answer an empty object for every argument, documented as "no
/// wrapper objects here" — true when it was written, and stale the day
/// `new Number`/`new String`/`new Boolean` started boxing for real (see
/// [`super::primitive_proto::wrap`]). `Object(5)` is the ODD constructor call
/// here: the specification boxes a primitive even though the call has no
/// `new` in it, where `Number(5)` deliberately does not — so this cannot go
/// through `wrap`, which is guarded by `new_targets` being non-empty
/// precisely to keep the two apart. It builds the wrapper by hand instead.
extern "C" fn make(_e: u64, _this: u64, a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let absent = undefined_of(context);
        let is_nullish = a0 == absent
            || a0 == Value::from_singleton(context.singletons.undefined).bits()
            || a0 == Value::from_singleton(context.singletons.null).bits();
        if is_nullish {
            return empty_object(context);
        }
        // A string cell wraps around `String.prototype` — its own case,
        // because a string is a heap cell here (unlike a number or a
        // boolean) and `primitive_proto::prototype_of` does not classify one;
        // [`super::string::prototype_of`] is where that link already lives.
        let string_prototype = Value(a0).as_slot().filter(|&cell| context.text_at(cell).is_some());
        // Already an object (including an array or a callable): `ToObject`
        // answers it unchanged, not a copy.
        if Value(a0).as_slot().is_some() && string_prototype.is_none() {
            return a0;
        }
        // A primitive — a number, a boolean, or a string cell — becomes a
        // wrapper around it, the same `[[…Data]]` slot `new Number`/
        // `new String`/`new Boolean` record.
        let found = match string_prototype {
            Some(_) => super::string::prototype_of(context),
            None => super::primitive_proto::prototype_of(context, Value(a0)),
        };
        let Some(prototype) = found else {
            return empty_object(context);
        };
        let Some(cell) = super::native::plain(context) else {
            return empty_object(context);
        };
        context.set_prototype(cell, Value::from_slot(prototype).bits());
        context.set_boxed(cell, a0);
        Value::from_slot(cell).bits()
    })
}

/// A new empty object, inheriting from whatever `new` named — or
/// `Object.prototype`, for the ordinary call with no `new` in sight.
fn empty_object(context: &mut Context) -> u64 {
    // `objects::object_new(0)` enters `with_current` itself; every caller of
    // `empty_object` already holds that borrow (this function takes
    // `&mut Context` rather than opening its own), so calling the entry point
    // here would re-enter the `RefCell` and abort. `object_new_in` is the same
    // allocation without the second borrow.
    let fresh = super::objects::object_new_in(context);
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
}

/// `Object.keys(o)` — the own enumerable keys, as an array of strings.
///
/// Exactly what `for (k in o)` walks, minus what it inherits — and the runtime
/// already answers that question for the loop, so this is the same call rather
/// than a second enumeration that would drift from it.
extern "C" fn keys(_e: u64, _this: u64, object: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    // A proxy's keys are filtered by their descriptors, which `Reflect.ownKeys`
    // does not do — see `super::proxy::enumerable_keys` for why that difference
    // is the specification's and not an inconsistency.
    if let Some(answered) = super::proxy::enumerable_keys(object) {
        return answered;
    }
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
extern "C" fn define_property(
    _e: u64,
    _this: u64,
    object: u64,
    name: u64,
    descriptor: u64,
    _a3: u64,
) -> u64 {
    define(object, name, descriptor);
    object
}

/// One property defined from one descriptor, throwing when it is refused.
///
/// Its own function because `Object.create` and `Object.defineProperties` are
/// this in a loop, and a second reading of what a descriptor means is where one
/// of the three learns that `{}` defines `undefined` and the others do not.
///
/// The throw is here rather than in [`descriptor`] because that is the whole
/// difference between this and `Reflect.defineProperty`, which reports the same
/// refusal as `false`.
pub(in crate::entry) fn define(object: u64, name: u64, stated: u64) {
    let Some(wanted) = descriptor::read(stated) else {
        // Already thrown: either the descriptor was not an object, or reading a
        // field of it ran a getter that threw. Rule 8 — the answer is not
        // looked at.
        return;
    };
    define_read(object, name, &wanted);
}

/// The second half, for a caller that read the descriptor earlier.
///
/// `Object.defineProperties` is that caller, and it has to be: the language
/// reads EVERY descriptor before it defines the first property, so a set whose
/// second descriptor throws leaves the object untouched.
pub(in crate::entry) fn define_read(object: u64, name: u64, wanted: &descriptor::Descriptor) {
    match descriptor::apply(object, name, wanted) {
        descriptor::Verdict::Done => {}
        descriptor::Verdict::Refused => {
            let named = with_current(|context| {
                super::text::to_text(context, Value(name))
                    .and_then(|text| text.to_rust())
                    .unwrap_or_default()
            });
            super::throw::type_error(&format!("Cannot redefine property: {named}"));
        }
        descriptor::Verdict::NotObject => {
            super::throw::type_error("Object.defineProperty called on non-object");
        }
    }
}

/// The key a property name value denotes.
///
/// Shared with [`describe`] and [`descriptor`] so that a descriptor is read back
/// under the same key it was written to — two spellings of "intern the text" is
/// how a key written as a name gets read as an index.
///
/// `ToPropertyKey` itself rather than a second interning of the text: a symbol
/// has no string spelling, and resolving one here by `to_text` gave it a key the
/// ordinary access path would never look under.
pub(super) fn key_for(context: &mut Context, name: u64) -> Option<Key> {
    super::computed::property_key(context, Value(name))
}

/// `Object.entries(o)` — `[key, value]` per own key.
///
/// Built from `keys` and `values` rather than from a third walk, for the reason
/// [`values`] records: the three are useful because they agree about order.
extern "C" fn entries(_e: u64, _this: u64, object: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let names = super::array::own_keys(object);
    let Some(names) = held(names) else {
        return names;
    };
    let pairs: Vec<u64> = names
        .into_iter()
        .map(|name| {
            let value = super::computed::get_indexed(object, name);
            super::array_proto::built(vec![name, value])
        })
        .collect();
    super::array_proto::built(pairs)
}

/// `Object.fromEntries(entries)` — the inverse.
///
/// Through the runtime's own iteration, so a `Map` is accepted: `for-of` over
/// one yields exactly the pairs this reads, which is what makes
/// `Object.fromEntries(map)` the spelling programs use.
extern "C" fn from_entries(_e: u64, _this: u64, entries: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let produced = super::iterate::iterate(entries);
    let made = super::objects::object_new(0);
    let Some(pairs) = held(produced) else {
        return made;
    };
    for pair in pairs {
        let Some(pair) = held(pair) else {
            continue;
        };
        let absent = with_current(|context| undefined_of(context));
        let key = pair.first().copied().unwrap_or(absent);
        let value = pair.get(1).copied().unwrap_or(absent);
        super::computed::set_indexed(made, key, value);
    }
    made
}

/// `Object.hasOwn(o, k)`.
///
/// Own rather than inherited, which is the whole reason it exists beside `in` —
/// and it answers for a key holding `undefined`, which is why it cannot be
/// written as a read compared against `undefined`.
extern "C" fn has_own(_e: u64, _this: u64, object: u64, name: u64, _a2: u64, _a3: u64) -> u64 {
    let owns = with_current(|context| {
        let Some(cell) = Value(object).as_slot() else {
            return false;
        };
        let Some(key) = key_for(context, name) else {
            return false;
        };
        if let Key::Name(named) = key
            && context.accessor_at(cell, named.index() as u32).is_some()
        {
            return true;
        }
        super::objects::own_property(context, cell, key).is_some()
    });
    Value::from_bool(owns).bits()
}

/// `Object.is(a, b)` — SameValue.
///
/// Not `===`, and the two cases that differ are the reason it exists:
/// `Object.is(NaN, NaN)` is true and `Object.is(0, -0)` is false.
extern "C" fn is(_e: u64, _this: u64, left: u64, right: u64, _a2: u64, _a3: u64) -> u64 {
    let same = with_current(|context| {
        crate::value::same_value(Value(left), Value(right), |a, b| context.same_text(a, b))
    });
    Value::from_bool(same).bits()
}

/// `Object.groupBy(items, f)` — an object of arrays, keyed by what `f` answered.
///
/// The key is a **property key**, so `1` and `"1"` name one group — which is the
/// difference from `Map.groupBy` and the reason the two exist side by side. A
/// program that needs `1` and `"1"` apart reaches for that one.
///
/// The object inherits nothing, which the specification requires: a group named
/// `toString` must be a group rather than the method it would otherwise shadow.
/// Nothing here arranges that — `object_new` allocates with no prototype, which
/// the module documentation records as a gap and which this one method wants.
extern "C" fn group_by(_e: u64, _this: u64, items: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    let elements = super::collections::elements_of(items);
    let made = super::objects::object_new(0);
    let absent = with_current(|context| undefined_of(context));
    for (at, element) in elements.into_iter().enumerate() {
        let index = Value::from_f64(at as f64).bits();
        // No borrow held: the callback is user code.
        // A throw stops the grouping: the object built so far is what the
        // caller gets, and the compiled site above re-raises.
        let Some(key) = super::collections::invoke(callback, absent, element, index, absent)
        else {
            break;
        };
        // The group is read back off the object rather than accumulated in a
        // Rust map, for the reason `Map::group_by` states from the other side:
        // what makes two keys one group is the key space, and the object is the
        // thing that already decides it. A second answer here is where `1` and
        // `"1"` would stop naming one property.
        let group = super::computed::get_indexed(made, key);
        if group == absent {
            let fresh = super::array_proto::built(vec![element]);
            super::computed::set_indexed(made, key, fresh);
        } else {
            super::iterate::array_append(group, element);
        }
    }
    made
}

/// An array's elements, the borrow ending here.
fn held(array: u64) -> Option<Vec<u64>> {
    with_current(|context| {
        let cell = Value(array).as_slot()?;
        Some(context.elements_at(cell)?.clone())
    })
}

/// `Object.assign(target, ...sources)` — copies the own keys across.
///
/// Three sources rather than any number, because the arity a call carries is
/// four and the receiver takes one of them. It used to read ONE and the comment
/// beside it claimed a call with more was "refused at the site"; it was not —
/// `Object.assign({}, a, b)` silently dropped `b`, which is the failure mode a
/// merge must not have, because the result looks like a merge.
///
/// Beyond three the arguments genuinely are not here, and that stays a gap
/// rather than a quiet loss: it needs the gathered-arguments path, which is a
/// change to how a native is called and not to this function.
extern "C" fn assign(_e: u64, _this: u64, target: u64, source: u64, a2: u64, a3: u64) -> u64 {
    for source in [source, a2, a3] {
        // An absent argument arrives as `undefined`, and `undefined` has no own
        // keys — so the same skip serves both the spec's rule (a null or
        // undefined source contributes nothing) and the missing-argument case,
        // without this having to know which it is looking at.
        let Some(found) = held(super::array::own_keys(source)) else {
            continue;
        };
        for name in found {
            // Through the ordinary paths on both sides, so a getter on the
            // source runs and a setter on the target runs — which is what
            // `assign` does and what a slot-to-slot copy would have skipped.
            let value = super::computed::get_indexed(source, name);
            super::computed::set_indexed(target, name, value);
        }
        // `own_keys` never reports a symbol — it is the string enumeration,
        // and a symbol has no string spelling for it to report — so a
        // `[sym]: v` entry used to vanish across `Object.assign` and spread
        // silently, which is a merge that drops data rather than one that
        // says it cannot copy something.
        let symbols = with_current(|context| super::array::symbol_keyed(context, source));
        for (key, value) in symbols {
            with_current(|context| {
                if let Some(cell) = crate::value::Value(target).as_slot() {
                    super::objects::put(context, cell, key, value);
                }
            });
        }
    }
    target
}
