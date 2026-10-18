//! Declaration and layout computation for aggregates.

use super::{AggregateLayout, FieldLayout, Fields, RefOffsets, TypeId};
use crate::repr::Repr;

/// Owns every aggregate layout in one compilation.
///
/// Layout is computed once, at declaration. Fields keep their declared order:
/// reordering to reduce padding is a real optimization, but it makes a layout
/// depend on a policy rather than on the declaration, and a client reasoning
/// about a boundary it shares with foreign code needs the declared order to be
/// the actual order.
#[derive(Default)]
pub struct TypeRegistry {
    layouts: Vec<AggregateLayout>,
}

impl TypeRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Declares an aggregate from its field representations.
    ///
    /// Each field is placed at the next offset satisfying its natural
    /// alignment; the aggregate's alignment is the widest of its fields, and its
    /// size is rounded up to that alignment so that arrays of it stay aligned.
    pub fn declare(&mut self, fields: &[Repr]) -> TypeId {
        if let Some(&first) = fields.first()
            && fields.iter().all(|&repr| repr == first)
        {
            return self.declare_uniform(first, fields.len() as u32);
        }
        let mut offset: u32 = 0;
        let mut align: u32 = 1;
        let mut laid_out = Vec::with_capacity(fields.len());
        let mut ref_offsets = Vec::new();

        for &repr in fields {
            let field_align = align_of_repr(repr);
            align = align.max(field_align);
            offset = round_up(offset, field_align);

            if repr.is_gc_relevant() {
                ref_offsets.push(offset);
            }
            laid_out.push(FieldLayout { repr, offset });
            offset += size_of_repr(repr);
        }

        let id = TypeId(self.layouts.len() as u32);
        self.layouts.push(AggregateLayout {
            size: round_up(offset, align),
            align,
            fields: Fields::Explicit(laid_out),
            ref_offsets: RefOffsets::Explicit(ref_offsets),
        });
        id
    }

    /// Declares an aggregate of `count` fields that all hold `repr`, in O(1).
    ///
    /// The same layout [`Self::declare`] computes for a list of equal entries
    /// (which delegates here), without the list: see [`Fields`].
    pub fn declare_uniform(&mut self, repr: Repr, count: u32) -> TypeId {
        let size = size_of_repr(repr);
        let id = TypeId(self.layouts.len() as u32);
        self.layouts.push(AggregateLayout {
            size: count * size,
            align: size.max(1),
            fields: Fields::Uniform { repr, count },
            ref_offsets: if repr.is_gc_relevant() {
                RefOffsets::Stride { step: size, count }
            } else {
                RefOffsets::Explicit(Vec::new())
            },
        });
        id
    }

    /// The layout of a declared aggregate.
    ///
    /// Panics if the identifier did not come from this registry. A type
    /// identifier is only obtainable from `declare`, so reaching this panic
    /// means two registries were mixed — a wiring mistake worth failing loudly
    /// rather than a condition a caller should handle.
    pub fn layout(&self, id: TypeId) -> &AggregateLayout {
        self.layouts
            .get(id.index())
            .expect("type identifier does not belong to this registry")
    }

    /// The same, by the raw index a cell's header carries, answering `None`
    /// where no aggregate was declared under it.
    ///
    /// A runtime reads a cell's type out of the header as a NUMBER, because that
    /// is what this layer put there. [`Self::layout`] cannot serve it — a
    /// `TypeId` is only obtainable from `declare`, which is the property that
    /// makes its panic a wiring mistake rather than a condition — so this is the
    /// lookup for the one caller that legitimately has an index and not an
    /// identifier, and it fails by answering nothing instead of panicking.
    pub fn layout_at(&self, index: usize) -> Option<&AggregateLayout> {
        self.layouts.get(index)
    }

    /// Whether the identifier was issued by this registry.
    ///
    /// The verifier uses this to reject a program built against a different
    /// registry, which would otherwise read plausible offsets from the wrong
    /// layouts.
    pub fn contains(&self, id: TypeId) -> bool {
        id.index() < self.layouts.len()
    }

    /// How many aggregates are declared.
    pub fn len(&self) -> usize {
        self.layouts.len()
    }

    /// Whether nothing is declared.
    pub fn is_empty(&self) -> bool {
        self.layouts.is_empty()
    }
}

/// Size in bytes of a value with this representation, when stored in memory.
pub(super) fn size_of_repr(repr: Repr) -> u32 {
    match repr {
        Repr::Bool => 1,
        other => other.bit_width() / 8,
    }
}

/// Natural alignment of a value with this representation.
fn align_of_repr(repr: Repr) -> u32 {
    size_of_repr(repr)
}

/// Rounds `value` up to the next multiple of `align`, which must be a power of two.
fn round_up(value: u32, align: u32) -> u32 {
    debug_assert!(align.is_power_of_two());
    (value + align - 1) & !(align - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repr::RefKind;

    #[test]
    fn fields_are_padded_to_natural_alignment() {
        let mut reg = TypeRegistry::new();
        let id = reg.declare(&[Repr::Bool, Repr::F64, Repr::I32]);
        let layout = reg.layout(id);

        assert_eq!(layout.field(0).unwrap().offset, 0);
        assert_eq!(layout.field(1).unwrap().offset, 8, "f64 aligns to 8, not to 1");
        assert_eq!(layout.field(2).unwrap().offset, 16);
        assert_eq!(layout.align, 8);
        assert_eq!(layout.size, 24, "size rounds up so arrays stay aligned");
    }

    #[test]
    fn reference_offsets_are_precomputed() {
        let mut reg = TypeRegistry::new();
        let id = reg.declare(&[Repr::I32, Repr::Ref(RefKind::Bytes), Repr::Tagged]);
        let layout = reg.layout(id);

        assert!(layout.has_references());
        assert_eq!(layout.ref_offsets.to_vec(), vec![8, 16]);
    }

    #[test]
    fn a_uniform_aggregate_lays_out_like_the_explicit_one() {
        let mut reg = TypeRegistry::new();
        let uniform = reg.declare(&[Repr::Tagged, Repr::Tagged, Repr::Tagged]);
        let layout = reg.layout(uniform);
        assert_eq!(layout.size, 24);
        assert_eq!(layout.align, 8);
        assert_eq!(layout.fields.len(), 3);
        assert_eq!(layout.field(2).unwrap().offset, 16);
        assert!(layout.field(3).is_none());
        assert_eq!(layout.ref_offsets.to_vec(), vec![0, 8, 16]);
        let bytes = reg.declare(&[Repr::Bool, Repr::Bool, Repr::Bool]);
        assert_eq!(reg.layout(bytes).size, 3);
        assert_eq!(reg.layout(bytes).field(2).unwrap().offset, 2);
        assert!(!reg.layout(bytes).has_references());
    }

    #[test]
    fn an_aggregate_of_scalars_needs_no_tracing() {
        let mut reg = TypeRegistry::new();
        let id = reg.declare(&[Repr::F64, Repr::F64]);
        assert!(!reg.layout(id).has_references());
    }
}
