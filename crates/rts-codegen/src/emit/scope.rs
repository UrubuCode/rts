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

use std::collections::{BTreeMap, BTreeSet};

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

/// What a name means BESIDES where its storage is.
///
/// # Why this is beside [`Binding`] rather than a third variant of it
///
/// Because it answers a different question. `Binding` says *where the storage
/// is* — a register or a property of an environment — and every reader of a
/// binding has to know that. This says *what the storage holds*, which only the
/// read and the write care about, and a variant would have forced the other
/// readers to re-decide something they have no opinion about.
///
/// Both members exist because a module's bindings are not the module's alone: an
/// `import` names a slot another module writes, and an `export` names a slot
/// another module reads. That is what the specification means by an indirect
/// binding, and the namespace object is the shared slot this engine has.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Alias {
    /// `import { p as name } from "m"` — the binding holds **m's namespace**,
    /// and the value is this property of it, read where the program uses the
    /// name rather than where the `import` was written.
    Import {
        /// The name inside the exporting module.
        property: Name,
    },
    /// A name this module exports. The binding holds the value, as any local
    /// does; an assignment to it also writes the namespace an importer reads.
    Export,
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

    /// The names this layer declares with `let`, `const` or `class` and has
    /// **not reached the declaration of yet** — the temporal dead zone, as a
    /// compile-time list.
    ///
    /// # Why the zone is lexical rather than a sentinel in the storage
    ///
    /// The usual implementation puts a distinguished "empty" value in the slot
    /// and makes every read compare against it. That is a check on the fast
    /// path of every local read, paid by every program, to serve the one case
    /// where the read is *lexically* before the declaration in the same
    /// emission. The emitter already knows which reads those are: it collects a
    /// block's lexical names before emitting the block (`binding::block_layer`
    /// needs the same list), so a read reaching a name still on this list is in
    /// the dead zone by position, decided while compiling and costing a
    /// compiled program nothing.
    ///
    /// It is exact rather than approximate for the reads it covers, and it
    /// covers only reads emitted in this layer or one nested inside it. A read
    /// in a FUNCTION body is not covered — `Scope::for_function` starts a fresh
    /// scope with no pending names — which is required rather than a shortfall:
    /// `let f = () => x; let x = 1; f();` is a legal program, and the arrow's
    /// body runs after the declaration however early it was written.
    pending: Vec<Name>,

    /// Aliases this layer took out of force by declaring the same spelling, put
    /// back when it is left.
    ///
    /// # Why the alias is suspended rather than consulted with a shadow test
    ///
    /// Because the shadow test is the thing that gets written wrongly. An alias
    /// is in force for ONE binding, and a declaration of the same spelling — a
    /// parameter, a `let` in a block, a `catch` name — introduces a different
    /// one. A reader that asked "is this name aliased" by name alone would read
    /// a parameter as a property of somebody else's namespace, silently, which
    /// is the shape of wrongness this crate's README names first. So the map
    /// holds only what is in force, and the layer that shadowed an alias owns
    /// putting it back: [`Scope::leave`] is the single place that happens.
    shadowed: Vec<(Name, Alias)>,
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
    /// Quantas entradas do primeiro `Layer` vieram do escopo ENVOLVENTE.
    ///
    /// Um nome de fora e um nome desta função vivem na mesma lista, e para
    /// `lookup` isso é o que se quer — o de dentro sombreia por ser o último.
    /// Mas há uma pergunta que NÃO é essa: *"esta função já declarou este
    /// nome?"*, que [`Self::declared_in_function`] responde saltando estas.
    ///
    /// Sem a distinção, `hoist_vars` perguntava `lookup(name).is_none()` e
    /// obtinha `Some` para todo o nome que o envolvente tivesse — logo um
    /// `var parent` numa função nunca criava binding, e todas as leituras e
    /// escritas iam para o objeto de fora. Num `<script>` de página, onde o
    /// envolvente é o `window`, isso é `parent`, `top`, `self`, `name`,
    /// `length`, `status`, `origin` e `location`: nomes que código real usa
    /// como variável local todos os dias.
    from_enclosing: usize,
    /// What `this` is in this function, when it has an answer.
    this_value: Option<ValueId>,
    /// The name `this` is held under, for a function where it is assigned
    /// partway through rather than handed over — see [`Scope::bind_this_late`].
    late_this: Option<Name>,
    /// Whether THIS function owns `late_this` as a derived constructor, rather
    /// than borrowing an enclosing function's as an arrow does.
    ///
    /// Only the owner may treat an `undefined` there as "`super()` has not
    /// run": an arrow in an ordinary method borrows a `this` that is
    /// legitimately `undefined` whenever the method was called plainly.
    derived: bool,
    /// The aliases in force, by the name each is in force for.
    ///
    /// Flat rather than per layer because an alias is a fact about a MODULE and
    /// every function nested inside it sees the same ones. [`Layer::shadowed`]
    /// is what keeps that from running over a declaration of the same spelling.
    aliases: BTreeMap<Name, Alias>,
    /// Whether this scope is a MODULE's own body. See [`Scope::in_module_top`].
    module_body: bool,
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
            from_enclosing: 0,
            captured: BTreeSet::new(),
            this_value: None,
            late_this: None,
            derived: false,
            aliases: BTreeMap::new(),
            module_body: false,
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
    /// `aliases` are the enclosing scope's, from [`Scope::aliases`]. They travel
    /// because the names they are in force for travel: an imported name a nested
    /// function reads is a capture candidate, so it arrives in `enclosing` like
    /// any other captured name — and arriving without its alias would read the
    /// NAMESPACE where the program wrote the imported name.
    pub fn for_function(
        environment: Option<ValueId>,
        captured: BTreeSet<Name>,
        own_level: &BTreeSet<Name>,
        enclosing: &[(Name, u32)],
        aliases: &[(Name, Alias)],
    ) -> Self {
        let from_enclosing = enclosing.len();
        let mut entries: Vec<(Name, Binding)> = enclosing
            .iter()
            .map(|(name, hops)| {
                (
                    *name,
                    Binding::InEnvironment {
                        hops: *hops,
                        name: *name,
                    },
                )
            })
            .collect();
        // Only the names bound at this function's OWN level, never every name
        // its environment has room for. The two sets differ because
        // `capture::captured` is deliberately over-inclusive — a name declared
        // in a nested block is counted, so the environment has a slot for it —
        // and binding one of those here at zero hops would shadow the correct
        // outer binding of the same spelling for the whole function.
        //
        // Measured 2026-08-21, before this filter existed: `{ let v = 10;
        // function inner() { return v; } } return v;` inside a function whose
        // enclosing scope also has a captured `v` answered `undefined` where
        // Node and Bun answer `1`. The read resolved at zero hops into a slot
        // the block never wrote, because the block wrote its OWN object.
        //
        // `capture::declared_at_own_level` carries what belongs here and why
        // `var` at any depth is part of it while `let` in a block is not.
        entries.extend(
            captured
                .iter()
                .filter(|name| own_level.contains(name))
                .map(|name| {
                    (
                        *name,
                        Binding::InEnvironment {
                            hops: 0,
                            name: *name,
                        },
                    )
                }),
        );
        // Dropped for every name this function declares AT ITS OWN LEVEL, which
        // is a declaration and therefore a different binding: `own_level` is
        // exactly the list `for_function` just bound at zero hops, so a name in
        // it is this function's and not the module's import. The parameters and
        // the block declarations are dropped later, by `declare`, because that
        // is when they come into existence.
        let aliases = aliases
            .iter()
            .copied()
            .filter(|(name, _)| !own_level.contains(name))
            .collect();
        Scope {
            layers: vec![Layer {
                entries,
                pending: Vec::new(),
                shadowed: Vec::new(),
            }],
            environment,
            captured,
            from_enclosing,
            this_value: None,
            late_this: None,
            derived: false,
            aliases,
            module_body: false,
        }
    }

    /// Every alias in force, for a nested function's scope.
    ///
    /// See [`Scope::for_function`] for why they travel at all.
    pub fn aliases(&self) -> Vec<(Name, Alias)> {
        self.aliases.iter().map(|(name, alias)| (*name, *alias)).collect()
    }

    /// Records that the binding this name currently resolves to is not an
    /// ordinary local.
    ///
    /// Called right after the declaration it describes, by the one place that
    /// knows: [`super::module::emit_import`] for a name an `import` introduced,
    /// and [`super::binding::declare`] for one this module exports.
    pub fn set_alias(&mut self, name: Name, alias: Alias) {
        self.aliases.insert(name, alias);
    }

    /// What this name resolves THROUGH, when it resolves through anything.
    pub fn alias_of(&self, name: Name) -> Option<Alias> {
        self.aliases.get(&name).copied()
    }

    /// Records that this scope is a MODULE's own body, not a function inside it.
    ///
    /// Asked by [`super::binding::declare`], which has to tell a declaration
    /// that `export` could have named from one of the same spelling anywhere
    /// else. Nothing else may ask: a module body is otherwise an ordinary
    /// function body here, and a second question answered from this flag would
    /// be a second thing a module is.
    pub fn mark_module_body(&mut self) {
        self.module_body = true;
    }

    /// Whether this is the outermost layer of a module's own body — the only
    /// place an `export` can have declared a name.
    pub fn in_module_top(&self) -> bool {
        self.module_body && self.layers.len() == 1
    }

    /// Se ESTA função já ligou o nome — os seus parâmetros, os seus capturados,
    /// o que já declarou — ignorando o que o escopo envolvente tem.
    ///
    /// A pergunta de [`Self::lookup`] é outra: *"a que é que este nome
    /// resolve?"*, e aí um nome de fora é uma resposta legítima. Esta serve
    /// quem vai DECLARAR, e para esse um nome de fora não é resposta nenhuma —
    /// é precisamente o que a declaração tem de sombrear.
    pub fn declared_in_function(&self, name: Name) -> bool {
        self.layers.iter().enumerate().any(|(depth, layer)| {
            let proprias = match depth {
                0 => layer.entries.get(self.from_enclosing..).unwrap_or(&[]),
                _ => &layer.entries[..],
            };
            proprias.iter().any(|(bound, _)| *bound == name)
        })
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

    /// Records that `this` lives in this function's environment.
    ///
    /// # Why one function needs that
    ///
    /// A derived constructor. `this` does not exist in one until `super()`
    /// returns — the object is the base of the chain's to make, and until then
    /// there is nothing to be a receiver. So `this` is **assigned** partway
    /// through the body, and a block parameter cannot be: it is one SSA value
    /// for the whole function.
    ///
    /// The environment is where this engine already puts a name whose value
    /// changes and outlives a register, so it is where this goes rather than
    /// into a second mechanism for one construct.
    ///
    /// `derived` is whether this function is that constructor itself — see
    /// [`Scope::derived_this`] — as against an arrow reaching the same name.
    pub fn bind_this_late(&mut self, name: Name, derived: bool) {
        self.late_this = Some(name);
        self.derived = derived;
        // Cleared, so a read that forgot to ask about the late binding fails
        // loudly instead of answering the receiver the caller passed — which
        // for a derived constructor is `undefined` and would silently be the
        // wrong object.
        self.this_value = None;
    }

    /// The name `this` is held under, when it is held rather than passed.
    pub fn late_this(&self) -> Option<Name> {
        self.late_this
    }

    /// The name a DERIVED CONSTRUCTOR holds its `this` under, when this body is
    /// one — the only body where that binding can still be uninitialised, and
    /// so the only one that owes `GetThisBinding`'s `ReferenceError`.
    pub fn derived_this(&self) -> Option<Name> {
        self.late_this.filter(|_| self.derived)
    }

    /// What `this` is here, if this function has an answer.
    pub fn this_value(&self) -> Option<ValueId> {
        self.this_value
    }

    /// Swaps in a receiver for a SUBSTITUTED method body, answering the old one.
    ///
    /// A field rather than a layer, so it is saved and restored around the
    /// substitution the way `Ctx::substituting` is: the body reads `this`
    /// through `this_value` like any other body, and the caller's own `this` is
    /// back before the next statement. `late_this` is left alone — that is the
    /// arrow mechanism, and an arrow inside a substituted body is refused for
    /// its own reasons.
    pub fn swap_this(&mut self, value: Option<ValueId>) -> Option<ValueId> {
        std::mem::replace(&mut self.this_value, value)
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

    /// Enters a layer that lives in an environment of its OWN.
    ///
    /// # What this is for, and why it is not [`Scope::enter`]
    ///
    /// `for (let i = 0; …)` gives every pass its own `i`, so two closures made
    /// in two passes see two values. An ordinary block layer cannot express
    /// that: a captured name resolves to a property of *the function's*
    /// environment, one object for the whole activation, so every pass writes
    /// the slot the previous pass captured. That is the divergence this crate's
    /// README recorded, and the language calls the fix
    /// `CreatePerIterationEnvironment`.
    ///
    /// The chain is what carries it. `environment` becomes a fresh object whose
    /// `__rts_outer` is the one in force until now, `names` are bound in it at
    /// zero hops, and **everything already reachable is re-bound one hop
    /// further out** — which is the whole cost, and the reason this is a method
    /// here rather than a loop somewhere else: `hops` is a compile-time number,
    /// so inserting a link means every binding past it counts differently, and
    /// a caller that got that wrong would read another activation's variable.
    ///
    /// Only `InEnvironment` bindings are re-bound. A `Value` one is a register
    /// and no chain reaches it, so bumping it would be inventing a hop for a
    /// name that does not travel one.
    ///
    /// # Only the INNERMOST binding of a spelling travels, and dropping the rest
    /// is the shadowing rule
    ///
    /// Re-binding walks every layer, and a spelling can appear in several: an
    /// enclosing environment holds `t`, and a parameter or a local named `t`
    /// shadows it. Copying both into the new layer puts the OUTER one beside
    /// the inner one at the innermost level, and [`Scope::lookup`] scans in
    /// reverse — so the outer binding wins and the loop body reads the
    /// enclosing variable in place of the parameter.
    ///
    /// That is not a hypothetical ordering argument. `f(t)` under a `try`, with
    /// a module-level `t`, read the module's value inside `for (let w = …)` and
    /// answered it — silently, since the enclosing name exists and holds
    /// something. `tests/cross-runtime/syntax/claude-param-shadows-toplevel-in-try-loop.ts`
    /// is the fixture; the `try` is what makes the function build an
    /// environment at all, and the classic `for` is the only loop that opens a
    /// per-iteration one.
    ///
    /// So the innermost binding of each spelling is the one that travels, and a
    /// spelling whose innermost binding is a `Value` travels not at all —
    /// `lookup` then falls through to that `Value`, which is still in scope and
    /// still dominates the body.
    ///
    /// Returns the environment that was in force, for [`Scope::leave_environment`].
    pub fn enter_environment(&mut self, environment: ValueId, names: &[Name]) -> Option<ValueId> {
        // Innermost-wins, in one pass over the layers outermost-first: a later
        // entry of the same spelling overwrites the position it already holds,
        // so the ORDER of first appearance survives while the BINDING is the
        // one `lookup` would have answered.
        let mut seen: BTreeMap<Name, usize> = BTreeMap::new();
        let mut effective: Vec<(Name, Binding)> = Vec::new();
        for (bound, binding) in self.layers.iter().flat_map(|layer| layer.entries.iter()) {
            match seen.get(bound) {
                Some(&at) => effective[at].1 = *binding,
                None => {
                    seen.insert(*bound, effective.len());
                    effective.push((*bound, *binding));
                }
            }
        }
        let mut entries: Vec<(Name, Binding)> = effective
            .into_iter()
            .filter_map(|(bound, binding)| match binding {
                Binding::InEnvironment { hops, name } => Some((
                    bound,
                    Binding::InEnvironment {
                        hops: hops + 1,
                        name,
                    },
                )),
                Binding::Value(_) => None,
            })
            .collect();
        // After the outer names, so a head name of the same spelling shadows
        // the one it was copied from — `lookup` scans in reverse, so this
        // order IS the shadowing rule, the same way [`Scope::for_function`]
        // relies on it.
        entries.extend(names.iter().map(|name| {
            (
                *name,
                Binding::InEnvironment {
                    hops: 0,
                    name: *name,
                },
            )
        }));
        // A head name is a DECLARATION, so it shadows an alias of the same
        // spelling exactly as a parameter does — and it is bound here rather
        // than through `declare`, which is why the suspension is taken here too.
        let shadowed = names
            .iter()
            .filter_map(|name| self.aliases.remove(name).map(|alias| (*name, alias)))
            .collect();
        self.layers.push(Layer {
            entries,
            pending: Vec::new(),
            shadowed,
        });
        std::mem::replace(&mut self.environment, Some(environment))
    }

    /// Leaves a layer [`Scope::enter_environment`] opened.
    ///
    /// Takes the previous environment rather than remembering it here, for the
    /// reason the snapshot pair does: the emitter's bracketing is visible at
    /// the call site, and a stack held inside this type would be a second place
    /// for it to get out of step.
    pub fn leave_environment(&mut self, previous: Option<ValueId>) {
        self.leave();
        self.environment = previous;
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
        if let Some(layer) = self.layers.pop() {
            self.aliases.extend(layer.shadowed);
        }
    }

    /// Introduces a name in the innermost layer.
    ///
    /// Redeclaration is not rejected here. `let x; let x;` is an early error,
    /// and early errors are a checker's job (`PLAN.md` L10) — reporting it here
    /// would mean the rule lived in two places, and rule 3 says a semantic rule
    /// is stated once. Shadowing an *outer* declaration is legal and is what
    /// the layering is for.
    pub fn declare(&mut self, name: Name, value: ValueId) {
        // A declaration introduces a binding of its own, so whatever this
        // spelling meant through an alias it does not mean here. Suspended in
        // the layer that declared it and restored when that layer is left —
        // `Layer::shadowed` says why asking by name would be wrong instead.
        if let Some(alias) = self.aliases.remove(&name) {
            if let Some(layer) = self.layers.last_mut() {
                layer.shadowed.push((name, alias));
            }
        }
        let layer = self
            .layers
            .last_mut()
            .expect("a scope always has at least the function's own layer");
        // A name this layer already holds AS A VALUE is REPLACED in place rather
        // than pushed beside. That is the emitter's own seed and nothing else:
        // a `switch` gives its clauses one shared block scope and has to create
        // the slot before the tests branch, so `case 0: let x = 5` is a store
        // into a binding that already exists. Pushing a second entry would leave
        // the first where `snapshot` counted it, and the block parameter
        // carrying the name across a fall-through would carry the SEED.
        //
        // Only a `Binding::Value`, and the restriction is load-bearing.
        // `Scope::for_function` puts the enclosing function's names in this same
        // layer as `InEnvironment`, and its documentation says the ORDER is the
        // shadowing rule — `lookup` scans in reverse, so a parameter pushed
        // after one of them wins. Replacing those in place made
        // `function inner(x) { … }` inside a function with its own captured `x`
        // read the OUTER one: measured on
        // `fn-meta/codex2_104_nested_closure_shadowing.ts`, which answered
        // `param` where every other runtime answers `inner`.
        if let Some(entry) = layer
            .entries
            .iter_mut()
            .find(|(held, binding)| *held == name && matches!(binding, Binding::Value(_)))
        {
            entry.1 = Binding::Value(value);
            return;
        }
        layer.entries.push((name, Binding::Value(value)));
    }

    /// Records that the innermost layer declares these names further down.
    ///
    /// Called once per body, with exactly the `let`, `const` and `class` names
    /// the body declares **at its own level** — the list
    /// [`super::binding::lexical_names`] answers, which is the same list the
    /// block's environment is sized from. A `var` and a hoisted `function` are
    /// deliberately absent: neither has a dead zone, and a `var` read before
    /// its line is `undefined` by specification.
    pub fn expect_lexical(&mut self, names: &[Name]) {
        let layer = self
            .layers
            .last_mut()
            .expect("a scope always has at least the function's own layer");
        layer.pending.extend_from_slice(names);
    }

    /// Records that a pending name has now been declared.
    ///
    /// The dead zone ends at the declaration and not at the end of the block,
    /// so this is called from the one place a name comes into existence —
    /// [`super::binding::declare`] — rather than from each of its callers.
    pub fn initialize(&mut self, name: Name) {
        for layer in self.layers.iter_mut().rev() {
            if let Some(at) = layer.pending.iter().position(|pending| *pending == name) {
                layer.pending.remove(at);
                return;
            }
        }
    }

    /// Whether reading this name here is a read in the temporal dead zone.
    ///
    /// Scanned innermost-first and stopping at the first layer that says
    /// anything, because that is what shadowing means: a name pending in an
    /// inner block is in its dead zone even though an outer binding of the same
    /// spelling is perfectly readable — the inner declaration is what the name
    /// refers to for the whole block, which is the rule that makes the zone
    /// exist at all.
    pub fn in_dead_zone(&self, name: Name) -> bool {
        for layer in self.layers.iter().rev() {
            if layer.pending.contains(&name) {
                return true;
            }
            if layer.entries.iter().any(|(bound, _)| *bound == name) {
                return false;
            }
        }
        false
    }

    /// What a name currently means, innermost layer first.
    pub fn lookup(&self, name: Name) -> Option<Binding> {
        self.layers.iter().rev().find_map(|layer| {
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
#[path = "scope_tests.rs"]
mod tests;
