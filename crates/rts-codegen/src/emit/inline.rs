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
//! **Four gates, not three, and `rts-codegen`'s rule 11 is the binding text.**
//! Extending this pass produced four distinct failures and each was caught by a
//! different one: a fixture written first, the corpus compared per file, the
//! doctests, and — after all three were green — the CLOCK, which is what found
//! a guard that had turned the whole pass off while every correctness gate
//! stayed green. A disabled optimisation passes every test there is.
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
    AssignTarget, Binding, BindingKind, Class, ClassElement, Expr, ExprKind, ForEachTarget,
    Function,
    FunctionBody, Pattern, Spreadable, Stmt, StmtKind,
};

use super::capture::{Child, StmtChild, walk_expr, walk_stmt};
use super::{Ctx, EmitResult, Scope};

/// A function whose call may be replaced by its body.
pub(super) struct Inlinable {
    /// Its parameters, in order. Every one is a plain name with no default.
    pub parameters: Vec<Name>,
    /// The names the body reads that it does not declare, each proven to have
    /// exactly one declaration in the whole program. Kept so the call site can
    /// ask the CALLER's escape analysis about them, which is a question only
    /// the site can answer.
    pub free: Vec<Name>,
    /// The statements before the answer, in order. Empty for the one-expression
    /// shape this began as.
    ///
    /// Copied at every call site, which is why [`STATEMENT_BUDGET`] exists: a
    /// body admitted here is not made faster, it is made part of its caller.
    pub statements: Vec<Stmt>,
    /// The single expression it answers.
    pub body: Expr,
    /// A zero-fixed-parameter rest function whose body is exactly `rest.length`.
    pub rest_length: Option<Name>,
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
    length: Name,
) -> BTreeMap<Name, Rc<Inlinable>> {
    let mut found = BTreeMap::new();
    // EVERY declaration in the program, not only the top-level ones.
    //
    // A helper declared inside a function was invisible to this pass, and the
    // measurement says what that cost: the same one-expression helper is 8.2 ns
    // a call at the top level and 19.2 nested — the whole door, on a shape real
    // code writes constantly.
    //
    // Collecting them into ONE flat map is only sound because of the gate at
    // the call site: `emit_substituted` requires the name to be LEXICALLY BOUND
    // there. Without it, `function f() { function parseInt(x) { return 0 } }`
    // would make a `parseInt("5")` in an unrelated function substitute the
    // nested body instead of reaching the global — the name is declared exactly
    // once in the program, so the count below cannot tell the two apart. The
    // scope can, and that is the half that makes this legal.
    let mut declarations = Vec::new();
    for statement in body {
        collect_declarations(statement, &mut declarations);
    }
    for (name, function) in declarations {
        let Some((candidate, free, locals)) = shape_of(function, length, name) else {
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
        // AND THE SAME PAIR FOR EVERY NAME THE BODY READS THAT IT DOES NOT
        // DECLARE. This is the whole soundness argument for substituting a body
        // that is not closed over its parameters.
        //
        // The body is emitted in the CALLER's scope, so a free name resolves
        // against the caller's bindings. `declarations_of(body, free) == 1`
        // says the entire program declares that name exactly once — so there is
        // no second binding for any caller to resolve it to, and the name means
        // the same thing wherever the body lands. The counter over-counts on
        // purpose (`declarations_of` says so: a parameter, a `catch` binding and
        // a loop target all count), and over-counting refuses a candidate where
        // under-counting would substitute against the wrong binding.
        //
        // The OTHER half of `untouched` — the one about the program rather than
        // about one name — is asked once, below, and asking it per free name
        // would be the wrong question. `untouched` refuses a name that is
        // ASSIGNED anywhere, which is exactly right for a primordial: `Math`
        // being replaced is the disturbance. For a free VARIABLE it is exactly
        // wrong — being assigned is what a variable is for, and the body of the
        // function this was extended to admit assigns the very name it reads.
        // The substituted write lands on the same binding the call would have
        // written, in the same order, so an assignment says nothing about
        // whether the substitution is legal.
        if free.iter().any(|held| declarations_of(body, *held) != 1) {
            continue;
        }
        // AND THE BODY'S OWN LOCALS, which is the guard that lets a declaring
        // body be substituted at all.
        //
        // The body is emitted in the CALLER's scope, so a `const t` inside it
        // declares a `t` THERE. When the caller already has one — and a
        // module-level `const` lives in an environment object rather than in a
        // scope this can enter and leave — the substitution WROTE IT.
        // `localsOnly(3)` set the caller's `t` to 6 on the build that admitted
        // this without a guard, which is three wrong answers in the fixture.
        //
        // One declaration in the whole program means no caller has a binding of
        // that name to clobber: not a `const`, not a `let`, and not a parameter
        // either, because `declarations_of` counts those too. So the name the
        // body declares can only be its own.
        //
        // This is the same proof the free names get, asked in the opposite
        // direction — there, that no caller resolves the name DIFFERENTLY; here,
        // that no caller resolves it at all.
        if locals.iter().any(|held| declarations_of(body, *held) != 1) {
            continue;
        }
        // What a declaration count CANNOT see, asked once for the whole
        // program: `eval` and `globalThis` each put a binding in scope that no
        // declaration spells, so either of them present means the count above
        // proves nothing. `untouched` already ends on both, whatever name it is
        // given — so it is given `eval`, and answers the half that is left.
        if !free.is_empty() && !super::primordial::untouched(body, eval, eval, global_this) {
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
    // THE NAME MUST BE BOUND HERE, and this is what makes collecting candidates
    // from any depth legal.
    //
    // `declarations_of == 1` says the program declares this name once. It does
    // NOT say that a given call site can see that declaration: a helper written
    // inside one function is out of scope in the next, where the same spelling
    // reaches a GLOBAL instead — `function f() { function parseInt(x) {…} }`
    // beside a `parseInt("5")` elsewhere is the whole hazard, and the count
    // cannot tell the two apart. The scope chain can, so it is asked.
    //
    // A top-level declaration is bound at every site inside the module for the
    // same reason it is callable there, so this refuses nothing that already
    // worked — which is a claim the clock checks, not a comment.
    if scope.lookup(*name).is_none() {
        return Ok(None);
    }
    if candidate.rest_length.is_some() {
        // A spread has a runtime-dependent count, so it cannot use the exact
        // written-argument proof. Still emit every non-spread argument in source
        // order so side effects happen before the constant result.
        if arguments.iter().any(|argument| matches!(argument, Spreadable::Spread(_))) {
            return Ok(None);
        }
        for argument in arguments {
            let Spreadable::Single(value) = argument else {
                unreachable!("spread was rejected above")
            };
            super::expr::emit_expr(builder, scope, ctx, value)?;
        }
        let count = super::expr::number_constant(builder, arguments.len() as f64);
        return Ok(Some(super::expr::tagged(builder, count)));
    }
    if arguments.len() != candidate.parameters.len() {
        return Ok(None);
    }
    // No name the body uses may spell one the CALLER's escape analysis
    // flattened. The body is emitted in the caller's scope, so `p.x` in the
    // callee would be read as the caller's replaced field if the caller happened
    // to have an object named `p` — the one place where the proof is not enough,
    // because the flattened form is keyed by NAME rather than by binding.
    //
    // The free names are asked as well as the parameters, and they have to be:
    // a free name is precisely one the caller's scope resolves, which is where a
    // flattened object lives.
    if candidate
        .parameters
        .iter()
        .chain(candidate.free.iter())
        .any(|held| ctx.flattens(*held))
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
    // The statements before the answer, in the scope the parameters were just
    // bound in — so a `let` in the body shadows correctly and leaves with the
    // scope. A fresh `Loops` because nothing in an accepted body can jump:
    // `straight_line` refuses every loop, label, `break`, `continue`, `return`
    // and `try`, so there is no frame for one to reach.
    let mut ran = Ok(());
    for statement in &candidate.statements {
        if ran.is_err() {
            break;
        }
        ran = super::stmt::emit_stmt(
            builder,
            scope,
            ctx,
            &mut super::loops::Loops::default(),
            statement,
        )
        .map(|_| ());
    }
    let answered = ran.and_then(|()| super::expr::emit_expr(builder, scope, ctx, &candidate.body));
    scope.leave();
    Ok(Some(answered?))
}


/// Every `function f(…)` and `const f = …` in a statement and everything under
/// it, however deep.
///
/// [`declared_function`] answers for ONE statement; this is the same question
/// asked of a whole subtree, so a helper written inside the function that uses
/// it is reachable. The walk goes through `walk_stmt`'s children rather than
/// matching each statement kind, which is what keeps a statement added to the
/// tree tomorrow from being silently skipped.
fn collect_declarations<'a>(statement: &'a Stmt, found: &mut Vec<(Name, &'a Function)>) {
    if let Some(pair) = declared_function(statement) {
        found.push(pair);
    }
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => collect_declarations(inner, found),
        StmtChild::Binding(binding) => {
            let Some(value) = &binding.value else {
                return;
            };
            if let (Pattern::Name(name), ExprKind::Function(function)) =
                (&binding.target, &value.kind)
            {
                found.push((*name, function));
            }
            // And through the value whatever it is, because a function can be
            // written anywhere inside one. `const f = (function () { … })()` —
            // an IIFE — binds `f` to a CALL, so the arm above does not fire and
            // the declarations inside the immediately-invoked function were
            // never reached.
            collect_in_expr(value, found);
        }
        StmtChild::Catch(catch) => {
            for inner in &catch.body {
                collect_declarations(inner, found);
            }
        }
        StmtChild::Function(function) => {
            if let Some(name) = function.name {
                found.push((name, function));
            }
            for inner in body_statements(function) {
                collect_declarations(inner, found);
            }
        }
        // A function written in EXPRESSION position — an argument, an IIFE, a
        // property value — holds declarations like any other body, and the walk
        // stopped at the expression. `(function () { function h(){…} … })()` is
        // the shape: measured 19.0 ns a call against 7.75 for the same helper
        // inside a function DECLARATION, purely because of where the enclosing
        // function was written.
        StmtChild::Expr(expr) => collect_in_expr(expr, found),
        StmtChild::Class(_) => {}
    });
    // A declaration's own body, which `walk_stmt` does not descend into for a
    // `StmtKind::Function` — it hands the function over as a child and stops.
    if let Some((_, function)) = declared_function(statement) {
        for inner in body_statements(function) {
            collect_declarations(inner, found);
        }
    }
}

/// The same walk, through an expression, for the bodies written inside one.
///
/// Only functions are looked for: a class body is left alone because a method
/// is reached as a member and this pass only substitutes a bare name.
fn collect_in_expr<'a>(expr: &'a Expr, found: &mut Vec<(Name, &'a Function)>) {
    walk_expr(expr, &mut |child| match child {
        Child::Expr(inner) => collect_in_expr(inner, found),
        Child::Function(function) => {
            if let Some(name) = function.name {
                found.push((name, function));
            }
            for inner in body_statements(function) {
                collect_declarations(inner, found);
            }
        }
        Child::Class(_) => {}
    });
}

/// The statements of a body, or nothing for a concise arrow.
///
/// A concise `x => x + 1` has no statements to look inside, and answering an
/// empty slice for it is the whole of what this exists to say.
fn body_statements(function: &Function) -> &[Stmt] {
    match &function.body {
        FunctionBody::Block(statements) => statements,
        FunctionBody::Expression(_) => &[],
    }
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
/// What a function has to be for its body to stand in for a call to it.
///
/// Answers the candidate AND the names its body reads that it does not declare
/// — its FREE names. Deciding those needs the whole program, which this
/// function does not have, so it collects them and [`candidates`] applies the
/// same two questions to each that it already applies to the function's own
/// name.
fn shape_of(
    function: &Function,
    length: Name,
    own: Name,
) -> Option<(Inlinable, Vec<Name>, Vec<Name>)> {
    if function.is_async || function.is_generator {
        return None;
    }
    // The rest-length special case, unchanged: one expression, no statements,
    // and its own proof.
    if function.rest_parameter.is_some() {
        let answered = returned_expression(function)?;
        let Some(rest) = &function.rest_parameter else {
            return None;
        };
        let Pattern::Name(rest) = rest else {
            return None;
        };
        if !function.parameters.is_empty()
            || !matches!(
                &answered.kind,
                ExprKind::Member {
                    object,
                    property,
                    optional: false,
                } if matches!(&object.kind, ExprKind::Ident(name) if *name == *rest)
                    && *property == length
            )
        {
            return None;
        }
        return Some((
            Inlinable {
                parameters: Vec::new(),
                free: Vec::new(),
                statements: Vec::new(),
                body: answered.clone(),
                rest_length: Some(*rest),
            },
            Vec::new(),
            Vec::new(),
        ));
    }
    if !function.has_simple_parameter_list() {
        return None;
    }
    let mut parameters = Vec::with_capacity(function.parameters.len());
    for parameter in &function.parameters {
        match &parameter.target {
            Pattern::Name(name) => parameters.push(*name),
            _ => return None,
        }
    }

    let (statements, answered) = body_shape(function)?;
    if statements.len() > STATEMENT_BUDGET {
        return None;
    }

    // The parameters, plus whatever the body declares for itself. A declared
    // name is BOUND — reading it is not reading the caller's — and the count of
    // them travels out so `candidates` can ask the whole program about each.
    let mut bound = parameters.clone();
    let first_local = bound.len();
    for statement in &statements {
        if !declared_names(statement, &mut bound) {
            return None;
        }
    }
    let locals: Vec<Name> = bound[first_local..].to_vec();
    // A body that declares one name twice is refused rather than reasoned
    // about: `const a = 1; { const a = 2; }` is legal JavaScript and the
    // substitution has no way to keep the two apart in one scope.
    if locals.iter().enumerate().any(|(at, name)| locals[..at].contains(name)) {
        return None;
    }

    // Recursion is refused by NAME rather than by a call graph. With statements
    // admitted a body can contain calls, and a self-call would substitute for
    // ever; a mutual pair is caught because each is free in the other and the
    // free-name question below is asked of a name that IS a function.
    if bound.contains(&own) {
        return None;
    }

    let mut free = Vec::new();
    for statement in &statements {
        if !closed_over_statement(statement, &bound, &mut free) {
            return None;
        }
    }
    if !closed_over(answered, &bound, &mut free) {
        return None;
    }
    if free.contains(&own) {
        return None;
    }
    // A function EXPRESSION binds its own name INSIDE its own body and nowhere
    // else: `const f = function fact(n) { … fact(n - 1) … }` has a `fact` that
    // exists only while the body runs. The free-name proof cannot see that —
    // `declarations_of` counts the expression's name as a declaration, so the
    // count is one and the name looks admissible — and the substituted body then
    // lands in a caller's scope where nothing declares it.
    //
    // `ReferenceError: fact is not defined`, in `function_expression.test.ts`
    // and `claude-fnexpr-selfname-shadow.test.ts`, on the build that missed it.
    // The guard above only knew the name the DECLARATION uses, which for a
    // `const f = function fact(…)` is `f`.
    // Only when the body actually NAMES it. `inner == own` would be true of every
    // `function f()` — the declaration and the expression carry the same name —
    // and refusing on that alone turned the whole pass off, which the monte
    // carlo timing caught before this shipped.
    if let Some(inner) = function.name
        && free.contains(&inner)
    {
        return None;
    }

    Some((
        Inlinable {
            parameters,
            free: free.clone(),
            statements,
            body: answered.clone(),
            rest_length: None,
        },
        free,
        locals,
    ))
}

/// How many statements a body may have before its call sites are ONE program.
///
/// The body is copied at every site, so this is a code-size decision and not a
/// correctness one. Eight is the smallest number that admits the shape this was
/// extended for — a generator step is three — with room for a guard clause.
const STATEMENT_BUDGET: usize = 8;

/// The statements before the answer, and the answer.
///
/// The accepted shape is deliberately narrow: a run of statements that cannot
/// leave the body early, then exactly one `return` at the end. That is what
/// makes the substitution a straight splice — the body's value is the last
/// expression, and no jump has to be routed anywhere.
fn body_shape(function: &Function) -> Option<(Vec<Stmt>, &Expr)> {
    match &function.body {
        FunctionBody::Expression(expr) => Some((Vec::new(), expr)),
        FunctionBody::Block(statements) => {
            let (last, before) = statements.split_last()?;
            let StmtKind::Return(Some(answered)) = &last.kind else {
                return None;
            };
            for statement in before {
                if !straight_line(statement) {
                    return None;
                }
            }
            Some((before.to_vec(), answered))
        }
    }
}

/// Whether a statement runs and finishes, with no way out of the body.
///
/// An allowlist, for the reason [`closed_over`] is one: a statement kind added
/// tomorrow is refused by default. `Return`, `Break`, `Continue`, `Throw`,
/// `Try`, every loop and every label are absent on purpose — each of them can
/// leave the body somewhere other than the end, and the splice has nowhere to
/// send it.
///
/// # `Declare` is absent, and it was there for one build
///
/// A body that declares a local writes the CALLER's binding of that name,
/// because the body is emitted in the caller's scope and a module-level `const`
/// lives in an environment object rather than in a scope this can enter and
/// leave. Measured, on the build that admitted it:
///
/// ```text
/// function f(n) { const t = n * 2; return t + 1; }
/// const t = 500;
/// f(3);            // t is now 6
/// ```
///
/// — three wrong answers in the fixture written before the change, which is
/// what that fixture is for. Admitting a declaration needs the renaming pass
/// this module's header already says it does not have; until there is one, a
/// body with no declarations is the whole of what can be spliced, and it is
/// enough for the shape this was extended for.
fn straight_line(statement: &Stmt) -> bool {
    match &statement.kind {
        StmtKind::Empty => true,
        StmtKind::Expr(_) => true,
        // A DECLARATION is admitted again, and the guard that makes it safe is
        // not here — it is in `candidates`, which requires the declared name to
        // have exactly ONE declaration in the whole program. See `shape_of`.
        StmtKind::Declare { kind, bindings } => {
            !matches!(kind, BindingKind::Var)
                && bindings.iter().all(|binding| {
                    matches!(binding.target, Pattern::Name(_)) && binding.value.is_some()
                })
        }
        StmtKind::Block(inner) => inner.iter().all(straight_line),
        StmtKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            straight_line(then_branch)
                && else_branch
                    .as_ref()
                    .is_none_or(|branch| straight_line(branch))
        }
        _ => false,
    }
}

/// Every name a statement introduces, added to `bound`.
///
/// `false` for a shape `straight_line` has already refused, so this is a second
/// gate rather than the first: adding a statement kind to one list without the
/// other refuses instead of admitting.
fn declared_names(statement: &Stmt, bound: &mut Vec<Name>) -> bool {
    match &statement.kind {
        StmtKind::Declare { bindings, .. } => {
            for binding in bindings {
                let Pattern::Name(name) = &binding.target else {
                    return false;
                };
                bound.push(*name);
            }
            true
        }
        StmtKind::Block(inner) => inner.iter().all(|held| declared_names(held, bound)),
        StmtKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            declared_names(then_branch, bound)
                && else_branch
                    .as_ref()
                    .is_none_or(|branch| declared_names(branch, bound))
        }
        StmtKind::Empty | StmtKind::Expr(_) => true,
        _ => false,
    }
}

/// The same question [`closed_over`] asks, over a statement.
fn closed_over_statement(statement: &Stmt, bound: &[Name], free: &mut Vec<Name>) -> bool {
    match &statement.kind {
        StmtKind::Empty => true,
        StmtKind::Expr(expr) => closed_over(expr, bound, free),
        StmtKind::Declare { bindings, .. } => bindings.iter().all(|binding| {
            binding
                .value
                .as_ref()
                .is_some_and(|value| closed_over(value, bound, free))
        }),
        StmtKind::Block(inner) => inner
            .iter()
            .all(|statement| closed_over_statement(statement, bound, free)),
        StmtKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            closed_over(condition, bound, free)
                && closed_over_statement(then_branch, bound, free)
                && else_branch
                    .as_ref()
                    .is_none_or(|branch| closed_over_statement(branch, bound, free))
        }
        _ => false,
    }
}

/// Whether every identifier the body reads is one the substitution can name,
/// collecting the ones it cannot bind itself.
///
/// An allowlist rather than a list of refusals: a node added to the tree
/// tomorrow is refused by default, which is the direction a wrong answer here
/// cannot come from.
///
/// # What changed, and what did not
///
/// It used to answer "every identifier is a parameter" and nothing else was
/// admitted. A name that is neither a parameter nor declared by the body is now
/// COLLECTED instead of refused, and [`candidates`] decides it against the whole
/// program — `declarations_of(program, name) == 1` plus `primordial::untouched`,
/// the same pair the function's own name already has to pass. One declaration
/// in the entire program is exactly the property that makes emitting the body in
/// the CALLER's scope legal: there is no second binding for the caller to
/// resolve the name to.
///
/// An ASSIGNMENT is admitted for the same reason and only to a plain name.
/// Writing a member would need a receiver this substitution does not have, and
/// writing a PARAMETER would write a binding `emit_substituted` made out of an
/// SSA value rather than a cell — so both stay refused.
fn closed_over(expr: &Expr, bound: &[Name], free: &mut Vec<Name>) -> bool {
    match &expr.kind {
        ExprKind::Ident(name) => {
            if !bound.contains(name) && !free.contains(name) {
                free.push(*name);
            }
            return true;
        }
        ExprKind::Literal(_) => return true,
        // Everything with a body, a receiver, a suspension point, or a write
        // this substitution cannot make. A nested function would capture the
        // caller's scope rather than the callee's, and `this` has no answer at
        // a call site that passes none.
        ExprKind::Function(_)
        | ExprKind::Class(_)
        | ExprKind::This
        | ExprKind::Await(_)
        | ExprKind::Yield { .. }
        | ExprKind::SuperMember { .. }
        | ExprKind::SuperCall { .. }
        | ExprKind::PrivateName(_)
        | ExprKind::NewTarget
        | ExprKind::ImportMeta
        | ExprKind::ImportCall { .. } => return false,
        ExprKind::Assign { target, value, .. } => {
            let AssignTarget::Place(place) = target else {
                return false;
            };
            // A MEMBER or an INDEX, which is a write to the HEAP rather than to
            // a binding. The receiver is an ordinary expression and goes through
            // the same proof as any other, and the write lands on the same
            // object the call would have written, in the same order. `box.v = x`
            // measured 26.0 ns a call against 8 for the same function without
            // it, purely because this arm refused the shape.
            //
            // Distinct from writing a plain NAME below, which is refused for a
            // parameter because a parameter is bound to an SSA value here and a
            // write to one has nowhere to land. A member write does not touch
            // the binding at all.
            if matches!(
                &place.kind,
                ExprKind::Member { .. } | ExprKind::Index { .. }
            ) {
                return closed_over(place, bound, free) && closed_over(value, bound, free);
            }
            let ExprKind::Ident(name) = &place.kind else {
                return false;
            };
            let name = *name;
            // A parameter is bound to an SSA value here, not to a cell, so a
            // write to one would have nowhere to land.
            if bound.contains(&name) {
                return false;
            }
            if !free.contains(&name) {
                free.push(name);
            }
            return closed_over(value, bound, free);
        }
        ExprKind::Update { target, .. } => {
            // `o.x++` for the reason `o.x = v` above is admitted.
            if matches!(
                &target.kind,
                ExprKind::Member { .. } | ExprKind::Index { .. }
            ) {
                return closed_over(target, bound, free);
            }
            let ExprKind::Ident(name) = &target.kind else {
                return false;
            };
            let name = *name;
            if bound.contains(&name) {
                return false;
            }
            if !free.contains(&name) {
                free.push(name);
            }
            return true;
        }
        _ => {}
    }
    let mut ok = true;
    walk_expr(expr, &mut |child| match child {
        Child::Expr(inner) => ok = ok && closed_over(inner, bound, free),
        Child::Function(_) | Child::Class(_) => ok = false,
    });
    ok
}

/// The one expression a candidate returns, regardless of concise or block syntax.
fn returned_expression(function: &Function) -> Option<&Expr> {
    match &function.body {
        FunctionBody::Expression(expr) => Some(expr),
        FunctionBody::Block(statements) => match &statements[..] {
            [Stmt {
                kind: StmtKind::Return(Some(expr)),
                ..
            }] => Some(expr),
            _ => None,
        },
    }
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
