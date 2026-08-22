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
//! derived here from facts the layer already holds — what the field is, where
//! the object lives, and how many regions the heap has — and it happens during
//! lowering, where it cannot be forgotten.

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

/// Whether a heap of this many regions can have a crossing at all.
///
/// **One region cannot point at another when there is no other.** That is not a
/// judgement about which programs exist or which stores look safe — it is the
/// arithmetic of [`crate::mem::Addressing::Single`], where every reference
/// decodes into the one region and the only thing [`BarrierKind`] can report is
/// `CrossRegion`. Under that addressing the barrier records something
/// **unrepresentable**, not something merely rare.
///
/// # Why this is here and not at the two places that ask
///
/// Because of the rule at the top of this module: the decision is not exposed,
/// there is no place to pass `false`, and insertion is derived from facts the
/// layer already holds. The number of regions is such a fact. Putting the test
/// in the lowering instead would be exactly the flag this module refuses to
/// have — and the second copy of it would be the one that goes stale when a
/// second `BarrierKind` arrives.
///
/// # What must be revisited when a second kind arrives
///
/// This. A generational or card-marking barrier is not about crossing regions,
/// so it would fire in a single-region heap and this predicate would be
/// silently wrong for it. [`BarrierKind`] has two variants today and the elision
/// is sound for both; adding a third means deciding here, per kind, rather than
/// per heap.
pub fn crossing_is_possible(regions: u32) -> bool {
    regions > 1
}

/// Whether a store needs a barrier, and which.
///
/// Three conditions must all hold. The field must be one the collector traces —
/// a store of a number can never create a reference. The object must be
/// reachable from more than the storing thread, because a store into storage
/// only that thread can see cannot make a reference visible anywhere new. And
/// the heap must have a second region for a reference to cross into, which is
/// [`crossing_is_possible`].
///
/// The generic form counts as traced. It may hold a reference, and nothing at
/// this point can prove it does not; treating it as untraced would be the
/// missed barrier this module exists to prevent.
pub fn barrier_for(
    inst: &Inst,
    types: &TypeRegistry,
    object_region: Region,
    regions: u32,
) -> BarrierKind {
    if !crossing_is_possible(regions) {
        return BarrierKind::None;
    }

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

    /// A heap with somewhere to cross to, so the tests below are about the
    /// FIELD and the REGION rather than about the count.
    ///
    /// Spelled once rather than as a literal at each site: the four tests here
    /// are not about how many regions there are, and a `2` repeated four times
    /// would read as though they were.
    const SHARDED: u32 = 2;

    /// The heap every program this engine compiles today actually runs on.
    const SINGLE: u32 = 1;

    #[test]
    fn one_region_has_nowhere_to_cross_to() {
        // The elision, and the reason it is not a weakening of the rule at the
        // top of this file: `CrossRegion` is the only thing a barrier reports,
        // and under `Addressing::Single` every reference decodes into the one
        // region. There is no store that could produce a crossing, so there is
        // no crossing to miss.
        //
        // This is the case `rts run` is in. `crates/rts-host/src/run.rs` builds
        // one `Region` unless several threads were asked for, so the barrier
        // call that used to sit on every cached property store — and on every
        // `FieldStore` into a traced field — is not emitted at all.
        let mut types = TypeRegistry::new();
        let ty = types.declare(&[Repr::Ref(RefKind::Opaque)]);
        assert_eq!(
            barrier_for(&store(ty, 0), &types, Region::Shared, SINGLE),
            BarrierKind::None,
            "one region cannot point at another when there is no other"
        );
        assert!(!crossing_is_possible(SINGLE));
        assert!(crossing_is_possible(SHARDED));
    }

    #[test]
    fn storing_a_number_never_needs_a_barrier() {
        let mut types = TypeRegistry::new();
        let ty = types.declare(&[Repr::F64]);
        assert_eq!(
            barrier_for(&store(ty, 0), &types, Region::Shared, SHARDED),
            BarrierKind::None
        );
    }

    #[test]
    fn storing_a_reference_into_shared_storage_needs_one() {
        let mut types = TypeRegistry::new();
        let ty = types.declare(&[Repr::Ref(RefKind::Opaque)]);
        assert_eq!(
            barrier_for(&store(ty, 0), &types, Region::Shared, SHARDED),
            BarrierKind::CrossRegion
        );
    }

    #[test]
    fn storing_into_thread_local_storage_does_not() {
        let mut types = TypeRegistry::new();
        let ty = types.declare(&[Repr::Ref(RefKind::Opaque)]);
        assert_eq!(
            barrier_for(&store(ty, 0), &types, Region::Local, SHARDED),
            BarrierKind::None,
            "a reference no other thread can reach is not visible anywhere new"
        );
    }

    #[test]
    fn a_generic_field_is_treated_as_a_reference() {
        let mut types = TypeRegistry::new();
        let ty = types.declare(&[Repr::Tagged]);
        assert_eq!(
            barrier_for(&store(ty, 0), &types, Region::Shared, SHARDED),
            BarrierKind::CrossRegion,
            "it may hold one, and nothing here can prove it does not"
        );
    }
}
