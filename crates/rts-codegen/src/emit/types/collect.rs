//! Reading a body's annotations, and throwing away the ones it contradicts.
//!
//! # Why the traversal is [`super::super::capture`]'s
//!
//! Because a second description of the tree's shape is how a node comes to be
//! walked by one analysis and skipped by another, and this crate has been bitten
//! by that five separate times — `proven.rs` records every one. What this file
//! adds is the *position* a child sits in, which the shared walker deliberately
//! does not say, and nothing else.
//!
//! There is no `match` on `StmtKind` or `ExprKind` here ending in `_ => {}`.
//! That shape is the scar: it compiles, it looks total, and it silently stops
//! covering the node somebody adds next year.
//!
//! # The kill side is blunt on purpose
//!
//! A name assigned anywhere in the body loses its claim outright — not "loses it
//! if the assignment disagrees". The refinement is real and it waits for a
//! consumer that needs it, because the direction of the blunt rule is the safe
//! one: fewer claims survive, so fewer sites speculate, so a mistake here costs
//! a fast path rather than an answer.
//!
//! It also has to be ACTIVE rather than merely absent. Names are interned by
//! text with no scope — `proven.rs` states that and depends on it — so a `catch
//! (x)`, a `using x` and a for-each target all rebind a spelling the outer body
//! may have annotated, and a pass that only *skipped* them would leave the outer
//! claim standing over the inner binding.

use std::collections::HashSet;

use super::super::capture::{Child, StmtChild, walk_expr, walk_stmt};
use super::super::proven::Numeric;
use super::{Facts, Kind};
use crate::names::Name;
use crate::syntax::{AssignTarget, Claim, Expr, ExprKind, Pattern, Stmt};

/// What this body's annotations claim, after the body has had its say.
///
/// Takes the parameters separately rather than reading them off the tree,
/// because the common case has no binding to read: `bind_parameters`
/// short-circuits a plain name with no default and pushes no prologue statement
/// at all, so `function f(x: number)`'s claim exists only on the `Parameter` and
/// never reaches a `Binding`.
///
/// Takes `numeric` so that a claim about a name the body already PROVED is never
/// minted. That is not an optimisation: `Ctx::holds_number` is the one answer to
/// "does this name hold a number", and a second source for it is the rule this
/// crate's rule 3 exists to prevent.
///
/// Takes no `Ctx`, for the reason `proven` takes none: interning inside a pass
/// is interning inside something that may run more than once.
pub(in crate::emit) fn analyse(body: &[Stmt], parameters: &[(Name, Claim)], numeric: &Numeric) -> Facts {
    let mut facts = Facts::default();

    for (name, claim) in parameters {
        let kind = Kind::of(claim);
        if kind.is_definite() {
            facts.insert(*name, kind);
        }
    }

    let mut declared = Vec::new();
    for statement in body {
        seed(statement, &mut declared);
    }
    for (name, kind) in declared {
        // Two declarations of one spelling in one body — a shadowing block, a
        // loop that redeclares — meet rather than overwrite, so the surviving
        // claim is what both agreed about. Overwriting would make the answer
        // depend on which one the walk reached last.
        let settled = match facts.get(name) {
            Some(existing) => existing.meet(kind),
            None => kind,
        };
        facts.insert(name, settled);
    }

    let mut written = HashSet::new();
    for statement in body {
        assigned(statement, &mut written);
    }
    for name in written {
        facts.remove(name);
    }

    // Last, and after the kills: a name the body proved has nothing to
    // speculate about, and leaving one in would hand a caller two answers.
    facts.retain_unproved(numeric);

    facts
}

impl Facts {
    /// Drops every claim about a name the body already proved.
    ///
    /// Separate from the kill loop because it is a different rule: a contradicted
    /// claim is evidence that turned out false, and a proved one is evidence that
    /// was never needed.
    fn retain_unproved(&mut self, numeric: &Numeric) {
        let proved: Vec<Name> = self
            .names()
            .filter(|name| numeric.holds_number(*name))
            .collect();
        for name in proved {
            self.remove(name);
        }
    }
}

/// Every claim a declaration in this body makes.
///
/// Descends through the shared walker, so a declaration inside an `if`, a loop
/// body, a `try` or a `switch` is reached without this file knowing those exist.
/// A nested function is NOT descended into: its annotations are facts about its
/// own body, and this answer is scoped to one.
fn seed(statement: &Stmt, into: &mut Vec<(Name, Kind)>) {
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Binding(binding) => {
            if let (Pattern::Name(name), Some(claim)) = (&binding.target, &binding.claim) {
                let kind = Kind::of(claim);
                if kind.is_definite() {
                    into.push((*name, kind));
                }
            }
            if let Some(init) = &binding.value {
                seed_expr(init, into);
            }
        }
        StmtChild::Expr(expr) => seed_expr(expr, into),
        StmtChild::Stmt(inner) => seed(inner, into),
        // A handler's body declares like any other block. Its BINDING seeds
        // nothing — `catch (e)` carries no annotation the language allows —
        // but the statements inside it do.
        StmtChild::Catch(catch) => {
            for inner in &catch.body {
                seed(inner, into);
            }
        }
        StmtChild::Function(_) | StmtChild::Class(_) => {}
    });
}

/// The same, through an expression — an arrow body, a conditional, an argument.
fn seed_expr(expr: &Expr, into: &mut Vec<(Name, Kind)>) {
    walk_expr(expr, &mut |child| match child {
        Child::Expr(inner) => seed_expr(inner, into),
        Child::Function(_) | Child::Class(_) => {}
    });
}

/// Every name this body writes to, however it writes to it.
///
/// A superset on purpose. It counts a plain assignment, a compound one, an
/// increment, a destructuring target, a `catch` binding and a for-each target —
/// and where it cannot tell, it counts. The direction is what makes it safe:
/// over-counting removes a claim, which removes a speculation, which removes a
/// fast path and never an answer.
fn assigned(statement: &Stmt, into: &mut HashSet<Name>) {
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Binding(binding) => {
            // A declaration is BOTH a rebinding and, when it is annotated, a
            // seed — and the first version of this killed on every one, so
            // every annotated declaration destroyed its own claim before it
            // could be read. `const label: string = "d"` reported nothing, which
            // is what the census caught on its first run.
            //
            // The rule that separates them: a declaration that CLAIMS something
            // is the claim about that spelling from here on, and one that claims
            // nothing rebinds a spelling an outer claim may have named — and
            // names are interned by text with no scope, so that one must strip
            // actively rather than merely be skipped.
            let seeds = matches!(binding.target, Pattern::Name(_))
                && binding
                    .claim
                    .as_ref()
                    .is_some_and(|claim| Kind::of(claim).is_definite());
            if !seeds {
                bound(&binding.target, into);
            }
            if let Some(init) = &binding.value {
                assigned_expr(init, into);
            }
        }
        StmtChild::Expr(expr) => assigned_expr(expr, into),
        StmtChild::Stmt(inner) => assigned(inner, into),
        // The case this module's own documentation names, and the one a pass
        // that merely SKIPPED handlers would get wrong: `catch (x)` rebinds
        // the spelling `x`, and names are interned by text with no scope, so
        // an outer claim about `x` would otherwise stand over the handler's
        // binding.
        StmtChild::Catch(catch) => {
            if let Some(pattern) = &catch.binding {
                bound(pattern, into);
            }
            for inner in &catch.body {
                assigned(inner, into);
            }
        }
        // A nested function or class can hold a closure over one of these names
        // and write to it, and this pass has no way to see that from here. The
        // conservative answer is the one that removes claims, so every name the
        // nested code could possibly write is not something this can enumerate —
        // what it does instead is refuse to descend, and phase 1 spends nothing,
        // so nothing depends on the difference yet. A consumer that arrives
        // before the descent does must take `captured` the way `escape` does.
        StmtChild::Function(_) | StmtChild::Class(_) => {}
    });
}

/// The same, through an expression.
fn assigned_expr(expr: &Expr, into: &mut HashSet<Name>) {
    match &expr.kind {
        ExprKind::Assign { target, .. } => {
            if let AssignTarget::Place(place) = target
                && let ExprKind::Ident(name) = &place.kind
            {
                into.insert(*name);
            }
        }
        ExprKind::Update { target, .. } => {
            if let ExprKind::Ident(name) = &target.kind {
                into.insert(*name);
            }
        }
        _ => {}
    }
    walk_expr(expr, &mut |child| match child {
        Child::Expr(inner) => assigned_expr(inner, into),
        Child::Function(_) | Child::Class(_) => {}
    });
}

/// Every name a pattern binds.
fn bound(pattern: &Pattern, into: &mut HashSet<Name>) {
    match pattern {
        Pattern::Name(name) => {
            into.insert(*name);
        }
        _ => {
            // A destructuring pattern binds names this file would have to walk a
            // second shape to enumerate, and a second shape is the thing this
            // module refuses to write. Nothing is inserted, and nothing is
            // claimed about those names either — a pattern carries its claim on
            // the binding, and only `Pattern::Name` seeds one.
        }
    }
}
