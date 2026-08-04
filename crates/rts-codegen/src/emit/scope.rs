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
//! What that costs was paid, and the prediction held: **a local that a closure
//! captures, or that a loop merges across passes, cannot be a plain `ValueId`.**
//! The second needs a block parameter because two predecessors disagree. The
//! first needs heap storage because two activations share it — and when that
//! arrived, the distinction went in the entry and every reader was forced to
//! handle it, which is why they now all go through `emit::binding` instead.
//!
//! # Why shadowing is a stack of layers rather than a rename
//!
//! `{ let x = 1; { let x = 2; } }` declares two different bindings that share a
//! spelling. Renaming one would work and would lose the fact that they are
//! different, which a diagnostic pointing at the inner one needs.

use std::collections::BTreeSet;

use rts_cranelift::ir::ValueId;

use crate::names::Name;

/// What a name is bound to.
///
/// An enum rather than a bare `ValueId`, which is what made adding the second
/// variant a compiler error at every reader rather than a silent wrong read.
/// Nothing matches on this outside `emit::binding` any more — see there for why
/// four copies of the match was the thing worth removing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Binding {
    /// A value, directly. Reading is free; assigning rebinds.
    Value(ValueId),

    /// A property of an environment object, `hops` links out along the chain.
    ///
    /// What a captured local becomes. It is not a value because two activations
    /// of the capturing function share the variable, and a register belongs to
    /// one frame — so the storage has to be somewhere both can reach, which is
    /// the heap.
    ///
    /// `hops` is a **compile-time** number: the emitter knows how many
    /// environments out the name lives, so a read is that many loads and one
    /// property access, never a search up a chain comparing names.
    InEnvironment {
        /// How many `__outer` links to follow before looking the name up.
        hops: u32,
        /// Which property it is. The key is minted from this, so the name is
        /// kept rather than the number — `Names` is what remembers the mapping,
        /// and holding a raw key here would be a second table.
        name: Name,
    },
}

impl Binding {
    /// The value behind it.
    ///
    /// # Panics
    ///
    /// For a binding that lives in an environment, which has no single value to
    /// be. That is unreachable rather than merely unlikely, and the reason is
    /// worth stating because it is what makes the merge code correct: this is
    /// called only for names two paths **disagree** about, and an environment
    /// binding is identical along every path — it is a description of where the
    /// storage is, and no branch moves it.
    ///
    /// Returning an `Option` was rejected. Every caller would answer it the same
    /// way, by unwrapping, and an unwrap at four sites is four places for the
    /// reasoning above to be re-derived and got wrong once.
    pub fn value(self) -> ValueId {
        match self {
            Binding::Value(value) => value,
            Binding::InEnvironment { .. } => panic!(
                "a binding in an environment was merged, which cannot happen: \
                 it names heap storage, so two paths always agree about it"
            ),
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
    /// The environment object this function's captured names live in.
    ///
    /// `None` for a function that captures nothing and is captured by nothing,
    /// which is the common case and the one that must cost nothing: no object
    /// is allocated and every local stays a value.
    environment: Option<ValueId>,
    /// Which of this function's own names live in that object.
    ///
    /// Consulted when a name is *declared*, because that is the moment the
    /// decision has to be made and the only moment it can be: a binding cannot
    /// be a register in the statement that introduces it and heap storage four
    /// statements later, when the closure that captures it is written.
    captured: BTreeSet<Name>,
    /// What `this` is in this function, when it has an answer.
    this_value: Option<ValueId>,
}

impl Default for Scope {
    fn default() -> Self {
        Self::new()
    }
}

impl Scope {
    /// An environment with one layer, for a function body that captures
    /// nothing.
    pub fn new() -> Self {
        Scope {
            layers: vec![Layer::default()],
            environment: None,
            captured: BTreeSet::new(),
            this_value: None,
        }
    }

    /// An environment for a function body, with what it captures.
    ///
    /// # Why every captured name is bound immediately
    ///
    /// A binding normally appears when its declaration is emitted. A captured
    /// one cannot wait, because function declarations are **hoisted**: the body
    /// of an inner function is emitted before the outer function's `let`
    /// statements have run, and that body has to be able to resolve the names
    /// it closes over.
    ///
    /// ```text
    /// let k = 4;                    ← emitted second
    /// function get() { return k; }  ← emitted FIRST, and reads `k`
    /// ```
    ///
    /// So the storage exists from function entry and the declaration writes
    /// into it, which is also what the language describes: a captured variable
    /// is a slot in an environment record created when the function is entered,
    /// not when the declaration is reached.
    ///
    /// Reading one before its declaration answers `undefined` rather than
    /// throwing, where `let` specifies a temporal dead zone. A named gap: the
    /// zone needs a sentinel distinguishable from `undefined` and a throw to
    /// report it, and throwing is not emitted at all yet.
    ///
    /// `enclosing` is what the defining functions put in their environments,
    /// already at the hop counts seen from HERE. Bound before this function's
    /// own names so that a local of the same spelling shadows it, which is what
    /// the innermost binding winning means — `lookup` scans in reverse, so the
    /// order these are pushed in IS the shadowing rule.
    pub fn for_function(
        environment: Option<ValueId>,
        captured: BTreeSet<Name>,
        enclosing: &[(Name, u32)],
    ) -> Self {
        let mut entries: Vec<(Name, Binding)> = enclosing
            .iter()
            .map(|(name, hops)| (*name, Binding::InEnvironment { hops: *hops, name: *name }))
            .collect();
        entries.extend(
            captured
                .iter()
                .map(|name| (*name, Binding::InEnvironment { hops: 0, name: *name })),
        );
        Scope {
            layers: vec![Layer { entries }],
            environment,
            captured,
            this_value: None,
        }
    }

    /// The environment object captured names live in, if this function has one.
    pub fn environment(&self) -> Option<ValueId> {
        self.environment
    }

    /// Records what `this` is in this function.
    ///
    /// # Why an arrow records nothing
    ///
    /// An arrow takes `this` from where it was *written*, not from how it is
    /// called — which is, as the tree's own comment puts it, the one thing
    /// arrows actually change. So its `this` parameter is not its answer, and
    /// the right answer is the defining function's, which means carrying that
    /// through the environment.
    ///
    /// That is not built, so an arrow records `None` and `this` inside one is
    /// refused by name. Refused rather than answered with the parameter: the
    /// parameter holds whatever the caller passed, so using it would make
    /// `this` inside an arrow silently mean the wrong thing — which is worse
    /// than not compiling.
    pub fn set_this(&mut self, value: ValueId, is_arrow: bool) {
        self.this_value = if is_arrow { None } else { Some(value) };
    }

    /// What `this` is here, if this function has an answer.
    pub fn this_value(&self) -> Option<ValueId> {
        self.this_value
    }

    /// Whether a name this function declares has to live on the heap.
    pub fn is_captured(&self, name: Name) -> bool {
        self.captured.contains(&name)
    }

    /// Every name reachable through the environment chain, with how far out.
    ///
    /// Read when a nested function's scope is built. Ordered outermost-layer
    /// first, and innermost binding wins because the inner function re-declares
    /// in that order — which is what shadowing means.
    pub fn reachable(&self) -> Vec<(Name, u32)> {
        self.layers
            .iter()
            .flat_map(|layer| layer.entries.iter())
            .filter_map(|(name, binding)| match binding {
                Binding::InEnvironment { hops, .. } => Some((*name, *hops)),
                Binding::Value(_) => None,
            })
            .collect()
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
