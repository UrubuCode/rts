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

use super::{Cell, Context, with_current};
use crate::heap::Slot;
use crate::object::Key;
use crate::object::Object;
use crate::value::Value;

/// `{}` — a new object with no properties.
///
/// No prototype, and that is a stated gap rather than a decision:
/// `Object.prototype` does not exist in this runtime yet, so the chain is empty
/// and every inherited property is absent. Visible, rather than wrong-looking.
#[rtse::entry]
pub fn object_new() -> u64 {
    with_current(|context| {
        let shape = context.shapes.root();
        let object = Object::new(shape, Vec::new(), None);
        let slot = context.cells.insert(Cell::Object(object)).slot();
        Value::from_slot(slot.0).bits()
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
        match read(context, Slot(slot), key) {
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
        // Two fields of one context, borrowed apart: the heap holds the object
        // and the shape tree holds its layout, and a write needs both at once.
        let cells = &mut context.cells;
        let shapes = &mut context.shapes;
        if let Ok(Cell::Object(object)) = cells.at_mut(Slot(slot)) {
            object.set_own(shapes, key, Value(value));
        }
        value
    })
}

/// Reads a property, walking the prototype chain.
///
/// # Why the chain is collected before anything is read
///
/// The walk needs the heap immutably and the layout lookup needs the shape tree
/// mutably, and both live in one context. Taking the chain first — which needs
/// only the heap — leaves the two borrows disjoint, instead of forcing a clone
/// of an object per link.
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
fn read(context: &mut Context, start: Slot, key: Key) -> Option<Value> {
    let mut chain = vec![start];
    loop {
        let Ok(Cell::Object(object)) = context.cells.at(*chain.last()?) else {
            return None;
        };
        match object.prototype() {
            Some(next) => chain.push(next),
            None => break,
        }
    }

    let cells = &context.cells;
    let shapes = &mut context.shapes;
    for slot in chain {
        let Ok(Cell::Object(object)) = cells.at(slot) else {
            continue;
        };
        if let Some(value) = object.own_value(shapes, key) {
            return Some(value);
        }
    }
    None
}

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
fn undefined_of(context: &Context) -> u64 {
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
                let shape_of = |value: u64| {
                    let slot = Value(value).as_slot().expect("an object");
                    match context.cells.at(Slot(slot)) {
                        Ok(Cell::Object(object)) => object.shape(),
                        _ => panic!("an object"),
                    }
                };
                assert_eq!(shape_of(first), shape_of(second));
            });
        });
    }
}
