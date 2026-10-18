//! The type domain: everything semantic, declared by the language.
//!
//! README rule 2. The structure of the IR is neutral and lives in this crate; a
//! language's types, its join, what each of its primitives computes and what a
//! guard narrows to arrive through this trait.
//!
//! # Why a trait and not a shared lattice
//!
//! Because the lattice is exactly where two languages disagree, and
//! `docs/engine/a-second-language.md` measured it:
//!
//! | | one language | another |
//! |---|---|---|
//! | numbers | one type, integers an optimisation | integer and float DISTINCT |
//! | truth | seven false cases | two |
//! | indices | from zero | from one |
//!
//! A shared lattice would be the union of the two, so each language pays for the
//! other's cases, or it would be the first language's under a neutral name. Both
//! are worse than a parameter.
//!
//! # Why abstract interpretation makes this cheap
//!
//! A fixed-point analysis needs three things from a domain: a top, a join, and a
//! transfer function per operation. It does not need to know what the types mean.
//! So parameterising by domain is the ordinary form of the algorithm rather than a
//! trick played to satisfy a boundary — [`crate::infer`] is the whole of it.

use crate::cfg::{Const, Prim};
use crate::guard::Assertion;

/// What a language knows about its own values.
///
/// Implemented once per front end. `tests/toy_domain.rs` implements a second one
/// with three types, because rule 10 says a boundary with one client is not a
/// boundary.
pub trait Domain {
    /// What this domain says about a value.
    ///
    /// `Eq` because the fixed point is detected by comparing the previous answer
    /// with the new one, so a type that compares unequal to itself would loop for
    /// ever — which is why this is a bound rather than a convention.
    type Type: Clone + Eq;

    /// What is known about a value nothing has been proved about.
    ///
    /// Every analysis starts here and narrows. A domain whose `top` is not the
    /// maximum of its own `join` makes every result unsound, and `join_is_total`
    /// in the toy domain's tests is the shape of the check a front end owes.
    fn top(&self) -> Self::Type;

    /// What is known where control does not arrive.
    ///
    /// The identity of `join`: joining `bottom` with anything answers the
    /// anything. A block with no predecessors analysed yet holds this.
    fn bottom(&self) -> Self::Type;

    /// What two paths agreeing on nothing more specific amounts to.
    ///
    /// Must be commutative, associative, and monotone — the fixed point's
    /// termination rests on it never narrowing.
    fn join(&self, left: &Self::Type, right: &Self::Type) -> Self::Type;

    /// What a constant is.
    fn of_const(&self, value: &Const) -> Self::Type;

    /// What an operation answers, given what its arguments are.
    ///
    /// Answering `top()` is always sound and always useless, which is the right
    /// default for a primitive a domain has nothing to say about.
    fn transfer(&self, prim: Prim, args: &[Self::Type]) -> Self::Type;

    /// What an entry point answers. The language names them, so the language
    /// knows; nothing here does.
    fn of_entry(&self, entry: crate::cfg::EntryId) -> Self::Type;

    /// What a value is, given that a guard's assertion held about it.
    ///
    /// This is where a guard pays: `narrow` is the only thing that makes the
    /// specialised tier know more than the generic one.
    fn narrow(&self, assertion: Assertion, of: &Self::Type) -> Self::Type;

    /// What an operation does besides answer, given what its operands are.
    ///
    /// # Why the effect is asked of the domain and not carried by the primitive
    ///
    /// Because it depends on the operands, and that dependence is the whole
    /// mechanism a guard exists to exploit. An addition of two values a pass has
    /// proved numeric reads nothing, allocates nothing and cannot fail; the same
    /// addition over two unknowns may reach code the program wrote. One primitive,
    /// two effects, and which one applies is a fact about types.
    ///
    /// [`crate::passes::refine_effects`] is what that buys: an instruction lowered
    /// before inference ran carries the pessimistic answer, and asking again with
    /// the inferred types narrows it. A domain must therefore be consistent —
    /// asking about narrower types may not answer a WIDER effect — and the pass
    /// checks rather than trusts, because an inconsistent domain would otherwise
    /// silently make a program's operations movable when they are not.
    fn effect_of(&self, prim: Prim, args: &[Self::Type]) -> crate::Effect;

    /// Whether this type's values are all true, all false, or not decidable.
    ///
    /// On the domain because truth is not neutral — one language has seven false
    /// values and another has two. A pass folding a branch asks this and never
    /// inspects a type itself.
    fn truth_of(&self, of: &Self::Type) -> Option<bool>;

    /// What a block parameter becomes when `new` is what its predecessors now supply and
    /// `old` is what it held.
    ///
    /// The widening of a domain whose lattice is TALL: a parameter that climbs one step
    /// a round through a loop (a range `[0,0]`, `[0,1]`, `[0,2]`, ...) would take as many
    /// rounds as the lattice is high to settle, and [`crate::infer`]'s `ROUNDS` would call
    /// that a bug. A domain answers something at least as wide as BOTH arguments -- which
    /// is what keeps the fixed point sound -- and jumps a still-moving bound to its
    /// extreme, which is what bounds the climb.
    ///
    /// The default is `new`, the right answer for a lattice of finite and small height,
    /// where there is nothing to jump over. [`crate::infer`] asks it only once a block has
    /// been analysed a few times, so a merge that is not in a loop keeps its precision.
    fn widen(&self, old: &Self::Type, new: &Self::Type) -> Self::Type {
        let _ = old;
        new.clone()
    }

    /// Whether `prim` answers a truth value that is the truth (or the negation) of its one
    /// operand, so a fact about the answer is a fact about the operand.
    ///
    /// `Truthy` is the case: a branch over `a < b` is a branch over `truthy(a < b)`, and
    /// the comparison is the fact. The default is that no primitive is.
    fn forwards_truth(&self, prim: Prim) -> Option<Forward> {
        let _ = prim;
        None
    }

    /// What operand number `which` of `prim` is, given that `prim(args)` answered `truth`
    /// -- or `None` when nothing is learned.
    ///
    /// The one place a BRANCH teaches the analysis something: on the edge where `i < n`
    /// held, `i` is below `n`. `args` are the operands' types as the analysis has them, and
    /// the answer must be a type the operand's value certainly has on that edge: at most
    /// `args[which]`, and never a guess. `None` is always sound.
    fn refine(
        &self,
        prim: Prim,
        args: &[Self::Type],
        truth: bool,
        which: usize,
    ) -> Option<Self::Type> {
        let _ = (prim, args, truth, which);
        None
    }
}

/// How a primitive's truth value relates to its operand's. See [`Domain::forwards_truth`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Forward {
    /// True exactly when the operand is.
    Same,
    /// True exactly when the operand is not.
    Negated,
}
