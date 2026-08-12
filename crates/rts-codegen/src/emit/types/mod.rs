//! What a program's annotations claim, and what a claim is allowed to buy.
//!
//! # A claim is not a proof, and this module exists to keep them apart
//!
//! [`super::proven`] answers what the BODY established: `let a = 0` makes `a` a
//! number because the initialiser is one. That is evidence this compiler
//! produced, and `expr::stored` spends it by *not widening* — an unguarded
//! narrowing, which is only sound because the proof is total over the body.
//!
//! An annotation is a different thing entirely. `syntax::Claim` says so on its
//! first line: TypeScript's annotations are erased before anything runs and are
//! unchecked at every boundary the program does not own. What is easy to miss is
//! how much wider that hole is here than in a TypeScript toolchain — **this
//! compiler never runs `tsc`**, so an annotation is unchecked at *every*
//! boundary including the ones inside the file. `function add1(x: number) {
//! return x + 1 }` called as `add1("7")` answers `"71"`, today, with no cast and
//! no foreign call anywhere in the program.
//!
//! So an annotation is a guess. A good one — a program that annotates is usually
//! telling the truth — but a guess.
//!
//! # The rule that makes a guess safe to act on
//!
//! **A claim never eliminates a check. It chooses which check to emit.**
//!
//! `emit::expr::emit_guarded` is the shape: it does not prove two operands are
//! doubles, it GUARDS that they are, takes the instruction when they are, and
//! calls the runtime when they are not. A wrong guess costs a compare and a
//! predicted branch. It is never a wrong answer.
//!
//! That rule is enforced by a type rather than remembered. [`Speculation`] is
//! the only thing this module hands out, only [`Facts::claimed`] mints one, and
//! `expr::stored` neither takes one nor ever will — so a claim cannot reach the
//! unguarded narrowing a proof reaches, because the signature refuses it. A
//! `bool` in that position could not have said what checked it.
//!
//! # What this module is spent on today: nothing
//!
//! Deliberately. `emit_binary` already speculates *unconditionally* — every
//! operand pair nothing proved already gets two guards and a slow path — so a
//! `number` claim on an operand asks for a guard that is already emitted, and
//! buys exactly zero. The obvious first use of a type pass is a dead end, and it
//! is better to find that out from a counter than from a week of work.
//!
//! So this phase builds the answer and counts it. `RTS_CLAIM_STATS=1` prints how
//! many claims exist, how many are spendable at all, how many name something the
//! body already proved, and how many sit at the two sites a later phase would
//! spend them at. The precedent is `RTS_ESCAPE_STATS`, which killed a campaign
//! for the price of one environment variable by reporting 299 candidates and 10
//! replacements.

mod census;
mod classes;
mod collect;

pub(in crate::emit) use census::Census;
pub(in crate::emit) use classes::{Classes, declared};
pub(in crate::emit) use collect::analyse;

use std::collections::HashMap;

use rts_cranelift::repr::Repr;

use crate::names::Name;
use crate::syntax::Claim;

/// What a value is, in the vocabulary a representation decision can act on.
///
/// `Copy` and seven points, where [`Claim`] is nine and owns a `Vec` and a
/// `Box`. The flattening is the point: an element type and a union's members are
/// facts nothing downstream can spend, and carrying them would be modelling
/// TypeScript's type language a second time — which `syntax::Claim`'s own
/// documentation already refuses to do once.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(in crate::emit) enum Kind {
    /// `number`.
    Number,
    /// `boolean`.
    Boolean,
    /// `string`.
    Str,
    /// An object of a named kind — the name as written, never resolved.
    Instance(Name),
    /// An array of anything.
    ///
    /// The element type is dropped rather than carried: there is no per-element
    /// representation for it to decide, so it is a fact with no consumer.
    Array,
    /// `undefined` or `null` — a definite kind with no machine test of its own.
    Other,
    /// Anything this vocabulary does not name, and every union.
    ///
    /// The honest bottom. A union answers `Unknown` even when every member is a
    /// number, for `Claim::is_definite`'s stated reason: a claim that has to be
    /// examined before it answers is a claim that did not answer.
    Unknown,
}

impl Kind {
    /// What a claim says, in this vocabulary.
    pub(super) fn of(claim: &Claim) -> Kind {
        match claim {
            Claim::Number => Kind::Number,
            Claim::Boolean => Kind::Boolean,
            Claim::Str => Kind::Str,
            Claim::Object(name) => Kind::Instance(*name),
            Claim::Array(_) => Kind::Array,
            Claim::Undefined | Claim::Null => Kind::Other,
            Claim::Union(_) | Claim::Unknown => Kind::Unknown,
        }
    }

    /// What two claims about one name agree on.
    ///
    /// Equality or nothing, which is `rts_cranelift::repr::Repr::join`'s rule one
    /// level up and deliberately not a second lattice: that module's argument is
    /// that picking one of two disagreeing answers would make a value's meaning
    /// depend on which edge control arrived from, and the same is true here.
    pub(super) fn meet(self, other: Kind) -> Kind {
        match self == other {
            true => self,
            false => Kind::Unknown,
        }
    }

    /// The representation a guard could test for, if any.
    ///
    /// `None` for five of the seven, and that is a fact about the MACHINE rather
    /// than a gap here: `lower::value::test` offers exactly one reference guard,
    /// `Repr::Ref(RefKind::Opaque)`, which tests the reference tag and nothing
    /// more — an object, an array, a function and a string all pass it. So a
    /// `string` claim has no exact test, and a guard that accepted the nearest
    /// available one would be a check that does not check the claim.
    ///
    /// Returning `Option` rather than a nearest match is what makes that
    /// unrepresentable instead of merely discouraged.
    ///
    /// # What it would take to make `Str` spendable, and why it is not next
    ///
    /// Asked and answered rather than left open, because the census makes it
    /// the obvious next move: three quarters of the claims that survive a body
    /// are `Str`, `Array` or `Instance`, and all three are `None` here.
    ///
    /// The TEST is not the missing piece. `lower::value::test` cannot narrow to
    /// any `RefKind` but `Opaque` — it answers `CannotProveReferenceKind` — but
    /// `Terminator::GuardType` already tests a cell's HEADER against a
    /// `TypeId`, which is what distinguishes text from an object, and the
    /// compiler already holds the `TypeRegistry` the runtime mints from.
    ///
    /// What is missing is a CONSUMER. There is no instruction that does
    /// anything with a reference proved to be text: a length is a runtime call,
    /// an index is a runtime call, a comparison is a runtime call. A guard whose
    /// success path reaches the same call as its failure path has bought
    /// nothing, and a capability with no producer is what rule 9 refuses.
    ///
    /// # The consumer arrived, and it did not need the guard
    ///
    /// `s.length` is a LOAD now — `entry::cache` answers for the one property a
    /// string has that a load can serve, and `entry::context` keeps the length
    /// in the cell so there is something at an offset to read. That is the text
    /// operation this paragraph was waiting for, and it arrived through the
    /// inline cache rather than through a claim: the site recognises a string by
    /// the type in its header, which is what it already compared.
    ///
    /// So the guard is still unspent, and now for a sharper reason than "no
    /// consumer": the consumer that arrived does not need one. A claim could
    /// only tell the compiler to emit a check the cache performs anyway — which
    /// is exactly what `emit_binary` already showed for `number`.
    ///
    /// The numbering blocker is also resolved and is worth recording so nobody
    /// re-derives it: `rts-host` builds ONE `TypeRegistry`, hands it to the
    /// compiler at emission and carries the same one into the runtime's
    /// `Context`. The two do share a number space, the way keys do. What the
    /// compiler cannot do is name the TEXT layout, because nothing declares it
    /// until the `Context` is built — after emission. Declaring it first would
    /// close that, and it should not be done until something needs the answer.
    ///
    /// So the order is the reverse of the obvious one: the machine gains a text
    /// operation first, and this returns `Some` for `Str` in the same change
    /// that gives it something to reach.
    pub(in crate::emit) fn repr(self) -> Option<Repr> {
        match self {
            Kind::Number => Some(Repr::F64),
            Kind::Boolean => Some(Repr::Bool),
            Kind::Str | Kind::Instance(_) | Kind::Array | Kind::Other | Kind::Unknown => None,
        }
    }

    /// Whether this names one kind of value at all.
    pub(super) fn is_definite(self) -> bool {
        self != Kind::Unknown
    }
}

/// A claim about a name, and the permission to spend it as a guard.
///
/// # Why this is a type and not a `Kind`
///
/// Because it is the enforcement of this module's one rule. A `Kind` reaching
/// `expr::stored` — which narrows with no guard, on the strength of a proof —
/// would be a claim spent as a proof, and the wrong answer that follows is a
/// machine double holding a string reference. A separate type that `stored`
/// does not accept makes that a compile error rather than a review comment.
///
/// It carries no `Source`, deliberately: the only way to obtain one is
/// [`Facts::claimed`], which mints one only for a name the body did NOT prove,
/// so "this came from an annotation" is true by construction rather than
/// recorded and trusted.
#[derive(Copy, Clone, Debug)]
pub(in crate::emit) struct Speculation(Kind);

impl Speculation {
    /// What is being guessed.
    pub(in crate::emit) fn kind(self) -> Kind {
        self.0
    }

}

/// What one function body's annotations claim about its names.
///
/// Scoped exactly as `proven::Numeric` is — one body, saved and restored around
/// a nested function — because an annotation in an enclosing function says
/// nothing about a name an inner one happens to spell the same way.
#[derive(Default)]
pub(super) struct Facts {
    claimed: HashMap<Name, Kind>,
}

impl Facts {
    /// What a name is claimed to hold, if a claim about it survived.
    ///
    /// `None` for a name nothing annotated, for one whose claim the body
    /// contradicted, and — importantly — for one the body PROVED. A proof and a
    /// claim are not two answers to be weighed: where there is a proof there is
    /// nothing to speculate, and returning one anyway would give a caller two
    /// sources for one fact.
    pub(in crate::emit) fn claimed(&self, name: Name) -> Option<Speculation> {
        self.claimed.get(&name).copied().map(Speculation)
    }

    /// How many names carry a surviving claim.
    pub(in crate::emit) fn len(&self) -> usize {
        self.claimed.len()
    }

    pub(super) fn insert(&mut self, name: Name, kind: Kind) {
        self.claimed.insert(name, kind);
    }

    pub(super) fn remove(&mut self, name: Name) {
        self.claimed.remove(&name);
    }

    pub(super) fn get(&self, name: Name) -> Option<Kind> {
        self.claimed.get(&name).copied()
    }

    /// Every name a claim survived about.
    pub(super) fn names(&self) -> impl Iterator<Item = Name> + '_ {
        self.claimed.keys().copied()
    }

    /// Every surviving claim, for the census.
    pub(in crate::emit) fn kinds(&self) -> impl Iterator<Item = (Name, Kind)> + '_ {
        self.claimed.iter().map(|(name, kind)| (*name, *kind))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_union_is_not_definite_even_when_every_member_is_a_number() {
        // `Claim::is_definite` states the rule and this is it in the flattened
        // vocabulary: a claim that has to be examined before it answers did not
        // answer. Collapsing `number | number` here would mean carrying the
        // members to find that out, which is modelling the type language twice.
        let union = Claim::Union(vec![Claim::Number, Claim::Number]);
        assert_eq!(Kind::of(&union), Kind::Unknown);
        assert!(!Kind::of(&union).is_definite());
    }

    #[test]
    fn only_the_two_kinds_the_machine_can_test_are_spendable() {
        // Not an omission. `lower::value::test` has one reference guard and an
        // object, an array, a function and a string all pass it — so a `string`
        // claim has no test, and the nearest available one is not the one.
        assert_eq!(Kind::Number.repr(), Some(Repr::F64));
        assert_eq!(Kind::Boolean.repr(), Some(Repr::Bool));
        assert_eq!(Kind::Str.repr(), None);
        assert_eq!(Kind::Array.repr(), None);
        assert_eq!(Kind::Other.repr(), None);
    }

    #[test]
    fn two_claims_that_disagree_answer_nothing_rather_than_one_of_them() {
        assert_eq!(Kind::Number.meet(Kind::Number), Kind::Number);
        assert_eq!(Kind::Number.meet(Kind::Str), Kind::Unknown);
        assert_eq!(Kind::Unknown.meet(Kind::Number), Kind::Unknown);
    }
}
