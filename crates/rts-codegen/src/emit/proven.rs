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

use std::collections::HashSet;

use crate::names::Name;
use crate::syntax::{
    AssignOp, AssignTarget, BinaryOp, Expr, ExprKind, ForInit, Literal, Pattern, Stmt, StmtKind,
    UnaryOp,
};

/// The locals a function body only ever puts numbers in.
#[derive(Default, Debug)]
pub struct Numeric {
    names: HashSet<Name>,
}

impl Numeric {
    /// Whether a name is known to hold a number.
    pub fn holds_number(&self, name: Name) -> bool {
        self.names.contains(&name)
    }

    /// How many were proved. For tests and for saying what a pass achieved.
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Whether nothing was proved.
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

/// Proves what can be proved about a function body's locals.
pub fn analyse(body: &[Stmt]) -> Numeric {
    // Optimistic start: everything declared with an initialiser that could be
    // numeric. A declaration with no initialiser is `undefined`, which is not a
    // number, so it never enters.
    let mut candidates = HashSet::new();
    for statement in body {
        collect_candidates(statement, &mut candidates);
    }

    // Shrink until stable. Each round asks the same question with a smaller set,
    // so a local removed can remove the ones that depended on it.
    loop {
        let mut surviving = candidates.clone();
        let known = Numeric {
            names: candidates.clone(),
        };
        for statement in body {
            keep_only_numeric(statement, &known, &mut surviving);
        }
        if surviving.len() == candidates.len() {
            return Numeric { names: surviving };
        }
        candidates = surviving;
    }
}

/// Every local declared with an initialiser, as a candidate.
fn collect_candidates(statement: &Stmt, into: &mut HashSet<Name>) {
    match &statement.kind {
        StmtKind::Declare { bindings, .. } => {
            for binding in bindings {
                if let (Pattern::Name(name), Some(_)) = (&binding.target, &binding.value) {
                    into.insert(*name);
                }
            }
        }
        StmtKind::Block(body) => body.iter().for_each(|inner| collect_candidates(inner, into)),
        StmtKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            collect_candidates(then_branch, into);
            if let Some(otherwise) = else_branch {
                collect_candidates(otherwise, into);
            }
        }
        StmtKind::While { body, .. } | StmtKind::DoWhile { body, .. } => {
            collect_candidates(body, into)
        }
        StmtKind::For { init, body, .. } => {
            if let Some(ForInit::Declare { bindings, .. }) = init {
                for binding in bindings {
                    if let (Pattern::Name(name), Some(_)) = (&binding.target, &binding.value) {
                        into.insert(*name);
                    }
                }
            }
            collect_candidates(body, into);
        }
        StmtKind::Labelled { body, .. } => collect_candidates(body, into),
        _ => {}
    }
}

/// Removes any candidate this statement puts something non-numeric into.
fn keep_only_numeric(statement: &Stmt, known: &Numeric, surviving: &mut HashSet<Name>) {
    match &statement.kind {
        StmtKind::Declare { bindings, .. } => {
            for binding in bindings {
                if let Pattern::Name(name) = &binding.target {
                    let numeric = binding
                        .value
                        .as_ref()
                        .is_some_and(|value| is_numeric(value, known));
                    if !numeric {
                        surviving.remove(name);
                    }
                }
            }
        }
        StmtKind::Expr(expr) | StmtKind::Return(Some(expr)) | StmtKind::Throw(expr) => {
            check_expr(expr, known, surviving)
        }
        StmtKind::Block(body) => body
            .iter()
            .for_each(|inner| keep_only_numeric(inner, known, surviving)),
        StmtKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            check_expr(condition, known, surviving);
            keep_only_numeric(then_branch, known, surviving);
            if let Some(otherwise) = else_branch {
                keep_only_numeric(otherwise, known, surviving);
            }
        }
        StmtKind::While { condition, body } => {
            check_expr(condition, known, surviving);
            keep_only_numeric(body, known, surviving);
        }
        StmtKind::DoWhile { body, condition } => {
            keep_only_numeric(body, known, surviving);
            check_expr(condition, known, surviving);
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
                                surviving.remove(name);
                            }
                        }
                    }
                }
                Some(ForInit::Expr(expr)) => check_expr(expr, known, surviving),
                None => {}
            }
            if let Some(test) = test {
                check_expr(test, known, surviving);
            }
            if let Some(update) = update {
                check_expr(update, known, surviving);
            }
            keep_only_numeric(body, known, surviving);
        }
        StmtKind::Labelled { body, .. } => keep_only_numeric(body, known, surviving),
        _ => {}
    }
}

/// Removes any candidate an expression assigns something non-numeric to.
fn check_expr(expr: &Expr, known: &Numeric, surviving: &mut HashSet<Name>) {
    match &expr.kind {
        ExprKind::Assign { target, value, op } => {
            check_expr(value, known, surviving);
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
                    surviving.remove(name);
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
                surviving.remove(name);
            }
        }
        ExprKind::Binary { left, right, .. } | ExprKind::Logical { left, right, .. } => {
            check_expr(left, known, surviving);
            check_expr(right, known, surviving);
        }
        ExprKind::Unary { operand, .. } => check_expr(operand, known, surviving),
        ExprKind::Sequence { operands } => operands
            .iter()
            .for_each(|one| check_expr(one, known, surviving)),
        ExprKind::Conditional {
            condition,
            then_branch,
            else_branch,
        } => {
            check_expr(condition, known, surviving);
            check_expr(then_branch, known, surviving);
            check_expr(else_branch, known, surviving);
        }
        _ => {}
    }
}

/// Whether an operator's result is a number whatever its operands are.
///
/// `+` is absent, and that absence is the whole subtlety: it is the one
/// arithmetic-looking operator that can produce a string. Every other one
/// converts both operands to numbers and has no second answer.
fn arithmetic(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem
    )
}

/// Whether an expression certainly produces a number.
fn is_numeric(expr: &Expr, known: &Numeric) -> bool {
    match &expr.kind {
        ExprKind::Literal(Literal::Number(_)) => true,
        ExprKind::Ident(name) => known.holds_number(*name),

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
            UnaryOp::Negate | UnaryOp::Plus => is_numeric(operand, known),
            _ => false,
        },

        // `(a, b)` is `b`.
        ExprKind::Sequence { operands } => operands
            .last()
            .is_some_and(|last| is_numeric(last, known)),

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
        let program = parse_script(&format!("function t() {{ {source} }}"), &mut names)
            .expect("parses");
        let [ModuleItem::Stmt(statement)] = program.body.as_slice() else {
            panic!("one statement");
        };
        let StmtKind::Function(function) = &statement.kind else {
            panic!("a function");
        };
        let FunctionBody::Block(body) = &function.body else {
            panic!("a block");
        };
        let numeric = analyse(body);
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
