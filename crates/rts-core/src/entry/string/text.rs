//! What a string answers without inheriting it.
//!
//! # Why these two are not on the prototype
//!
//! `length` and an index are facts about the text itself, not functions to call,
//! and they are answered before the chain walk begins. The methods went the
//! other way — see [`super`] — and the line between them is whether the answer
//! is a value the string already determines.
//!
//! # Why a special case is safe here and was not for an array
//!
//! An array`s `length` had to become a real property, because compiled code
//! reaches `cached_get` and finds whatever is stored — a special case only the
//! runtime knew about stopped applying the moment the fast path started
//! working.
//!
//! A string cell has no shape, so `cache_resolve` answers negative for it and
//! every read of one takes the slow path. There is nothing for a special case
//! to disagree with, and nothing can store a property on a string to shadow it:
//! `put` is already a no-op for a cell with no shape, which is what sloppy mode
//! does to `"x".foo = 1`.

use super::super::Context;
use crate::object::Key;
use crate::value::Value;


/// What a property read on a **string** answers, if this is one.
///
/// # Why a special case is safe here and was not for an array
///
/// An array's `length` had to become a real property, because compiled code
/// reaches `cached_get` and finds whatever is stored — a special case only the
/// runtime knew about stopped applying the moment the fast path started
/// working.
///
/// A string cell has no shape, so `cache_resolve` answers negative for it and
/// every read of one takes the slow path. There is nothing for a special case
/// to disagree with, and nothing can store a property on a string to shadow it:
/// `put` is already a no-op for a cell with no shape, which is what sloppy mode
/// does to `"x".foo = 1`.
///
/// # `length` counts code units
///
/// Not characters and not scalar values. `"\u{1F600}".length` is 2, because a
/// JavaScript string IS a sequence of UTF-16 code units — the decision
/// `crate::text` is built around, read out here rather than re-derived.
pub(in crate::entry) fn string_property(context: &mut Context, slot: u32, key: Key) -> Option<u64> {
    let wanted = super::super::computed::length_key(context);
    let text = context.text_at(slot)?;
    (key == wanted).then(|| Value::from_f64(text.len() as f64).bits())
}

/// The code unit at an index of a string, as a one-unit string.
///
/// Out of range is `undefined` rather than an empty string, which is the
/// difference between `s[9]` and `s.charAt(9)` — and the reason this answers an
/// `Option` rather than always producing text.
pub(in crate::entry) fn string_element(context: &mut Context, slot: u32, key: Value) -> Option<u64> {
    let at = super::super::array::as_index(context, key)?;
    let unit = context.text_at(slot)?.unit_at(at)?;
    Some(
        context
            .intern_value(crate::text::Str::from_utf16(&[unit]))
            .bits(),
    )
}
