//! Declaration and layout computation for aggregates.

use super::{AggregateLayout, FieldLayout, TypeId};
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
            fields: laid_out,
            ref_offsets,
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
fn size_of_repr(repr: Repr) -> u32 {
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

        assert_eq!(layout.fields[0].offset, 0);
        assert_eq!(layout.fields[1].offset, 8, "f64 aligns to 8, not to 1");
        assert_eq!(layout.fields[2].offset, 16);
        assert_eq!(layout.align, 8);
        assert_eq!(layout.size, 24, "size rounds up so arrays stay aligned");
    }

    #[test]
    fn reference_offsets_are_precomputed() {
        let mut reg = TypeRegistry::new();
        let id = reg.declare(&[Repr::I32, Repr::Ref(RefKind::Bytes), Repr::Tagged]);
        let layout = reg.layout(id);

        assert!(layout.has_references());
        assert_eq!(layout.ref_offsets, vec![8, 16]);
    }

    #[test]
    fn an_aggregate_of_scalars_needs_no_tracing() {
        let mut reg = TypeRegistry::new();
        let id = reg.declare(&[Repr::F64, Repr::F64]);
        assert!(!reg.layout(id).has_references());
    }
}
