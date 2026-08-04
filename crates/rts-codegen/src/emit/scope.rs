//! What a name currently means.
//!
//! # A binding is a name for a value, not a cell
//!
//! The obvious first implementation gives every local a stack slot: declaring
//! stores, reading loads. It is obvious because it is what a machine does, and
//! it is wrong to start with, because undoing it later is a rewrite rather than
//! an optimisation — every read has become a memory operation that a subsequent
//! pass has to prove away.
//!
//! So a binding maps to a `ValueId` directly, and assigning rebinds the name.
//! The IR is already in SSA form and the machine's builder already refuses what
//! that requires, so this is the representation that fits rather than a clever
//! one.
//!
//! What that costs is paid later and named now: **a local that a closure
//! captures, or that a loop merges across passes, cannot be a plain `ValueId`.**
//! The first needs a cell because two frames share it; the second needs a block
//! parameter because two predecessors disagree. Neither is emitted yet, and both
//! are why this type is a shallow structure rather than a `HashMap` — the moment
//! it must distinguish "value" from "cell", the distinction goes in the entry
//! and every reader is forced to handle it.
//!
//! # Why shadowing is a stack of layers rather than a rename
//!
//! `{ let x = 1; { let x = 2; } }` declares two different bindings that share a
//! spelling. Renaming one would work and would lose the fact that they are
//! different, which a diagnostic pointing at the inner one needs.

use rts_cranelift::ir::ValueId;

use crate::names::Name;

/// What a name is bound to.
///
/// One variant today. It is an enum rather than a bare `ValueId` because the
/// second variant is known to be coming — a captured local lives in a cell, and
/// a reader that pattern-matches here will be told to handle it, where one that
/// dereferenced a `ValueId` would silently read the wrong thing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Binding {
    /// A value, directly. Reading is free; assigning rebinds.
    Value(ValueId),
}

impl Binding {
    /// The value behind it.
    ///
    /// A method rather than a free function, and it lives here rather than
    /// beside either caller because this enum is the thing that will grow: when
    /// a captured local becomes a cell, every site reading a binding has to
    /// learn to load it. Two copies of this match — which is what `emit/loops.rs`
    /// and `emit/stmt.rs` each had — means two places to teach, with nothing
    /// tying them together.
    pub fn value(self) -> ValueId {
        match self {
            Binding::Value(value) => value,
        }
    }
}

/// One lexical layer.
#[derive(Default)]
struct Layer {
    /// The bindings introduced in it, in declaration order.
    ///
    /// A `Vec` rather than a map, and the reason is measured elsewhere in this
    /// repository rather than assumed: a scope holds a handful of names, and a
    /// linear scan over a handful beats hashing one. The lookup walks layers
    /// innermost-first, which is also what shadowing means, so the two are the
    /// same loop.
    entries: Vec<(Name, Binding)>,
}

/// The lexical environment during emission.
pub struct Scope {
    layers: Vec<Layer>,
}

impl Default for Scope {
    fn default() -> Self {
        Self::new()
    }
}

impl Scope {
    /// An environment with one layer, for a function body.
    pub fn new() -> Self {
        Scope {
            layers: vec![Layer::default()],
        }
    }

    /// Enters a nested block.
    pub fn enter(&mut self) {
        self.layers.push(Layer::default());
    }

    /// Leaves the innermost block.
    ///
    /// # Panics
    ///
    /// If it would leave the function's own layer. That is a defect in this
    /// module's own bracketing rather than anything a program can cause, so it
    /// panics instead of returning a `Result` nobody could act on.
    pub fn leave(&mut self) {
        assert!(
            self.layers.len() > 1,
            "left more scopes than were entered — the function's own layer is \
             not a block and cannot be popped"
        );
        self.layers.pop();
    }

    /// Introduces a name in the innermost layer.
    ///
    /// Redeclaration is not rejected here. `let x; let x;` is an early error,
    /// and early errors are a checker's job (`PLAN.md` L10) — reporting it here
    /// would mean the rule lived in two places, and rule 3 says a semantic rule
    /// is stated once. Shadowing an *outer* declaration is legal and is what
    /// the layering is for.
    pub fn declare(&mut self, name: Name, value: ValueId) {
        let layer = self
            .layers
            .last_mut()
            .expect("a scope always has at least the function's own layer");
        layer.entries.push((name, Binding::Value(value)));
    }

    /// What a name currently means, innermost layer first.
    pub fn lookup(&self, name: Name) -> Option<Binding> {
        self.layers
            .iter()
            .rev()
            .find_map(|layer| {
                layer
                    .entries
                    .iter()
                    .rev()
                    .find(|(bound, _)| *bound == name)
                    .map(|(_, binding)| *binding)
            })
    }

    /// Rebinds an existing name, wherever it was declared.
    ///
    /// Returns whether one was found. Assignment does not introduce: `x = 1`
    /// with no `x` in scope is a global store in sloppy mode and a `ReferenceError`
    /// in strict, and neither is "declare a local" — so this reports the miss
    /// rather than papering over it with a declaration.
    pub fn assign(&mut self, name: Name, value: ValueId) -> bool {
        for layer in self.layers.iter_mut().rev() {
            if let Some(entry) = layer
                .entries
                .iter_mut()
                .rev()
                .find(|(bound, _)| *bound == name)
            {
                entry.1 = Binding::Value(value);
                return true;
            }
        }
        false
    }

    /// Where a name sits in [`Self::snapshot`], innermost binding first.
    ///
    /// A loop asks this to turn "the body assigns `x`" into a position it can
    /// carry as a block parameter. `None` means the name is not in scope out
    /// here — a body-local, which is a fresh binding every pass and which
    /// nothing outside the body can refer to.
    pub fn position_of(&self, name: Name) -> Option<usize> {
        let mut base = 0;
        let mut found = None;
        for layer in &self.layers {
            for (offset, (bound, _)) in layer.entries.iter().enumerate() {
                if *bound == name {
                    found = Some(base + offset);
                }
            }
            base += layer.entries.len();
        }
        found
    }

    /// Every binding in scope, outermost first, as a flat list.
    ///
    /// # What this is for
    ///
    /// Merging. After an `if`, a name assigned in one branch and not the other
    /// has two definitions reaching its use, and the machine's answer to that
    /// is a block parameter. Finding *which* names those are means comparing
    /// the environment on each path, which means being able to take it apart.
    ///
    /// # Why a flat list rather than a map is sound here
    ///
    /// Two snapshots are only ever compared when they come from the same point
    /// in the same emission, so they have the same names in the same positions
    /// — a branch that declares something does it in a layer that is popped
    /// before the merge. Comparing by position is therefore comparing by name,
    /// without the allocation a keyed diff would cost at every branch.
    pub fn snapshot(&self) -> Vec<Binding> {
        self.layers
            .iter()
            .flat_map(|layer| layer.entries.iter().map(|(_, binding)| *binding))
            .collect()
    }

    /// Puts the bindings back, by position.
    ///
    /// Used to emit the second branch of an `if` from the same environment the
    /// first one started in, rather than from whatever the first one left.
    ///
    /// # Panics
    ///
    /// If the snapshot does not describe this scope. That means it came from a
    /// different point in the emission, which is a defect in this module's
    /// bracketing rather than anything a program can express.
    pub fn restore(&mut self, snapshot: &[Binding]) {
        let mut taken = snapshot.iter();
        for layer in &mut self.layers {
            for entry in &mut layer.entries {
                entry.1 = *taken
                    .next()
                    .expect("a snapshot describes exactly the scope it was taken from");
            }
        }
        assert!(
            taken.next().is_none(),
            "a snapshot describes exactly the scope it was taken from"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::names::Names;
    use rts_cranelift::ir::{Function, Signature};
    use rts_cranelift::repr::Repr;

    /// Two distinct values to bind, without needing a whole emission.
    fn two_values() -> (ValueId, ValueId) {
        let mut func = Function::new(Signature::default());
        let block = func.push_block();
        (
            func.push_block_param(block, Repr::Tagged),
            func.push_block_param(block, Repr::Tagged),
        )
    }

    #[test]
    fn an_inner_declaration_hides_an_outer_one_and_the_outer_survives() {
        let mut names = Names::default();
        let x = names.intern("x");
        let (outer, inner) = two_values();

        let mut scope = Scope::new();
        scope.declare(x, outer);
        scope.enter();
        scope.declare(x, inner);
        assert_eq!(scope.lookup(x), Some(Binding::Value(inner)));
        scope.leave();
        assert_eq!(
            scope.lookup(x),
            Some(Binding::Value(outer)),
            "`{{ let x = 1; {{ let x = 2; }} }}` leaves the outer binding \
             untouched — a rename-based implementation gets this right and \
             loses the fact that they are two bindings"
        );
    }

    #[test]
    fn assigning_reaches_an_outer_layer_where_declaring_would_not() {
        let mut names = Names::default();
        let x = names.intern("x");
        let (first, second) = two_values();

        let mut scope = Scope::new();
        scope.declare(x, first);
        scope.enter();
        assert!(scope.assign(x, second));
        scope.leave();
        assert_eq!(
            scope.lookup(x),
            Some(Binding::Value(second)),
            "assignment writes the binding it found; it does not introduce a \
             new one in the block it was written in"
        );
    }

    #[test]
    fn assigning_a_name_nothing_declared_reports_the_miss() {
        let mut names = Names::default();
        let x = names.intern("x");
        let (value, _) = two_values();

        // Sloppy mode makes this a global store and strict mode makes it a
        // ReferenceError. Both need to know it was not found, so neither is
        // served by quietly declaring a local.
        assert!(!Scope::new().assign(x, value));
    }
}
