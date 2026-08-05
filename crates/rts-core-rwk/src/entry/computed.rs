//! Property access whose key the program computed, and the operations that
//! follow from one.
//!
//! # Why these are apart from the named ones
//!
//! `o.x` carries a key the **compiler** resolved: a number, decided while
//! compiling, with no text crossing at all. `o[e]` carries a value that has to
//! become a key while running, which is `ToPropertyKey` and interns text.
//!
//! That is one difference and it reaches everything downstream — `in` and
//! `delete` both take a computed key, and an array element is only reachable
//! through this side. Filing them with the named read would put two answers to
//! "what is a key" in one file.

use super::objects::{machine_key, put, set_slot_value, slot_value, undefined_of};
use super::string::text::{string_element, string_property};
use super::{Context, with_current};
use crate::object::Key;
use crate::value::Value;

/// The key a value names, through `ToPropertyKey`.
///
/// # Why every key becomes a name, including one that spells an index
///
/// `o[0]` and `o["0"]` are
/// one property, which is what the language says, and the only thing lost is an
/// enumeration order nothing implements. Routing it through `Key::from_str` and
/// letting the `None` through would have made `o[0] = 1; o[0]` read as absent —
/// a wrong program that runs, which is the outcome this whole layer refuses.
///
/// `None` means the key was an object, whose `ToPropertyKey` runs a `toString`
/// — user code an entry point cannot call.
fn property_key(context: &mut Context, key: Value) -> Option<Key> {
    // A symbol is its own key rather than one derived from text, and it is
    // asked first because `ToString` of a symbol is a `TypeError` in the
    // language — converting it here would give `o[Symbol.iterator]` a key the
    // spelling `o["Symbol(Symbol.iterator)"]` could also reach.
    if let Some(text) = super::symbol::key_text_of(context, key.bits()) {
        let text = crate::text::Str::from_str(&text);
        return Some(Key::Name(context.interner.intern(&text, &mut context.keys)));
    }
    let text = super::text::to_text(context, key)?;
    Some(Key::Name(context.interner.intern(&text, &mut context.keys)))
}

/// `object[key]`, where the key is a value rather than a resolved name.
///
/// Two statements, like the named read and for the same reason: the answer may
/// be a getter, which is user code that must not run inside a borrow of the
/// context.
#[rtse::entry]
pub fn get_indexed(object: u64, key: u64) -> u64 {
    let found = with_current(|context| {
        let Some(slot) = Value(object).as_slot() else {
            return super::accessor::Found::Value(undefined_of(context));
        };
        // An element, if this is an array and the key is a canonical index.
        // Asked BEFORE `ToPropertyKey`, because that would turn the number into
        // text and lose the distinction the array store is built on.
        if let Some(at) = super::array::as_index(context, Value(key))
            && let Some(elements) = context.elements_at(slot)
        {
            // Past the end is absent, not an error: `[1,2][9]` is `undefined`.
            let answer = elements
                .get(at)
                .copied()
                .unwrap_or_else(|| undefined_of(context));
            return super::accessor::Found::Value(answer);
        }
        // A typed array's element, which is a byte range rather than a slot in
        // an element vector. Asked here for the reason the array branch is
        // asked before `ToPropertyKey`: the index is a number, and converting
        // it to text loses what the view is addressed by.
        //
        // An index past the end answers `undefined` and does NOT fall through
        // to a property — `new Uint8Array(2)[9]` is absent, not a lookup for
        // the name "9".
        if let Some(answer) = super::buffers::indexed_get(context, slot, Value(key)) {
            return super::accessor::Found::Value(answer);
        }
        if let Some(answer) = string_element(context, slot, Value(key)) {
            return super::accessor::Found::Value(answer);
        }
        let Some(key) = property_key(context, Value(key)) else {
            return super::accessor::Found::Value(undefined_of(context));
        };
        if let Some(answer) = string_property(context, slot, key) {
            return super::accessor::Found::Value(answer);
        }
        // Through the accessor-aware walk, not `read_property`, and the reason
        // is the whole point of a computed read: `o[k]` and `o.x` name the same
        // property, so one of them finding a getter and the other reading a
        // slot would make which spelling was written decide what a property IS.
        super::accessor::resolve(context, slot, key)
    });
    match found {
        super::accessor::Found::Value(value) => value,
        super::accessor::Found::Getter(getter) => {
            let undefined = with_current(|context| undefined_of(context));
            super::functions::call(getter, object, undefined, undefined, undefined, undefined)
        }
        super::accessor::Found::Absent => with_current(|context| undefined_of(context)),
    }
}

/// `object[key] = value`. Answers the value, because an assignment is an
/// expression.
///
/// Two statements, like the named write: a setter is user code and runs after
/// the borrow ends.
#[rtse::entry]
pub fn set_indexed(object: u64, key: u64, value: u64) -> u64 {
    let setter = with_current(|context| {
        let Some(slot) = Value(object).as_slot() else {
            return None;
        };
        if let Some(at) = super::array::as_index(context, Value(key))
            && let Some(elements) = context.elements_at_mut(slot)
        {
            // Writing past the end grows the array and fills the gap with
            // `undefined`, which is what the language does — `let a = []; a[2]
            // = 1` leaves length 3. Holes are `undefined` here rather than a
            // distinct absent-ness, which is a stated gap: `0 in [,1]` is
            // false and this cannot say so.
            if at >= elements.len() {
                elements.resize(at + 1, 0);
                let absent = undefined_of(context);
                let elements = context
                    .elements_at_mut(slot)
                    .expect("the array was just found");
                for hole in elements.iter_mut() {
                    if *hole == 0 {
                        *hole = absent;
                    }
                }
            }
            let elements = context
                .elements_at_mut(slot)
                .expect("the array was just found");
            elements[at] = value;
            // `length` is a property both paths read, so growing has to write it
            // — compiled code reads the stored one and never asks the runtime
            // for a hit.
            let count = elements.len();
            super::array::set_length(context, slot, count);
            return None;
        }
        // A typed array's element. Answering true means the write landed in the
        // view's bytes; a write past the end is DROPPED rather than falling
        // through to a property, which is what the language does — a typed
        // array does not grow and `a[9] = 1` on a two-element one stores
        // nothing anybody can read back.
        if super::buffers::indexed_set(context, slot, Value(key), value) {
            return None;
        }
        let Some(key) = property_key(context, Value(key)) else {
            return None;
        };
        // The same question the named write asks, and it has to be asked here
        // too: `o[k] = v` and `o.x = v` reach one property, so a setter found
        // by one spelling and a slot written by the other is two answers to
        // what that property IS.
        if let Some(setter) = super::accessor::setter_for(context, slot, key) {
            return Some(setter);
        }
        put(context, slot, key, value);
        None
    });
    if let Some(setter) = setter {
        let undefined = with_current(|context| undefined_of(context));
        super::functions::call(setter, object, value, undefined, undefined, undefined);
    }
    value
}

/// `key in object`.
///
/// Answers whether the object HAS the property, which is not whether reading it
/// yields `undefined`: `({x: undefined})` has `x`, and `"x" in it` is true.
/// That is the whole reason the operator exists, so it is what this asks.
///
/// A receiver that is not an object answers `false` where the language throws a
/// `TypeError` — the same stated gap every property operation has, and for the
/// same reason: throwing needs protected regions and nothing emits those.
#[rtse::entry]
pub fn has_property(key: u64, object: u64) -> bool {
    with_current(|context| {
        let Some(slot) = Value(object).as_slot() else {
            return false;
        };
        let Some(key) = property_key(context, Value(key)) else {
            return false;
        };
        // An accessor is a property the object HAS, and it is not in the
        // layout — so asking the shape alone answers false for
        // `"x" in { get x() {} }`, which is the operator getting its one job
        // wrong.
        !matches!(
            super::accessor::resolve(context, slot, key),
            super::accessor::Found::Absent
        )
    })
}

/// The key `length` has.
///
/// Interned rather than held as a constant, because the number is whatever the
/// registry issued — and the registry was seeded from what the compilation
/// resolved, so a program that reads `.length` already put it there and this
/// finds the same number. A program that never mentions it mints one here that
/// nothing else uses, which costs a key and answers nothing differently.
pub(super) fn length_key(context: &mut Context) -> Key {
    context.well_known("length")
}

/// `delete o.x` / `delete o[k]`.
///
/// Answers whether the object now lacks the property, which is `true` for one
/// it never had — the language's answer for `delete` of anything that was not
/// there, including a non-object.
///
/// # Why this is a rebuild and not an unlink
///
/// `ShapeTree::remove` says it: the tree only grows, a node is shared by
/// everything that extends it, and unlinking one would change a layout other
/// objects are already using. So the shape is rebuilt without the key, which is
/// a **different layout with a different identity** — code compiled to load a
/// property at a fixed offset in the old one guards on a type number it will no
/// longer see.
///
/// The values move with it. Removing a property shifts every later one down a
/// slot, so they are read out against the old layout before the header changes
/// and written back against the new — reading after would read the new offsets
/// out of the old contents.
#[rtse::entry]
pub fn delete_property(object: u64, key: u64) -> bool {
    with_current(|context| {
        let Some(slot) = Value(object).as_slot() else {
            return true;
        };
        let Some(key) = property_key(context, Value(key)) else {
            return true;
        };
        let Some(machine) = machine_key(key) else {
            return true;
        };
        let Some(ty) = context.region.type_of(slot) else {
            return true;
        };
        let Some(shape) = context.shape_of(ty) else {
            return true;
        };
        if context.shapes.slot_of(shape, machine).is_none() {
            // Never had it. `delete` answers true, which is not "it was
            // removed" but "the object does not have it", and those agree here.
            return true;
        }

        // Read every survivor against the OLD layout first.
        let kept: Vec<(rts_cranelift::shape::Key, u64)> = context
            .shapes
            .properties(shape)
            .into_iter()
            .filter(|(existing, _)| *existing != machine)
            .filter_map(|(existing, _)| {
                let at = context.shapes.slot_of(shape, existing)?;
                let value = slot_value(context, slot, at)?;
                Some((existing, value))
            })
            .collect();

        let shrunk = context.shapes.remove(shape, machine);
        let ty = context.layout_of(shrunk).index() as u32;
        context.region.set_type(slot, ty);
        for (existing, value) in kept {
            if let Some(at) = context.shapes.slot_of(shrunk, existing) {
                set_slot_value(context, slot, at, value);
            }
        }
        true
    })
}
