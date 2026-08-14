//! The two places `JSON` hands a value to user code: `stringify`'s replacer and
//! `parse`'s reviver.
//!
//! # Why they share a file
//!
//! They are the same shape from opposite ends — a walk that calls back per
//! member, on a tree the walk is allowed to change while it goes. Both were
//! absent for one reason, written in the module header as "the `toJSON`
//! problem with a second argument", and both arrive on the discipline that
//! made `toJSON` possible: no borrow is held across a call.
//!
//! # Rule 8, and where the check is
//!
//! `crates/rts-core/README.md` rule 8 says a native that calls user code asks
//! whether it threw before looking at the answer. Every call here does, and
//! answers `undefined` upward when one is in flight — a reviver that throws
//! must not have its `undefined` written into the tree as a deletion, which is
//! exactly the silent wrong answer the rule exists to stop.

use super::super::{Context, throw, with_current};
use super::write::{Shape, shape_of};
use crate::text::Str;
use crate::value::Value;

/// What `stringify` was told to do with each member.
pub(super) enum Replacer {
    /// No replacer, or one of a type the specification ignores — which is most
    /// of them: anything that is neither callable nor an array.
    None,
    /// Called with `(key, value)` and the holder as its receiver, per member.
    Function(u64),
    /// The only keys an OBJECT writes, in this order. Arrays are unaffected,
    /// which is the specification's asymmetry and not an omission here: an
    /// array's members are addressed by position, so a name list cannot select
    /// among them.
    List(Vec<Str>),
}

/// Classifies the second argument to `stringify`, once, before the walk.
///
/// Once rather than per member because the answer cannot change: the walk calls
/// user code, but this argument is read before any of it runs.
pub(super) fn replacer_of(replacer: u64) -> Replacer {
    let callable = with_current(|context| {
        super::super::modules::is_callable_in(context, replacer)
    });
    if callable {
        return Replacer::Function(replacer);
    }
    // The elements are copied out of the borrow before anything is converted:
    // `shape_of` unwraps a wrapper object, which is a read, and the conversion
    // below is not.
    let elements = with_current(|context| {
        Value(replacer)
            .as_slot()
            .and_then(|cell| context.elements_at(cell).cloned())
    });
    let Some(elements) = elements else {
        return Replacer::None;
    };
    let mut kept: Vec<Str> = Vec::with_capacity(elements.len());
    with_current(|context| {
        for element in elements {
            // Strings and numbers only, wrappers included — `shape_of` already
            // substitutes a wrapper's primitive, so `new String("a")` selects
            // the same member `"a"` does. Anything else is not a property name
            // the specification will accept, and skipping it is what the
            // language says rather than a simplification.
            let name = match shape_of(context, element) {
                Shape::Text(text) => text,
                Shape::Number(number) => crate::coerce::number_to_string(number),
                _ => continue,
            };
            if !kept.iter().any(|seen| seen.units().eq(name.units())) {
                kept.push(name);
            }
        }
    });
    Replacer::List(kept)
}

/// `{"": value}` — the synthetic holder the specification serialises the root
/// under, so a replacer called for the root has a receiver and a key like every
/// other member.
///
/// Built only when there IS a function replacer: it is a cell per `stringify`
/// call, and nothing else can observe it.
pub(super) fn root_holder(value: u64) -> u64 {
    with_current(|context| {
        let Some(cell) = super::super::native::plain(context) else {
            return super::super::objects::undefined_of(context);
        };
        let key = context.well_known("");
        super::super::objects::put(context, cell, key, value);
        Value::from_slot(cell).bits()
    })
}

/// `InternalizeJSONProperty` — the reviver's walk, in post-order.
///
/// Children first, then the member itself, which is what lets a reviver see
/// values its own nested calls already replaced. A child answering `undefined`
/// is DELETED rather than stored, which is the one part of this a caller can
/// see without a reviver that inspects: `JSON.parse(t, () => undefined)`
/// answers `undefined` and not an object full of holes.
pub(super) fn internalized(holder: u64, key: u64, reviver: u64) -> u64 {
    let value = super::super::computed::get_indexed(holder, key);
    if throw::in_flight() {
        return absent();
    }
    // Held across the loop for the reason `write::object` holds its key array:
    // every step below allocates, the reference lives in a Rust local the
    // conservative scan cannot be relied on to see, and a collection in the
    // middle of the walk would free the tree being revived.
    let anchor = super::super::external::hold_current(value);
    match descent_of(value) {
        Descent::Elements(count) => {
            for at in 0..count {
                let name = with_current(|context| {
                    context
                        .intern_value(crate::coerce::number_to_string(at as f64))
                        .bits()
                });
                if !place(value, name, reviver) {
                    break;
                }
            }
        }
        Descent::Members => {
            let names = super::super::array::own_keys(value);
            let names_anchor = super::super::external::hold_current(names);
            let names = with_current(|context| {
                Value(names)
                    .as_slot()
                    .and_then(|cell| context.elements_at(cell).cloned())
                    .unwrap_or_default()
            });
            for name in names {
                if !place(value, name, reviver) {
                    break;
                }
            }
            super::super::external::release_current(names_anchor);
        }
        Descent::None => {}
    }
    super::super::external::release_current(anchor);
    if throw::in_flight() {
        return absent();
    }
    let absent = absent();
    super::super::functions::call(reviver, holder, key, value, absent, absent)
}

/// One member of a composite: revive it, then store or delete what came back.
///
/// Answers whether the walk may continue — false once a throw is in flight, so
/// a reviver that raises stops the walk instead of deleting every remaining
/// member with the `undefined` its failed call answered.
fn place(holder: u64, key: u64, reviver: u64) -> bool {
    let revived = internalized(holder, key, reviver);
    if throw::in_flight() {
        return false;
    }
    match revived == absent() {
        true => {
            super::super::computed::delete_property(holder, key);
        }
        false => {
            super::super::computed::set_indexed(holder, key, revived);
        }
    }
    true
}

/// What a value's children are, as far as the reviver's walk is concerned.
enum Descent {
    /// An array, and how many elements it had when the walk reached it.
    Elements(usize),
    /// An object with named members.
    Members,
    /// A primitive: nothing to descend into.
    None,
}

fn descent_of(value: u64) -> Descent {
    with_current(|context| {
        let Some(cell) = Value(value).as_slot() else {
            return Descent::None;
        };
        if context.text_at(cell).is_some() {
            return Descent::None;
        }
        match context.elements_at(cell) {
            Some(elements) => Descent::Elements(elements.len()),
            None => Descent::Members,
        }
    })
}

/// `undefined`, which this file compares against often enough to name.
fn absent() -> u64 {
    with_current(|context| super::super::objects::undefined_of(context))
}

/// The receiver and key a function replacer is called with, for one member.
///
/// A free function rather than a method on [`Replacer`] because the writer
/// holds the replacer and the *caller* holds the holder, and threading a `&mut
/// Writer` through a call into user code is the borrow this module refuses.
pub(super) fn replaced(hook: u64, holder: u64, key: u64, value: u64) -> u64 {
    let absent = absent();
    let answered = super::super::functions::call(hook, holder, key, value, absent, absent);
    match throw::in_flight() {
        true => absent,
        false => answered,
    }
}

/// Whether a member survives a list replacer — used by the writer's object
/// walk, which enumerates the list instead of the object when there is one.
pub(super) fn interned(context: &mut Context, name: &Str) -> u64 {
    context.intern_value(name.clone()).bits()
}
