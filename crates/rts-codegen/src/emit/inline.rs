//! Calling a small function without issuing a call.
//!
//! # Why this is a proof and not a heuristic
//!
//! `f(a)` costs 20.7 ns in `bench/analytic.ts` when `f` is
//! `function f(x) { return x + 1 }` — a call convention, an argument pad, a
//! throw check and a return, to compute one addition. Every engine that matters
//! removes that call; the ones with a deoptimiser do it by GUESSING which
//! function `f` will be and unwinding when the guess was wrong.
//!
//! This engine has no deoptimiser, so the same rule as `primordial` applies:
//! the answer must be a fact about the whole program before anything runs, and
//! the whole program is available because all of it is compiled first. What is
//! asked here is exactly what `primordial` asks about `Math`, plus one more
//! question — whether any OTHER declaration anywhere spells the same name, since
//! a shadow would make the call site refer to something else entirely.
//!
//! # The slice that is taken, and why it is this small
//!
//! One expression, no `this`, no captures, no recursion, no assignment, and
//! every identifier in the body is one of the function's own parameters. The
//! last condition is what makes the substitution legal without renaming
//! anything: the body is emitted in the CALLER's scope, so a body naming
//! anything else would resolve that name against the caller's bindings, which is
//! a wrong answer rather than a missed optimisation.
//!
//! Recorded as deliberately conservative rather than as the finished shape. The
//! plan entry this implements notes that the wider form — a body that calls
//! other proven functions, several statements, a `let` — was refused once for
//! being unprovable and that the refusal was cautious to a fault. It is still
//! refused HERE, and the reason is written above: each of those needs a renaming
//! pass this does not have.

use std::collections::BTreeMap;
use std::rc::Rc;

use rts_cranelift::ir::{FuncBuilder, ValueId};

use crate::Name;
use crate::syntax::{
    Binding, BindingKind, Class, ClassElement, Expr, ExprKind, ForEachTarget, Function,
    FunctionBody, Pattern, Spreadable, Stmt, StmtKind,
};

use super::capture::{Child, StmtChild, walk_expr, walk_stmt};
use super::{Ctx, EmitResult, Scope};

/// A function whose call may be replaced by its body.
pub(super) struct Inlinable {
    /// Its parameters, in order. Every one is a plain name with no default.
    pub parameters: Vec<Name>,
    /// The single expression it answers.
    pub body: Expr,
}

/// Every function in `body` a call site may substitute, by the name it is
/// called under.
///
/// `eval` and `global_this` end the question exactly as they do in
/// [`super::primordial`], and for the same reason: an indirect write is the
/// case an analysis gets wrong.
pub(super) fn candidates(
    body: &[Stmt],
    eval: Name,
    global_this: Name,
) -> BTreeMap<Name, Rc<Inlinable>> {
    let mut found = BTreeMap::new();
    for statement in body {
        let Some((name, function)) = declared_function(statement) else {
            continue;
        };
        let Some(candidate) = shape_of(function) else {
            continue;
        };
        // The three whole-program questions, asked only for a name that got
        // this far — each walks the entire tree, and asking them first would
        // walk it once per top-level statement.
        if declarations_of(body, name) != 1 {
            continue;
        }
        if !super::primordial::untouched(body, name, eval, global_this) {
            continue;
        }
        found.insert(name, Rc::new(candidate));
    }
    found
}

/// Emits a call as the callee's body, when this is one of those calls.
///
/// `None` is "not one of those", and the ordinary call follows — so every
/// refusal below costs a comparison while compiling and nothing at run time.
///
/// # What is checked HERE rather than while collecting
///
/// Two facts that belong to the call site and not to the callee. The arity must
/// match exactly: a call passing fewer arguments than the function declares
/// would need the missing parameters bound to `undefined`, which is correct and
/// is simply not written yet, and one passing more would have to evaluate the
/// extra arguments for their side effects with nothing to bind them to.
///
/// And no parameter may spell a name the CALLER's escape analysis flattened.
/// The body is emitted in the caller's scope, so `p.x` in the callee would be
/// read as the caller's replaced field if the caller happened to have an object
/// named `p` — the one place where "every identifier is a parameter" is not
/// enough, because the flattened form is keyed by name rather than by binding.
pub(super) fn emit_substituted(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    callee: &Expr,
    arguments: &[Spreadable],
) -> EmitResult<Option<ValueId>> {
    let ExprKind::Ident(name) = &callee.kind else {
        return Ok(None);
    };
    let Some(candidate) = ctx.inlinable(*name) else {
        return Ok(None);
    };
    if arguments.len() != candidate.parameters.len() {
        return Ok(None);
    }
    if candidate
        .parameters
        .iter()
        .any(|parameter| ctx.flattens(*parameter))
    {
        return Ok(None);
    }

    // The arguments in source order, before anything is bound: an argument
    // expression is evaluated where it was written, and a parameter bound
    // early would be visible to the argument after it.
    let mut values = Vec::with_capacity(arguments.len());
    for argument in arguments {
        let Spreadable::Single(value) = argument else {
            return Ok(None);
        };
        values.push(super::expr::emit_expr(builder, scope, ctx, value)?);
    }

    scope.enter();
    for (parameter, value) in candidate.parameters.iter().zip(values) {
        scope.declare(*parameter, value);
    }
    let answered = super::expr::emit_expr(builder, scope, ctx, &candidate.body);
    scope.leave();
    Ok(Some(answered?))
}

/// The function a top-level statement declares under a name, in either
/// spelling.
///
/// `const f = x => …` is included because the benchmark's own arrow row is one,
/// and because a `const` binding is the stronger of the two: the language
/// refuses to reassign it, where a `function` declaration's name is an ordinary
/// mutable binding that `untouched` has to prove nothing writes.
fn declared_function(statement: &Stmt) -> Option<(Name, &Function)> {
    match &statement.kind {
        StmtKind::Function(function) => Some((function.name?, function)),
        StmtKind::Declare {
            kind: BindingKind::Const,
            bindings,
        } => {
            let [Binding {
                target: Pattern::Name(name),
                value: Some(value),
                ..
            }] = &bindings[..]
            else {
                return None;
            };
            match &value.kind {
                ExprKind::Function(function) => Some((*name, function)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// What a function has to be for its body to stand in for a call to it.
fn shape_of(function: &Function) -> Option<Inlinable> {
    if function.is_async || function.is_generator || !function.has_simple_parameter_list() {
        return None;
    }
    let mut parameters = Vec::with_capacity(function.parameters.len());
    for parameter in &function.parameters {
        match &parameter.target {
            Pattern::Name(name) => parameters.push(*name),
            _ => return None,
        }
    }
    // A body is one expression, however it was spelled: `x => x + 1` and
    // `function (x) { return x + 1 }` are the same function and a rule that
    // took only one of them would be a rule about syntax.
    let answered = match &function.body {
        FunctionBody::Expression(expr) => expr,
        FunctionBody::Block(statements) => match &statements[..] {
            [Stmt {
                kind: StmtKind::Return(Some(expr)),
                ..
            }] => expr,
            _ => return None,
        },
    };
    if !closed_over_parameters(answered, &parameters) {
        return None;
    }
    Some(Inlinable {
        parameters,
        body: answered.clone(),
    })
}

/// Whether every identifier the body reads is one of its own parameters, and
/// nothing in it needs a binding, a receiver or a frame of its own.
///
/// An allowlist rather than a list of refusals: a node added to the tree
/// tomorrow is refused by default, which is the direction a wrong answer here
/// cannot come from.
fn closed_over_parameters(expr: &Expr, parameters: &[Name]) -> bool {
    match &expr.kind {
        ExprKind::Ident(name) => return parameters.contains(name),
        ExprKind::Literal(_) => return true,
        // Everything with a body, a receiver, a suspension point or a write.
        // A nested function would capture the caller's scope rather than the
        // callee's; `this` has no answer at a call site that passes none; an
        // assignment would write a binding this substitution did not make.
        ExprKind::Function(_)
        | ExprKind::Class(_)
        | ExprKind::This
        | ExprKind::Await(_)
        | ExprKind::Yield { .. }
        | ExprKind::Assign { .. }
        | ExprKind::Update { .. }
        | ExprKind::SuperMember { .. }
        | ExprKind::SuperCall { .. }
        | ExprKind::PrivateName(_)
        | ExprKind::NewTarget
        | ExprKind::ImportMeta
        | ExprKind::ImportCall { .. } => return false,
        _ => {}
    }
    let mut ok = true;
    walk_expr(expr, &mut |child| match child {
        Child::Expr(inner) => ok = ok && closed_over_parameters(inner, parameters),
        Child::Function(_) | Child::Class(_) => ok = false,
    });
    ok
}

/// How many declarations anywhere in the program spell `name`.
///
/// Counted rather than answered as a boolean because the candidate's own
/// declaration is one of them: a name declared exactly once is a name a call
/// site cannot be reading a shadow of.
///
/// Deliberately over-counts. A parameter, a `catch` binding and a loop target
/// all count, and none of them could shadow a top-level function at the call
/// sites that matter — but over-counting refuses a candidate, and under-counting
/// substitutes the wrong function.
fn declarations_of(body: &[Stmt], name: Name) -> usize {
    let mut count = 0;
    for statement in body {
        count_in_statement(statement, name, &mut count);
    }
    count
}

fn count_in_statement(statement: &Stmt, name: Name, count: &mut usize) {
    match &statement.kind {
        StmtKind::Function(function) => {
            if function.name == Some(name) {
                *count += 1;
            }
            count_in_function(function, name, count);
            return;
        }
        StmtKind::Class(class) => {
            if class.name == Some(name) {
                *count += 1;
            }
            count_in_class(class, name, count);
            return;
        }
        StmtKind::ForEach { target, .. } => match target {
            ForEachTarget::Declare { target, .. } => count_in_pattern(target, name, count),
            ForEachTarget::Dispose { target, .. } => {
                if *target == name {
                    *count += 1;
                }
            }
            ForEachTarget::Assign(_) => {}
        },
        _ => {}
    }
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => count_in_statement(inner, name, count),
        StmtChild::Expr(expr) => count_in_expr(expr, name, count),
        StmtChild::Binding(binding) => {
            count_in_pattern(&binding.target, name, count);
            if let Some(value) = &binding.value {
                count_in_expr(value, name, count);
            }
        }
        StmtChild::Catch(catch) => {
            if let Some(binding) = &catch.binding {
                count_in_pattern(binding, name, count);
            }
            for inner in &catch.body {
                count_in_statement(inner, name, count);
            }
        }
        StmtChild::Function(function) => {
            if function.name == Some(name) {
                *count += 1;
            }
            count_in_function(function, name, count);
        }
        StmtChild::Class(class) => {
            if class.name == Some(name) {
                *count += 1;
            }
            count_in_class(class, name, count);
        }
    });
}

fn count_in_expr(expr: &Expr, name: Name, count: &mut usize) {
    walk_expr(expr, &mut |child| match child {
        Child::Expr(inner) => count_in_expr(inner, name, count),
        Child::Function(function) => {
            if function.name == Some(name) {
                *count += 1;
            }
            count_in_function(function, name, count);
        }
        Child::Class(class) => {
            if class.name == Some(name) {
                *count += 1;
            }
            count_in_class(class, name, count);
        }
    });
}

fn count_in_function(function: &Function, name: Name, count: &mut usize) {
    for parameter in &function.parameters {
        count_in_pattern(&parameter.target, name, count);
        if let Some(default) = &parameter.default {
            count_in_expr(default, name, count);
        }
    }
    if let Some(rest) = &function.rest_parameter {
        count_in_pattern(rest, name, count);
    }
    match &function.body {
        FunctionBody::Block(statements) => {
            for statement in statements {
                count_in_statement(statement, name, count);
            }
        }
        FunctionBody::Expression(expr) => count_in_expr(expr, name, count),
    }
}

fn count_in_class(class: &Class, name: Name, count: &mut usize) {
    if let Some(heritage) = &class.heritage {
        count_in_expr(heritage, name, count);
    }
    for element in &class.body {
        match element {
            ClassElement::Method(method) => count_in_function(&method.function, name, count),
            ClassElement::Field(field) => {
                if let Some(value) = &field.value {
                    count_in_expr(value, name, count);
                }
            }
            ClassElement::StaticBlock(statements) => {
                for statement in statements {
                    count_in_statement(statement, name, count);
                }
            }
        }
    }
}

fn count_in_pattern(pattern: &Pattern, name: Name, count: &mut usize) {
    let mut bound = Vec::new();
    pattern.bound_names(&mut bound);
    *count += bound.iter().filter(|held| **held == name).count();
}
