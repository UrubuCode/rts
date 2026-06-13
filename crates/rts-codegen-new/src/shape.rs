//! Hidden classes ("shapes") — the object model.
//!
//! Replaces the old engine's default `HashMap<String,i64>` property-bag (V8
//! dictionary/slow-mode made the DEFAULT) and the O(N) string-compare virtual
//! dispatch. An object is `{ shape_id, slots: [PolyValue; N] }`. Property access
//! is a shape-id compare + a fixed-offset slot load; method dispatch is keyed on
//! the shape's class, not a chain of `gc.string_eq`.
//!
//! Simplicity line vs V8 (held deliberately): shapes + monomorphic/polymorphic
//! data ICs, a transition tree, and a fall-to-dictionary mode for pathological
//! objects. NO speculative deopt, NO on-stack-replacement, NO dependent-code
//! invalidation graph. See `docs/specs/rts-codegen-new-design.md`.

use std::collections::HashMap;

pub type ShapeId = u32;
pub type SlotIdx = u32;

/// A hidden class: the layout shared by all objects built the same way.
#[derive(Default)]
pub struct Shape {
    pub id: ShapeId,
    /// property name -> inline slot index.
    pub slots: HashMap<String, SlotIdx>,
    /// add-property transitions: key -> resulting shape (the transition tree).
    pub transitions: HashMap<String, ShapeId>,
    /// prototype object's shape, for chain walking + proto ICs.
    pub proto: Option<ShapeId>,
}

/// Interns shapes and owns the transition tree.
#[derive(Default)]
pub struct ShapeTable {
    shapes: Vec<Shape>,
}

impl ShapeTable {
    pub fn empty_shape(&mut self) -> ShapeId {
        todo!("phase: object-model — intern the root (no-property) shape")
    }

    /// Return the shape reached by adding `key` to `from`, creating it if needed.
    pub fn transition(&mut self, _from: ShapeId, _key: &str) -> ShapeId {
        todo!("phase: object-model — transition-tree insert/lookup")
    }

    /// Slot index of `key` in `shape`, if present (the IC fast-path resolves this).
    pub fn slot_of(&self, _shape: ShapeId, _key: &str) -> Option<SlotIdx> {
        todo!("phase: object-model")
    }
}
