//! A claim, turned into a guard.
//!
//! # Rule 4 already said this, and the code did not do it
//!
//! *"An annotation is treated as a **claim**: it may be used to prove a representation
//! where the language can check it, and **it becomes a guard where it cannot**."* The
//! second half had no implementation. `function f(a: number)` proved nothing about `a`,
//! every primitive over it was refused at the machine boundary, and a test in
//! `tests/machine_boundary.rs` pinned *"an annotation changes nothing"* as though that
//! were the rule rather than the gap.
//!
//! It was the gap. This file is the second half.
//!
//! # What a guard makes true, and why that is not trusting the annotation
//!
//! Nothing here believes TypeScript. The annotation is not evidence that the value IS a
//! number — it is evidence that assuming so is worth the check. The guard performs the
//! check, and **after** it the narrowing is a proof in the ordinary way: `Op::Guard`'s
//! result is the same value with a narrowed type, which is rule 6 of `rts-mir`'s README
//! and the reason a guard is a value in the dataflow rather than emission.
//!
//! So rule 4's *"any place a claim becomes a proof must say what checked it"* is answered
//! structurally here. What checked it is the instruction standing between them, and a
//! narrowed type with no guard above it is unrepresentable rather than merely
//! discouraged.
//!
//! # Why only the specialised tier
//!
//! Because a guard needs somewhere to fall, and the generic tier is that somewhere. A
//! guard in the generic body would be a check whose failure had no destination —
//! `guard.rs` states it from the other side: *"without a deoptimiser there is nothing to
//! reconstruct a frame into, so a guard may only be"* placed where a fall lands
//! somewhere.
//!
//! The generic tier therefore emits none and is slower on purpose. That is the whole
//! two-tier arrangement, not a shortfall in this file.
//!
//! # Which claims become guards, and which cannot yet
//!
//! `number` and `string`, because `JsAssertion` has a row for each. `boolean`,
//! `undefined`, `null`, an object of a named kind and an array have no row — so they
//! produce no guard, the parameter stays generic, and rule 5 is satisfied by that being
//! visible here rather than three lowerings later.
//!
//! A UNION produces none either, and `Claim::is_definite` is what says so: *"a claim
//! that has to be examined before it answers is a claim that did not answer"*. Guarding
//! `number | string` would need two assertions and two falls for one parameter, which is
//! a different shape and not a bigger version of this one.
//!
//! # `number` asserts a DOUBLE and not an integer
//!
//! A JavaScript number is a double, and `Int32` is a subset the lattice tracks
//! separately. Asserting `IsInt32` from `: number` would be a check the annotation does
//! not support — `f(1.5)` is a perfectly good call — so it would fall on ordinary input
//! and the specialised tier would be dead weight.

use rts_mir::Domain;
use rts_mir::cfg::Op;
use rts_mir::guard::{PointId, Tier};

use super::Lowering;
use crate::domain::JsAssertion;
use crate::names::Name;
use crate::names::resolve::BindingId;
use crate::syntax::{Claim, Expr, ExprKind};

/// What a claim asserts, where the assertion table has a row for it.
///
/// `None` is "this claim names something no assertion checks", which is a different
/// thing from a claim that names nothing: `Claim::Unknown` and `Claim::Boolean` both
/// answer `None` here and only the first of them is uninformative.
fn asserted(claim: &Claim) -> Option<JsAssertion> {
    match claim {
        // A JavaScript number is a double. See the header for why this is not `IsInt32`.
        Claim::Number => Some(JsAssertion::IsDouble),
        Claim::Str => Some(JsAssertion::IsStr),
        _ => None,
    }
}

impl Lowering<'_> {
    /// Guards a parameter on what the program claimed it holds.
    ///
    /// Answers whether a guard was emitted, so a caller can count them rather than
    /// re-deriving the decision from the claim a second time.
    pub(super) fn guard_claim(
        &mut self,
        name: Name,
        binding: BindingId,
        claim: &Claim,
        at: &Expr,
    ) -> bool {
        if self.builder.tier() != Tier::Specialised {
            return false;
        }
        if !claim.is_definite() {
            return false;
        }
        let Some(assertion) = asserted(claim) else {
            return false;
        };
        let Some(held) = self.values.get(&binding).copied() else {
            return false;
        };

        let assertion = self.domain.assertion(assertion);
        let of = self.type_of(held);
        let narrowed = self.domain.narrow(assertion, &of);
        // THE POINT IS THE PARAMETER'S POSITION, and nothing else may share it: a fall
        // from here lands in the generic body at the same point, so two guards numbered
        // alike would be two assumptions with one destination.
        let point = PointId(self.points);
        self.points += 1;
        let proved = self.builder.push(
            Op::Guard {
                assertion,
                on: held,
                point,
            },
            // PURE, and that is a real claim rather than a default: checking a
            // representation reads no heap, allocates nothing, and calls nothing. What it
            // may do is not fall through -- and `Effect` has no flag for that because a
            // guard's failure is a BRANCH to the other tier, not a raise. README rule 8
            // of `rts-mir` is the whole of why that distinction matters.
            rts_mir::Effect::PURE,
            at.at,
        );
        self.types.insert(proved, narrowed);
        // EVERY LATER READ SEES THE GUARDED VALUE, which is what makes the narrowing
        // usable at all. Rebinding is the mechanism: the name now holds the instruction's
        // result, so nothing downstream can reach the unproven one by accident.
        self.values.insert(binding, proved);
        let _ = name;
        true
    }

    /// The expression a parameter's guard is attributed to.
    ///
    /// A parameter has no expression of its own in the tree, and a guard needs a position
    /// for the fault record the machine keeps per instruction. The parameter's name at
    /// the function's own position is the honest answer: it is where a reader would look.
    pub(super) fn at_parameter(&self, name: Name, at: rts_cranelift::fault::Position) -> Expr {
        Expr {
            kind: ExprKind::Ident(name),
            at,
        }
    }
}
