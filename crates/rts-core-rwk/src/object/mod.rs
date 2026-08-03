//! Objects: a machine layout, a row of values, and a prototype.
//!
//! # The layout is not ours
//!
//! [`rts_cranelift::shape::ShapeTree`] owns it. Its own documentation is the
//! argument for why nothing here reimplements it:
//!
//! > A shape *is* an aggregate — it is a way of arriving at one incrementally.
//! > […] reading a property needs no new instruction.
//!
//! So an object is a `ShapeId`, a row of values, and a prototype. Reading a
//! property is `slot_of` and an indexed read; the machine's `layout()` turns the
//! same shape into a registered aggregate so that compiled code reads the same
//! field at a fixed offset. One layout, two ways of arriving at the field, and
//! they cannot disagree because there is one tree.
//!
//! # A class is this mechanism with the answer known early
//!
//! A class declares its fields, so the shape is known **before any instance
//! exists**. Walk the transitions once when the class is defined, keep the
//! `ShapeId`, and every instance starts there — no dictionary, no per-instance
//! discovery, and `layout()` gives compiled code fixed offsets for all of them.
//! [`shape_for`] is that walk, and it is the whole of what class creation needs
//! from this module.
//!
//! # What is deliberately still an open question
//!
//! Whether a property is a data property or an accessor is held **on the
//! object**, not in the shape. In a mature engine it belongs in the shape: two
//! objects that differ in it differ in how a read behaves, which is exactly what
//! a shape is for.
//!
//! It is not there because the machine's shape carries a `Repr` per key and no
//! notion of kind, and adding one is a change to the machine — which is where
//! the fix belongs. Working around it by inventing a second layout notion here
//! is the failure the neighbouring README names. Recorded rather than resolved,
//! and the cost until then is a map per object that is empty for nearly all of
//! them.

mod key;

pub use key::{Key, MAX_ARRAY_INDEX, as_array_index};

use std::collections::HashMap;

use rts_cranelift::repr::Repr;
use rts_cranelift::shape::{Key as ShapeKey, KeyRegistry, ShapeId, ShapeTree};

use crate::heap::{Slab, Slot};
use crate::text::{Interner, Str};
use crate::value::Value;

/// How a property answers a read.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Accessor {
    /// What runs on a read, if anything.
    pub getter: Option<Value>,
    /// What runs on a write, if anything.
    pub setter: Option<Value>,
}

/// One object.
pub struct Object {
    /// Which layout.
    shape: ShapeId,
    /// The values, positioned by the layout.
    slots: Vec<Value>,
    /// What this inherits from.
    prototype: Option<Slot>,
    /// Which properties are accessors. Empty for nearly every object.
    accessors: HashMap<Key, Accessor>,
}

/// What a read found.
///
/// An accessor is **not** invoked here. This module has no way to call anything
/// and should not acquire one — what it owes the caller is the function to run
/// *and the receiver to run it on*, which is the thing that is easy to get
/// wrong.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Found {
    /// A plain value.
    Data(Value),
    /// A getter, and what `this` must be when it runs.
    ///
    /// The receiver is the object the read **started** at, not the one the
    /// property was found on. An inherited getter reading `this.x` must see the
    /// instance, and a walk that passed the prototype along instead would make
    /// every inherited accessor read the prototype's own fields.
    Getter {
        /// What to run.
        getter: Value,
        /// What `this` must be while it runs.
        receiver: Slot,
    },
    /// The property is an accessor with no getter, which reads as `undefined`.
    WriteOnly,
}

impl crate::collect::Trace for Object {
    /// Every slot this object refers to: its prototype, and whatever its
    /// properties hold that is a reference.
    ///
    /// A property holding a number contributes nothing, which is why the value
    /// is asked rather than assumed — `as_slot` answers only for a reference.
    fn trace(&self, out: &mut Vec<Slot>) {
        out.extend(self.prototype);
        out.extend(
            self.slots
                .iter()
                .filter_map(|value| value.as_slot())
                .map(Slot),
        );
    }
}

impl Object {
    /// An object of a given layout, with every slot filled.
    ///
    /// The caller supplies the values because the shape says how many and in
    /// what order; a constructor that filled them itself would need to invent a
    /// default, and the language has more than one.
    pub fn new(shape: ShapeId, slots: Vec<Value>, prototype: Option<Slot>) -> Self {
        Object {
            shape,
            slots,
            prototype,
            accessors: HashMap::new(),
        }
    }

    /// Which layout this has.
    pub fn shape(&self) -> ShapeId {
        self.shape
    }

    /// What this inherits from.
    pub fn prototype(&self) -> Option<Slot> {
        self.prototype
    }

    /// Make a property an accessor.
    pub fn define_accessor(&mut self, key: Key, accessor: Accessor) {
        self.accessors.insert(key, accessor);
    }

    /// Whether this object has the property itself, ignoring what it inherits.
    ///
    /// The question `in` and `hasOwnProperty` answer, and the one the prototype
    /// walk asks — **not** whether reading it yields something other than
    /// `undefined`. A property explicitly set to `undefined` is present, and
    /// shadows an inherited one of the same name.
    pub fn has_own(&self, shapes: &mut ShapeTree, key: Key) -> bool {
        self.accessors.contains_key(&key) || self.slot_of(shapes, key).is_some()
    }

    fn slot_of(&self, shapes: &mut ShapeTree, key: Key) -> Option<u32> {
        shapes.slot_of(self.shape, machine_key(key)?)
    }

    /// The value of an own data property.
    fn own_data(&self, shapes: &mut ShapeTree, key: Key) -> Option<Value> {
        let position = self.slot_of(shapes, key)?;
        self.slots.get(position as usize).copied()
    }
}

/// The machine key a [`Key`] is, for the layout to be asked about.
///
/// An index has no machine key today, which is the honest state of this module
/// rather than a design: indexed storage is an array's problem, and an array is
/// not yet a thing. Until it is, an index key finds nothing, and answering
/// `None` says so instead of pretending.
fn machine_key(key: Key) -> Option<ShapeKey> {
    match key {
        Key::Name(name) => Some(name),
        Key::Index(_) => None,
    }
}

/// The layout a fixed list of properties arrives at.
///
/// What a class definition calls, once, before any instance exists. Walking the
/// transitions here means every instance of the class shares one `ShapeId` — so
/// `ShapeTree::layout` gives compiled code one aggregate with fixed offsets, and
/// an inline cache that has seen one instance recognises all of them.
pub fn shape_for(shapes: &mut ShapeTree, fields: &[(Key, Repr)]) -> Option<ShapeId> {
    let mut shape = shapes.root();
    for (key, repr) in fields {
        shape = shapes.transition(shape, machine_key(*key)?, *repr).ok()?;
    }
    Some(shape)
}

/// Read a property, walking the prototype chain.
///
/// Two rules, and each has a straightforward implementation that is wrong:
///
/// - **The receiver is threaded, not replaced.** It stays the object the read
///   started at, so an inherited getter reading `this.x` sees the instance.
/// - **The walk stops when a property is *present*, not when it is non-
///   `undefined`.** An own property holding `undefined` shadows an inherited
///   one; a walk testing the value falls through to the wrong property.
pub fn ordinary_get(
    objects: &Slab<Object>,
    shapes: &mut ShapeTree,
    start: Slot,
    key: Key,
) -> Option<Found> {
    let mut current = start;
    loop {
        let object = objects.at(current).ok()?;

        if let Some(accessor) = object.accessors.get(&key) {
            return Some(match accessor.getter {
                // The receiver is `start`, not `current`.
                Some(getter) => Found::Getter {
                    getter,
                    receiver: start,
                },
                None => Found::WriteOnly,
            });
        }

        if let Some(value) = object.own_data(shapes, key) {
            return Some(Found::Data(value));
        }

        current = object.prototype?;
    }
}

/// Every own key, in the order the language enumerates them.
///
/// Integer indices first, **ascending numerically**, then every other string in
/// the order it was added, then symbols — which do not exist yet and are why
/// this returns a `Vec` rather than an iterator over the shape.
///
/// This is not insertion order, and the difference is not exotic. An object that
/// mixes `obj.b = 1; obj[2] = 1; obj.a = 1; obj[1] = 1` enumerates `1, 2, b, a`.
/// A shape's slot list is insertion-ordered, so returning it directly is wrong
/// for any object with an index key — which is most arrays.
pub fn own_keys(object: &Object, shapes: &ShapeTree) -> Vec<Key> {
    let mut indices: Vec<u32> = Vec::new();
    let mut names: Vec<Key> = Vec::new();

    for (machine, _) in shapes.properties(object.shape) {
        names.push(Key::Name(machine));
    }
    for key in object.accessors.keys() {
        match key {
            Key::Index(index) => indices.push(*index),
            Key::Name(_) => names.push(*key),
        }
    }

    indices.sort_unstable();

    let mut keys: Vec<Key> = indices.into_iter().map(Key::Index).collect();
    keys.extend(names);
    keys
}

/// The key a runtime string names.
///
/// The one place a string becomes a key, so the array-index rule and the one
/// number space are applied in exactly one place.
pub fn key_of(text: &Str, interner: &mut Interner, registry: &mut KeyRegistry) -> Key {
    Key::from_str(text, interner, registry)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct World {
        shapes: ShapeTree,
        registry: KeyRegistry,
        interner: Interner,
        objects: Slab<Object>,
    }

    impl World {
        fn new() -> Self {
            World {
                shapes: ShapeTree::new(),
                registry: KeyRegistry::new(),
                interner: Interner::new(),
                objects: Slab::new(),
            }
        }

        fn key(&mut self, text: &str) -> Key {
            key_of(&Str::from_str(text), &mut self.interner, &mut self.registry)
        }
    }

    #[test]
    fn a_class_shape_is_walked_once_and_every_instance_shares_it() {
        let mut world = World::new();
        let x = world.key("x");
        let y = world.key("y");
        let fields = [(x, Repr::Tagged), (y, Repr::Tagged)];

        let first = shape_for(&mut world.shapes, &fields).unwrap();
        let second = shape_for(&mut world.shapes, &fields).unwrap();

        assert_eq!(
            first, second,
            "a class declares its fields, so the layout is known before any \
             instance exists"
        );
        assert_eq!(
            world.shapes.slot_of(first, machine_key(x).unwrap()),
            Some(0)
        );
        assert_eq!(
            world.shapes.slot_of(first, machine_key(y).unwrap()),
            Some(1)
        );
    }

    #[test]
    fn an_inherited_getter_receives_the_instance_and_not_the_prototype() {
        let mut world = World::new();
        let accessor_key = world.key("value");

        let mut prototype = Object::new(world.shapes.root(), vec![], None);
        prototype.define_accessor(
            accessor_key,
            Accessor {
                getter: Some(Value::from_i32(7)),
                setter: None,
            },
        );
        let prototype_slot = world.objects.insert(prototype).slot();

        let instance = Object::new(world.shapes.root(), vec![], Some(prototype_slot));
        let instance_slot = world.objects.insert(instance).slot();

        let found = ordinary_get(
            &world.objects,
            &mut world.shapes,
            instance_slot,
            accessor_key,
        );

        match found {
            Some(Found::Getter { receiver, .. }) => assert_eq!(
                receiver, instance_slot,
                "recursing with the prototype as receiver breaks `this` in every \
                 inherited getter"
            ),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_own_property_holding_undefined_shadows_an_inherited_one() {
        let mut world = World::new();
        let key = world.key("x");

        // The prototype holds 1.
        let proto_shape = shape_for(&mut world.shapes, &[(key, Repr::Tagged)]).unwrap();
        let prototype = Object::new(proto_shape, vec![Value::from_i32(1)], None);
        let prototype_slot = world.objects.insert(prototype).slot();

        // The instance holds a singleton — stand-in for `undefined`, whose
        // number the language chooses and this crate is told.
        let undefined = Value::from_singleton(0);
        let own_shape = shape_for(&mut world.shapes, &[(key, Repr::Tagged)]).unwrap();
        let instance = Object::new(own_shape, vec![undefined], Some(prototype_slot));
        let instance_slot = world.objects.insert(instance).slot();

        assert_eq!(
            ordinary_get(&world.objects, &mut world.shapes, instance_slot, key),
            Some(Found::Data(undefined)),
            "the walk stops on a property being PRESENT; testing the value would \
             fall through to the prototype's 1"
        );
    }

    #[test]
    fn a_missing_property_walks_the_whole_chain_and_then_answers_nothing() {
        let mut world = World::new();
        let present = world.key("here");
        let absent = world.key("gone");

        let shape = shape_for(&mut world.shapes, &[(present, Repr::Tagged)]).unwrap();
        let prototype = Object::new(shape, vec![Value::from_i32(1)], None);
        let prototype_slot = world.objects.insert(prototype).slot();

        let instance = Object::new(world.shapes.root(), vec![], Some(prototype_slot));
        let instance_slot = world.objects.insert(instance).slot();

        assert_eq!(
            ordinary_get(&world.objects, &mut world.shapes, instance_slot, present),
            Some(Found::Data(Value::from_i32(1))),
            "found on the prototype"
        );
        assert_eq!(
            ordinary_get(&world.objects, &mut world.shapes, instance_slot, absent),
            None
        );
    }

    #[test]
    fn has_own_asks_whether_a_property_is_present_not_what_it_holds() {
        let mut world = World::new();
        let key = world.key("x");
        let shape = shape_for(&mut world.shapes, &[(key, Repr::Tagged)]).unwrap();

        let object = Object::new(shape, vec![Value::from_singleton(0)], None);
        assert!(
            object.has_own(&mut world.shapes, key),
            "holding undefined is not the same as being absent"
        );

        let other = world.key("y");
        assert!(!object.has_own(&mut world.shapes, other));
    }

    #[test]
    fn indices_enumerate_first_and_in_numeric_order() {
        let mut world = World::new();
        let b = world.key("b");
        let a = world.key("a");

        // b and a as ordinary properties, 2 and 1 as index keys.
        let shape = shape_for(&mut world.shapes, &[(b, Repr::Tagged), (a, Repr::Tagged)]).unwrap();
        let mut object = Object::new(shape, vec![Value::from_i32(0), Value::from_i32(0)], None);
        for index in [2u32, 1] {
            object.define_accessor(
                Key::Index(index),
                Accessor {
                    getter: Some(Value::from_i32(0)),
                    setter: None,
                },
            );
        }

        let keys = own_keys(&object, &world.shapes);
        assert_eq!(
            &keys[..2],
            &[Key::Index(1), Key::Index(2)],
            "indices come first, ascending — not in the order they were added"
        );
        assert_eq!(
            &keys[2..],
            &[
                Key::Name(machine_key(b).unwrap()),
                Key::Name(machine_key(a).unwrap())
            ],
            "and the string keys keep insertion order behind them"
        );
    }
}
