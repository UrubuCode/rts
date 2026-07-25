//! Hidden classes ("shapes") — the object model.
//!
//! Replaces the old engine's default `HashMap<String,i64>` property-bag (V8
//! dictionary/slow-mode made the DEFAULT) and the O(N) string-compare virtual
//! dispatch. An object is `{ shape_id, slots: [PolyValue; N] }`. Property access
//! is a shape-id compare + a fixed-offset slot load; method dispatch is keyed on
//! the shape's class, not a chain of `gc.string_eq`.
//!
//! ## What this increment (P3) implements
//!
//! The COMPILE-TIME slice: a [`Shape`] is the static key→slot-index map for an
//! object literal whose keys are all known at compile time. [`ShapeTable`]
//! **interns** distinct key-sequences to a [`ShapeId`]; an object literal
//! `{a, b, c}` reuses the same shape as any other literal built with the same
//! ordered key list. A property access `obj.a` lowers to a `VEC_GET(obj, slot)`
//! with `slot` the COMPILE-TIME constant from the shape; `obj.a = v` to
//! `VEC_SET`. The runtime object value is an `Entry::Vec` of PolyValue words (the
//! inline slot array), boxed as a `TAG_OBJECT` PolyValue.
//!
//! The DYNAMIC runtime machinery — a transition tree for incremental
//! property-adds, data inline caches ([`crate::ic`]), shape-keyed method
//! dispatch, dictionary fallback — is a LATER increment. Adding a key not in the
//! shape, or accessing a property on an object whose shape is not statically
//! proven, BAILS (`Unsupported`) rather than guess.

use std::collections::HashMap;

pub type ShapeId = u32;
pub type SlotIdx = u32;

// The process-global shape-id registry MOVED to `rts-engine::heap::shapes` so
// the whole runtime layer (rts-std/rts-shared) can mint shaped objects instead
// of legacy `Entry::Map` dictionary rows. Re-exported here so every existing
// `crate::shape::*` / `rts_adapters::shape::*` path keeps working.
pub use rts_engine::heap::shapes::{
    GLOBAL_SHAPE_BASE, GlobalShapeId, error_class_info, global_shape_count, global_shape_keys,
    global_shape_slot_of, intern_class_shape, intern_global_shape, register_error_class, reset_global_shapes,
};

/// A hidden class: the layout shared by all objects built the same way. In this
/// (compile-time) increment a shape is fully described by its ordered key list;
/// slot `i` holds the value for `keys[i]`.
#[derive(Default, Clone)]
pub struct Shape {
    pub id: ShapeId,
    /// The ordered property names; the slot index of a key is its position here.
    pub keys: Vec<String>,
    /// property name → inline slot index (the cached inverse of `keys`).
    pub slots: HashMap<String, SlotIdx>,
}

impl Shape {
    /// Slot index of `key` in this shape, if present.
    pub fn slot_of(&self, key: &str) -> Option<SlotIdx> {
        self.slots.get(key).copied()
    }

    /// Number of inline slots (= number of keys).
    pub fn slot_count(&self) -> usize {
        self.keys.len()
    }
}

/// Interns shapes by their ordered key-sequence. Two object literals built with
/// the same ordered keys share one [`ShapeId`].
#[derive(Default)]
pub struct ShapeTable {
    shapes: Vec<Shape>,
    /// ordered-key-sequence → ShapeId (the interning index).
    by_keys: HashMap<Vec<String>, ShapeId>,
}

impl ShapeTable {
    pub fn new() -> ShapeTable {
        ShapeTable::default()
    }

    /// The root (no-property) shape — `{}`. Interned like any other.
    pub fn empty_shape(&mut self) -> ShapeId {
        self.intern(&[])
    }

    /// Intern the shape for an object literal with the given ordered `keys`,
    /// creating it on first sight and reusing it thereafter. Returns its
    /// [`ShapeId`]. Duplicate keys in a literal (`{a:1, a:2}`) keep the LAST
    /// (JS semantics): the caller is responsible for de-duplicating to the last
    /// occurrence before interning; this table treats the key vector verbatim.
    pub fn intern(&mut self, keys: &[String]) -> ShapeId {
        if let Some(&id) = self.by_keys.get(keys) {
            return id;
        }
        let id = self.shapes.len() as ShapeId;
        let slots = keys
            .iter()
            .enumerate()
            .map(|(i, k)| (k.clone(), i as SlotIdx))
            .collect();
        self.shapes.push(Shape {
            id,
            keys: keys.to_vec(),
            slots,
        });
        self.by_keys.insert(keys.to_vec(), id);
        id
    }

    /// The interned [`Shape`] for an id (panics on an id this table did not mint —
    /// a codegen bug, never a user error).
    pub fn get(&self, id: ShapeId) -> &Shape {
        &self.shapes[id as usize]
    }

    /// Slot index of `key` in `shape`, if present (the access fast-path resolves
    /// this at compile time).
    pub fn slot_of(&self, shape: ShapeId, key: &str) -> Option<SlotIdx> {
        self.get(shape).slot_of(key)
    }
}
