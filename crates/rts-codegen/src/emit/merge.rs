//! What a name means where two paths come back together.
//!
//! # Why this is a module and not two copies
//!
//! It was two. `stmt.rs` merged the arms of an `if` and `choice.rs` merged the
//! operands of `?:` and `&&`, and both had written out the same rule: find the
//! names the two paths disagree about, give the join a parameter for each at
//! the representation that arrives, and rebind the scope to those parameters.
//!
//! The give-away was the comments. Both carried a near-identical paragraph
//! explaining why the parameter is not `Tagged`, which is what a rule written
//! twice looks like from the outside — and rule 3 says a semantic rule is
//! stated once, *where it is decided*, because the second statement is the one
//! that goes stale.
//!
//! # What is deliberately NOT shared
//!
//! The control flow around it. A statement's arm can fail to reach the join —
//! `if (c) return 1; else return 2;` is a whole function — so `emit_if` decides
//! which predecessors exist before asking anything here. An operand of an
//! expression always reaches the join and always carries a value, so
//! `choice.rs` never has that question.
//!
//! Collapsing those into one function was tried in the head and rejected: it
//! would take a list of optional paths and an optional value, and every caller
//! would be handling outcomes that its own shape makes impossible.

use rts_cranelift::ir::{BlockId, FuncBuilder, ValueId};

use super::Scope;
use super::scope::Binding;

/// Which positions two environments disagree about.
///
/// Comparing by position is comparing by name: two snapshots are only ever
/// taken from the same point in the same emission, so they hold the same names
/// in the same order. `Scope::snapshot` states that and this depends on it.
pub fn disagreements(first: &[Binding], second: &[Binding]) -> Vec<usize> {
    (0..first.len())
        .filter(|&position| first[position] != second[position])
        .collect()
}

/// A join parameter per disagreement, at the representation both paths reach.
///
/// # Why not `Tagged`
///
/// A generic parameter would widen every proven value passed to it — silently,
/// because the builder inserts that — and a proof would not survive an `if`.
///
/// # Why both sides are read, and what reading one cost
///
/// This took a single `reference` environment and read the representation off
/// it, on the grounds that reading one side is reading both: the numeric
/// analysis decides per **name** rather than per store, so a proved local is
/// numeric on every path and an unproved one is widened at every store.
///
/// That reasoning is sound for the analysis it names and it is not sound for
/// the *machine*, which is where the two can still differ — a value narrowed by
/// a guard on one path, an `Repr::I32` binding whose other arm carries the
/// widened form. The consequence was not a wrong answer, and that is worth
/// saying precisely: `FuncBuilder::jump` refuses to narrow, so the disagreement
/// surfaced as `BuildError::ImplicitNarrowing` and the program was REFUSED.
///
/// So the parameter is the join of the two, which is the machine's own merge
/// rule and the only total one: agreement keeps the representation, and
/// disagreement widens to the generic form, where the builder's automatic
/// widening then meets it from both sides. A program that used to be refused
/// compiles, and one that used to compile is unchanged — the join of a
/// representation with itself is itself.
pub fn parameters(
    builder: &mut FuncBuilder,
    join: BlockId,
    merged: &[usize],
    first: &[Binding],
    second: &[Binding],
) -> Vec<ValueId> {
    merged
        .iter()
        .map(|&position| {
            let repr = builder
                .repr_of(first[position].value())
                .join(builder.repr_of(second[position].value()));
            builder.add_block_param(join, repr)
        })
        .collect()
}

/// What each name means after the join.
///
/// The merged ones are the join's parameters; the rest are whatever survived
/// from the path that reached it, which is why the caller says which
/// environment to start from — for an `if` where one arm returned, that is the
/// *other* arm's.
pub fn settle(scope: &mut Scope, mut after: Vec<Binding>, merged: &[usize], params: Vec<ValueId>) {
    for (position, param) in merged.iter().zip(params) {
        after[*position] = Binding::Value(param);
    }
    scope.restore(&after);
}
