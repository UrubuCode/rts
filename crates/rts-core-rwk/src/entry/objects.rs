//! Objects: making one, reading a property, writing one.
//!
//! # Why a property key crosses as a number
//!
//! A property name in a compiled program is known while compiling, so handing
//! over its text at every access would hand over something the compiler already
//! resolved. The machine's key registry numbers names, and the number is what
//! crosses.
//!
//! That makes the two sides agree about *which* registry — a compiler numbering
//! `x` as 3 and a runtime reading 3 as `y` is a program that runs and is wrong.
//! Nothing here can check it, because a number carries no evidence of where it
//! came from. The host wires one registry into both and says so, which is the
//! same shape as the singleton numbering and for the same reason.
//!
//! # Why these are entry points
//!
//! Making an object allocates. Reading a property walks a prototype chain
//! through the heap. Writing one may move the object to a new layout. For once
//! the membership rule needs no argument.
//!
//! # What is not here
//!
//! The fast path. Every access below is a call that looks a key up in a layout,
//! and the machine has `cached_get` and `guard_type` for a site that keeps
//! seeing the same shape. That is the next step and deliberately a separate one:
//! this makes property access *correct*, and a cache built before there was
//! something correct to cache would be a cache over a guess.

use super::{Context, with_current};
use crate::object::Key;
use crate::value::Value;
use rts_cranelift::repr::Repr;
use rts_cranelift::shape::Key as ShapeKey;

/// `{}` — a new object with no properties.
///
/// No prototype, and that is a stated gap rather than a decision:
/// `Object.prototype` does not exist in this runtime yet, so the chain is empty
/// and every inherited property is absent. Visible, rather than wrong-looking.
#[rtse::entry]
pub fn object_new() -> u64 {
    with_current(|context| {
        // The empty layout, which every object that gains its first property
        // transitions out of — which is what makes two objects built the same
        // way share a shape.
        let shape = context.shapes.root();
        let ty = context.layout_of(shape).index() as u32;
        match context.region.alloc(crate::heap::STRIDE, ty) {
            Some(cell) => Value::from_slot(cell).bits(),
            // The region is full and there is no collector to ask. Answering
            // `undefined` is wrong — the language makes an object here — and it
            // is less wrong than handing back cell zero, which is a real object
            // belonging to somebody else.
            None => undefined_of(context),
        }
    })
}

/// `object.name`, the name given as its key number.
///
/// Answers `undefined` for a property that is not there, which is what the
/// language does rather than an error being swallowed: reading an absent
/// property is legal.
///
/// It also answers `undefined` for a receiver that is not an object, and that is
/// **not** what the language does — `null.x` throws a `TypeError`. A stated gap:
/// throwing needs the machine's protected regions, and nothing emits those yet.
#[rtse::entry]
pub fn get_property(object: u64, key: i64) -> u64 {
    with_current(|context| {
        let Some(slot) = Value(object).as_slot() else {
            return undefined_of(context);
        };
        let Some(key) = key_of(context, key) else {
            return undefined_of(context);
        };
        if let Some(answer) = string_property(context, slot, key) {
            return answer;
        }
        match read(context, slot, key) {
            Some(value) => value.bits(),
            None => undefined_of(context),
        }
    })
}

/// `object.name = value`. Answers the value, because an assignment is an
/// expression.
#[rtse::entry]
pub fn set_property(object: u64, key: i64, value: u64) -> u64 {
    with_current(|context| {
        let Some(slot) = Value(object).as_slot() else {
            // A write to a non-object is a silent no-op in sloppy mode and a
            // `TypeError` in strict. Neither is emitted yet, and the value comes
            // back either way because that is what the expression produces.
            return value;
        };
        let Some(key) = key_of(context, key) else {
            return value;
        };
        put(context, slot, key, value);
        value
    })
}

/// Puts a value at a key, taking the shape transition if the object does not
/// have that property yet.
///
/// Shared by the named write and the computed one. The transition is the part
/// that is easy to get subtly wrong, and there is no version of it that differs
/// between the two: by the time either arrives here, a key is a key.
///
/// Named `put` rather than `write` because `write` is a macro in scope, and a
/// call that silently resolves to one instead is a compile error whose message
/// points at the wrong thing.
pub(super) fn put(context: &mut Context, slot: u32, key: Key, value: u64) {
        let Some(machine) = machine_key(key) else {
            return;
        };
        let Some(ty) = context.region.type_of(slot) else {
            return;
        };
        let Some(shape) = context.shape_of(ty) else {
            // A string, or a layout nothing recorded. Writing a property to a
            // string is a silent no-op in sloppy mode, which is what this is.
            return;
        };

        // Already in the layout: a store, at the offset the layout decided.
        if let Some(at) = context.shapes.slot_of(shape, machine) {
            set_slot_value(context, slot, at, value);
            return;
        }

        // A new property changes what the object IS, so the shape moves and the
        // header moves with it. Taking the transition rather than choosing a
        // slot is what keeps two objects built the same way at one layout.
        // The representation the shape records is what the VALUE turned out to
        // be, not `Tagged` unconditionally. A shape already carries one per
        // property — `transition` takes it and `repr_of` reads it back — and
        // writing `Tagged` for everything made that field a place where a fact
        // could have been and was not.
        //
        // It is an observation about one write rather than a promise about the
        // property: a later write of something else takes a different
        // transition, so the object arrives at a different shape and every site
        // that remembered the old one stops recognising it. Which is what a
        // shape is for.
        let observed = if Value(value).numeric().is_some() {
            Repr::F64
        } else {
            Repr::Tagged
        };
        let Ok(grown) = context.shapes.transition(shape, machine, observed) else {
            return;
        };
        let Some(at) = context.shapes.slot_of(grown, machine) else {
            return;
        };
        let ty = context.layout_of(grown).index() as u32;
        context.region.set_type(slot, ty);
        // Past the seventh this goes to the spill beside the cell rather than
        // being refused, which is what "the overflow indirection, and it is not
        // implemented here" was waiting for.
        set_slot_value(context, slot, at, value);
}

/// Reads a property: header to type, type to shape, shape to offset, load.
///
/// Three lookups where there were a hash map and a call, and none of them is
/// what compiled code will do — it will guard the type and load at a constant
/// offset, with no lookup at all. This is the runtime's own path, for the case
/// where the shape was not known while compiling.
///
/// # There is no walk any more, and that is a REGRESSION
///
/// A prototype chain needs somewhere to put the prototype, and a region cell is
/// a header and seven slots with no field reserved for one. Objects made here
/// have no prototype — they did not before either, because `Object.prototype`
/// does not exist in this runtime — so nothing observable changes today.
///
/// What changes is that the mechanism is gone rather than unused. It comes back
/// when a cell has a place for a prototype, and this is where to look.
///
/// # What it does not implement, and does not pretend to
///
/// Accessors. `crate::object::ordinary_get` states the two rules a correct read
/// obeys — the receiver is threaded rather than replaced, and the walk stops on
/// *presence* rather than on a non-`undefined` value — and it is the one
/// implementation of them. This is data-only, because a getter is a call into
/// user code and an entry point may not make one.
///
/// The second rule is still obeyed here and it matters: an own property holding
/// `undefined` shadows an inherited one, so the walk stops when the key is
/// **found**, not when a non-`undefined` value is found.
pub(super) fn read_property(context: &mut Context, start: u32, key: Key) -> Option<Value> {
    read(context, start, key)
}

fn read(context: &mut Context, start: u32, key: Key) -> Option<Value> {
    let machine = machine_key(key)?;
    let mut cell = start;
    // Bounded rather than trusting the chain to end. Nothing here builds a
    // cycle, but a prototype is a value a program can set, and a walk that
    // trusted it would hang instead of answering — which is a worse failure
    // than a wrong value, because nothing reports it at all.
    for _ in 0..CHAIN_LIMIT {
        if let Some(ty) = context.region.type_of(cell)
            && let Some(shape) = context.shape_of(ty)
            && let Some(at) = context.shapes.slot_of(shape, machine)
        {
            return slot_value(context, cell, at).map(Value);
        }
        // Not here, so ask what this inherits from. An element or a string.s
        // length never reaches this loop: those are answered before it.
        cell = Value(context.prototype_at(cell)?).as_slot()?;
    }
    None
}

/// How far a prototype walk goes before giving up.
///
/// A depth no real chain reaches. It is a guard against a cycle a program
/// built, not a limit on inheritance — and answering `undefined` for one is
/// better than not answering, which is what a plain loop would do.
pub(super) const CHAIN_LIMIT: usize = 1 << 16;

/// The key a number names, if the registry issued it.
///
/// `None` for a number nothing minted, which is a compiled program naming a
/// property the host never wired up. It reads as an absent property rather
/// than as an invented one — a key cannot be conjured from an integer, and the
/// registry refusing is what says so.
fn key_of(context: &Context, number: i64) -> Option<Key> {
    let number = u32::try_from(number).ok()?;
    context.keys.key(number).map(Key::Name)
}

/// The encoded `undefined`, from the numbering the language declared.
pub(super) fn undefined_of(context: &Context) -> u64 {
    rts_cranelift::tags::encode(
        rts_cranelift::tags::TAG_SINGLETON,
        u64::from(context.singletons.undefined),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::with_context;
    use crate::value::Singletons;

    fn hosted<T>(body: impl FnOnce() -> T) -> T {
        let singletons = Singletons {
            undefined: 0,
            null: 1,
        };
        let mut context = Context::new(singletons);
        // The keys a test names have to have been issued, exactly as a host
        // issues the ones a program names. A registry that minted nothing
        // refuses every number, which is the behaviour being relied on.
        context.keys.declare(16);
        with_context(context, body).1
    }

    #[test]
    fn a_property_written_is_the_property_read() {
        hosted(|| {
            let object = object_new();
            set_property(object, 7, Value::from_f64(42.0).bits());
            assert_eq!(
                rts_cranelift::tags::decode_double(get_property(object, 7)),
                42.0
            );
        });
    }

    #[test]
    fn an_absent_property_is_undefined_rather_than_an_error() {
        hosted(|| {
            let object = object_new();
            let read = get_property(object, 7);
            assert_eq!(rts_cranelift::tags::tag_of(read), rts_cranelift::tags::TAG_SINGLETON);
        });
    }

    #[test]
    fn writing_a_second_property_does_not_disturb_the_first() {
        // What the shape transition has to get right: the second key lands in a
        // new slot rather than over the first, and the first object's layout
        // moves with it.
        hosted(|| {
            let object = object_new();
            set_property(object, 1, Value::from_f64(10.0).bits());
            set_property(object, 2, Value::from_f64(20.0).bits());
            assert_eq!(
                rts_cranelift::tags::decode_double(get_property(object, 1)),
                10.0
            );
            assert_eq!(
                rts_cranelift::tags::decode_double(get_property(object, 2)),
                20.0
            );
        });
    }

    #[test]
    fn overwriting_a_property_reuses_its_slot_rather_than_growing() {
        // The other half of the transition: a key already in the layout is a
        // store, not a new property. An implementation that transitioned every
        // time would grow an object without bound and give two objects built the
        // same way different shapes.
        hosted(|| {
            let object = object_new();
            set_property(object, 1, Value::from_f64(10.0).bits());
            set_property(object, 1, Value::from_f64(11.0).bits());
            assert_eq!(
                rts_cranelift::tags::decode_double(get_property(object, 1)),
                11.0
            );
        });
    }

    #[test]
    fn two_objects_built_the_same_way_share_one_layout() {
        // The reason a shape is a tree rather than a per-object list, and the
        // property an inline cache depends on: a site that has seen one of these
        // recognises the other.
        hosted(|| {
            let first = object_new();
            let second = object_new();
            set_property(first, 3, Value::from_f64(1.0).bits());
            set_property(second, 3, Value::from_f64(2.0).bits());
            crate::entry::with_current(|context| {
                // The header IS the shape, now: a cell records the type its
                // layout arrived at, so two objects at one layout carry the
                // same word.
                let type_of = |value: u64| {
                    let cell = Value(value).as_slot().expect("an object");
                    context.region.type_of(cell).expect("a live cell")
                };
                assert_eq!(type_of(first), type_of(second));
            });
        });
    }
}

/// The machine key a [`Key`] is.
///
/// An index has no machine key, for the reason `crate::object` gives: indexed
/// storage is an array's problem and an array is not yet a thing.
pub(super) fn machine_key(key: Key) -> Option<ShapeKey> {
    match key {
        Key::Name(name) => Some(name),
        Key::Index(_) => None,
    }
}

/// `ToPropertyKey`, for a key a program computed rather than wrote.
///
/// # Why an index becomes a name here
///
/// [`Key`] distinguishes a canonical integer index from any other string,
/// because enumeration order does: indices come first, in numeric order. That
/// distinction is real and it is **not usable yet** — `machine_key` answers
/// `None` for an index, because indexed storage waits for arrays.
///
/// So an index is held under its own spelling instead: `o[0]` and `o["0"]` are
/// choice is here rather than at each of the four call sites.
pub(super) fn slot_value(context: &Context, cell: u32, at: u32) -> Option<u64> {
    match at.checked_sub(crate::heap::INLINE_SLOTS) {
        None => context.region.field(cell, at),
        Some(past) => context.spill_at(cell)?.get(past as usize).copied(),
    }
}

/// Writes a property's value, wherever the slot puts it.
pub(super) fn set_slot_value(context: &mut Context, cell: u32, at: u32, value: u64) {
    match at.checked_sub(crate::heap::INLINE_SLOTS) {
        None => {
            context.region.set_field(cell, at, value);
        }
        Some(past) => context.spill_set(cell, past as usize, value),
    }
}

impl Context {
    /// A cell's spilled properties, if it has any.
    fn spill_at(&self, cell: u32) -> Option<&Vec<u64>> {
        self.spills.at(self.spill_of.copied(cell)?).ok()
    }

    /// Puts a value in a cell's spill, making one and growing it as needed.
    ///
    /// Grown with `undefined` rather than zero: a gap here is a property the
    /// shape says exists, and zero is not a value — it decodes as a double, so
    /// a reader would get `0` where the language says `undefined`.
    fn spill_set(&mut self, cell: u32, past: usize, value: u64) {
        let absent = undefined_of(self);
        let slot = match self.spill_of.copied(cell) {
            Some(slot) => slot,
            None => {
                let slot = self.spills.insert(Vec::new()).slot();
                self.spill_of.set(cell, slot);
                slot
            }
        };
        let Ok(spill) = self.spills.at_mut(slot) else {
            return;
        };
        if spill.len() <= past {
            spill.resize(past + 1, absent);
        }
        spill[past] = value;
    }
}

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
pub(super) fn string_property(context: &mut Context, slot: u32, key: Key) -> Option<u64> {
    let wanted = super::computed::length_key(context);
    let text = context.text_at(slot)?;
    (key == wanted).then(|| Value::from_f64(text.len() as f64).bits())
}

/// The code unit at an index of a string, as a one-unit string.
///
/// Out of range is `undefined` rather than an empty string, which is the
/// difference between `s[9]` and `s.charAt(9)` — and the reason this answers an
/// `Option` rather than always producing text.
pub(super) fn string_element(context: &mut Context, slot: u32, key: Value) -> Option<u64> {
    let at = super::array::as_index(context, key)?;
    let unit = context.text_at(slot)?.unit_at(at)?;
    Some(context.intern_value(crate::text::Str::from_utf16(&[unit])).bits())
}
