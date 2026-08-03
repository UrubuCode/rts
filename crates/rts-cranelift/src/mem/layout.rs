//! What an object looks like in memory.
//!
//! An object is a header followed by its fields. The header is what the
//! collector reads without knowing the type, and it is deliberately small: every
//! byte in it is paid by every object, including the many small ones that
//! measurement showed dominate.

use crate::types::{AggregateLayout, TypeId, TypeRegistry};

/// How wide one field slot is.
///
/// A machine word. Narrower slots would pack better and cost a mask and a shift
/// on every access, which is the trade this design refuses: the measurements put
/// the cost in reaching a field at all, not in how much space it takes.
pub const SLOT_BYTES: u32 = 8;

/// What every object carries, whatever its type.
///
/// One word: the type. The collector needs to know which fields to trace, and
/// the type is what answers that — through the registry, once, rather than by a
/// per-object list of offsets that would cost more than the object.
///
/// Deliberately not here: a size, because the type gives it; a mark bit, because
/// a collector that keeps marks beside the objects does not dirty a cache line
/// per object it visits; a lock, because per-object locking is one of the three
/// costs the measurements named.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct HeaderLayout;

impl HeaderLayout {
    /// How many bytes the header occupies.
    pub const BYTES: u32 = 8;

    /// Where the type identifier sits.
    pub const TYPE_OFFSET: i32 = 0;
}

/// Where an object's fields are, relative to its address.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ObjectLayout {
    /// The type this describes.
    pub ty: TypeId,
    /// Total bytes, header included.
    pub size: u32,
    /// Byte offsets of each field from the object's address, in declared order.
    pub field_offsets: Vec<i32>,
    /// Offsets of the fields the collector must trace.
    pub traced_offsets: Vec<i32>,
}

impl ObjectLayout {
    /// Derives the in-memory layout of a registered aggregate.
    ///
    /// The field offsets come from the type registry and are shifted past the
    /// header. Deriving rather than storing a second set means the registry stays
    /// the one place a field's position is decided — the alternative is two
    /// answers that agree until they do not.
    pub fn of(ty: TypeId, types: &TypeRegistry) -> Self {
        let aggregate: &AggregateLayout = types.layout(ty);
        let shift = HeaderLayout::BYTES as i32;

        Self {
            ty,
            size: HeaderLayout::BYTES + aggregate.size,
            field_offsets: aggregate
                .fields
                .iter()
                .map(|field| field.offset as i32 + shift)
                .collect(),
            traced_offsets: aggregate
                .ref_offsets
                .iter()
                .map(|&offset| offset as i32 + shift)
                .collect(),
        }
    }

    /// Where a field sits, or `None` if the aggregate has no such field.
    pub fn field_offset(&self, index: u32) -> Option<i32> {
        self.field_offsets.get(index as usize).copied()
    }

    /// Whether the collector must look inside objects of this type.
    pub fn has_traced_fields(&self) -> bool {
        !self.traced_offsets.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repr::{RefKind, Repr};

    #[test]
    fn fields_sit_past_the_header() {
        let mut types = TypeRegistry::new();
        let ty = types.declare(&[Repr::I64, Repr::I64]);
        let layout = ObjectLayout::of(ty, &types);

        assert_eq!(layout.field_offsets, vec![8, 16]);
        assert_eq!(layout.size, 24);
    }

    #[test]
    fn the_traced_offsets_are_the_same_ones_shifted() {
        let mut types = TypeRegistry::new();
        let ty = types.declare(&[Repr::F64, Repr::Ref(RefKind::Bytes), Repr::Tagged]);
        let layout = ObjectLayout::of(ty, &types);

        assert_eq!(
            layout.traced_offsets,
            vec![16, 24],
            "the registry decides where a field is; this only moves it past the header"
        );
        assert!(layout.has_traced_fields());
    }

    #[test]
    fn an_object_of_numbers_needs_no_tracing() {
        let mut types = TypeRegistry::new();
        let ty = types.declare(&[Repr::F64]);
        assert!(!ObjectLayout::of(ty, &types).has_traced_fields());
    }
}
