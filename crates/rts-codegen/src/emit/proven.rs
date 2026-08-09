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

use super::escape::Flattened;
use crate::names::Name;
use crate::syntax::{
    AssignOp, AssignTarget, BinaryOp, Expr, ExprKind, ForInit, Literal, Pattern, Property,
    PropertyKey, Stmt, StmtKind,
    UnaryOp,
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
fn collect_candidates(statement: &Stmt, flattened: &Flattened, into: &mut Numeric) {
    match &statement.kind {
        StmtKind::Declare { bindings, .. } => {
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
        StmtKind::Block(body) => body
            .iter()
            .for_each(|inner| collect_candidates(inner, flattened, into)),
        StmtKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            collect_candidates(then_branch, flattened, into);
            if let Some(otherwise) = else_branch {
                collect_candidates(otherwise, flattened, into);
            }
        }
        StmtKind::While { body, .. } | StmtKind::DoWhile { body, .. } => {
            collect_candidates(body, flattened, into)
        }
        StmtKind::For { init, body, .. } => {
            if let Some(ForInit::Declare { bindings, .. }) = init {
                for binding in bindings {
                    if let (Pattern::Name(name), Some(_)) = (&binding.target, &binding.value) {
                        // Never a replaced object: `escape::collect` only makes
                        // a candidate of a `StmtKind::Declare`, and a `for`'s
                        // initialiser is a `ForInit`.
                        into.names.insert(*name);
                    }
                }
            }
            collect_candidates(body, flattened, into);
        }
        StmtKind::Labelled { body, .. } => collect_candidates(body, flattened, into),
        StmtKind::Switch { clauses, .. } => {
            for clause in clauses {
                for inner in &clause.body {
                    collect_candidates(inner, flattened, into);
                }
            }
        }
        StmtKind::ForEach { body, .. } => {
            // The target is deliberately NOT a candidate: what it holds is a
            // key, and `for-in` yields strings even for array indices. Adding
            // it would claim a representation the loop never produces.
            collect_candidates(body, flattened, into);
        }
        _ => {}
    }
}

/// Removes any candidate this statement puts something non-numeric into.
fn keep_only_numeric(
    statement: &Stmt,
    flattened: &Flattened,
    known: &Numeric,
    surviving: &mut Numeric,
) {
    match &statement.kind {
        StmtKind::Declare { bindings, .. } => {
            for binding in bindings {
                let Pattern::Name(name) = &binding.target else {
                    continue;
                };
                // A replaced object is proved key by key against its literal's
                // own values, which is the same rule one level down: the
                // initialiser of `«o.a»` is what the literal wrote at `a`.
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
                    continue;
                }
                let numeric = binding
                    .value
                    .as_ref()
                    .is_some_and(|value| is_numeric(value, known));
                if !numeric {
                    surviving.names.remove(name);
                }
            }
        }
        StmtKind::Expr(expr) | StmtKind::Return(Some(expr)) | StmtKind::Throw(expr) => {
            check_expr(expr, flattened, known, surviving)
        }
        StmtKind::Block(body) => body
            .iter()
            .for_each(|inner| keep_only_numeric(inner, flattened, known, surviving)),
        StmtKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            check_expr(condition, flattened, known, surviving);
            keep_only_numeric(then_branch, flattened, known, surviving);
            if let Some(otherwise) = else_branch {
                keep_only_numeric(otherwise, flattened, known, surviving);
            }
        }
        StmtKind::While { condition, body } => {
            check_expr(condition, flattened, known, surviving);
            keep_only_numeric(body, flattened, known, surviving);
        }
        StmtKind::DoWhile { body, condition } => {
            keep_only_numeric(body, flattened, known, surviving);
            check_expr(condition, flattened, known, surviving);
        }
        StmtKind::For {
            init,
            test,
            update,
            body,
        } => {
            match init {
                Some(ForInit::Declare { bindings, .. }) => {
                    for binding in bindings {
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
                Some(ForInit::Expr(expr)) => check_expr(expr, flattened, known, surviving),
                None => {}
            }
            if let Some(test) = test {
                check_expr(test, flattened, known, surviving);
            }
            if let Some(update) = update {
                check_expr(update, flattened, known, surviving);
            }
            keep_only_numeric(body, flattened, known, surviving);
        }
        StmtKind::Labelled { body, .. } => keep_only_numeric(body, flattened, known, surviving),
        StmtKind::Switch {
            discriminant,
            clauses,
        } => {
            check_expr(discriminant, flattened, known, surviving);
            for clause in clauses {
                if let Some(test) = &clause.test {
                    check_expr(test, flattened, known, surviving);
                }
                for inner in &clause.body {
                    keep_only_numeric(inner, flattened, known, surviving);
                }
            }
        }
        StmtKind::ForEach {
            target,
            subject,
            body,
            ..
        } => {
            check_expr(subject, flattened, known, surviving);
            // A `for-in` target holds a STRING, so a name it writes to is not
            // numeric however it was declared.
            if let crate::syntax::ForEachTarget::Declare {
                target: Pattern::Name(name),
                ..
            } = target
            {
                surviving.names.remove(name);
            }
            keep_only_numeric(body, flattened, known, surviving);
        }
        _ => {}
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
        ExprKind::Binary { left, right, .. } | ExprKind::Logical { left, right, .. } => {
            check_expr(left, flattened, known, surviving);
            check_expr(right, flattened, known, surviving);
        }
        ExprKind::Unary { operand, .. } => check_expr(operand, flattened, known, surviving),
        ExprKind::Sequence { operands } => operands
            .iter()
            .for_each(|one| check_expr(one, flattened, known, surviving)),
        ExprKind::Conditional {
            condition,
            then_branch,
            else_branch,
        } => {
            check_expr(condition, flattened, known, surviving);
            check_expr(then_branch, flattened, known, surviving);
            check_expr(else_branch, flattened, known, surviving);
        }
        _ => {}
    }
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
fn arithmetic(op: BinaryOp) -> bool {
    matches!(op, BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div)
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
        ExprKind::Assign { value, op, .. } => match op {
            AssignOp::Plain => is_numeric(value, known),
            AssignOp::Compound(binary) => {
                (arithmetic(*binary) || *binary == BinaryOp::Add) && is_numeric(value, known)
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
}
