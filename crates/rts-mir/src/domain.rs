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

    /// Whether this type's values are all true, all false, or not decidable.
    ///
    /// On the domain because truth is not neutral — one language has seven false
    /// values and another has two. A pass folding a branch asks this and never
    /// inspects a type itself.
    fn truth_of(&self, of: &Self::Type) -> Option<bool>;
}
