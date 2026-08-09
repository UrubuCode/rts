//! Arrays: elements addressed by number rather than by name.
//!
//! # Why an array is not an object with numeric keys
//!
//! It could be, and the shape tree would refuse it: a shape is a chain of
//! transitions, one per property, so an array of a thousand elements would be a
//! thousand-deep chain and a new layout for every length a program reaches.
//! Shapes exist to make objects built the same way share a layout; a thousand
//! arrays of different lengths share nothing.
//!
//! So elements live apart from properties, in a growable store, and the cell is
//! the identity. That is the same split text already makes — a string's bytes
//! are not in the region either, because a cell is sixty-four bytes and text is
//! any length — and it is what every engine does for the same reason.
//!
//! # What an array still is
//!
//! An object. `a.x = 1` works and `a[0] = 1` does not go anywhere near the
//! shape tree, which is why [`super::objects::get_indexed`] asks *which* before
//! deciding. `length` is an ordinary property holding the count — see
//! [`set_length`] for why it is stored rather than invented.

use super::objects::undefined_of;
use super::{Context, with_current};
use crate::heap::Slot;
use crate::text::Str;
use crate::value::Value;

/// `[…]` — a new array of `length` elements, each `undefined`.
///
/// Allocated as an ordinary object, because that is what an array is: it has
/// properties and a shape like any other. Being an array is recorded beside the
/// cell rather than in place of its layout — see `Context::array_elements` for
/// why the first version, which gave arrays a reserved layout, made `a.tag = 9`
/// a silent no-op.
#[rtse::entry]
pub fn array_new(length: i64) -> u64 {
    with_current(|context| {
        let absent = undefined_of(context);
        built_in(context, vec![absent; length.max(0) as usize])
    })
}

/// The same, from a context already in hand and with its elements.
///
/// Split out because a HOST building a namespace has a `&mut Context` and no
/// ambient one — `process.argv` is an array built before the program starts —
/// and the entry point above would abort there with nothing installed.
pub(in crate::entry) fn built_in(context: &mut Context, elements: Vec<u64>) -> u64 {
    let count = elements.len();
    let store = context.arrays.insert(elements).slot();
    let shape = context.shapes.root();
    let ty = context.layout_of(shape).index() as u32;
    match context.region.alloc(crate::heap::STRIDE, ty) {
        Some(cell) => {
            context.mark_array(cell, store);
            set_length(context, cell, count);
            Value::from_slot(cell).bits()
        }
        // The region is full — see [`super::alloc::heap_exhausted`].
        None => super::alloc::heap_exhausted(context),
    }
}

impl Context {
    /// Records that a cell is an array, and where its elements are.
    fn mark_array(&mut self, cell: u32, store: Slot) {
        self.array_elements.set(cell, store);
    }

    /// Where a cell's elements are, if it is an array.
    fn store_of(&self, reference: u32) -> Option<Slot> {
        self.array_elements.copied(reference)
    }

    /// The elements of an array, if this reference names one.
    pub(super) fn elements_at(&self, reference: u32) -> Option<&Vec<u64>> {
        self.arrays.at(self.store_of(reference)?).ok()
    }

    /// The same, to write through.
    pub(super) fn elements_at_mut(&mut self, reference: u32) -> Option<&mut Vec<u64>> {
        let store = self.store_of(reference)?;
        self.arrays.at_mut(store).ok()
    }
}

/// The element a value names, if it is a canonical index in range.
///
/// # Why a whole non-negative double and not "a number"
///
/// `a[1.5]` and `a[-1]` are ordinary **properties**, not elements — the
/// language says an array index is a canonical non-negative integer below
/// 2^32-1, and everything else is a name. Getting this wrong makes `a[1.5] = 9`
/// write into element 1, which is a wrong program that runs.
pub(super) fn as_index(context: &Context, key: Value) -> Option<usize> {
    // A STRING that spells a canonical index is one too, and this is not a
    // nicety: `for (k in a)` yields strings, so `a[k]` inside such a loop is
    // always the string form. The first version took numbers only, and
    // `for (k in [1,2,3]) s += a[k]` answered NaN — every read missed the
    // elements and found an absent property.
    //
    // `as_array_index` is the runtime's own answer to which strings those are,
    // and reusing it is what keeps this agreeing with `Key::from_str` about
    // where the boundary is.
    if let Some(slot) = key.as_slot()
        && let Some(text) = context.text_at(slot)
    {
        return crate::object::as_array_index(text).map(|index| index as usize);
    }

    let number = key.numeric()?;
    if number < 0.0 || number.fract() != 0.0 || number >= 4_294_967_295.0 {
        return None;
    }
    Some(number as usize)
}

/// `for (k in o)` — the keys, as an array of strings.
///
/// # Why an array and not an iterator
///
/// Because an iterator is a call, and a call from inside an entry point is what
/// this layer cannot do. Handing back an array lets the compiler emit an
/// ordinary indexed loop, which is machinery that already exists and is already
/// tested.
///
/// # What the order is
///
/// Integer indices first in numeric order, then the other keys in the order
/// they were added. That is what the specification says and what `Key` was
/// split into two variants to record — see [`crate::object::key`]. An array's
/// elements come first for the same reason: they are the integer indices.
///
/// # The divergence, named
///
/// The keys are collected **once**, so a property deleted during the loop is
/// still visited, where the specification says it must not be. Properties
/// *added* during one need not be visited, which snapshotting also satisfies —
/// so the error is in one direction only, and it is the direction a program
/// notices least. Fixing it needs the enumeration to hold a cursor into the
/// shape rather than a copy, which is a different mechanism.
///
/// # What is missing
///
/// Inherited keys. `for-in` walks the prototype chain and there are no
/// prototypes, so this is own keys — which is the same absence every property
/// operation here has.
#[rtse::entry]
pub fn own_keys(object: u64) -> u64 {
    // A proxy answers by running its handler, and it has no own keys of its
    // own to walk — see `super::proxy` for why the interception is here rather
    // than in anything the compiled site does.
    if let Some(answered) = super::proxy::own_keys(object) {
        return answered;
    }
    keys_of(object, true)
}

/// Every own key, INCLUDING the ones an enumeration does not report.
///
/// What `Object.getOwnPropertyNames` answers, and the reason the two are one
/// function with a flag rather than two walks: they differ in a single `if`,
/// and the ordering, the symbol rule and the accessor pass are the same rules
/// — which this crate keeps refusing to state twice.
pub(in crate::entry) fn own_names(object: u64) -> u64 {
    // A proxy answers both spellings the same way — its handler's `ownKeys` IS
    // `[[OwnPropertyKeys]]`, and the enumerable/every distinction is applied by
    // the caller rather than by the trap. Asking here as well as in `own_keys`
    // is what stopped `Reflect.ownKeys` over a proxy from answering the empty
    // list, which is what a cell with no properties of its own really has.
    if let Some(answered) = super::proxy::own_keys(object) {
        return answered;
    }
    keys_of(object, false)
}

/// The shared walk, from a context already in hand.
///
/// Split out from [`keys_of`] because a host walking a value's structure holds a
/// context by construction — `entry::member_names` is that caller — and the
/// ambient form would be a nested borrow, which in an `extern "C"` frame is an
/// abort rather than an error.
pub(in crate::entry) fn key_texts(
    context: &mut Context,
    object: u64,
    enumerable_only: bool,
) -> Vec<Str> {
    {
        let Some(slot) = Value(object).as_slot() else {
            return Vec::new();
        };

        let mut keys: Vec<Str> = Vec::new();
        // Elements first, as strings: `for (k in [1,2])` yields "0" and "1",
        // not 0 and 1. A loop that compared `k === 0` would find nothing, and
        // that is the language rather than a quirk of this implementation.
        if let Some(elements) = context.elements_at(slot) {
            let count = elements.len();
            for index in 0..count {
                keys.push(crate::coerce::number_to_string(index as f64));
            }
        }
        let Some(ty) = context.region.type_of(slot) else {
            return keys;
        };
        let Some(shape) = context.shape_of(ty) else {
            return keys;
        };
        for (key, _) in context.shapes.properties(shape) {
            // An array's `length` and a collection's `size` are real properties
            // — so that compiled code and the runtime answer them the same way
            // — and the language says neither is enumerable. That used to be a
            // special case here naming `length`; it is now the ordinary
            // attribute every property has, recorded where each is written.
            //
            // Caught by a probe when it was missing: `for (k in [1,2,3])`
            // summed to 9 instead of 6, because the loop visited "length".
            if enumerable_only && !super::integrity::enumerable(context, slot, key) {
                continue;
            }
            if let Some(text) = context.interner.text(key) {
                // A symbol-keyed property is not enumerated. Its key lives in a
                // reserved name space rather than in a third `Key` variant —
                // see [`super::symbol`] for why — so this is the one place the
                // encoding has to be known, and it is the whole cost of that
                // decision.
                if text
                    .to_rust()
                    .as_deref()
                    .is_some_and(super::symbol::is_symbol_key)
                {
                    continue;
                }
                keys.push(text.clone());
            }
        }
        // Accessors are own properties too, and enumerable ones. They are not
        // in the layout — deliberately, so a cached read cannot find a getter
        // and return it — so a walk of the shape alone reports an object with
        // `get b()` as having no `b` at all. `Object.keys` and `for (k in o)`
        // both read this, and both were wrong until it was added.
        //
        // After the layout's, because the shape's order is the order they were
        // created in and an accessor was created by a separate operation. That
        // is a divergence for an object mixing the two: the specification
        // interleaves them in creation order, and recording that needs the
        // shape to know about a property it is deliberately not holding.
        if let Some(defined) = context.accessors_at(slot) {
            for key in defined {
                if let Some(text) = context.interner.text(key) {
                    keys.push(text.clone());
                }
            }
        }
        ordered(keys)
    }
}

/// The shared walk.
fn keys_of(object: u64, enumerable_only: bool) -> u64 {
    let texts = with_current(|context| key_texts(context, object, enumerable_only));

    // Built outside the borrow above, because interning each string and
    // allocating the array both need the context again.
    let array = array_new(texts.len() as i64);
    with_current(|context| {
        let values: Vec<u64> = texts
            .into_iter()
            .map(|text| context.intern_value(text).bits())
            .collect();
        if let Some(slot) = Value(array).as_slot()
            && let Some(elements) = context.elements_at_mut(slot)
        {
            *elements = values;
        }
    });
    array
}

/// Writes an array's `length` as an ordinary property.
///
/// # Why a real property and not an answer the runtime invents
///
/// It WAS invented: `get_property` special-cased the key and answered the
/// element count. That worked until something stored a `length` property, and
/// then the two paths disagreed — because compiled code does not go through
/// `get_property` for a hit. It emits `cached_get`, which finds the stored
/// property and never asks the runtime at all.
///
/// A special case only the slow path knows about is a special case that stops
/// applying the moment the fast path starts working, which is the opposite of
/// how a fast path is supposed to fail. So the count is a property both read.
///
/// The divergence that remains, named: assigning `length` stores a number and
/// does not truncate the array, where the language shortens it. Truncating
/// needs the write to know it is writing to an array, which is `put`'s caller
/// rather than `put`.
pub(super) fn set_length(context: &mut Context, cell: u32, length: usize) {
    let key = super::computed::length_key(context);
    let value = Value::from_f64(length as f64).bits();
    super::objects::put(context, cell, key, value);
    // Not enumerable, which is the language — and recorded here, at the one
    // funnel every array's length passes through, rather than as a name
    // `own_keys` knows to skip. Written every time rather than once at
    // construction: an array reaches this before it has the property at all,
    // and the attribute has nowhere to live until it does.
    if let crate::object::Key::Name(key) = key {
        super::integrity::set_attributes(context, cell, key, super::integrity::Attributes {
            enumerable: false,
            ..super::integrity::Attributes::default()
        });
    }
}

/// The enumeration order the language states, over keys already collected.
///
/// **Array-index keys first, in ascending numeric order; then everything else
/// in the order it was added.** Not insertion order overall, and the difference
/// is not exotic: `o.b = 1; o[2] = 1; o.a = 1; o[1] = 1` enumerates
/// `1, 2, b, a`.
///
/// # Why this is a sort here rather than a `Key::Index`
///
/// [`super::computed`] turns every computed key into a NAME, including one that
/// spells an index, and its own documentation says why: `o[0]` and `o["0"]` are
/// one property, and routing an object's key through `Key::from_str` made
/// `o[0] = 1; o[0]` read as absent. What that gave up was exactly this ordering,
/// recorded there as "an enumeration order nothing implements".
///
/// So it is implemented from the text instead. A key is an index when its
/// spelling is the canonical one for a number below 2^32 − 1 — which is what
/// makes `"01"` and `"1.0"` ordinary names, as the specification says, and what
/// a `parse` alone would have got wrong.
fn ordered(keys: Vec<Str>) -> Vec<Str> {
    let index_of = |text: &Str| -> Option<u32> {
        let text = text.to_rust()?;
        let number: u32 = text.parse().ok()?;
        // The canonical spelling, and only that: `"01"` parses to 1 and is not
        // an index, because the language decides by the round trip rather than
        // by the value.
        (number.to_string() == text && number != u32::MAX).then_some(number)
    };

    let mut indices: Vec<(u32, Str)> = Vec::new();
    let mut names: Vec<Str> = Vec::new();
    for key in keys {
        match index_of(&key) {
            Some(number) => indices.push((number, key)),
            None => names.push(key),
        }
    }
    indices.sort_by_key(|(number, _)| *number);

    let mut out: Vec<Str> = indices.into_iter().map(|(_, key)| key).collect();
    out.extend(names);
    out
}
