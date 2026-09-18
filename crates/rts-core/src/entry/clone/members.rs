//! An object's members, read the cheap way when that is the same answer.
//!
//! # Two reads, one answer
//!
//! The language reads a member with `[[Get]]`, which runs a getter. A walk
//! that reads every member that way leaves the borrow once per member, because
//! a getter is user code — and that was the clone's only way to read one.
//!
//! Most objects have no getter. For those, `[[Get]]` of an own data property
//! IS the slot, so reading the slot inside the borrow is the same answer
//! without the crossing. [`data`] does that and answers `None` the moment a
//! key turns out to be an accessor, which sends the object down [`called`] —
//! the old path, outside the borrow, running whatever it has to.

use super::super::{Context, with_current};
use super::Refusal;
use crate::object::Key;
use crate::value::Value;

/// Every enumerable own member, as a key and the value its slot holds, or
/// `None` when one of them is an accessor.
///
/// `private` adds the class's `#` fields after the public ones, in the order
/// the shape holds them: they are an instance's state, which is what the
/// pickle writes and the clone never does. The ORDER of the public ones is the
/// runtime's one answer — `key_list` — and not re-derived here.
pub(super) fn data(context: &mut Context, value: u64, cell: u32, private: bool) -> Option<Vec<(Key, u64)>> {
    let keys = super::super::array::key_list(context, value, true);
    let mut read = Vec::with_capacity(keys.len());
    for key in keys {
        // `key_list` over an object that is not an array answers names only —
        // an index is an element, and only an array or a string has those.
        if matches!(key, Key::Index(_)) {
            continue;
        }
        let held = super::super::objects::own_property(context, cell, key)?;
        read.push((key, held.bits()));
    }
    if private {
        let Some(shape) = context.region.type_of(cell).and_then(|ty| context.shape_of(ty)) else {
            return Some(read);
        };
        for (named, _) in context.shapes.properties(shape) {
            let is_private = context
                .interner
                .text(named)
                .is_some_and(super::super::symbol::is_private_key);
            if !is_private {
                continue;
            }
            let key = Key::Name(named);
            if let Some(held) = super::super::objects::own_property(context, cell, key) {
                read.push((key, held.bits()));
            }
        }
    }
    Some(read)
}

/// The same members, read the way the language reads them: through
/// `own_keys` and `get_indexed`, outside any borrow, so an accessor runs its
/// getter and a proxy its traps.
///
/// Asks after every read whether the getter threw — rule 8 of this crate's
/// README — and stops there rather than walking on with `undefined` standing
/// in for a value that was never produced.
pub(super) fn called(value: u64) -> Result<Vec<(Key, u64)>, Refusal> {
    let names = super::super::array::own_keys(value);
    let names = with_current(|context| {
        Value(names)
            .as_slot()
            .and_then(|cell| context.elements_at(cell).cloned())
            .unwrap_or_default()
    });
    let mut read = Vec::with_capacity(names.len());
    for name in names {
        let held = super::super::computed::get_indexed(value, name);
        if super::super::throw::in_flight() {
            return Err(Refusal::Thrown);
        }
        let key = with_current(|context| {
            Value(name).as_slot().and_then(|cell| context.key_of_text_cell(cell))
        });
        if let Some(key) = key {
            read.push((key, held));
        }
    }
    Ok(read)
}

/// An array's own members that are NOT one of its indices — every named
/// property a program hung on it after the literal — or `None` when one of
/// them is an accessor.
///
/// `length` is not among them: it is not enumerable, and the array's own
/// `length` write reproduces it.
pub(super) fn array_extra(context: &mut Context, value: u64, cell: u32) -> Option<Vec<(Key, u64)>> {
    let keys = super::super::array::key_list(context, value, true);
    let mut read = Vec::new();
    for key in keys {
        if matches!(key, Key::Index(_)) {
            continue;
        }
        let held = super::super::objects::own_property(context, cell, key)?;
        read.push((key, held.bits()));
    }
    Some(read)
}
