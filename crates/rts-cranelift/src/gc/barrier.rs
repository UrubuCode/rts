//! Write barriers.
//!
//! A collector that reclaims one region without examining the others must be
//! told when a reference crosses from one into another. That notification is the
//! barrier, and the one thing that must never happen is a store that skips it:
//! a missed barrier produces a reference the collector does not know about,
//! which becomes a use-after-free that reproduces rarely and explains nothing.
//!
//! So the decision is not exposed. There is no flag on a store, no barrier
//! instruction a client emits, and no place to pass `false`. Insertion is
//! derived here from facts the layer already holds — what the field is, and
//! where the object lives — and it happens during lowering, where it cannot be
//! forgotten.

use crate::ir::{Inst, Region};
use crate::types::TypeRegistry;

/// What a store must do beyond writing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BarrierKind {
    /// Nothing: the store cannot create a reference the collector must learn of.
    None,
    /// Record that a region now holds a reference into another.
    CrossRegion,
}

/// Whether a store needs a barrier, and which.
///
/// Two conditions must both hold. The field must be one the collector traces —
/// a store of a number can never create a reference. And the object must be
/// reachable from more than the storing thread, because a store into storage
/// only that thread can see cannot make a reference visible anywhere new.
///
/// The generic form counts as traced. It may hold a reference, and nothing at
/// this point can prove it does not; treating it as untraced would be the
/// missed barrier this module exists to prevent.
pub fn barrier_for(inst: &Inst, types: &TypeRegistry, object_region: Region) -> BarrierKind {
    let Inst::FieldStore { ty, field, .. } = inst else {
        return BarrierKind::None;
    };

    let traced = types
        .layout(*ty)
        .field(*field as usize)
        .is_some_and(|f| f.repr.is_gc_relevant());

    match (traced, object_region) {
        (true, Region::Shared) => BarrierKind::CrossRegion,
        _ => BarrierKind::None,
    }
}

// `can_carry_reference(repr)` used to live here, wrapping `repr.is_gc_relevant()`
// with no added logic. It had no caller outside its own tests: the frame-root
// question at `gc::describe_frames` and the traced-field question above both
// ask `Repr::is_gc_relevant` directly. A wrapper that only forwards is not a
// second place the question is answered — it is a second *name* for the one
// place, and keeping it invited a real second answer to grow inside it later
// while looking, from the call site, indistinguishable from the first. Deleted
// rather than wired up: wiring it up would have meant passing a `Repr` out of
// `barrier_for`'s own field lookup and back through it for no behavioural
// difference. `Repr::is_gc_relevant` is the one place now.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Inst, ValueId};
    use crate::repr::{RefKind, Repr};

    fn store(ty: crate::types::TypeId, field: u32) -> Inst {
        Inst::FieldStore {
            object: ValueId(0),
            ty,
            field,
            value: ValueId(1),
        }
    }

    #[test]
    fn storing_a_number_never_needs_a_barrier() {
        let mut types = TypeRegistry::new();
        let ty = types.declare(&[Repr::F64]);
        assert_eq!(
            barrier_for(&store(ty, 0), &types, Region::Shared),
            BarrierKind::None
        );
    }

    #[test]
    fn storing_a_reference_into_shared_storage_needs_one() {
        let mut types = TypeRegistry::new();
        let ty = types.declare(&[Repr::Ref(RefKind::Opaque)]);
        assert_eq!(
            barrier_for(&store(ty, 0), &types, Region::Shared),
            BarrierKind::CrossRegion
        );
    }

    #[test]
    fn storing_into_thread_local_storage_does_not() {
        let mut types = TypeRegistry::new();
        let ty = types.declare(&[Repr::Ref(RefKind::Opaque)]);
        assert_eq!(
            barrier_for(&store(ty, 0), &types, Region::Local),
            BarrierKind::None,
            "a reference no other thread can reach is not visible anywhere new"
        );
    }

    #[test]
    fn a_generic_field_is_treated_as_a_reference() {
        let mut types = TypeRegistry::new();
        let ty = types.declare(&[Repr::Tagged]);
        assert_eq!(
            barrier_for(&store(ty, 0), &types, Region::Shared),
            BarrierKind::CrossRegion,
            "it may hold one, and nothing here can prove it does not"
        );
    }
}
