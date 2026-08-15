//! `[[DefineOwnProperty]]` where the receiver is an **array**.
//!
//! # Why an array needs its own definition at all
//!
//! Because two of its keys are not properties. `super::array`'s first page says
//! why an element lives in a growable store instead of in the shape tree, and
//! `length` is the count of that store written back as an ordinary property —
//! so the generic definition, which resolves a key to a shape slot and writes
//! there, answers a different object than the one the program is looking at.
//!
//! It was observable and not theoretical.
//! `Object.defineProperty(a, "6", {value: 7})` on `[1,2,3,4]` created a *named*
//! property spelled `"6"`: `Object.keys(a)` reported it, `a.length` stayed 4,
//! `a[6]` read the element store and answered `undefined`, and `a.length = 2`
//! did not remove it. Four readable facts about one array, three of them wrong.
//!
//! # What is here and what is deliberately not
//!
//! `ArraySetLength` for the `length` key, and the element write for a canonical
//! index. An ACCESSOR at an index falls through to the generic path unchanged:
//! the element store holds values, so a getter there has nowhere to live, and
//! answering the generic path's behaviour is the honest half-answer rather than
//! a silently dropped definition.
//!
//! The other stated gap: an element carries no attributes. `writable: false` on
//! `a[0]` is not recorded, because [`super::super::integrity`] keys its records
//! by SHAPE key and an element has none. That is the same absence a plain
//! `a[0] = 1` already has, rather than a new one — an array's elements have
//! never had a per-element record here — and it is named because the definition
//! ACCEPTS such a descriptor instead of refusing it.

use super::super::array;
use super::super::integrity;
use super::super::objects::undefined_of;
use super::super::{Context, with_current};
use super::descriptor::{Descriptor, Verdict};
use crate::object::Key;
use crate::value::Value;

/// Which of an array's two non-ordinary keys a definition names.
enum Named {
    /// `length` — the count, which resizes the store rather than storing a
    /// number beside it.
    Length,
    /// A canonical array index.
    Element(usize),
}

/// The definition, when the receiver is an array and the key is one of its own.
///
/// `None` sends the caller to the ordinary path: the receiver is not an array,
/// the key is an ordinary name (`a.tag = 1` is a real property on an array), or
/// the descriptor asks for something an element cannot be.
pub(super) fn define(object: u64, name: u64, wanted: &Descriptor) -> Option<Verdict> {
    let found = with_current(|context| {
        let cell = Value(object).as_slot()?;
        // An array, and only an array: a `{length: 3}` object has nothing to
        // reconcile, which is the same line `objects::reconcile_length` draws.
        context.elements_at(cell)?;
        let key = super::key_for(context, name)?;
        if key == super::super::computed::length_key(context) {
            return Some((cell, Named::Length));
        }
        let Key::Name(named) = key else {
            return None;
        };
        // The CANONICAL spelling only, which `as_array_index` is the one answer
        // to: `a["01"]` and `a["1.0"]` are ordinary properties, and a `parse`
        // here would have made them elements while `a[k]` for the same `k`
        // still read a property.
        let text = context.interner.text(named)?;
        let at = crate::object::as_array_index(text)? as usize;
        Some((cell, Named::Element(at)))
    })?;
    match found {
        (cell, Named::Length) => length(cell, wanted),
        (cell, Named::Element(at)) => element(cell, at, wanted),
    }
}

/// `ArraySetLength` — the length a descriptor states, and the store resized to
/// match it.
///
/// # Why the number is checked before anything else
///
/// The specification puts the `RangeError` at step 3, ahead of every
/// compatibility question a definition normally asks. `{value: 1.5}` is not a
/// property that refuses to be redefined — it is a length that is not a length,
/// and reporting it as the former names the wrong mistake.
fn length(cell: u32, wanted: &Descriptor) -> Option<Verdict> {
    let Some(stated) = wanted.value else {
        // Only the flags: `{writable: false}` is what stops `push` growing the
        // array, and the ordinary path already records it against the `length`
        // key. Nothing to resize.
        return None;
    };
    // OUTSIDE the borrow: `ToNumber` runs `valueOf`, which is user code.
    let number = super::super::class_support::to_number(stated);
    if super::super::throw::in_flight() {
        return Some(Verdict::Done);
    }
    let wanted_length = to_uint32(number);
    if f64::from(wanted_length) != number {
        return Some(Verdict::BadLength);
    }
    Some(with_current(|context| {
        let held = context.elements_at(cell).map_or(0, Vec::len);
        if held == wanted_length as usize {
            return Verdict::Done;
        }
        // A `length` the object itself refuses to write is a refusal to resize,
        // which is the whole of what `{writable: false}` on it buys a program.
        if let Key::Name(key) = super::super::computed::length_key(context)
            && integrity::refuses_key_write(context, cell, key)
        {
            return Verdict::Refused;
        }
        resize(context, cell, wanted_length as usize);
        Verdict::Done
    }))
}

/// The definition of one element, and the `length` that has to follow it.
fn element(cell: u32, at: usize, wanted: &Descriptor) -> Option<Verdict> {
    if wanted.get.is_some() || wanted.set.is_some() {
        // An accessor has nowhere to live in the element store — see this
        // module's first page for why that is the generic path's problem
        // rather than a refusal here.
        return None;
    }
    Some(with_current(|context| {
        let held = context.elements_at(cell).map_or(0, Vec::len);
        let grows = at >= held;
        // Growth is what every integrity level refuses, and an array's `length`
        // being non-writable refuses it a second way — `Object.freeze([1])`
        // reaches both.
        if grows {
            if integrity::refuses_growth(context, cell) {
                return Verdict::Refused;
            }
            if let Key::Name(key) = super::super::computed::length_key(context)
                && integrity::refuses_key_write(context, cell, key)
            {
                return Verdict::Refused;
            }
        } else if integrity::refuses_write(context, cell) {
            return Verdict::Refused;
        }
        // A descriptor with no `value` gives a NEW element `undefined` — present
        // rather than a hole, which is the difference between
        // `Object.defineProperty(a, "5", {})` and `a.length = 6`.
        let absent = undefined_of(context);
        let value = match wanted.value {
            Some(stated) => stated,
            None if grows => absent,
            None => return Verdict::Done,
        };
        if grows {
            resize(context, cell, at + 1);
        }
        if let Some(elements) = context.elements_at_mut(cell) {
            elements[at] = value;
        }
        Verdict::Done
    }))
}

/// The store at a new length, and the `length` property that reports it.
///
/// Grown with HOLES rather than `undefined`, which is what the language says an
/// index a definition skipped is: `Object.defineProperty([1], "3", {value: 4})`
/// leaves 1 and 2 absent, so `Object.keys` must not report them and `join` must
/// print nothing for them.
///
/// The property is written through `array::set_length` rather than by hand, so
/// that an array's count keeps having one writer — that function is also where
/// its non-enumerability is recorded.
fn resize(context: &mut Context, cell: u32, length: usize) {
    let hole = array::hole_of(context);
    if let Some(elements) = context.elements_at_mut(cell) {
        elements.resize(length, hole);
    }
    array::set_length(context, cell, length);
}

/// `ToUint32`, for the one comparison [`length`] makes.
///
/// Written here rather than reached for because the runtime has no other caller:
/// every arithmetic path takes a double, and this is the single place the
/// language asks whether a number is the canonical spelling of a uint32.
fn to_uint32(number: f64) -> u32 {
    if !number.is_finite() {
        return 0;
    }
    let wrapped = number.trunc().rem_euclid(4_294_967_296.0);
    wrapped as u32
}
