//! AOT-safe inline caches — DATA caches, not self-modifying code.
//!
//! Classic V8 ICs patch machine code at runtime. With Cranelift AOT object files
//! we cannot self-modify code portably, so an RTS IC is a **data cell** the
//! emitted code loads and checks: `if obj.shape_id == cache.shape { load
//! slot[cache.slot] } else { slow_path(updates cache) }`. The guard is an `icmp`
//! on a loaded `u32`; the cache cell lives in a writable data segment (works
//! identically for JIT and AOT). This is the simplification that keeps the engine
//! lean while still turning megamorphic string lookups into a pointer-compare.

use crate::shape::{ShapeId, SlotIdx};

/// Inline-cache state machine for a single property-access (or call) site.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IcState {
    /// Never executed.
    Uninit,
    /// One shape seen — the fast path: compare id, load slot.
    Mono { shape: ShapeId, slot: SlotIdx },
    /// A few shapes seen — a small inline table is checked.
    Poly,
    /// Too many shapes — give up caching, always call the generic resolver.
    Mega,
}

/// The runtime cache cell emitted next to a property-access site (data segment).
#[repr(C)]
pub struct PropIcCell {
    pub shape: ShapeId,
    pub slot: SlotIdx,
    pub state: u32, // IcState discriminant, mutated by the slow path
}

impl PropIcCell {
    pub const UNINIT: PropIcCell = PropIcCell {
        shape: 0,
        slot: 0,
        state: 0,
    };
}
