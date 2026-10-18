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
    pub fields: Fields,
    /// Offsets of the fields the collector must trace.
    ///
    /// Precomputed at declaration so that deriving a frame's reference set never
    /// re-walks the field list at a use site.
    pub ref_offsets: RefOffsets,
}

/// The fields of an aggregate, in declaration order.
///
/// An aggregate whose every field has the same representation is described by
/// that representation and a count, not by a list: its offsets are arithmetic
/// (`index * size`, and alignment equals size so no padding arises). A runtime
/// object of N properties is exactly that — every property is `Tagged` — and one
/// list per intermediate layout while it grows is 1 + 2 + ... + N entries,
/// which measured 3 GB for 20 000 properties (#2883). The .NET analogue is a
/// type's field table being its parent's plus its own rather than a copy.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Fields {
    /// Each field listed.
    Explicit(Vec<FieldLayout>),
    /// `count` fields, all `repr`, packed.
    Uniform {
        /// What every field holds.
        repr: Repr,
        /// How many fields.
        count: u32,
    },
}

impl Fields {
    /// How many fields.
    pub fn len(&self) -> usize {
        match self {
            Fields::Explicit(list) => list.len(),
            Fields::Uniform { count, .. } => *count as usize,
        }
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The field at `index`.
    pub fn get(&self, index: usize) -> Option<FieldLayout> {
        match self {
            Fields::Explicit(list) => list.get(index).copied(),
            Fields::Uniform { repr, count } => (index < *count as usize).then(|| FieldLayout {
                repr: *repr,
                offset: index as u32 * registry::size_of_repr(*repr),
            }),
        }
    }

    /// The first field.
    pub fn first(&self) -> Option<FieldLayout> {
        self.get(0)
    }

    /// Every field, in order.
    pub fn iter(&self) -> impl Iterator<Item = FieldLayout> + '_ {
        (0..self.len()).filter_map(move |index| self.get(index))
    }
}

impl<'a> IntoIterator for &'a Fields {
    type Item = FieldLayout;
    type IntoIter = Box<dyn Iterator<Item = FieldLayout> + 'a>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}

/// The byte offsets of the fields the collector traces.
///
/// Stored as a stride when every field is traced and packed, for the reason
/// [`Fields`] stores a uniform aggregate as a count.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RefOffsets {
    /// Each offset listed.
    Explicit(Vec<u32>),
    /// `count` offsets, `0, step, 2 * step, ...`.
    Stride {
        /// Distance between consecutive offsets.
        step: u32,
        /// How many.
        count: u32,
    },
}

impl RefOffsets {
    /// How many offsets.
    pub fn len(&self) -> usize {
        match self {
            RefOffsets::Explicit(list) => list.len(),
            RefOffsets::Stride { count, .. } => *count as usize,
        }
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Every offset, in order.
    pub fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        (0..self.len()).map(move |index| match self {
            RefOffsets::Explicit(list) => list[index],
            RefOffsets::Stride { step, .. } => index as u32 * step,
        })
    }

    /// The offsets as a list.
    pub fn to_vec(&self) -> Vec<u32> {
        self.iter().collect()
    }
}

impl AggregateLayout {
    /// The field at `index`, or `None` if the aggregate has no such field.
    pub fn field(&self, index: usize) -> Option<FieldLayout> {
        self.fields.get(index)
    }

    /// Whether any field must be traced by the collector.
    pub fn has_references(&self) -> bool {
        !self.ref_offsets.is_empty()
    }
}
