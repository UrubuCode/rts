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
    match scope.lookup(name) {
        Some(Binding::Value(value)) => Ok(value),
        Some(Binding::InEnvironment { hops, name }) => {
            let environment = walk(builder, scope, ctx, hops)?;
            super::property::emit_read(builder, ctx, environment, name)
        }
        // Nothing declared it. A few names are still readable — three the
        // emitter produces itself and a few the runtime holds — and the rest is
        // the program being wrong. See [`predefined`] and [`super::globals`].
        None => match predefined(builder, ctx, name) {
            Some(value) => Ok(value),
            None => match super::globals::read(builder, ctx, name) {
                Some(answered) => answered,
                None => Err(EmitError::UnboundName(name)),
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
