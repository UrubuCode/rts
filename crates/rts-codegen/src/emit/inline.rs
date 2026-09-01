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
use crate::syntax::Literal;
use crate::values::Singleton;
use crate::syntax::{
    AssignTarget, Binding, BindingKind, Class, ClassElement, Expr, ExprKind, ForEachTarget,
    Function,
    FunctionBody, Pattern, Spreadable, Stmt, StmtKind,
};

use super::capture::{Child, StmtChild, walk_expr, walk_stmt};
use super::{Ctx, EmitResult, Scope, UNPROVEN};

/// A function whose call may be replaced by its body.
pub(super) struct Inlinable {
    /// Its parameters, in order. Every one is a plain name.
    pub parameters: Vec<Name>,
    /// The default each parameter carries, or `None` where it carries none.
    ///
    /// Parallel to [`Self::parameters`] rather than a pair, because every other
    /// consumer of that list wants only the names and a tuple would make each
    /// of them say so.
    ///
    /// Only the ABSENT-argument case is substituted. A call that writes the
    /// argument may still be passing `undefined` at run time, which the
    /// language says also takes the default, and deciding that needs a test the
    /// call site cannot settle — so a written argument for a defaulted
    /// parameter falls back to a real call. That is strictly more than was
    /// accepted before, which was nothing.
    pub defaults: Vec<Option<Expr>>,
    /// The names the body reads that it does not declare. Kept so the call site
    /// can ask the CALLER's escape analysis about them, which is a question only
    /// the site can answer.
    pub free: Vec<Name>,
    /// Whether every free name has exactly one declaration in the whole program.
    ///
    /// # Why this is a flag and no longer a refusal
    ///
    /// The proof it carries is about WHERE a substituted body lands: the body is
    /// emitted in the caller's scope, so a free name resolves against the
    /// caller's bindings, and one declaration program-wide means there is no
    /// second binding for any caller to resolve it to.
    ///
    /// Refusing outright made the count decide it, and the count over-counts on
    /// purpose — a parameter, a `catch` binding and a LOOP TARGET all count. So
    /// a helper that reads its loop variable was refused in every program that
    /// has two loops, because both spell it `i`:
    ///
    /// ```text
    /// for (let i   = …) { const q = (x) => x + i;   a = q(a) | 0; }   233.67 ns
    /// for (let zwq = …) { const q = (x) => x + zwq; a = q(a) | 0; }    46.33 ns
    /// ```
    ///
    /// Measured 2026-08-30, release, min of 9. Five times, and the only
    /// difference is spelling.
    ///
    /// The proof is still required; it is asked at the SITE, where a second and
    /// stronger one is available. `Ctx::omits` says the helper is declared in
    /// the body being emitted, is never read as a value, and is not captured —
    /// so every call to it is in the declaring function, the caller IS the
    /// declarer, and a free name resolves to the binding it was written against
    /// by construction. That is what the count was approximating.
    pub free_proved: bool,
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
    arguments: Name,
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
        let Some((candidate, free, locals)) = shape_of(function, length, name, false) else {
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
        //
        // ZERO declarations is admitted too, and it is a STRONGER proof than
        // one, not a weaker one. A name the whole program declares nowhere is
        // resolved through the global object at every site there is — no scope
        // binds it, so there is no second answer for a caller to have — which is
        // the same conclusion the `== 1` case reaches by a longer road.
        //
        // What it needs beside that is `untouched`, and here the comment above
        // is exactly right rather than exactly wrong: a zero-declaration name IS
        // a primordial, so being ASSIGNED anywhere — including through
        // `globalThis` — is the disturbance, and refusing it is the point.
        //
        // It matters because it is most helper code. `Math`, `Error`, `JSON`,
        // `Object`, `console` all count zero, so a body mentioning any of them
        // was refused: measured 2026-08-30, a helper reading `Math.abs` cost
        // 52.75 ns where the same helper without it cost 8.00, and about twelve
        // of that difference was the call the refusal kept.
        //
        // `arguments` is the one zero-declaration name that is NOT a global, and
        // it has to be named here because the count cannot see it: every
        // function gets one bound implicitly, so a body reading it reads its
        // OWN, and a substituted body would read the CALLER's — a different
        // object, with different contents, or none at all in an arrow.
        //
        // It was safe for as long as zero was refused outright, and the comment
        // in `emit_substituted` said exactly that. Admitting zero broke the
        // premise it rested on, and `tests/arguments_object.test.ts` and
        // `tests/claude-arguments-fn-expr.test.ts` both failed on the build that
        // did — which is what the per-file corpus comparison is for.
        //
        // `eval` and `globalThis` are the same kind of name and are refused
        // below, once for the whole candidate rather than per free name.
        if free.iter().any(|held| *held == arguments) {
            continue;
        }
        // RECORDED RATHER THAN REFUSED, and `free_proved` says why. A site that
        // cannot offer the stronger proof still refuses on it, so nothing that
        // was substituted before stops being and nothing new is substituted
        // without a proof — only the proof may now come from the site.
        let mut candidate = candidate;
        let free_proved = !free.iter().any(|held| match declarations_of(body, *held) {
            1 => false,
            0 => !super::primordial::untouched(body, *held, eval, global_this),
            _ => true,
        });
        candidate.free_proved = free_proved;


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
/// One fact that belongs to the call site and not to the callee, and it is no
/// longer the arity. A call passing FEWER arguments than the function declares
/// binds the missing parameters to `undefined`, which is exactly what the
/// convention does for a real call; one passing MORE evaluates the extra
/// arguments where they were written and drops their values, which is also what
/// a real call does with them. Both were refused for as long as neither was
/// written, and both were worth about 14 ns a call — an accepted call measured
/// 9.00 ns against 23.00 for a refused one on 2026-08-29.
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
    // TWO SHAPES OF CALLEE, and the second is why this file grew a receiver.
    //
    // `f(x)` names one function when the program declares `f` once, or when
    // `omit` proved the declaration in hand is the one every call reaches.
    // `o.m(x)` names one just as surely when `receiver.rs` proved it — `o` is a
    // `const` holding `new C()`, neither `o` nor `C` is ever read as a value, so
    // nothing can reassign `o.m` or `C.prototype` without spelling one of them.
    //
    // What it is worth: a method call is 19.00 ns against 6.00 for the property
    // read alone and 1.00 for a substituted call, measured 2026-08-30. The call
    // is a runtime crossing and being a method costs about one nanosecond, so
    // what a substitution removes here is the crossing.
    let (candidate, receiver) = match &callee.kind {
        ExprKind::Ident(name) => match ctx.inlinable_here(*name) {
            Some(candidate) => (candidate, None),
            None => return Ok(None),
        },
        _ => match super::receiver::receiver_of(callee) {
            Some((held, method)) => match ctx.static_method(held, method) {
                Some(candidate) => (candidate, Some(held)),
                None => return Ok(None),
            },
            None => return Ok(None),
        },
    };
    // The rest of this function asks about the callee BY NAME — the cycle
    // check, the omission, the scope. A method's name is the receiver's, because
    // that is the binding the site actually reads and the one a cycle would come
    // back through.
    let name = &match &callee.kind {
        ExprKind::Ident(name) => *name,
        _ => receiver.expect("a non-identifier callee resolved through its receiver"),
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
    //
    // AN OMITTED HELPER IS THE ONE NAME THAT IS NOT BOUND AND IS STILL RIGHT.
    // `omit::omittable` proved, for this whole body and before any of it was
    // emitted, that every call to it is substituted — so the declaration was
    // never emitted and there is nothing in the scope chain to find.
    //
    // Asked BEFORE the lookup rather than after, because the lookup's own
    // reason for existing is the opposite hazard: a name that resolves to
    // something else at this site. An omitted name resolves to nothing
    // anywhere, by construction, which is why the proof had to be complete.
    if !ctx.omits(*name) && scope.lookup(*name).is_none() {
        return Ok(None);
    }
    // ALREADY BEING SUBSTITUTED, further out. The check below refuses a body
    // that mentions its own name, which stops `f` calling `f` and says nothing
    // about `f` calling `g` calling `f` — and the comment above `bound.contains`
    // claimed a mutual pair was caught "because each is free in the other",
    // which refuses nothing. It went unnoticed only because such a body has a
    // `return` in it and `return` was refused for its own reasons.
    //
    // Admitting a guard clause removed that reason, and
    // `two_functions_can_call_each_other` overflowed the COMPILER's stack.
    // THE FREE-NAME PROOF, ASKED HERE BECAUSE THE SITE HAS A STRONGER ONE.
    //
    // `free_proved` is the count: every free name declared exactly once in the
    // whole program, so no caller can resolve one to a different binding. It is
    // sound and it over-counts, because `declarations_of` counts a parameter, a
    // `catch` binding and a LOOP TARGET — so a helper reading its loop variable
    // was refused in every program with two loops, both spelling it `i`. That
    // cost five times:
    //
    //     for (let i   = …) { const q = (x) => x + i;   … }   233.67 ns
    //     for (let zwq = …) { const q = (x) => x + zwq; … }    46.33 ns
    //
    // `ctx.omits` is the other proof, and it is stronger rather than weaker.
    // `omit::omittable` established, over this whole body and before any of it
    // was emitted, that the helper is declared HERE, is never read as a value,
    // and is not captured. So every call to it is in the declaring function,
    // the caller IS the declarer, and a free name resolves to the binding it
    // was written against — not because no other binding exists anywhere, but
    // because no other scope is between the two.
    //
    // Either proof suffices and neither is skipped. A candidate with no count
    // proof, at a site that does not omit it, refuses exactly as it did before.
    if !candidate.free_proved && !ctx.omits(*name) {
        return Ok(None);
    }
    if ctx.substituting(*name) {
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
    // A DEFAULT whose argument was written is refused, and this is the whole of
    // what is not substituted about defaults. `f(a, undefined)` writes the
    // argument and still takes the default — the language decides that from the
    // VALUE, at run time — so binding the written argument would be wrong and
    // binding the default would be wrong, and the test that separates them is
    // one this pass would have to emit. A real call already does it.
    //
    // The absent case needs no test at all: an argument nobody wrote cannot be
    // anything, so the default applies by construction. That is the shape
    // `f(x)` on `function f(x, y = 1)` takes, and it measured 24.50 ns against
    // 9.00 for a substituted call.
    if candidate
        .defaults
        .iter()
        .enumerate()
        .any(|(at, default)| default.is_some() && at < arguments.len())
    {
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

    // THE RECEIVER, EMITTED ONCE AND BOUND AS `this`.
    //
    // Read before the layer is entered, so it resolves against the caller's
    // scope — which is where `o` is — and after the arguments, which the
    // language evaluates in that order for a member callee too.
    let held_this = match receiver {
        Some(held) => Some(super::binding::read(builder, scope, ctx, held)?),
        None => None,
    };

    scope.enter();
    // Swapped rather than pushed: `this_value` is a field of the whole scope
    // and not a layer, so the caller's own answer is put back below. A body
    // that does not read `this` is unaffected either way; one that does is only
    // ever admitted when a receiver was proved, so `None` here cannot reach a
    // body that would ask.
    let outer_this = match held_this {
        Some(value) => Some(scope.swap_this(Some(value))),
        None => None,
    };
    // A parameter the call did not pass is `undefined`, which is what the calling
    // convention hands a real call for the same shape — the six argument slots
    // are filled with it before the callee looks at them. An argument with no
    // parameter to bind was still EMITTED above, in source order, so its side
    // effects happen; what it does not get is a name, and a real call gives it
    // none either.
    //
    // `arguments` is not affected because a body that mentions it is refused
    // long before here: it is a free name the program declares nowhere, so
    // `declarations_of` answers zero and the candidate never forms.
    for (at, parameter) in candidate.parameters.iter().enumerate() {
        let value = match (values.get(at), candidate.defaults.get(at).and_then(Option::as_ref)) {
            // Written, and the parameter has no default: the argument.
            (Some(value), None) => *value,
            // Not written, and no default: `undefined`.
            (None, None) => super::expr::undefined(builder, ctx),
            // Not written, and a default: the default, evaluated HERE — after
            // the parameters to its left are bound, because a default may read
            // them (`f(a, b = a + 1)`), and before the ones to its right, which
            // is the order the language states.
            (None, Some(default)) => super::expr::emit_expr(builder, scope, ctx, default)?,
            // Written, and a default. Refused at the top of this function, so
            // this arm cannot run; it is written out rather than left to a
            // wildcard so that admitting the case later has one place to change.
            (Some(_), Some(_)) => unreachable!("a written argument for a defaulted parameter is refused above"),
        };
        scope.declare(*parameter, value);
    }
    // The statements before the answer, in the scope the parameters were just
    // bound in — so a `let` in the body shadows correctly and leaves with the
    // scope. A fresh `Loops` because nothing in an accepted body can jump:
    // `straight_line` refuses every loop, label, `break`, `continue`, `return`
    // and `try`, so there is no frame for one to reach.
    ctx.enter_substitution(*name);
    // A body with no guard clause leaves through its tail and needs no join at
    // all — that is the shape this pass began as, and it stays exactly as it
    // was so that admitting guards costs nothing where there are none.
    let guarded = candidate
        .statements
        .iter()
        .any(|statement| guard_return(statement).is_some());
    let join = guarded.then(|| builder.create_block());
    let merged = join.map(|block| builder.add_block_param(block, UNPROVEN));

    let mut ran = Ok(());
    for statement in &candidate.statements {
        if ran.is_err() {
            break;
        }
        // A GUARD CLAUSE is emitted as a branch to the join rather than as a
        // statement, because `stmt::emit_stmt` would emit its `Return` as a
        // terminator leaving the CALLER. The two halves are the same two the
        // body has: take the guard's answer, or carry on to the next statement.
        if let Some((condition, answer)) = guard_return(statement) {
            ran = (|| -> EmitResult<()> {
                let cond = super::expr::emit_condition(builder, scope, ctx, condition)?;
                let taken = builder.create_block();
                let carried = builder.create_block();
                // The scope each side starts from is the one the guard started
                // in. A guard's answer cannot see what the statements after it
                // bind, because it left before them.
                let before = scope.snapshot();
                builder.branch(cond, (taken, &[]), (carried, &[]))?;

                builder.switch_to(taken);
                let value = super::expr::emit_expr(builder, scope, ctx, answer)?;
                let value = builder.widen(value);
                let Some(block) = join else {
                    unreachable!("a guard was found, so the join was created")
                };
                builder.jump(block, &[value])?;

                scope.restore(&before);
                builder.switch_to(carried);
                Ok(())
            })();
            continue;
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
    // Popped before the `?` below, so a body that fails to emit leaves the stack
    // as it found it — an unbalanced push would refuse every later call to the
    // same helper, which is a silent loss rather than a failure.
    ctx.leave_substitution();
    let answered = answered?;
    let result = match (join, merged) {
        (Some(block), Some(merged)) => {
            let tail = builder.widen(answered);
            builder.jump(block, &[tail])?;
            builder.switch_to(block);
            merged
        }
        _ => answered,
    };
    if let Some(previous) = outer_this {
        scope.swap_this(previous);
    }
    scope.leave();
    Ok(Some(result))
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
pub(super) fn shape_of(
    function: &Function,
    length: Name,
    own: Name,
    // Whether `this` has an answer at the call sites this candidate is for.
    // True only for a method whose receiver `receiver.rs` decided; every other
    // door passes false and a body reading `this` is refused as it always was.
    this_ok: bool,
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
                defaults: Vec::new(),
                free: Vec::new(),
                free_proved: true,
                statements: Vec::new(),
                body: answered.clone(),
                rest_length: Some(*rest),
            },
            Vec::new(),
            Vec::new(),
        ));
    }
    // `has_simple_parameter_list` is NOT the question any more: it refuses a
    // default as well as a pattern, and a default is substitutable where a
    // pattern is not. The two conditions are asked apart — every target must be
    // a plain name, and a default is carried rather than refused.
    if function.rest_parameter.is_some() {
        return None;
    }
    let mut parameters = Vec::with_capacity(function.parameters.len());
    let mut defaults = Vec::with_capacity(function.parameters.len());
    for parameter in &function.parameters {
        match &parameter.target {
            Pattern::Name(name) => parameters.push(*name),
            _ => return None,
        }
        defaults.push(parameter.default.clone());
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
    // A DEFAULT is emitted at the call site exactly as the body is, so every name
    // it reads needs the same proof — and it is asked FIRST, because a default
    // is evaluated before any statement of the body.
    //
    // Without this the pass would be unsound rather than merely incomplete:
    // `function f(x, y = held)` substituted into a caller resolves `held` in the
    // CALLER's scope, which is a different binding wherever the caller has one.
    // The free-name set is computed from the body, and a default is not in the
    // body.
    //
    // `bound` is the parameters and the body's locals, which is the right
    // question by accident of order rather than by luck: a default may read a
    // parameter to its LEFT and may not read one to its right, and a body local
    // does not exist yet — so a default reading either is refused here, where
    // reading a left-hand parameter is admitted because the name is bound.
    for default in defaults.iter().flatten() {
        if !closed_over(default, &bound, &mut free, this_ok) {
            return None;
        }
    }
    for statement in &statements {
        if !closed_over_statement(statement, &bound, &mut free, this_ok) {
            return None;
        }
    }
    if !closed_over(&answered, &bound, &mut free, this_ok) {
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
            defaults,
            free: free.clone(),
            // Filled in by the caller, which is where the count is taken.
            free_proved: true,
            statements,
            body: answered,
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
fn body_shape(function: &Function) -> Option<(Vec<Stmt>, Expr)> {
    match &function.body {
        FunctionBody::Expression(expr) => Some((Vec::new(), (**expr).clone())),
        FunctionBody::Block(statements) => {
            // A VOID BODY answers `undefined`, and that is a value like any
            // other — so the shape is "statements, then an answer" and the
            // answer is synthesised where the source did not write one.
            //
            // It was refused outright, and a census of the corpus put it at 11%
            // of every named function: a helper that exists for its effects is
            // how most side-effecting code is written, and none of it could be
            // substituted. Nothing else about the body changes, so the whole
            // saving is the door — about 16 ns, the difference between a real
            // call at ~25 and a substituted one at ~9.
            //
            // `return;` with no value is the same case written explicitly, and
            // takes the same arm.
            let undefined = |at| Expr {
                kind: ExprKind::Literal(Literal::Singleton(Singleton::Undefined)),
                at,
            };
            let Some((last, before)) = statements.split_last() else {
                // An EMPTY body. Every statement before the answer is vacuously
                // straight-line and the answer is `undefined`.
                return Some((Vec::new(), undefined(function.at)));
            };
            let (before, answered) = match &last.kind {
                StmtKind::Return(Some(expr)) => (before, expr.clone()),
                StmtKind::Return(None) => (before, undefined(last.at)),
                // Not a `return` at all: the last statement is part of the body
                // and the answer is `undefined`.
                _ => (statements.as_slice(), undefined(function.at)),
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
/// The `if (c) { return e; }` a body opens with, if this statement is one.
///
/// # Why this shape and not `return` in general
///
/// Because a `Return` inside a substituted body would return from the CALLER —
/// the body is spliced into the caller's block, and there is no frame of its own
/// for a terminator to leave. That is what refuses every other jump here and
/// always will.
///
/// A GUARD CLAUSE is the one shape where the value it leaves with is knowable
/// without a frame: the statement has no `else`, so the body either answers `e`
/// or carries on to the next statement, and both are expressions the call site
/// can merge into one block parameter. `emit_substituted` does exactly that.
///
/// It is worth the special case because it is the shape small helpers are
/// written in — `if (n < 0) return 0;` ahead of the real answer — and one
/// measured 23.75 ns against 8.75 for the same helper without the guard.
///
/// Refuses a `return` with no value: `return;` answers `undefined`, which is
/// expressible, but the shape is vanishingly rare in a helper that has a value
/// to give and admitting it would need a constant nobody asked for.
fn guard_return(statement: &Stmt) -> Option<(&Expr, &Expr)> {
    let StmtKind::If {
        condition,
        then_branch,
        else_branch: None,
    } = &statement.kind
    else {
        return None;
    };
    // `{ return e; }` and `return e;` are the same guard written two ways, and
    // both are written.
    let inner = match &then_branch.kind {
        StmtKind::Block(statements) => match statements.as_slice() {
            [only] => only,
            _ => return None,
        },
        _ => then_branch.as_ref(),
    };
    let StmtKind::Return(Some(answer)) = &inner.kind else {
        return None;
    };
    Some((condition, answer))
}

fn straight_line(statement: &Stmt) -> bool {
    straight_line_at(statement, true)
}

/// The same, told whether it is looking at a TOP-LEVEL statement of the body.
///
/// # Why the depth is a parameter and not an oversight
///
/// A guard clause is admitted here and intercepted in `emit_substituted`, which
/// walks `candidate.statements` — the top level and nothing else. Anything
/// deeper falls through to `stmt::emit_stmt`, whose `StmtKind::Return` arm is
/// `builder.ret(&[result])`: **a return from the CALLER**.
///
/// This function used to recurse through `Block` and `If` with the guard arm
/// still live, so the two disagreed about which statements existed. Measured on
/// the engine, 2026-08-30:
///
/// ```text
/// function classify(x) {
///   if (x > 0) { if (x > 10) { return 99; } }   // a guard, one level down
///   return x;
/// }
/// console.log(classify(5), classify(50), classify(-1));
/// ```
///
/// node prints `5 99 -1`; this engine printed NOTHING and exited zero, because
/// the substituted body returned out of `console.log`'s caller and then out of
/// the module. A silent wrong answer with a successful exit, which is the worst
/// class this repository names.
///
/// Every test written for the guard clause put it at the top level, because that
/// is the shape the change was designed around — a test written from a design
/// tests the design. `tests/inline_statement_body.test.ts` now pins the nested
/// shapes as well.
///
/// Admitting a nested guard properly means `emit_substituted` walking the body
/// the way `straight_line` does and merging from any depth. That is a real
/// change and is not smuggled in here; refusing is correct and cheap.
fn straight_line_at(statement: &Stmt, top: bool) -> bool {
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
        // `Try` AND `Throw` ARE REFUSED, and that is a measured decision rather
        // than a gap. Both were admitted for one build — a `try` is CONTAINED,
        // so control reaches the statement after it however the arms end, which
        // is the whole of what refuses a loop — and .NET's RyuJIT needed to
        // merge an exception-handling table to do the same thing that
        // substituting a TREE gets for nothing.
        //
        // It does not pay. Measured 2026-08-30, release, three alternations,
        // with the mechanism actually present: `try`/`catch` with straight-line
        // arms 58.00 -> 60.00 ns, with a catch binding 58.00 -> 62.00, against
        // a plain-helper control that did not move. A body with a `try` costs
        // about 58 ns and the protected region is nearly all of it, so removing
        // the call buys a couple of nanoseconds and copying the arms into the
        // caller costs at least as many blocks back.
        //
        // A previous commit claimed this WAS admitted and reported a 4% win.
        // The arms had never landed — a patch script died on its second hunk —
        // so the 4% was layout noise attributed to a mechanism that did not
        // exist. See `docs/codegen/inlining-survey-2026-08-30.md`.
        StmtKind::Block(inner) => inner.iter().all(|held| straight_line_at(held, false)),
        // A guard clause, which is the one way a `Return` is admitted here — see
        // [`guard_return`] for why this shape and no other, and this function's
        // own comment for why only at the top.
        _ if top && guard_return(statement).is_some() => true,
        StmtKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            straight_line_at(then_branch, false)
                && else_branch
                    .as_ref()
                    .is_none_or(|branch| straight_line_at(branch, false))
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
        // BEFORE the `If` arm, and the position is load-bearing: a `match` takes
        // the first arm that fits, so an `If` guard placed after it would be
        // matched as an ordinary `if`, descend into the `return`, and refuse.
        // It was written after it for one build, the whole pass silently
        // refused every guarded body, and the only thing that said so was the
        // clock — the tests all passed, because refusing is always correct.
        //
        // A guard clause introduces no name: its `return` is the whole of its
        // body, and a `return` binds nothing.
        _ if guard_return(statement).is_some() => true,
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
fn closed_over_statement(
    statement: &Stmt,
    bound: &[Name],
    free: &mut Vec<Name>,
    this_ok: bool,
) -> bool {
    match &statement.kind {
        StmtKind::Empty => true,
        StmtKind::Expr(expr) => closed_over(expr, bound, free, this_ok),
        StmtKind::Declare { bindings, .. } => bindings.iter().all(|binding| {
            binding
                .value
                .as_ref()
                .is_some_and(|value| closed_over(value, bound, free, this_ok))
        }),
        StmtKind::Block(inner) => inner
            .iter()
            .all(|statement| closed_over_statement(statement, bound, free, this_ok)),
        // A guard clause, whose two halves are both expressions the call site
        // emits: asked BEFORE the general `If` arm, which would descend into the
        // `return` and refuse it.
        _ if guard_return(statement).is_some() => {
            let Some((condition, answer)) = guard_return(statement) else {
                unreachable!("the arm's own guard just answered")
            };
            closed_over(condition, bound, free, this_ok) && closed_over(answer, bound, free, this_ok)
        }
        StmtKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            closed_over(condition, bound, free, this_ok)
                && closed_over_statement(then_branch, bound, free, this_ok)
                && else_branch
                    .as_ref()
                    .is_none_or(|branch| closed_over_statement(branch, bound, free, this_ok))
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
fn closed_over(expr: &Expr, bound: &[Name], free: &mut Vec<Name>, this_ok: bool) -> bool {
    match &expr.kind {
        ExprKind::Ident(name) => {
            if !bound.contains(name) && !free.contains(name) {
                free.push(*name);
            }
            return true;
        }
        ExprKind::Literal(_) => return true,
        // `this` IS an answer when the call site provides a receiver, and is
        // none when it does not. `receiver.rs` decides which by proving that
        // `o.m` names one function; every other call site passes `this_ok`
        // false and this arm refuses exactly as it always did.
        ExprKind::This if this_ok => return true,
        // Everything with a body, a suspension point, or a write this
        // substitution cannot make. A nested function would capture the
        // caller's scope rather than the callee's.
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
        // A CONSTRUCTION, refused because a constructed object may capture the
        // stack — `new Error(…)` records `.stack` where it is BUILT — and a
        // substituted body has no frame to be named in it.
        //
        // The frame loss is not new and is not this arm's doing: any inlined
        // body gives up its frame, and always has. What is new is which bodies
        // reach the pass. Admitting a global as a free name made
        // `function made() { return new Error('later'); }` substitutable for the
        // first time, and `rts-host/tests/running.rs::an_error_says_where_it_came_from`
        // stopped finding `at made` in the trace.
        //
        // Refusing `new` costs the campaign nothing measurable: every body the
        // globals admission was built for — `Math.abs`, `JSON.stringify`,
        // `Object.keys` — constructs nothing. Weakening the test instead was the
        // alternative and is the one the honesty floor names.
        ExprKind::New { .. } => return false,

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
                return closed_over(place, bound, free, this_ok) && closed_over(value, bound, free, this_ok);
            }
            let ExprKind::Ident(name) = &place.kind else {
                return false;
            };
            let name = *name;
            // A BOUND name is written through `Scope::assign`, and the sentence
            // that used to refuse it here — "a parameter is bound to an SSA
            // value, so a write would have nowhere to land" — was false about
            // this emitter. `scope.rs`'s `assign` does
            // `entry.1 = Binding::Value(value)`: it REBINDS the SSA value, and
            // the layer it rebinds in is the one `emit_substituted` opened, not
            // the caller's. So a write to a parameter or to a body local lands
            // where the call's own frame would have put it.
            //
            // It is not a free name either way, which is why nothing is pushed:
            // the substitution declared it, so the caller's scope never sees it
            // and the whole-program proof has nothing to ask.
            //
            // This is what every accumulator needs — `let seen = 0; seen++` —
            // and it is the gate a loop in the body would have to pass through
            // before a loop could be admitted at all.
            if bound.contains(&name) {
                return closed_over(value, bound, free, this_ok);
            }
            if !free.contains(&name) {
                free.push(name);
            }
            return closed_over(value, bound, free, this_ok);
        }
        ExprKind::Update { target, .. } => {
            // `o.x++` for the reason `o.x = v` above is admitted.
            if matches!(
                &target.kind,
                ExprKind::Member { .. } | ExprKind::Index { .. }
            ) {
                return closed_over(target, bound, free, this_ok);
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
        Child::Expr(inner) => ok = ok && closed_over(inner, bound, free, this_ok),
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
pub(super) fn declarations_of(body: &[Stmt], name: Name) -> usize {
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

/// The candidate for ONE helper, built from its declaration rather than looked
/// up by name.
///
/// # Why a second door into `shape_of`
///
/// [`candidates`] is keyed by NAME over the whole program, so it must refuse a
/// spelling two functions use — `ctx.inlinable(name)` would otherwise answer
/// somebody else's body. `declarations_of(body, name) != 1` is that refusal, and
/// it is right for a map with that shape.
///
/// It is also why the pass does nothing on ordinary code. `bench/analytic.ts`
/// declares `c` four times — the closure benchmark's helper, the next
/// benchmark's, and a `for (const c of CASES)` — so the row that exists to
/// measure closure cost could not be helped by anything that asks the map.
///
/// `omit::omittable` does not need the map. It holds the declaration it is
/// reasoning about, and it proves that every call to that name is inside the
/// body being emitted — so the name is unambiguous HERE however many other
/// functions spend the same spelling elsewhere. This builds the candidate from
/// that declaration, for that body alone.
///
/// `free_proved` is false by construction: the whole-program count was never
/// taken. The site accepts it on `Ctx::omits`, which is the stronger proof and
/// the one that made this door worth opening.
pub(super) fn local_candidate(
    function: &Function,
    length: Name,
    own: Name,
    arguments: Name,
    this_ok: bool,
) -> Option<(Inlinable, Vec<Name>)> {
    let (mut candidate, free, _) = shape_of(function, length, own, this_ok)?;
    // `arguments` IS REFUSED HERE TOO, and forgetting it cost four assertions in
    // `tests/claude-arguments-fn-expr.test.ts` on the first build.
    //
    // It is the one free name no proof of LOCALITY can help with, and that is
    // what made it easy to miss: every other clause this door skips is about
    // which binding a name reaches, and locality answers those. `arguments` is
    // not a binding any scope holds — every function gets its own implicitly —
    // so a substituted body reads the CALLER's, which is a different object with
    // different contents, or none at all in an arrow. `candidates` refuses it by
    // name for exactly this reason and says so; this is the same refusal, at the
    // second door into the same shape.
    if free.iter().any(|held| *held == arguments) {
        return None;
    }
    // FALSE, and the caller decides what to do about it. `omit` answers it with
    // locality — the helper is declared in the body being emitted and called
    // only from there — and a static method has no such argument, so it takes
    // the ordinary whole-program count through `free_names_proved`.
    candidate.free_proved = false;
    Some((candidate, free))
}

/// The whole-program free-name proof, for a candidate built at the second door.
///
/// The same three arms [`candidates`] applies, in one place so the two cannot
/// drift: one declaration means no caller resolves the name differently; zero
/// means no scope binds it at all, which is stronger, provided nothing assigns
/// it; anything else is refused.
pub(super) fn free_names_proved(
    body: &[Stmt],
    free: &[Name],
    eval: Name,
    global_this: Name,
    arguments: Name,
) -> bool {
    if free.iter().any(|held| *held == arguments) {
        return false;
    }
    if free.iter().any(|held| match declarations_of(body, *held) {
        1 => false,
        0 => !super::primordial::untouched(body, *held, eval, global_this),
        _ => true,
    }) {
        return false;
    }
    free.is_empty() || super::primordial::untouched(body, eval, eval, global_this)
}
