//! Reading and writing a name, wherever it turned out to live.
//!
//! # Why this is one module and not four matches
//!
//! Before closures there was one kind of binding, so every site that touched
//! one matched on it directly: `expr.rs` for a read and an assignment,
//! `unary.rs` for `++`, `stmt.rs` for a declaration, `loops.rs` for a `for`
//! header. Four places, and the enum's own documentation predicted what that
//! would cost — *"the moment it must distinguish value from cell, the
//! distinction goes in the entry and every reader is forced to handle it"*.
//!
//! It now must. A captured local is a property of an environment object, and a
//! read of one is a chain walk and a property access rather than a register.
//! Four copies of that would be four chances to walk the chain a different
//! number of times, and the one that walked it wrong would read a *different
//! function's* variable — a wrong program that runs.
//!
//! # What the chain is
//!
//! An environment is an ordinary object whose properties are the captured
//! names, plus one link — `__outer` — to the environment of the function that
//! made it. A function that captures nothing creates no environment at all and
//! its `__outer` is whatever it was handed.
//!
//! ```text
//!   inner's env ──__outer──▶ outer's env ──__outer──▶ …
//!      hops = 0                 hops = 1
//! ```
//!
//! `hops` is decided while compiling. Nothing at run time compares a name
//! against a chain, which is the difference between this and a scope object in
//! an interpreter.

use rts_cranelift::ir::{FuncBuilder, ValueId};

use super::expr;
use super::scope::Binding;
use super::{Ctx, EmitError, EmitResult, Scope};
use crate::names::Name;

/// The property an environment reaches its enclosing one through.
///
/// Spelled so it cannot collide with anything a program writes — and a
/// collision would be harmless anyway, because an environment object is never
/// handed to JavaScript. Named as a constant rather than written twice, since
/// the reader and the writer disagreeing about it is a chain that goes nowhere.
const OUTER: &str = "__rts_outer";

/// Reads a name.
pub fn read(
    builder: &mut FuncBuilder,
    scope: &Scope,
    ctx: &mut Ctx,
    name: Name,
) -> EmitResult<ValueId> {
    if !ctx.with_objects.is_empty() {
        return super::with_scope::read(builder, scope, ctx, name);
    }
    lexical_read(builder, scope, ctx, name)
}

/// The read a name has when no `with` encloses it.
///
/// Split from [`read`] so [`with_scope`] can use it as the LAST link of the
/// chain it builds, which is the whole of what "the object does not have it"
/// means. Written once for that reason: a second copy of the lexical rules is a
/// second chance to resolve a name differently inside a `with` than outside it.
pub(super) fn lexical_read(
    builder: &mut FuncBuilder,
    scope: &Scope,
    ctx: &mut Ctx,
    name: Name,
) -> EmitResult<ValueId> {
    if scope.in_dead_zone(name) && !ctx.in_cleanup {
        return dead_zone(builder, ctx, name);
    }
    match scope.lookup(name) {
        Some(Binding::Value(value)) => Ok(value),
        Some(Binding::InEnvironment { hops, name }) => {
            let environment = walk(builder, scope, ctx, hops)?;
            super::property::emit_read(builder, ctx, environment, name)
        }
        // Nothing declared it. A few names are still readable — three the
        // emitter produces itself and a few the runtime holds — and what is
        // left is a name the scope walk, [`predefined`] and [`super::globals::
        // resolves`] all failed to place. That is not a program being wrong:
        // it is `ReferenceError`, raised at the moment this read actually
        // runs rather than refused before the program does anything at all —
        // see [`super::globals::unbound_read`] for why the compile-time
        // refusal this used to be would have been WRONG for a read on a path
        // never taken, and for why every name this crate could have proven a
        // typo still fails right here at compile time.
        None => match predefined(builder, ctx, name) {
            Some(value) => Ok(value),
            None => match super::globals::read(builder, ctx, name) {
                Some(answered) => answered,
                None => super::globals::unbound_read(builder, ctx, name),
            },
        },
    }
}

/// Writes a name that already exists.
///
/// Answers the value the assignment produces, which for a captured name is the
/// value written rather than the binding — the same rule a property assignment
/// follows, and for the same reason: an assignment is an expression.
pub fn write(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    name: Name,
    value: ValueId,
) -> EmitResult<ValueId> {
    if !ctx.with_objects.is_empty() {
        return super::with_scope::write(builder, scope, ctx, name, value);
    }
    lexical_write(builder, scope, ctx, name, value)
}

/// The write a name has when no `with` encloses it. [`lexical_read`]'s reason.
pub(super) fn lexical_write(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    name: Name,
    value: ValueId,
) -> EmitResult<ValueId> {
    // Assigning into the zone throws for the same reason reading does: the name
    // exists lexically and has no binding to write yet. Without this the write
    // falls past `Scope::lookup` and CREATES a global, so `x = 1; let x;` would
    // publish a property nothing meant to publish.
    if scope.in_dead_zone(name) && !ctx.in_cleanup {
        return dead_zone(builder, ctx, name);
    }
    match scope.lookup(name) {
        Some(Binding::InEnvironment { hops, name }) => {
            let environment = walk(builder, scope, ctx, hops)?;
            super::property::emit_write(builder, ctx, environment, name, value)
        }
        Some(Binding::Value(_)) => {
            // A value binding is rebound rather than stored to, which is what
            // makes reading a local free. The representation a name holds is
            // decided by the analysis rather than by whichever value reached it
            // last, which is what a merge needs.
            let held = expr::stored(builder, ctx, name, value);
            if !scope.assign(name, held) {
                return Err(EmitError::UnboundName(name));
            }
            Ok(held)
        }
        // Nothing declared it, so this is either a global the program creates
        // by assigning to it — which is what sloppy mode does — or the program
        // being wrong.
        None => match super::globals::resolves(ctx, name) {
            true => super::globals::write(builder, ctx, name, value),
            false => Err(EmitError::UnboundName(name)),
        },
    }
}

/// Introduces a name, putting it wherever the capture analysis decided.
///
/// The decision is made here and only here, at the moment the name comes into
/// existence — a binding cannot be a register in the statement that declares it
/// and heap storage four statements later, when the closure that captures it is
/// written.
pub fn declare(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    name: Name,
    value: ValueId,
) -> EmitResult<()> {
    // The dead zone ends here, at the declaration itself — which is why this is
    // the one place that says so. `let x = x;` still throws: the initialiser is
    // emitted by the caller before this is reached.
    scope.initialize(name);
    if scope.is_captured(name) {
        // The binding already exists — `Scope::for_function` created it at
        // function entry, because a hoisted inner function may have read it
        // before this declaration was reached. So declaring is only the store.
        let environment = walk(builder, scope, ctx, 0)?;
        super::property::emit_write(builder, ctx, environment, name, value)?;
        return Ok(());
    }
    let held = expr::stored(builder, ctx, name, value);
    scope.declare(name, held);
    Ok(())
}

/// The environment `hops` links out from this function's own.
///
/// # Why the absent case is a defect rather than a gap
///
/// A binding says it lives in an environment, so the analysis put it there, so
/// this function has one. Reaching the `None` arm means the scope was built
/// without the environment the bindings in it refer to — which is this module's
/// own bracketing being wrong, not anything a program can express.
fn walk(builder: &mut FuncBuilder, scope: &Scope, ctx: &mut Ctx, hops: u32) -> EmitResult<ValueId> {
    let mut environment = scope
        .environment()
        .expect("a scope holding an environment binding has an environment");
    for _ in 0..hops {
        let outer = ctx.names.intern(OUTER);
        environment = super::property::emit_read(builder, ctx, environment, outer)?;
    }
    Ok(environment)
}

/// The name an environment's link to its enclosing one has.
///
/// Exposed so the code that *builds* an environment sets the same property this
/// module reads. One constant, two users, which is the whole point of it being
/// one.
pub fn outer_link(ctx: &mut Ctx) -> Name {
    ctx.names.intern(OUTER)
}

/// The three global values a program can read without a global object.
///
/// # Why these three and not a global object
///
/// `undefined`, `NaN` and `Infinity` are properties of the global object in the
/// specification, and they are the only ones that are **non-writable,
/// non-configurable constants**. Nothing a program does can change what they
/// mean, so nothing has to store them: each is a constant the emitter already
/// knows how to produce, and reading one costs an instruction rather than a
/// property lookup.
///
/// That is the whole reason this exists before a global object does. A real
/// global object is a mechanism — an object every unbound name resolves
/// against, with `globalThis`, and writes that create properties — and it
/// cannot be faked by a lookup table. These three can, because they are not
/// lookups at all.
///
/// # Why an unbound name is still refused
///
/// Reading an undeclared name is a `ReferenceError`, not `undefined` — only
/// `typeof` is exempt, and only because it takes a reference rather than a
/// value. So a name that is not one of these three stays [`EmitError::
/// UnboundName`] rather than becoming a silent `undefined`, which would turn
/// every typo into a program that runs.
///
/// `typeof undeclared` therefore still fails, and that is the honest state: it
/// needs the global object, not this.
pub(super) fn predefined(builder: &mut FuncBuilder, ctx: &mut Ctx, name: Name) -> Option<ValueId> {
    match ctx.names.text(name) {
        "undefined" => Some(expr::undefined(builder, ctx)),
        // Proven doubles, like any other number literal — so `NaN + 1` takes
        // the instruction rather than the call, and `1 / Infinity` folds the
        // way the arithmetic already does.
        "NaN" => Some(expr::number_constant(builder, f64::NAN)),
        "Infinity" => Some(expr::number_constant(builder, f64::INFINITY)),
        _ => None,
    }
}

/// Opens a fresh environment holding `names`, linked to the enclosing one.
///
/// # Why this exists rather than a second `scope.enter()`
///
/// `Scope::enter` is lexical bookkeeping: it hides a name from what follows.
/// That is enough for a binding held in a register, and not enough for one a
/// closure captured — a captured name is a PROPERTY of the function's
/// environment, keyed by its spelling, so two declarations of `e` in one
/// function are one slot however many `enter` calls stand between them. Giving
/// the inner declaration its own environment is what makes them two.
///
/// The alternative was minting a distinct property name per declaration site.
/// It was rejected because every reader would then have to agree on the minted
/// spelling — the per-iteration layer binds by the name as written, and would
/// have had to learn a second rule to stay correct.
///
/// Answers `None` when nothing here is captured: a layer no closure can observe
/// is an allocation per pass with no reader.
pub fn push_environment(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    names: &[Name],
) -> EmitResult<Option<Option<ValueId>>> {
    if names.is_empty() {
        return Ok(None);
    }
    let enclosing = scope
        .environment()
        .expect("a captured name means the function built an environment");
    // One slot per name plus the link — the same width rule `function::emit`
    // uses when it builds the activation's own environment.
    let width = builder.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
        repr: rts_cranelift::repr::Repr::I64,
        bits: rts_cranelift::ir::ScalarBits(names.len() as u64 + 1),
    });
    let width = builder.use_const(width);
    let fresh = super::expr::call(builder, ctx, crate::runtime::RuntimeOp::ObjectNew, &[width])?[0];
    let outer = outer_link(ctx);
    super::property::emit_write(builder, ctx, fresh, outer, enclosing)?;
    Ok(Some(scope.enter_environment(fresh, names)))
}

/// The `let`, `const` and `class` names a body declares at its OWN level.
///
/// One list, two readers, which is why it is a function rather than a loop
/// written twice: [`block_layer`] sizes the block's environment from it, and
/// [`Scope::expect_lexical`] arms the dead zone from it. Two traversals would be
/// two answers to "what does this block declare", and the one that disagreed
/// would either allocate a slot nothing uses or refuse a legal program.
///
/// `var` is excluded because a `var` was hoisted to the function and reading one
/// before its line is `undefined`, not an error. A `function` declaration is
/// excluded for the stronger reason: it is bound before the body runs, so it has
/// no dead zone at all.
pub fn lexical_names(body: &[crate::syntax::Stmt]) -> Vec<Name> {
    use crate::syntax::StmtKind;
    let mut names = Vec::new();
    for statement in body {
        match &statement.kind {
            StmtKind::Declare { kind, bindings } if *kind != crate::syntax::BindingKind::Var => {
                for binding in bindings {
                    binding.target.bound_names(&mut names);
                }
            }
            StmtKind::Class(class) => names.extend(class.name),
            _ => {}
        }
    }
    names
}

/// Emits a read of a name whose declaration this body has not reached yet.
///
/// # Why a `throw` here rather than a check at every read
///
/// The zone is decided while compiling — see [`Scope::in_dead_zone`] — so the
/// program that has no such read pays nothing, and the one that has it gets the
/// error unconditionally because the read cannot be anything else. The
/// alternative, a sentinel in the storage compared on every read, taxes every
/// local read in every program to serve this one.
///
/// # Why the error is constructed rather than raised by an entry point
///
/// `ReferenceError` is an ordinary constructor the program can reach, and
/// `globals::unbound_read` is deliberately NOT reused: that entry consults the
/// global object first and answers what it finds, which is right for a name
/// nothing declared and wrong here — `globalThis.x` existing does not make a
/// block's own `let x` readable before its line.
///
/// # Why a cleanup copy is exempt
///
/// The callers guard on [`Ctx::in_cleanup`]. A `finally` emitted as a CLEANUP
/// has a block shape the machine checks — `CleanupDoesNotEnd` — and a `throw`
/// is a terminator with no successor, so raising inside one produces IR the
/// verifier refuses. That is the same limit `expr::raise_if_thrown` already
/// states from the other side: nothing can throw out of a cleanup copy in this
/// engine yet, so a dead-zone read written there reads as it did before rather
/// than being refused. It is a stated divergence, not a case handled quietly.
///
/// Emission continues in a fresh block with no predecessor. Everything after a
/// `throw` in the same statement list is unreachable, and the machine's verifier
/// accepts a block with no predecessor at all — the alternative, refusing to
/// emit the rest, would mean this function deciding for its caller what a
/// terminator means.
fn dead_zone(builder: &mut FuncBuilder, ctx: &mut Ctx, name: Name) -> EmitResult<ValueId> {
    let spelling = ctx.names.text(name).to_owned();
    let reference_error = ctx.names.intern("ReferenceError");
    let constructor = super::globals::force_read(builder, ctx, reference_error)?;
    let message = super::expr::string_literal(
        builder,
        ctx,
        &format!("Cannot access '{spelling}' before initialization"),
    )?;
    let error = super::call::construct_value(builder, ctx, constructor, &[message])?;
    builder.throw(super::protect::JS_THROW, error);
    let unreached = builder.create_block();
    builder.switch_to(unreached);
    Ok(super::expr::undefined(builder, ctx))
}

/// The environment a BLOCK needs, if any of what it declares was captured.
///
/// Only the block's own level: a nested block opens its own layer when it
/// reaches it, and hoisting the inner name here would put a slot in the outer
/// record that the inner declaration then shadows anyway.
///
/// `var` is excluded because a `var` is not the block's — it was hoisted to the
/// function, and binding it here would make the block's write invisible outside
/// it. That is the same rule [`super::loops::per_iteration_names`] states.
pub fn block_layer(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    body: &[crate::syntax::Stmt],
) -> EmitResult<Option<Option<ValueId>>> {
    let mut names = lexical_names(body);
    // A hoisted function is bound before the block runs and is therefore not in
    // the dead zone — which is why it is added HERE rather than in
    // [`lexical_names`], although the environment must still hold a slot for it.
    for statement in body {
        if let crate::syntax::StmtKind::Function(function) = &statement.kind {
            names.extend(function.name);
        }
    }
    names.retain(|name| scope.is_captured(*name));
    names.dedup();
    push_environment(builder, scope, ctx, &names)
}
