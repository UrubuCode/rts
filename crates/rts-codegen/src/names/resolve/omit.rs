//! A `const` arrow that is only ever called, folded into the function that declares it.
//!
//! # What it is for
//!
//! `for (let i = 0; i < n; i++) { const c = (x) => x + i; a = c(a); }` makes a closure
//! per pass, and the closure's read of `i` makes `i` CAPTURED -- so `i` lives in an
//! environment built per pass, with a copy at every step. The MIR stage substitutes
//! `c(a)` with `a + i` (`lower/substitute.rs`), which removes the call and, with the
//! dead-value pass, the closure; the environment it forced stays, because the capture
//! was decided here, by the scope tree, before anything was lowered. Measured
//! 2026-09-26: 97 ns per pass against 10.8 on the running emitter, whose `omit.rs`
//! decides the same question up front.
//!
//! So it is decided up front here too: an arrow every use of which will be
//! substituted is not a function at all, and its body's scope is a BLOCK of the
//! function that declares it. Its reads are then ordinary reads, nothing is captured
//! on its account, and the door neither compiles nor makes it.
//!
//! # The conditions, each the reason a substitution could fail or mean something else
//!
//! - an ARROW (its `this` and `arguments` are the caller's) that is not async or a
//!   generator, with plain parameters and no default or rest, answering one
//!   expression the lowering substitutes (`lower::substitutable`);
//! - every use of its `const` is the callee of a direct call with no spread, in the
//!   SAME function, written after the arrow -- so the declaration has been lowered and
//!   is in force at every call -- and none from inside the arrow itself, which would
//!   be a recursion no substitution ends;
//! - every name its body reads, parameters aside, resolves at each call to the binding
//!   it resolves to where the arrow was written -- asked of the scope tree per call,
//!   which is exact where the running emitter's count of declarations program-wide
//!   refused every helper reading a loop counter in a program with two loops.
//!
//! Folding one arrow can make another foldable -- one that calls it from inside its
//! body now counts as calling from the declaring function -- so this runs to a
//! fixpoint.

use rts_cranelift::fault::Position;

use super::captured::Reference;
use super::{BindingId, Resolution, ScopeId, ScopeKind};
use crate::names::Name;
use crate::syntax::{Binding, Expr, ExprKind, FunctionBody, Pattern, Stmt, StmtKind};

/// A `const` bound to an arrow of the foldable shape, met during the walk.
pub(super) struct Arrow {
    binding: BindingId,
    at: Position,
    parameters: Vec<Name>,
    body: Expr,
}

impl super::Walker<'_> {
    /// Records `binding` if it binds a `const` to an arrow of the foldable shape;
    /// `scope` is where the name landed.
    pub(super) fn arrow_candidate(&mut self, binding: &Binding, scope: ScopeId) {
        let (Pattern::Name(name), Some(value)) = (&binding.target, &binding.value) else {
            return;
        };
        let ExprKind::Function(function) = &value.kind else {
            return;
        };
        if !function.captures_this
            || function.is_async
            || function.is_generator
            || function.rest_parameter.is_some()
        {
            return;
        }
        let mut parameters = Vec::with_capacity(function.parameters.len());
        for parameter in &function.parameters {
            let (Pattern::Name(held), None) = (&parameter.target, &parameter.default) else {
                return;
            };
            parameters.push(*held);
        }
        let body = match &function.body {
            FunctionBody::Expression(expr) => expr.as_ref().clone(),
            FunctionBody::Block(statements) => match statements.as_slice() {
                [Stmt {
                    kind: StmtKind::Return(Some(expr)),
                    ..
                }] => expr.clone(),
                _ => return,
            },
        };
        if !crate::lower::substitutable(&body) {
            return;
        }
        let Some(declared) = self
            .out
            .scope(scope)
            .bindings
            .iter()
            .rev()
            .copied()
            .find(|held| self.out.binding(*held).name == *name)
        else {
            return;
        };
        self.arrows.push(Arrow {
            binding: declared,
            at: function.at,
            parameters,
            body,
        });
    }
}

impl Resolution {
    /// Folds every arrow that meets the conditions, to a fixpoint, rewriting the uses
    /// written inside each to count as the declaring function's.
    pub(super) fn settle_omitted(&mut self, arrows: &[Arrow], references: &mut [Reference]) {
        loop {
            let mut folded = false;
            for arrow in arrows {
                if self.omitted.contains_key(&arrow.at) || !self.foldable(arrow, references) {
                    continue;
                }
                let Some(own) = self.functions.get(&arrow.at).copied() else {
                    continue;
                };
                let declaring = self.owner(self.binding(arrow.binding).scope);
                self.scopes[own.0 as usize].kind = ScopeKind::Block;
                for reference in references.iter_mut() {
                    if reference.function == own {
                        reference.function = declaring;
                    }
                }
                self.omitted.insert(arrow.at, arrow.binding);
                folded = true;
            }
            if !folded {
                return;
            }
        }
    }

    fn foldable(&self, arrow: &Arrow, references: &[Reference]) -> bool {
        let Some(own) = self.functions.get(&arrow.at).copied() else {
            return false;
        };
        let declaring = self.owner(self.binding(arrow.binding).scope);
        // NOT INTO A MODULE: a module's own code is compiled by the running emitter,
        // never by the MIR stage, so it substitutes no call and the arrow is a real
        // function there -- whose body the door would then lower with a scope tree
        // that calls it a block, reading every name around it as its own.
        if self.scope(declaring).kind == ScopeKind::Module {
            return false;
        }
        let name = self.binding(arrow.binding).name;
        let Some(written) = self.scope(own).parent else {
            return false;
        };
        let mut read = Vec::new();
        names_read(&arrow.body, &mut read);
        read.retain(|held| !arrow.parameters.contains(held));
        references
            .iter()
            .filter(|held| {
                held.name == name && self.binding_in(held.scope, held.name) == Some(arrow.binding)
            })
            .all(|held| {
                held.called
                    && held.function == declaring
                    && held.at > arrow.at
                    && !self.within(held.scope, own)
                    // EVERY FREE NAME means at the call what it meant where the arrow
                    // was written: a block between the two that shadows one keeps the
                    // arrow a function.
                    && read
                        .iter()
                        .all(|free| self.binding_in(held.scope, *free) == self.binding_in(written, *free))
            })
    }

    /// Records which of `declarations` nothing writes.
    ///
    /// The lowering substitutes such a function's calls the way it does a `const`
    /// arrow's (`lower/substitute.rs`), and this is what makes that sound: the binding
    /// holds the function declared, from the top of its function to the end. A write is
    /// an assignment, an update, a pattern's leaf or a loop head's target -- each
    /// recorded as one by the walk -- or Annex B's block-level declaration of the same
    /// name, which writes the function's own binding when it is evaluated. A direct
    /// `eval` could write it too, by name; the MIR door declines any body that mentions
    /// one.
    ///
    /// It is NOT folded as an arrow is: its body has its own `arguments` and `this`,
    /// which only the lowering can spell, so it stays a function.
    pub(super) fn settle_never_written(&mut self, declarations: &[BindingId], references: &[Reference]) {
        for &declared in declarations {
            let record = self.binding(declared);
            let (name, scope) = (record.name, record.scope);
            let alone = self
                .scope(scope)
                .bindings
                .iter()
                .all(|held| *held == declared || self.binding(*held).name != name);
            let annexed = self.annex.values().any(|held| *held == declared);
            let written = references.iter().any(|held| {
                held.written
                    && held.name == name
                    && self.binding_in(held.scope, held.name) == Some(declared)
            });
            if alone && !annexed && !written {
                self.never_written.insert(declared);
            }
        }
    }

    /// Whether `binding` is a function declaration nothing writes -- so a call to it
    /// calls the function declared.
    pub fn never_written(&self, binding: BindingId) -> bool {
        self.never_written.contains(&binding)
    }

    /// Whether `scope` is `outer` or written inside it.
    fn within(&self, scope: ScopeId, outer: ScopeId) -> bool {
        let mut at = Some(scope);
        while let Some(here) = at {
            if here == outer {
                return true;
            }
            at = self.scope(here).parent;
        }
        false
    }

    /// Whether the arrow written at `at` was folded into the function declaring it.
    pub fn omitted(&self, at: Position) -> bool {
        self.omitted.contains_key(&at)
    }

    /// Whether `binding` is the `const` of an arrow folded into its function -- which
    /// holds no closure, so a call to it that is not substituted has nothing to call.
    pub fn binds_omitted(&self, binding: BindingId) -> bool {
        self.omitted.values().any(|held| *held == binding)
    }
}

/// Every identifier an expression reads.
fn names_read(expr: &Expr, into: &mut Vec<Name>) {
    if let ExprKind::Ident(name) = expr.kind {
        into.push(name);
    }
    crate::emit::capture::walk_expr(expr, &mut |child| {
        if let crate::emit::capture::Child::Expr(inner) = child {
            names_read(inner, into);
        }
    });
}
