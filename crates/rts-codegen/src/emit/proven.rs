//! Which locals hold a number, everywhere they hold anything.
//!
//! # What this buys, measured before it was written
//!
//! Every operator the emitter produced was a call into the runtime, because
//! nothing had been proved about any operand. That cost **24.5 ns per operator
//! per pass**, measured by varying only the operator count in a loop
//! (`docs/engine/new-engine-speed.md`), and it is why the same kernel ran 130×
//! slower than the old engine.
//!
//! A call is the correct emission for `a + b` in general: it converts both
//! operands to primitives and then decides between adding and concatenating.
//! What removes it is not a faster call — it is knowing that both operands are
//! numbers, because then the decision has one answer and the machine has an
//! instruction for it.
//!
//! # Why this is an analysis and not a type annotation
//!
//! Rule 4: *a type annotation is evidence, not proof.* `let x: number = f()`
//! claims something the program can violate, and TypeScript's own soundness
//! holes are not a list worth memorising. What is checked here is what the
//! function itself does to a binding, which no declaration can contradict.
//!
//! # The rule, and why it is this shape
//!
//! A local is numeric when **its initialiser is numeric and every assignment to
//! it is numeric**. That is a fixpoint, because "numeric" for one local can
//! depend on another: `let a = 1; let b = a;` makes `b` numeric only once `a`
//! is known to be.
//!
//! It starts optimistic — every local with a numeric-looking initialiser — and
//! removes what does not survive, until nothing changes. Starting pessimistic
//! and adding would be wrong for a loop: `let i = 0; while (…) { i = i + 1; }`
//! needs `i` numeric to prove `i + 1` numeric to prove `i` numeric, and only the
//! optimistic direction reaches that.
//!
//! # What it deliberately does not try to prove
//!
//! Anything that leaves the function or comes from outside it: parameters, the
//! result of a call, a property, a captured local. Each is a claim this pass has
//! no evidence for, and a wrong answer here is not slow code — it is `arith` on
//! a string.
//!
//! # The one property it does prove, and why that is not an exception
//!
//! A property [`super::escape`] replaced. `let o = {a: 1}` with `o` proved not
//! to escape leaves no object and no property — it leaves a binding, and every
//! store to it is in this body where this pass can see it. So it is proved as
//! `(o, a)` rather than as `o`, and named once at the end.
//!
//! This is not a softening of the rule above; a replaced property is not a
//! property. The rule is unchanged for every object that survives, and it is
//! `escape.rs` that decides which those are.
//!
//! It also has to be able to take the proof BACK: `let o = {a: 1}; o.a = "x";`
//! must not leave `(o, a)` proved. That is the same shape as the assignment
//! rule for a local, applied to the same fixpoint.
//!
//! Leaving it out cost the whole of what scalar replacement had bought.
//! Measured 2026-08-06, release, 400 statements: the replaced program compiled
//! in 49.05 ms against 5.82 ms for the same program written with two plain
//! locals, because every operator on a replaced property was widened at its
//! store and fell back to the generic call. With this, 7.11 ms.

use std::collections::HashSet;

use super::capture::{self, Child, StmtChild};
use super::escape::Flattened;
use crate::names::Name;
use crate::syntax::{
    AssignOp, AssignTarget, Binding, BinaryOp, Expr, ExprKind, ForEachTarget, ForInit, Literal,
    Pattern, Property, PropertyKey, Stmt, StmtKind, UnaryOp,
};

/// The locals a function body only ever puts numbers in.
#[derive(Default, Debug, Clone)]
pub struct Numeric {
    names: HashSet<Name>,
    /// The replaced object properties, before they have names.
    ///
    /// # Why these are pairs and not names
    ///
    /// A replaced property is a binding like any other and wants to be proved
    /// like any other — otherwise `«o.a»` is widened at its store and `t + o.a`
    /// is the generic operator with a runtime call under it, which measured as
    /// the whole of what scalar replacement was leaving on the table.
    ///
    /// But its name does not exist yet. `escape::field_name` mints it from
    /// `ctx.names`, and this pass has no `Ctx` — deliberately, since taking one
    /// would let a fixpoint that runs to convergence intern on every round.
    /// So the proof is carried on `(object, key)`, which the tree already
    /// spells, and [`Numeric::name_fields`] turns the survivors into names once
    /// at the end.
    fields: HashSet<(Name, Name)>,
}

impl Numeric {
    /// Whether a name is known to hold a number.
    pub fn holds_number(&self, name: Name) -> bool {
        self.names.contains(&name)
    }

    /// Whether a replaced property is known to hold a number.
    fn field_holds_number(&self, object: Name, property: Name) -> bool {
        self.fields.contains(&(object, property))
    }

    /// Mints a name for every proved property and adopts it as an ordinary one.
    ///
    /// After this the rest of the emitter cannot tell a replaced property from a
    /// local the program declared, which is the same reason `escape.rs` mints
    /// through `Names` rather than inventing a second kind of binding.
    pub fn name_fields(&mut self, mut name_of: impl FnMut(Name, Name) -> Name) {
        for (object, property) in std::mem::take(&mut self.fields) {
            let name = name_of(object, property);
            self.names.insert(name);
        }
    }

    /// How many were proved. For tests and for saying what a pass achieved.
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Everything still standing, whether or not it has a name yet. The
    /// fixpoint's measure of progress, which has to count both or a round that
    /// only dropped properties would look like convergence.
    fn total(&self) -> usize {
        self.names.len() + self.fields.len()
    }

    /// Whether nothing was proved.
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

/// Proves what can be proved about a function body's locals.
///
/// `flattened` is [`super::escape`]'s answer, so a local it replaced is proved
/// as its properties rather than as itself: `let o = {a: 1}` puts `(o, a)` in
/// and never `o`, because after replacement there is no binding called `o` for
/// a proof about it to describe.
pub(super) fn analyse(body: &[Stmt], flattened: &Flattened) -> Numeric {
    // Optimistic start: everything declared with an initialiser that could be
    // numeric. A declaration with no initialiser is `undefined`, which is not a
    // number, so it never enters.
    let mut candidates = Numeric::default();
    for statement in body {
        collect_candidates(statement, flattened, &mut candidates);
    }

    // Shrink until stable. Each round asks the same question with a smaller set,
    // so a local removed can remove the ones that depended on it.
    loop {
        let mut surviving = candidates.clone();
        for statement in body {
            keep_only_numeric(statement, flattened, &candidates, &mut surviving);
        }
        if surviving.total() == candidates.total() {
            return surviving;
        }
        candidates = surviving;
    }
}

/// Every local declared with an initialiser, as a candidate.
///
/// # Why `var` never becomes one
///
/// The rule this pass proves a local numeric BY is "its initialiser is
/// numeric and every assignment to it is numeric" — and a hoisted `var`'s
/// real initialiser, the one `function::hoist_vars` actually runs, is
/// `undefined`, at every reachable point before the line it is written on.
/// `var q = 5;` inside an `if` with no `else` is the plain case: the merge
/// this analysis never sees has an edge where `q` is still that `undefined`,
/// Tagged, and an edge where it is the `5` this pass saw and proved `F64` —
/// two representations meeting at one block parameter, which is exactly the
/// `ImplicitNarrowing` the verifier is right to refuse. A `try`, a `switch`
/// case or a loop body that assigns a `var` is the same join with a
/// different shape around it, not a different bug.
///
/// So a `var`'s explicit initialiser is not evidence this pass can use at
/// all — rule 5: what cannot be proven becomes generic, visibly, and a
/// `Tagged` store for a proven-looking value is what "generic" costs here.
/// `escape.rs` excludes `var` from its own optimisation for the same reason,
/// one door down (`kind.is_block_scoped()`).
fn collect_candidates(statement: &Stmt, flattened: &Flattened, into: &mut Numeric) {
    match &statement.kind {
        StmtKind::Declare { kind, bindings } => {
            if !kind.is_block_scoped() {
                return;
            }
            for binding in bindings {
                if let (Pattern::Name(name), Some(_)) = (&binding.target, &binding.value) {
                    match flattened.properties(*name) {
                        // Replaced, so what exists afterwards is one binding per
                        // key and the name itself is gone.
                        Some(keys) => {
                            for key in keys {
                                into.fields.insert((*name, *key));
                            }
                        }
                        None => {
                            into.names.insert(*name);
                        }
                    }
                }
            }
        }
        StmtKind::For { init, .. } => {
            // `for (var i = 0; …)` is the same hazard as the module doc's
            // `if`: `i` is `undefined` at function entry and only becomes
            // `0` at this line, so a read reachable WITHOUT passing through
            // here first would meet a `Tagged` edge here otherwise proved
            // `F64`. `for (let i = …)` has no such edge — the header owns
            // `i` and nothing before it could read one — which is what
            // `is_block_scoped` is checked for here too.
            let declares_var = matches!(
                init,
                Some(ForInit::Declare { kind, .. }) if !kind.is_block_scoped()
            );
            if let Some(ForInit::Declare { bindings, .. }) = init
                && !declares_var
            {
                for binding in bindings {
                    if let (Pattern::Name(name), Some(_)) = (&binding.target, &binding.value) {
                        // Never a replaced object: `escape::collect` only makes
                        // a candidate of a `StmtKind::Declare`, and a `for`'s
                        // initialiser is a `ForInit`.
                        into.names.insert(*name);
                    }
                }
            }
        }
        // A resource is never a candidate — see the module doc's reasoning
        // about `var`, applied one door over: what a `using` binds is not a
        // plain local this pass proves across every write, whatever the
        // initialiser looks like.
        StmtKind::Using { .. } => {}
        // The target is deliberately NOT a candidate, whichever of the three
        // spellings it is written with: what arrives is an element or a key,
        // never something this pass proved the shape of.
        StmtKind::ForEach { .. } => {}
        _ => {}
    }
    // Every remaining candidate-bearing shape is a nested STATEMENT — a
    // declaration cannot hide inside an expression — so only `StmtChild::
    // Stmt` and a `catch` clause's body matter here. Routed through
    // `capture::walk_stmt`, the one place this tree's shape is described
    // exhaustively, rather than a second match repeating it: that match is
    // what used to end `_ => {}`, which is how `StmtKind::Try` came to be
    // invisible to this whole pass — see `keep_only_numeric`'s doc for what
    // that cost.
    capture::walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => collect_candidates(inner, flattened, into),
        StmtChild::Catch(catch) => {
            for inner in &catch.body {
                collect_candidates(inner, flattened, into);
            }
        }
        StmtChild::Expr(_)
        | StmtChild::Binding(_)
        | StmtChild::Function(_)
        | StmtChild::Class(_) => {}
    });
}

/// Removes any candidate this statement puts something non-numeric into.
///
/// # The FIFTH time, and why this is no longer a hand-written traversal
///
/// This pass has now claimed a representation nothing produces four times in
/// one session — `%` (no remainder instruction), unary `-` (always a runtime
/// call), a hoisted `var` (a real `undefined` edge this pass never saw), and
/// `for (q of xs)` writing an EXISTING binding through a pattern this walk did
/// not know was a write. All four were the same shape: a way of writing a
/// name that a hand-maintained `match` had no arm for, ending in `_ => {}`.
///
/// A fifth was found by construction rather than by a crash: `StmtKind::Try`
/// had no arm at all, in this function OR in `collect_candidates`. Nothing
/// here ever looked inside a `try`, `catch` or `finally` body — so
/// `let x = 1; try { x = "s"; } catch {} return x + 1;` kept `x` proved `F64`
/// straight through an assignment this pass never visited, for the same
/// reason `for`-each did: an arm that exists for the shapes it was written
/// against and is silent about the rest.
///
/// The fix is not a sixth arm. `capture::walk_stmt` and `capture::walk_expr`
/// already describe this tree's shape exhaustively — no wildcard, everything
/// enumerated by name — because `capture.rs` was fixed for exactly this
/// failure mode before ([`StmtKind::Try`] missing from ITS OWN walk, once).
/// Recursion here is delegated to that one description instead of a second,
/// independently-maintained copy: a `StmtKind` or `ExprKind` variant added
/// anywhere now has exactly one match to update, and the compiler refuses to
/// build `capture.rs` until it is exhaustive there. What stays HERE, matched
/// by name, is only the part `capture`'s generic children cannot carry — a
/// declaration's `BindingKind`, which decides whether an initialiser is
/// evidence at all.
fn keep_only_numeric(
    statement: &Stmt,
    flattened: &Flattened,
    known: &Numeric,
    surviving: &mut Numeric,
) {
    match &statement.kind {
        StmtKind::Declare { bindings, .. } => {
            for binding in bindings {
                binding_written(binding, flattened, known, surviving);
            }
        }
        StmtKind::For {
            init: Some(ForInit::Declare { bindings, .. }),
            ..
        } => {
            for binding in bindings {
                if let Some(value) = &binding.value {
                    check_expr(value, flattened, known, surviving);
                }
                if let Pattern::Name(name) = &binding.target {
                    let numeric = binding
                        .value
                        .as_ref()
                        .is_some_and(|value| is_numeric(value, known));
                    if !numeric {
                        surviving.names.remove(name);
                    }
                }
            }
        }
        // A resource is Tagged, always — and the reason this strips rather
        // than merely skips: `Names` interns by TEXT, not by scope (see
        // `crate::names`), so a `using x` nested inside a block and an
        // outer, proved-numeric `x` are ONE `Name` as far as this whole pass
        // can tell. Not stripping here would let the outer proof survive
        // through a binding that shares its spelling by accident.
        StmtKind::Using { bindings, .. } => {
            for binding in bindings {
                let mut written = Vec::new();
                binding.target.bound_names(&mut written);
                for name in written {
                    surviving.names.remove(&name);
                }
                if let Some(value) = &binding.value {
                    check_expr(value, flattened, known, surviving);
                }
            }
        }
        // Whatever this loop writes per pass, it writes an ELEMENT or a KEY
        // — Tagged, because nothing here proves the shape of the subject —
        // so every name the target writes loses the proof, whichever of the
        // three spellings it is. `Dispose` was the fifth-and-a-half gap:
        // added the same day as `Declare`/`Assign` above, by the same
        // construction rather than by a program that hit it.
        StmtKind::ForEach { target, .. } => {
            let mut written = Vec::new();
            match target {
                ForEachTarget::Declare { target, .. } | ForEachTarget::Assign(target) => {
                    target.bound_names(&mut written);
                }
                ForEachTarget::Dispose { target, .. } => written.push(*target),
            }
            for name in written {
                surviving.names.remove(&name);
            }
        }
        // A `catch` binds a fresh name to whatever was thrown — never a
        // number this pass can trust — and the same shadow-by-text hazard
        // `using` has applies here too.
        StmtKind::Try { catch: Some(catch), .. } => {
            if let Some(binding) = &catch.binding {
                let mut written = Vec::new();
                binding.bound_names(&mut written);
                for name in written {
                    surviving.names.remove(&name);
                }
            }
        }
        _ => {}
    }
    capture::walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => keep_only_numeric(inner, flattened, known, surviving),
        StmtChild::Expr(inner) => check_expr(inner, flattened, known, surviving),
        // `Binding` is only ever produced for `Declare`, a `for`'s own
        // declaration, and `Using` — the three already matched above, with
        // the `BindingKind` context this generic child does not carry. A
        // second visit here would be idempotent, not wrong, but there is
        // nothing left for it to do.
        StmtChild::Binding(_) => {}
        StmtChild::Catch(catch) => {
            for inner in &catch.body {
                keep_only_numeric(inner, flattened, known, surviving);
            }
        }
        // A nested function or class body is its own scope, proved
        // separately when IT is emitted (`function.rs` calls `analyse`
        // again for it) — see the module doc on what this pass deliberately
        // does not try to prove.
        StmtChild::Function(_) | StmtChild::Class(_) => {}
    });
}

/// Removes a declared name's candidacy when its own initialiser is not
/// numeric. Shared between an ordinary `let`/`const`/`var` and — once this
/// crate proves a numeric `for`-header binding worth sharing too — anywhere
/// else a plain [`Binding`] decides a name's proof the same way.
fn binding_written(binding: &Binding, flattened: &Flattened, known: &Numeric, surviving: &mut Numeric) {
    // The initialiser is visited for its OWN sake before anything below asks
    // whether it looks numeric: `is_numeric` only ever ANSWERS a question, it
    // never strips a proof, so a nested assignment inside one —
    // `let a = [x = g()];`, `let s = \`${x = g()}\`;` — was invisible until
    // this call existed. Found the same way as the rest of this file's
    // holes: by asking whether every writing FORM reaches `check_expr`, not
    // by a program that hit it.
    if let Some(value) = &binding.value {
        check_expr(value, flattened, known, surviving);
    }
    let Pattern::Name(name) = &binding.target else {
        // A destructuring declaration's names are never candidates in the
        // first place — `collect_candidates` only adds `Pattern::Name` — so
        // there is nothing here to strip either.
        return;
    };
    // A replaced object is proved key by key against its literal's own
    // values, which is the same rule one level down: the initialiser of
    // `«o.a»` is what the literal wrote at `a`.
    if flattened.properties(*name).is_some()
        && let Some(Expr {
            kind: ExprKind::Object { properties },
            ..
        }) = &binding.value
    {
        for property in properties {
            if let Property::Value {
                key: PropertyKey::Named(key),
                value,
                ..
            } = property
                && !is_numeric(value, known)
            {
                surviving.fields.remove(&(*name, *key));
            }
        }
        return;
    }
    let numeric = binding
        .value
        .as_ref()
        .is_some_and(|value| is_numeric(value, known));
    if !numeric {
        surviving.names.remove(name);
    }
}

/// Removes any candidate an expression assigns something non-numeric to.
fn check_expr(
    expr: &Expr,
    flattened: &Flattened,
    known: &Numeric,
    surviving: &mut Numeric,
) {
    match &expr.kind {
        ExprKind::Assign { target, value, op } => {
            check_expr(value, flattened, known, surviving);
            if let AssignTarget::Place(place) = target
                && let ExprKind::Ident(name) = &place.kind
            {
                let numeric = match op {
                    AssignOp::Plain => is_numeric(value, known),
                    // `x += y` is numeric when the result is: which for `+`
                    // needs BOTH sides numeric, because `+` on anything else may
                    // concatenate.
                    AssignOp::Compound(binary) => {
                        known.holds_number(*name) && arithmetic(*binary) && is_numeric(value, known)
                    }
                    // Short-circuiting: `x ||= "a"` puts a string in `x`.
                    AssignOp::Logical(_) => false,
                };
                if !numeric {
                    surviving.names.remove(name);
                }
            }
            // A DESTRUCTURING assignment writes names too, and nothing here saw
            // it. What arrives through a pattern is an element or a property —
            // `Tagged`, always, since nothing proves the shape of what is being
            // taken apart — so every name it writes loses the proof.
            //
            // This is the fourth time this pass has claimed a representation
            // that nothing produces (after `%`, unary `-`, and a hoisted `var`),
            // and it surfaced the same way each time: the verifier refusing a
            // block parameter, here for `let q = 0; for (q of [1, 2]) {}`. The
            // for-of desugaring writes `q` through a pattern, so `q` stayed
            // proved F64 while carrying a Tagged element.
            if let AssignTarget::Pattern(pattern) = target {
                let mut written = Vec::new();
                pattern.bound_names(&mut written);
                for name in written {
                    surviving.names.remove(&name);
                }
            }
            // `o.k = v` on a replaced object is a store to that binding, and it
            // has to be able to take the proof away — otherwise `let o = {a: 1};
            // o.a = "x";` would leave `(o, a)` proved and the next `+` would be
            // `arith` on a string.
            if let AssignTarget::Place(place) = target
                && let ExprKind::Member {
                    object,
                    property,
                    optional: false,
                } = &place.kind
                && let ExprKind::Ident(object) = &object.kind
                && flattened.has(*object, *property)
            {
                let numeric = match op {
                    AssignOp::Plain => is_numeric(value, known),
                    AssignOp::Compound(binary) => {
                        known.field_holds_number(*object, *property)
                            && arithmetic(*binary)
                            && is_numeric(value, known)
                    }
                    AssignOp::Logical(_) => false,
                };
                if !numeric {
                    surviving.fields.remove(&(*object, *property));
                }
            }
        }
        // `x++` on a number yields a number, and on anything else yields NaN —
        // which is still a number. But the operand must already be one, or
        // `"a"++` would make `x` numeric out of nothing.
        ExprKind::Update { target, .. } => {
            if let ExprKind::Ident(name) = &target.kind
                && !known.holds_number(*name)
            {
                surviving.names.remove(name);
            }
        }
        _ => {}
    }
    // Everything else that can hide an assignment is a nested EXPRESSION —
    // a call argument, a `new` argument, a template substitution, an array
    // element, an object property's value, a computed key, the object or
    // index of `a[e]`, the operand of `await`/`yield`, either side of `?:`
    // — and each of those was invisible here before this delegated to
    // `capture::walk_expr`. `f(x = "s")`, `[y = "s"]`, `` `${z = "s"}` ``,
    // `o[k = "s"]` were all unreachable to a hand-written match that only
    // recursed into `Binary`/`Logical`/`Unary`/`Sequence`/`Conditional` — an
    // assignment nested inside any of the rest was never invalidated,
    // which is a wider version of the same bug class this file is named
    // after: a way of WRITING that a walk did not know was a write, one
    // level further out than the four that were found by a crash.
    capture::walk_expr(expr, &mut |child| match child {
        Child::Expr(inner) => check_expr(inner, flattened, known, surviving),
        // Its own scope, proved separately when it is emitted.
        Child::Function(_) | Child::Class(_) => {}
    });
}

/// Whether an operator's result is a number whatever its operands are, AND
/// whether the emitter keeps that number in its proven machine representation
/// rather than boxing it.
///
/// `+` is absent, and that absence is the whole subtlety: it is the one
/// arithmetic-looking operator that can produce a string. `%` is also absent,
/// for a narrower reason that this pass has to agree with `emit::expr` about:
/// `crates/rts-cranelift`'s `NumOp` has no remainder instruction (nothing
/// under `%`, `-`, `*`, `/` in `crates/rts-cranelift/src/lower/body.rs`), so
/// `proven_binary` in `emit/expr.rs` never proves a `%` result — it is always
/// a boxed runtime call, even when both operands are proven doubles. Claiming
/// here that a local reassigned through `%` "keeps its machine representation"
/// (see `stored` in `emit/expr.rs`) was a rule stated twice that disagreed:
/// this pass proved `i` numeric across `i = i % 7`, `stored` trusted that proof
/// and skipped widening, and the value it left unwidened was `Repr::Tagged` —
/// which a loop back edge then tried to pass into a header block parameter
/// typed `Repr::F64` from the entry edge, and
/// `rts_cranelift::ir::builder::BuildError::ImplicitNarrowing` is exactly the
/// verifier this crate promises will refuse it. `%` on numbers is still always
/// a number — that fact just cannot be spent as a proven representation until
/// something here also narrows a boxed remainder back to `F64` with a guard.
///
/// # `+` ENTRA, e o que faz isso ser seguro é o chamador
///
/// Ele estava fora, e a mesma pergunta era respondida noutro lugar deste arquivo
/// com ele DENTRO — duas respostas escritas no mesmo commit, discordando em
/// direções opostas. A estrita tirava a prova de `acc += i`, que é como um
/// acumulador se escreve; a inclusiva nunca lia o ALVO, então `s += 1` numa
/// string respondia "numérico", que não é código lento — é `arith` sobre uma
/// string.
///
/// `+` é seguro aqui porque TODO chamador estabelece os dois lados antes de
/// perguntar: as duas formas de atribuição composta pedem `holds_number` do alvo
/// e `is_numeric` do valor, e a forma binária pede `is_numeric` dos dois
/// operandos. `+` só concatena quando um dos lados não é número, e nenhum
/// chamador chega aqui sem ter recusado esse caso.
///
/// `%` continua fora pela razão acima: não há instrução de resto, então uma
/// prova de `%` reivindica uma representação de máquina para um valor que chega
/// encaixotado.
fn arithmetic(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Add
    )
}

/// Se o alvo de uma atribuição já guarda um número.
///
/// Só um nome simples responde `true`: é o único caso sobre o qual este passe
/// mantém evidência. Um campo, um índice ou uma desestruturação respondem
/// `false` — não porque não possam guardar um número, mas porque afirmar que
/// guardam seria a reivindicação sem evidência que este arquivo recusa em toda
/// parte.
fn target_holds_number(target: &AssignTarget, known: &Numeric) -> bool {
    match target {
        AssignTarget::Place(place) => match &place.kind {
            ExprKind::Ident(name) => known.holds_number(*name),
            _ => false,
        },
        _ => false,
    }
}

/// Whether an expression certainly produces a number.
fn is_numeric(expr: &Expr, known: &Numeric) -> bool {
    match &expr.kind {
        ExprKind::Literal(Literal::Number(_)) => true,
        ExprKind::Ident(name) => known.holds_number(*name),

        // A read of a replaced property. This is the ONE property read this
        // pass answers about, and it is not an exception to "a property is a
        // claim with no evidence" below — a replaced property is not a property.
        // `escape.rs` proved the object never leaves the function and that every
        // access names a key of its own literal, so `o.a` is a binding whose
        // every store this pass has seen, exactly like a local.
        ExprKind::Member {
            object,
            property,
            optional: false,
        } => match &object.kind {
            ExprKind::Ident(name) => known.field_holds_number(*name, *property),
            _ => false,
        },

        ExprKind::Binary { op, left, right } => match op {
            // `+` needs both sides proved, because two strings concatenate and
            // a string with anything concatenates too.
            BinaryOp::Add => is_numeric(left, known) && is_numeric(right, known),
            // The rest convert whatever they are given, so their result is a
            // number regardless — but the operands are still required to be
            // proved, because an unproved one might be an object whose
            // `valueOf` runs user code, and this pass may not decide that a
            // call happens.
            _ if arithmetic(*op) => is_numeric(left, known) && is_numeric(right, known),
            // A comparison is a boolean, and `in`/`instanceof` are booleans.
            // Bitwise operators and shifts produce numbers and are not emitted
            // yet, so claiming them would be claiming something untested.
            _ => false,
        },

        ExprKind::Unary { op, operand } => match op {
            // `+x` is `x * 1`, through `emit_binary`'s `Mul` — which IS in
            // `proven_binary`, so a proven `F64` operand keeps its
            // representation. `-x` is not: `emit_unary`'s `UnaryOp::Negate`
            // always calls `RuntimeOp::Negate` and always returns
            // `Repr::Tagged`, deliberately, because `x * -1` is wrong for a
            // bigint. There is no proven fast path here for it to fall back
            // on, unlike `%` above — so claiming this arm proved a name
            // reassigned through `-x` is the same false claim `arithmetic`
            // made about `%`, and `sign = -sign` in a loop hit it exactly the
            // same way: `stored` trusted the proof, skipped widening a
            // `Repr::Tagged` value, and the back edge failed
            // `ImplicitNarrowing` against the header's `Repr::F64` parameter.
            UnaryOp::Plus => is_numeric(operand, known),
            _ => false,
        },

        // `(a, b)` is `b`.
        ExprKind::Sequence { operands } => {
            operands.last().is_some_and(|last| is_numeric(last, known))
        }

        // Both arms, or it is not known which arrives.
        ExprKind::Conditional {
            then_branch,
            else_branch,
            ..
        } => is_numeric(then_branch, known) && is_numeric(else_branch, known),

        // An assignment's value is what was assigned.
        //
        // O ALVO É LIDO AQUI, e não era. Esta linha aceitava `+` sem perguntar o
        // que o alvo já guarda, então `s += 1` sobre uma string respondia
        // "numérico" — e a versão a 80 linhas daqui, que LÊ o alvo, recusava
        // `+`. As duas erravam, em direções opostas, e agora são um predicado só.
        ExprKind::Assign { target, value, op } => match op {
            AssignOp::Plain => is_numeric(value, known),
            AssignOp::Compound(binary) => {
                arithmetic(*binary)
                    && target_holds_number(target, known)
                    && is_numeric(value, known)
            }
            AssignOp::Logical(_) => false,
        },

        // Everything else is a claim with no evidence: a parameter, a call, a
        // property, a literal of another kind. Answering "yes" here is not slow
        // code, it is `arith` on a string.
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::names::Names;
    use crate::parse::parse_script;
    use crate::syntax::{FunctionBody, ModuleItem, StmtKind};

    /// The names proved numeric in a function body, as strings.
    fn proved(source: &str) -> Vec<String> {
        let mut names = Names::default();
        let program =
            parse_script(&format!("function t() {{ {source} }}"), &mut names).expect("parses");
        let [ModuleItem::Stmt(statement)] = program.body.as_slice() else {
            panic!("one statement");
        };
        let StmtKind::Function(function) = &statement.kind else {
            panic!("a function");
        };
        let FunctionBody::Block(body) = &function.body else {
            panic!("a block");
        };
        // Composed the way `function.rs` composes it, rather than against an
        // empty `Flattened`: what is being tested includes what this pass says
        // about a replaced property, and an empty one would answer "nothing was
        // replaced" for every source and pass by not looking.
        let captured = super::super::capture::captured(body, &[]);
        let flattened = super::super::escape::analyse(body, &[], &captured);
        let mut numeric = analyse(body, &flattened);
        numeric.name_fields(|object, property| {
            let text = format!(
                "{}{}.{}",
                super::super::escape::MARKER,
                names.text(object),
                names.text(property)
            );
            names.intern(&text)
        });
        // Every interned name, asked whether it survived. `Names` has no
        // iterator, and adding one for a test would be adding surface to the
        // crate for the benefit of this file.
        // Re-interning is how a test turns a name back into text without
        // `Names` growing an iterator for its benefit: interning is idempotent,
        // so asking for a spelling hands back the name it already had.
        let mut found: Vec<String> = ["a", "b", "i", "s", "x", "y"]
            .into_iter()
            .filter(|text| {
                let name = names.intern(text);
                numeric.holds_number(name)
            })
            .map(str::to_owned)
            .collect();
        found.sort();
        found
    }

    #[test]
    fn a_literal_initialiser_proves_a_local() {
        assert_eq!(proved("let x = 1;"), ["x"]);
    }

    #[test]
    fn a_local_that_is_later_given_something_else_is_not_proved() {
        // The reason this is an analysis and not a declaration: nothing about
        // the first line is wrong, and the second is what decides.
        assert!(proved("let x = 1; x = f();").is_empty());
    }

    #[test]
    fn the_fixpoint_reaches_a_loop_counter() {
        // `i` is numeric only if `i + 1` is, and `i + 1` is only if `i` is. The
        // optimistic start is what makes this reachable; starting from nothing
        // and adding never proves either.
        assert_eq!(proved("let i = 0; while (i) { i = i + 1; }"), ["i"]);
    }

    #[test]
    fn one_local_losing_its_proof_takes_the_ones_that_depended_on_it() {
        // The reason a single pass is not enough: `b` looks numeric until `a`
        // stops being, and only a second round sees it.
        assert!(proved("let a = 1; let b = a; a = f();").is_empty());
    }

    #[test]
    fn plus_needs_both_sides_because_it_might_concatenate() {
        // The one arithmetic-looking operator with two answers. `x` here is a
        // string, and proving it numeric would emit an add on one.
        assert!(!proved("let s = g(); let x = 1 + s;").contains(&"x".to_owned()));
    }

    #[test]
    fn subtraction_still_needs_both_sides_proved() {
        // `-` always produces a number, so it is tempting to prove `x` without
        // looking at `s`. It is wrong for a different reason: an unproved
        // operand might be an object whose `valueOf` runs user code, and this
        // pass may not decide that a call happens.
        assert!(!proved("let s = g(); let x = 1 - s;").contains(&"x".to_owned()));
    }

    #[test]
    fn a_comparison_is_a_boolean_and_not_a_number() {
        assert_eq!(proved("let a = 1; let b = a < 2;"), ["a"]);
    }

    #[test]
    fn a_declaration_with_no_initialiser_is_undefined_and_not_a_number() {
        assert!(proved("let x;").is_empty());
    }

    #[test]
    fn a_parameter_is_not_proved_because_nothing_here_knows_what_a_caller_passes() {
        // Not a limitation to fix later by guessing. A caller can pass anything,
        // and the evidence for what it passes is not in this function.
        assert!(proved("x = 1;").is_empty());
    }

    // The bug class: a way of writing a name that the walk did not recognise
    // as a write. Each test below pins one writing-form the earlier,
    // hand-maintained `match` in `keep_only_numeric`/`check_expr` had no arm
    // for, and each would have stayed "proved" before this file started
    // delegating recursion to `capture::walk_stmt`/`walk_expr`.

    #[test]
    fn an_assignment_inside_a_try_body_is_not_invisible_to_the_pass() {
        // `StmtKind::Try` had no arm at all in `keep_only_numeric`, so this
        // assignment was never visited and `x` stayed "proved" straight
        // through it.
        assert!(proved("let x = 1; try { x = f(); } catch (e) {}").is_empty());
    }

    #[test]
    fn an_assignment_inside_a_catch_body_is_not_invisible_to_the_pass() {
        assert!(proved("let x = 1; try {} catch (e) { x = f(); }").is_empty());
    }

    #[test]
    fn an_assignment_inside_a_finally_body_is_not_invisible_to_the_pass() {
        assert!(proved("let x = 1; try {} finally { x = f(); }").is_empty());
    }

    #[test]
    fn a_catch_binding_does_not_keep_an_outer_proof_alive_under_the_same_spelling() {
        // `Names` interns by TEXT, not by scope, so a `catch (x)` and an
        // outer numeric `x` are one `Name` to this whole pass. Written this
        // way because the emitter does not accept a caught value being used
        // as a number, so the hazard is the SHADOW, not the catch value's
        // own (non-)numeric-ness.
        assert!(proved("let x = 1; try { f(); } catch (x) {}").is_empty());
    }

    #[test]
    fn a_name_assigned_only_inside_a_call_argument_is_not_invisible_to_the_pass() {
        // Before this pass delegated to `capture::walk_expr`, `check_expr`
        // only recursed into `Binary`/`Logical`/`Unary`/`Sequence`/
        // `Conditional` — an assignment nested inside a call's arguments,
        // `new`'s arguments, an array element, an object property, or a
        // template substitution was invisible to it.
        assert!(proved("let x = 1; f(x = g());").is_empty());
    }

    #[test]
    fn a_name_assigned_only_inside_an_array_literal_is_not_invisible_to_the_pass() {
        assert!(proved("let x = 1; let a = [x = g()];").is_empty());
    }

    #[test]
    fn a_name_assigned_only_inside_a_template_substitution_is_not_invisible_to_the_pass() {
        assert!(proved("let x = 1; let s = `${x = g()}`;").is_empty());
    }

    #[test]
    fn a_name_assigned_only_inside_a_computed_member_index_is_not_invisible_to_the_pass() {
        assert!(proved("let x = 1; a[x = g()] = 1;").is_empty());
    }

    #[test]
    fn a_for_each_dispose_target_does_not_keep_an_outer_proof_alive_under_the_same_spelling() {
        // `for (using x of xs)` — the fifth-and-a-half gap: `Declare` and
        // `Assign` were handled, `Dispose` was not, and all three share the
        // same hazard the catch-binding test above pins.
        assert!(proved("let x = 1; for (using x of y) {}").is_empty());
    }

    #[test]
    fn a_using_binding_does_not_keep_an_outer_proof_alive_under_the_same_spelling() {
        assert!(proved("let x = 1; { using x = f(); }").is_empty());
    }

    #[test]
    fn an_ordinary_numeric_loop_stays_proved_after_the_rewrite() {
        // The regression this whole change must not cause: a plain counter
        // loop, with none of the constructs above anywhere near it, still
        // gets its instruction rather than a runtime call.
        assert_eq!(
            proved("let i = 0; while (i < 10) { i = i + 1; }"),
            ["i"]
        );
    }
}
