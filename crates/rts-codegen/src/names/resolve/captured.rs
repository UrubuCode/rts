//! Which bindings a nested function reaches, and what that makes an activation build.
//!
//! # Why this is part of resolution and not of a lowering
//!
//! Whether a binding is CAPTURED is the question every lowering that holds a local in
//! a register has to ask before it may: a local a closure reads cannot live in the
//! declarer's registers, because the closure runs after -- or beside -- the activation
//! that holds them. `emit/escape.rs` answers it for the running engine, keyed by
//! spelling and recomputed per function; the new stage had no answer at all, so a
//! function declaring a captured local kept it in SSA while the closure asked for it
//! somewhere else. Both sides compiled.
//!
//! It is answered HERE because the scope tree is the only structure that knows which
//! declaration a use reaches, and a use can reach a declaration written further down:
//! `function g() { return x } var x = 1` captures `x` although the walk meets the read
//! first. So the walk records every use with the scope it was written in, and this
//! resolves them once the tree is complete.
//!
//! # The layout this implies, which is the language's and nobody else's
//!
//! An activation that owns a captured binding builds ONE environment: an ordinary
//! object holding each captured binding under its spelling, plus a link to the
//! environment the function was created in. An activation that owns none builds
//! nothing and hands its own enclosing environment to whatever it creates. That is
//! the shape `emit/binding.rs` gives an environment, and it is this language's layout
//! -- `JsConst::Binding`'s note records three times a deferral like this one was sent
//! to the machine by mistake.
//!
//! So how far a binding is from a use -- the number of links to walk -- is a fact of
//! the scope tree: [`Resolution::hops`].
//!
//! # What is deliberately coarser than the running engine
//!
//! A binding declared in a scope that is fresh per pass of a loop is REPORTED, not
//! laid out: [`Resolution::per_pass`] says so, and the lowering refuses it rather
//! than folding every pass into one slot -- which is the divergence
//! `crates/rts-codegen/README.md` records the running engine closing with a
//! per-iteration environment.
//!
//! A class field initialiser and a static block count as a function of their own for
//! this question, because they run in a constructor or a class evaluation rather than
//! in the activation the class is written in. That over-reports, which is the safe
//! direction: a binding that did not need an environment costs a load, and one
//! missing from it is a wrong answer.

use std::collections::BTreeSet;

use super::{BindingId, Resolution, ScopeId, ScopeKind};
use crate::names::Name;

/// One use of a name, before resolution.
pub(super) struct Reference {
    /// The spelling used.
    pub(super) name: Name,
    /// Where it was written, which is where resolution starts.
    pub(super) scope: ScopeId,
    /// The activation it runs in: a function or module scope, or a class body for a
    /// field initialiser or a static block.
    pub(super) function: ScopeId,
}

/// What the finished walk says about capture.
#[derive(Clone, Debug, Default)]
pub(super) struct Capture {
    /// Every binding a use outside its owning activation reaches.
    captured: BTreeSet<BindingId>,
    /// The scopes opened inside a loop of their own function: one record per pass.
    pub(super) per_pass: BTreeSet<ScopeId>,
    /// The function and module scopes that own at least one captured binding, and
    /// therefore build an environment.
    builders: BTreeSet<ScopeId>,
    /// The function scopes something inside reaches PAST: a use of a binding owned
    /// further out, from the function itself or from one nested in it. Such a
    /// function needs the environment it was made in, to read through or to hand to
    /// a closure it makes.
    reaching: BTreeSet<ScopeId>,
}

/// Where a captured binding lives, seen from one activation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Environment {
    /// How many outer links to follow from the environment in force at the use --
    /// the activation's own when it builds one, the one it was created in otherwise.
    pub hops: u32,
}

impl Resolution {
    /// Resolves every recorded use, once the tree is complete.
    pub(super) fn settle_capture(&mut self, references: &[Reference]) {
        for reference in references {
            let Some(binding) = self.binding_in(reference.scope, reference.name) else {
                continue;
            };
            let target = self.owner(self.binding(binding).scope);
            if target == reference.function {
                continue;
            }
            self.capture.captured.insert(binding);
            // EVERY ACTIVATION BETWEEN the use and the owner has to carry the
            // environment down, whether or not it reads anything itself: the closure
            // doing the reading was made inside it, from the environment it held.
            let mut at = Some(self.owner(reference.function));
            while let Some(here) = at
                && here != target
            {
                self.capture.reaching.insert(here);
                at = self.scope(here).parent.map(|parent| self.owner(parent));
            }
        }
        let builders: BTreeSet<ScopeId> = self
            .capture
            .captured
            .iter()
            .map(|held| self.owner(self.binding(*held).scope))
            .collect();
        self.capture.builders = builders;
    }

    /// Whether a function reads or writes this binding without owning it.
    pub fn captured(&self, binding: BindingId) -> bool {
        self.capture.captured.contains(&binding)
    }

    /// The activation a scope belongs to: the nearest function or module scope
    /// enclosing it, itself included.
    pub fn owner(&self, scope: ScopeId) -> ScopeId {
        let mut at = scope;
        loop {
            let record = self.scope(at);
            match (record.kind, record.parent) {
                (ScopeKind::Function | ScopeKind::Module, _) | (_, None) => return at,
                (_, Some(parent)) => at = parent,
            }
        }
    }

    /// Whether a scope sits inside a class body of its activation -- a class's own name
    /// and what its static blocks declare.
    ///
    /// Asked by a lowering that leaves the class to another stage: those bindings are
    /// that stage's to lay out, reached only by the class's own code.
    pub fn in_class_body(&self, scope: ScopeId) -> bool {
        let mut at = scope;
        loop {
            let record = self.scope(at);
            match (record.kind, record.parent) {
                (ScopeKind::ClassBody, _) => return true,
                (ScopeKind::Function | ScopeKind::Module, _) | (_, None) => return false,
                (_, Some(parent)) => at = parent,
            }
        }
    }

    /// Whether an activation of this function or module scope builds an environment.
    pub fn builds_environment(&self, function: ScopeId) -> bool {
        self.capture.builders.contains(&function)
    }

    /// Whether something inside this function reaches a binding owned further out.
    pub fn reaches_out(&self, function: ScopeId) -> bool {
        self.capture.reaching.contains(&function)
    }

    /// Whether a scope is a fresh record on every pass of a loop in its function.
    pub fn per_pass(&self, scope: ScopeId) -> bool {
        self.capture.per_pass.contains(&scope)
    }

    /// Whether a scope builds an ENVIRONMENT OF ITS OWN on every pass, where a lowering
    /// lays one out per pass: a block or a loop head fresh per pass of a loop, which
    /// owns captured bindings. Its bindings are not in its function's environment --
    /// a closure made in one pass must not see the next pass's value -- but in one
    /// linked to whatever environment was in force where the scope was entered.
    ///
    /// Only the two kinds a lowering enters as statements. A `catch` clause or a
    /// `switch` body inside a loop is per pass too, and stays where it was.
    pub fn pass_environment(&self, scope: ScopeId) -> bool {
        self.per_pass(scope)
            && matches!(self.scope(scope).kind, ScopeKind::Block | ScopeKind::ForHead)
            && !self.captured_in(scope).is_empty()
    }

    /// The captured bindings declared in exactly this scope, in declaration order.
    pub fn captured_in(&self, scope: ScopeId) -> Vec<BindingId> {
        self.scope(scope)
            .bindings
            .iter()
            .copied()
            .filter(|held| self.captured(*held))
            .collect()
    }

    /// The captured bindings a function or module scope owns, in declaration order --
    /// which is the order its environment is laid out in.
    pub fn environment_of(&self, function: ScopeId) -> Vec<BindingId> {
        self.capture
            .captured
            .iter()
            .copied()
            .filter(|held| self.owner(self.binding(*held).scope) == function)
            .collect()
    }

    /// How far a captured binding is from a use in the function whose scope is `from`.
    ///
    /// `None` when the binding's owner does not enclose `from`, which a use resolved
    /// through [`Self::binding_in`] cannot produce -- so a `None` is a caller asking
    /// about the wrong function.
    pub fn hops(&self, from: ScopeId, binding: BindingId) -> Option<Environment> {
        let target = self.owner(self.binding(binding).scope);
        let mut at = self.owner(from);
        let mut hops = 0;
        while at != target {
            if self.builds_environment(at) {
                hops += 1;
            }
            at = self.owner(self.scope(at).parent?);
        }
        Some(Environment { hops })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Resolution, resolve_module};
    use crate::names::Names;
    use crate::names::resolve::BindingId;
    use crate::parse::parse_script;

    fn tree(source: &str) -> (Resolution, Names) {
        let mut names = Names::new();
        let program = parse_script(source, &mut names).expect("the fixture parses");
        (resolve_module(&program.body), names)
    }

    fn only(out: &Resolution, names: &mut Names, spelled: &str) -> BindingId {
        let name = names.intern(spelled);
        let found: Vec<BindingId> = (0..out.len())
            .map(BindingId::from_index)
            .filter(|held| out.binding(*held).name == name)
            .collect();
        assert_eq!(found.len(), 1, "one binding spelled {spelled}");
        found[0]
    }

    /// A closure reading its declarer's local is what makes the local live past
    /// the declarer's registers.
    #[test]
    fn a_local_a_closure_reads_is_captured_and_one_it_does_not_is_not() {
        let (out, mut names) =
            tree("function f() { let kept = 1; let plain = 2; return () => kept + 0 * plain; }");
        let (out, names) = (&out, &mut names);
        assert!(out.captured(only(out, names, "kept")));
        assert!(out.captured(only(out, names, "plain")));
        let (out2, mut names2) =
            tree("function f() { let kept = 1; let plain = 2; plain; return () => kept; }");
        assert!(out2.captured(only(&out2, &mut names2, "kept")));
        assert!(!out2.captured(only(&out2, &mut names2, "plain")));
    }

    /// The case a walk that resolved while walking would get wrong: the use is met
    /// before the declaration it reaches.
    #[test]
    fn a_use_written_above_a_hoisted_declaration_still_captures_it() {
        let (out, mut names) = tree("function f() { function g() { return late; } var late = 1; }");
        assert!(out.captured(only(&out, &mut names, "late")));
    }

    /// A write captures as much as a read does -- a closure assigning an outer local
    /// has to reach the same cell the declarer reads.
    #[test]
    fn a_closure_writing_an_outer_local_captures_it() {
        let (out, mut names) = tree(
            "function f() { let n = 0; const bump = () => { n = n + 1; }; bump(); return n; }",
        );
        assert!(out.captured(only(&out, &mut names, "n")));
    }

    /// The layout's one number: links are counted only through activations that
    /// build an environment, because one that builds none hands its own enclosing
    /// environment on unchanged.
    #[test]
    fn hops_count_only_the_activations_that_build_an_environment() {
        let (out, mut names) = tree(
            "function a() {
               let far = 1;
               function b() {
                 function c() {
                   let near = 2;
                   return () => far + near;
                 }
                 return c;
               }
               return b;
             }",
        );
        let far = only(&out, &mut names, "far");
        let near = only(&out, &mut names, "near");
        // The innermost function: an arrow, which owns nothing and builds nothing.
        let arrow = *out.functions.values().max().expect("four functions");
        // `near` is in `c`, one link up from the environment the arrow was made in.
        assert_eq!(out.hops(arrow, near).map(|held| held.hops), Some(0));
        // `far` is in `a`: past `c`, which builds one, and `b`, which does not.
        assert_eq!(out.hops(arrow, far).map(|held| held.hops), Some(1));
    }

    /// A `let` inside a loop body is a new binding per pass, which one slot per
    /// activation cannot hold.
    #[test]
    fn a_scope_inside_a_loop_is_one_record_per_pass_and_a_function_body_is_not() {
        let (out, mut names) = tree(
            "function f() { for (let i = 0; i < 2; i++) { let inner = i; } let outside = 0;
               while (outside) { const g = function () { let own = 1; }; } }",
        );
        let i = only(&out, &mut names, "i");
        let inner = only(&out, &mut names, "inner");
        let outside = only(&out, &mut names, "outside");
        let own = only(&out, &mut names, "own");
        assert!(out.per_pass(out.binding(i).scope));
        assert!(out.per_pass(out.binding(inner).scope));
        assert!(!out.per_pass(out.binding(outside).scope));
        assert!(!out.per_pass(out.binding(own).scope));
    }
}
