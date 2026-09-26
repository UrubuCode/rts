//! A function declared in a BLOCK, and the `var` of its function it also writes.
//!
//! # Why this engine has it everywhere
//!
//! Annex B.3.3 is web compatibility: in sloppy code, `{ function f() {} }` binds `f` in
//! the block AND in a `var` of the enclosing function, assigned when the declaration is
//! evaluated -- so a call from outside the block, written above it or run later,
//! reaches the function. The running emitter gives block functions that visibility in
//! every file, and a bundle depends on it (`tests/claude-bundle-real-gaps-2.test.ts`
//! calls two of them from a closure made before either block runs). So the scope tree
//! records the same `var`, or the two stages would disagree about what the name is.
//!
//! # When there is none
//!
//! Where replacing the declaration by `var f` would be an early error, which is the
//! specification's own condition: another binding of the name in a block between the
//! declaration and its function, or a binding of the function's that is not a `var`
//! -- a parameter, a `let`, a `const`. An existing `var` of the name is that `var`.

use rts_cranelift::fault::Position;

use super::{BindingId, Origin, Resolution, ScopeId};
use crate::names::Name;

/// A function declaration met in a block, to be settled once the tree is complete --
/// a conflicting `let` can be written below it.
pub(super) struct Candidate {
    pub(super) name: Name,
    pub(super) block: ScopeId,
    pub(super) function: ScopeId,
    pub(super) at: Position,
}

impl Resolution {
    /// Gives each candidate its function's `var`, where it may have one.
    pub(super) fn settle_annex(&mut self, candidates: &[Candidate]) {
        for candidate in candidates {
            if let Some(var) = self.annex_var(candidate) {
                self.annex.insert(candidate.at, var);
            }
        }
    }

    fn annex_var(&mut self, candidate: &Candidate) -> Option<BindingId> {
        let named = |out: &Resolution, scope: ScopeId| {
            out.scope(scope)
                .bindings
                .iter()
                .copied()
                .find(|held| out.binding(*held).name == candidate.name)
        };
        let mut at = self.scope(candidate.block).parent?;
        while at != candidate.function {
            if named(self, at).is_some() {
                return None;
            }
            at = self.scope(at).parent?;
        }
        match named(self, candidate.function) {
            Some(held) if self.binding(held).origin == Origin::Var => Some(held),
            Some(_) => None,
            None => Some(self.declare(candidate.name, Origin::Var, candidate.function)),
        }
    }

    /// The `var` a function declared in a block at `at` also writes, if it has one.
    pub fn annex_b(&self, at: Position) -> Option<BindingId> {
        self.annex.get(&at).copied()
    }
}
