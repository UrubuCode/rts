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
//! deciding. The one property that is not a property is `length`, which is the
//! element count and is answered from the store.

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
        let elements = vec![absent; length.max(0) as usize];
        let store = context.arrays.insert(elements).slot();

        let shape = context.shapes.root();
        let ty = context.layout_of(shape).index() as u32;
        match context.region.alloc(crate::heap::STRIDE, ty) {
            Some(cell) => {
                context.mark_array(cell, store);
                Value::from_slot(cell).bits()
            }
            // The region is full and there is no collector to ask — the same
            // answer `object_new` gives, and less wrong than handing back cell
            // zero, which is a real object belonging to somebody else.
            None => absent,
        }
    })
}

impl Context {
    /// Records that a cell is an array, and where its elements are.
    fn mark_array(&mut self, cell: u32, store: Slot) {
        if self.array_elements.len() <= cell as usize {
            self.array_elements.resize(cell as usize + 1, None);
        }
        self.array_elements[cell as usize] = Some(store);
    }

    /// Where a cell's elements are, if it is an array.
    fn store_of(&self, reference: u32) -> Option<Slot> {
        *self.array_elements.get(reference as usize)?
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
pub(super) fn as_index(key: Value) -> Option<usize> {
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
    let texts = with_current(|context| {
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
            if let Some(text) = context.interner.text(key) {
                keys.push(text.clone());
            }
        }
        keys
    });

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
