//! Guards, the two tiers, and the pairing that makes a fall land somewhere.
//!
//! `docs/engine/deopt-lateral.md` is the design. What lives here is the part the
//! IR needs: an identity for a program point, an assertion the domain interprets,
//! and the check that two tiers agree about which points exist.

use crate::cfg::Func;

/// A program point at which the specialised tier may fall to the generic one.
///
/// Opaque to the machine by design — `deopt-lateral.md` fixes that boundary in
/// writing: the machine pairs and resumes, it does not interpret. A `PointId` is
/// minted per guard site by whoever lowers the tree, and the SAME id is used in
/// both lowerings of that site, which is what makes pairing a construction rather
/// than a convention.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Hash)]
pub struct PointId(pub u32);

/// Which of a function's two bodies this is.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Tier {
    /// Compiled under assumptions, with a guard on each and a fall behind it.
    Specialised,
    /// Compiled under none, and therefore the place a fall lands.
    ///
    /// It is also the witness the whole design rests on: without a deoptimiser
    /// there is nothing to reconstruct a frame into, so a guard may only be
    /// emitted where this body exists.
    Generic,
}

/// What a guard asserts, as an index the language's domain interprets.
///
/// Opaque here for the same reason a [`crate::cfg::Prim`] is: "is this an int32"
/// is one language's question and "is this an integer rather than a float" is
/// another's. [`crate::domain::Domain::narrow`] is what turns one of these into a
/// type.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Hash)]
pub struct Assertion(pub u32);

/// Two tiers that do not agree about which points exist.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Mismatch {
    /// Both functions claim to be the same tier, so one of them was not lowered
    /// as the pair of the other.
    SameTier(Tier),
    /// The specialised tier may fall to a point the generic tier cannot resume.
    ///
    /// This is the failure the check exists for, and it is silent without it: the
    /// fall is a jump, so an unpaired point is a jump to a label that was never
    /// emitted — which the machine layer would refuse, but only after the whole
    /// module has been lowered and with nothing to say which guard was at fault.
    Unresumable(PointId),
}

/// Whether the specialised tier can only fall where the generic one can resume.
///
/// # Why this is the net and not the mechanism
///
/// README rule 7: both tiers are lowered from ONE MIR, so the ids match by
/// construction. This check exists because that is the property the design is
/// built to make impossible to break, rather than merely unlikely — and a
/// property nothing checks is a property nobody finds out about.
///
/// It is deliberately one-directional. The generic tier may carry resume labels
/// the specialised tier never falls to: a pass that removed a guard, having
/// proved its assertion, leaves the point unused and removing the label too would
/// be a second traversal buying nothing.
pub fn pair(specialised: &Func, generic: &Func) -> Result<(), Mismatch> {
    if specialised.tier == generic.tier {
        return Err(Mismatch::SameTier(specialised.tier));
    }
    let (specialised, generic) = match specialised.tier {
        Tier::Specialised => (specialised, generic),
        Tier::Generic => (generic, specialised),
    };
    for point in &specialised.points {
        if generic.points.binary_search(point).is_err() {
            return Err(Mismatch::Unresumable(*point));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::{Const, FuncBuilder, Op, Terminator};
    use crate::effect::Effect;
    use rts_cranelift::fault::Position;

    fn somewhere() -> Position {
        Position::default()
    }

    /// One tier with a guard, one without: the pair a lowering produces.
    fn tiers(points: &[u32], resumable: &[u32]) -> (Func, Func) {
        let mut fast = FuncBuilder::new(Tier::Specialised);
        let value = fast.push(Op::Const(Const::Int(1)), Effect::PURE, somewhere());
        for point in points {
            fast.push(
                Op::Guard {
                    assertion: Assertion(0),
                    on: value,
                    point: PointId(*point),
                },
                Effect::PURE,
                somewhere(),
            );
        }
        fast.end(Terminator::Return(Some(value)));

        let mut slow = FuncBuilder::new(Tier::Generic);
        let same = slow.push(Op::Const(Const::Int(1)), Effect::PURE, somewhere());
        // The generic tier declares a resume point rather than a guard, which a
        // `Fall` in an unreachable block is the smallest way to write.
        for point in resumable {
            let block = slow.block();
            slow.switch_to(block);
            slow.end(Terminator::Fall(PointId(*point)));
        }
        slow.switch_to(slow.current_entry());
        slow.end(Terminator::Return(Some(same)));
        (fast.finish(), slow.finish())
    }

    impl FuncBuilder {
        /// The entry block, for a test that switched away from it.
        fn current_entry(&self) -> crate::cfg::BlockId {
            crate::cfg::BlockId(0)
        }
    }

    #[test]
    fn a_point_both_tiers_hold_pairs() {
        let (fast, slow) = tiers(&[0, 1], &[0, 1]);
        assert_eq!(pair(&fast, &slow), Ok(()));
    }

    #[test]
    fn a_fall_the_generic_tier_cannot_resume_is_refused() {
        let (fast, slow) = tiers(&[0, 7], &[0]);
        assert_eq!(pair(&fast, &slow), Err(Mismatch::Unresumable(PointId(7))));
    }

    /// The direction that is allowed, and the reason rule 7 states it: a pass
    /// that proved an assertion removes the guard and leaves the label unused.
    #[test]
    fn a_resume_point_nothing_falls_to_is_allowed() {
        let (fast, slow) = tiers(&[0], &[0, 1, 2]);
        assert_eq!(pair(&fast, &slow), Ok(()));
    }

    #[test]
    fn two_bodies_of_one_tier_are_not_a_pair() {
        let (fast, _) = tiers(&[0], &[0]);
        let (other, _) = tiers(&[0], &[0]);
        assert_eq!(
            pair(&fast, &other),
            Err(Mismatch::SameTier(Tier::Specialised))
        );
    }

    /// The order the two are handed over in must not decide the answer.
    #[test]
    fn the_argument_order_does_not_change_the_verdict() {
        let (fast, slow) = tiers(&[0, 7], &[0]);
        assert_eq!(pair(&slow, &fast), Err(Mismatch::Unresumable(PointId(7))));
    }
}
