//! Aggregate layout: the one place field offsets exist.
//!
//! The code generator underneath has no aggregate type. It has scalars, stack
//! regions, and load/store at an offset. Everything about structures is
//! therefore this layer's invention, and the registry here is the single source
//! those decisions come from: the ABI reads it to classify a parameter, the
//! collector reads it to derive which fields are references, memory reads it to
//! size an allocation, and the builder reads it so a field access never needs
//! the caller to assert a width.
//!
//! Duplicating this metadata anywhere else re-creates the failure mode the
//! machine layer exists to remove: the same fact, written twice, drifting.

mod registry;

pub use registry::TypeRegistry;

use crate::repr::Repr;

/// Identifies a registered aggregate.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct TypeId(pub(crate) u32);

impl TypeId {
    /// The registry index, for consumers that key their own tables by type.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A field's position and representation within an aggregate.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FieldLayout {
    /// How the field exists on the machine.
    pub repr: Repr,
    /// Byte offset from the start of the aggregate.
    pub offset: u32,
}

/// The computed layout of a registered aggregate.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AggregateLayout {
    /// Total size in bytes, including trailing padding.
    pub size: u32,
    /// Required alignment in bytes.
    pub align: u32,
    /// Fields in declaration order.
    pub fields: Vec<FieldLayout>,
    /// Offsets of the fields the collector must trace.
    ///
    /// Precomputed at declaration so that deriving a frame's reference set never
    /// re-walks the field list at a use site.
    pub ref_offsets: Vec<u32>,
}

impl AggregateLayout {
    /// The field at `index`, or `None` if the aggregate has no such field.
    pub fn field(&self, index: usize) -> Option<FieldLayout> {
        self.fields.get(index).copied()
    }

    /// Whether any field must be traced by the collector.
    pub fn has_references(&self) -> bool {
        !self.ref_offsets.is_empty()
    }
}
