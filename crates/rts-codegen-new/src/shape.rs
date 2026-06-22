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
use std::sync::{Mutex, OnceLock};

pub type ShapeId = u32;
pub type SlotIdx = u32;

/// A process-global, GLOBALLY-UNIQUE shape id stored INSIDE a runtime object
/// (slot 0) so the inspect trampoline can recover its ordered keys. Distinct from
/// the per-compilation [`ShapeId`] (which is only valid for slot resolution within
/// one `ShapeTable`): two different compilations / functions may both mint local
/// `ShapeId(0)`, but their global ids differ.
pub type GlobalShapeId = u32;

/// The process-global registry mapping a [`GlobalShapeId`] → its ordered key list.
/// Populated at lowering time when an object literal interns its shape (so the id
/// baked into the emitted object is a real index here), and read at runtime by the
/// inspect host fn [`global_shape_keys`]. A `Vec` indexed by id (ids are dense,
/// assigned sequentially); the `by_keys` map de-duplicates identical key-sequences
/// so `{a,b}` from two literals share ONE global id (smaller registry, stable ids).
struct GlobalShapeRegistry {
    keys: Vec<Vec<String>>,
    by_keys: HashMap<Vec<String>, GlobalShapeId>,
}

fn registry() -> &'static Mutex<GlobalShapeRegistry> {
    static REG: OnceLock<Mutex<GlobalShapeRegistry>> = OnceLock::new();
    REG.get_or_init(|| {
        Mutex::new(GlobalShapeRegistry {
            keys: Vec::new(),
            by_keys: HashMap::new(),
        })
    })
}

/// Intern `keys` in the PROCESS-GLOBAL registry, returning a stable
/// [`GlobalShapeId`] that indexes [`global_shape_keys`]. Called at lowering time
/// (the id is baked into the object's slot 0 as a tagged int). Idempotent for an
/// identical key-sequence.
pub fn intern_global_shape(keys: &[String]) -> GlobalShapeId {
    let mut reg = registry().lock().expect("global shape registry poisoned");
    if let Some(&id) = reg.by_keys.get(keys) {
        return id;
    }
    let id = reg.keys.len() as GlobalShapeId;
    reg.keys.push(keys.to_vec());
    reg.by_keys.insert(keys.to_vec(), id);
    id
}

/// Mint a GLOBALLY-UNIQUE shape id for ONE user `class` declaration, registering
/// its ordered `keys` for [`global_shape_keys`] (inspect). Unlike
/// [`intern_global_shape`], this NEVER de-duplicates by keys: two distinct
/// classes with the SAME field layout (`class A {}` / `class B {}`) get
/// DIFFERENT ids, so the id stored in slot 0 is a sound per-class runtime
/// identity (used by `x instanceof C` on an opaque value). The key list is still
/// recorded — identical key lists simply map to multiple ids in `keys`, which is
/// fine for inspect (id → keys stays a function).
pub fn intern_class_shape(keys: &[String]) -> GlobalShapeId {
    let mut reg = registry().lock().expect("global shape registry poisoned");
    let id = reg.keys.len() as GlobalShapeId;
    reg.keys.push(keys.to_vec());
    // Seed `by_keys` only if this key-sequence has no id yet, so object literals
    // interning the same keys later still find a stable (first) id.
    reg.by_keys.entry(keys.to_vec()).or_insert(id);
    id
}

/// Drop every interned global shape. Called by
/// [`crate::state::reset_codegen_state`] at the START of each program compile
/// (before any lowering of the new program interns its shapes). SOUND because the
/// process compiles-then-runs ONE program at a time: a `GlobalShapeId` baked into
/// an emitted object is only read while THAT program runs, before the next compile
/// resets the registry. If concurrent live programs are ever supported, this
/// registry must move into the per-`Program` state instead.
pub fn reset_global_shapes() {
    let mut reg = registry().lock().expect("global shape registry poisoned");
    reg.keys.clear();
    reg.keys.shrink_to_fit();
    reg.by_keys.clear();
    reg.by_keys.shrink_to_fit();
}

/// The number of interned global shapes (leak-test probe).
pub fn global_shape_count() -> usize {
    registry().lock().map(|r| r.keys.len()).unwrap_or(0)
}

/// The ordered keys of a [`GlobalShapeId`], or `None` if the id was never
/// interned (a codegen bug — the runtime stored an id this process did not mint).
/// The inspect trampoline calls this to render `{ k0: v0, … }`.
pub fn global_shape_keys(id: GlobalShapeId) -> Option<Vec<String>> {
    let reg = registry().lock().expect("global shape registry poisoned");
    reg.keys.get(id as usize).cloned()
}

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
