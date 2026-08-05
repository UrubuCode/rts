//! A function: its own body of machine code, and the value that reaches it.
//!
//! # The convention, and why it is fixed
//!
//! Every compiled JavaScript function has one shape:
//!
//! ```text
//! (environment, this, a0, a1, a2, a3) -> value
//! ```
//!
//! JavaScript's arity is dynamic — a call may pass fewer arguments than the
//! function declares, or more — and expressing that needs somewhere to put an
//! argument vector. A caller-allocated one is a stack slot this compiler does
//! not emit, so the count is fixed at [`ARGUMENT_SLOTS`], missing arguments are
//! filled with `undefined` at the call, and a call with more is refused **by
//! name**. Refused rather than truncated: a call whose fifth argument silently
//! vanished is a wrong program that runs.
//!
//! Both extra parameters earn their place. `environment` is what makes a
//! closure a closure; `this` cannot be one of the arguments because
//! `f(a)` and `o.f(a)` pass the same arguments and differ only in it.
//!
//! # Why the body is a second `Function` and not an inlined block
//!
//! Because a function is called, possibly from somewhere that does not exist
//! yet, and possibly more than once. Inlining at the point of definition would
//! be a compiler deciding a program's structure from its syntax — and it would
//! be wrong the first time a closure escaped.
//!
//! # What the environment is
//!
//! An ordinary object whose properties are the captured names, plus a link to
//! the environment of the function that created it. Ordinary on purpose: it
//! reuses the shape tree, the property caches and the allocator rather than
//! introducing a second kind of storage with its own layout rules.
//!
//! Two closures made in one activation are handed the **same** object, which is
//! what makes them share the variable — and that is the whole observable content
//! of a closure, so it is what the tests pin.

use rts_cranelift::ir::{FuncBuilder, FuncId, Function as MachineFunction, Signature, ValueId};

use super::{Ctx, EmitError, EmitResult, Loops, Scope, UNPROVEN};
use super::{binding, capture, expr};
use crate::names::Name;
use crate::runtime::{ARGUMENT_SLOTS, RuntimeOp};
use crate::syntax::{Function, FunctionBody, Stmt, StmtKind};

/// Which entry-block parameter is the environment.
pub const ENVIRONMENT_PARAM: usize = 0;

/// Which is the receiver.
pub const THIS_PARAM: usize = 1;

/// How many parameters a compiled function has in total.
const TOTAL_PARAMS: usize = 2 + ARGUMENT_SLOTS;

/// The shape every compiled JavaScript function has.
///
/// Every parameter and the return are `Tagged`, because that is what a
/// JavaScript function is at the boundary: a caller cannot know what it is
/// handing over and a callee cannot know what it will get back. A signature
/// claiming anything narrower would be a claim about the program that nothing
/// proved.
pub fn signature() -> Signature {
    Signature {
        params: vec![UNPROVEN; TOTAL_PARAMS],
        returns: vec![UNPROVEN],
        ..Signature::default()
    }
}

/// Emits a function, and answers the callable value at the site that wrote it.
///
/// Two things happen and they are in different functions: the body becomes its
/// own machine function, added to what this compilation will place, and *here*
/// a closure is made from that function's address and the environment currently
/// in scope.
pub fn emit_closure(
    builder: &mut FuncBuilder,
    scope: &Scope,
    ctx: &mut Ctx,
    function: &Function,
) -> EmitResult<ValueId> {
    let id = emit_function(ctx, scope, function)?;

    // The address is not a number known here — it is a relocation the
    // destination fills in, which is the whole reason the machine had to grow
    // `FuncAddr` before any of this could be written.
    let code = builder.func_addr(ctx.funcs, id)?;

    // What the function closes over is the environment of whoever is defining
    // it. A function defined where nothing is captured closes over nothing, and
    // `undefined` is what it is handed — reading a name through it would be a
    // defect in the analysis rather than something to guard against here.
    let environment = match scope.environment() {
        Some(environment) => environment,
        None => expr::undefined(builder, ctx),
    };
    Ok(expr::call(builder, ctx, RuntimeOp::ClosureNew, &[code, environment])?[0])
}

/// Emits a function's body as a machine function, and answers its id.
fn emit_function(ctx: &mut Ctx, enclosing: &Scope, function: &Function) -> EmitResult<FuncId> {
    if function.is_async || function.is_generator {
        return Err(EmitError::Unsupported {
            construct: "an async function or a generator",
        });
    }
    if function.rest_parameter.is_some() {
        return Err(EmitError::Unsupported {
            construct: "a rest parameter",
        });
    }
    let Some(parameters) = capture::plain_parameters(&function.parameters) else {
        return Err(EmitError::Unsupported {
            construct: "a destructured or defaulted parameter",
        });
    };
    if parameters.len() > ARGUMENT_SLOTS {
        return Err(EmitError::Unsupported {
            construct: "a function of more than four parameters",
        });
    }

    // A concise arrow body is `x => e`, which produces `e`. Wrapped in a
    // synthetic `return` here rather than in the parser, because the tree
    // deliberately records which was written — `x => ({a: 1})` needs its
    // parentheses precisely because a `{` would have been a block, and a
    // rewrite at parse time loses that distinction for every later reader.
    let synthesised;
    let body: &[Stmt] = match &function.body {
        FunctionBody::Block(body) => body,
        FunctionBody::Expression(value) => {
            synthesised = [Stmt {
                kind: StmtKind::Return(Some((**value).clone())),
                at: function.at,
            }];
            &synthesised
        }
    };

    let sig = ctx.funcs.declare_signature(signature());
    let id = ctx.funcs.declare_function(sig);
    let emitted = emit_body(ctx, enclosing, &parameters, body, function.captures_this)?;
    ctx.pending.push((id, emitted));
    Ok(id)
}

/// Emits a body against the convention, with the environment chain set up.
pub(super) fn emit_body(
    ctx: &mut Ctx,
    enclosing: &Scope,
    parameters: &[Name],
    body: &[Stmt],
    captures_this: bool,
) -> EmitResult<MachineFunction> {
    let captured = capture::captured(body, parameters);
    let builds_environment = !captured.is_empty();

    let mut func = MachineFunction::new(signature());
    let entry = func.entry;
    let incoming = func
        .block(entry)
        .expect("a function always has its entry block")
        .params
        .clone();

    // What this body proved about its own locals. Saved and restored around the
    // emission because it is a fact about ONE body, and a nested function
    // emitted in the middle of an outer one would otherwise be read against the
    // outer's answers.
    let outer_numeric = std::mem::replace(&mut ctx.numeric, super::analyse(body));

    let result = emit_body_into(
        ctx,
        enclosing,
        parameters,
        body,
        captures_this,
        &mut func,
        entry,
        &incoming,
        captured,
        builds_environment,
    );
    ctx.numeric = outer_numeric;
    result?;
    Ok(func)
}

/// The part that holds the builder, split out so the numeric state above is
/// restored on the failing path as well as the succeeding one.
#[allow(clippy::too_many_arguments)]
fn emit_body_into(
    ctx: &mut Ctx,
    enclosing: &Scope,
    parameters: &[Name],
    body: &[Stmt],
    captures_this: bool,
    func: &mut MachineFunction,
    entry: rts_cranelift::ir::BlockId,
    incoming: &[ValueId],
    captured: std::collections::BTreeSet<Name>,
    builds_environment: bool,
) -> EmitResult<()> {
    let types = ctx.types;
    let mut builder = FuncBuilder::new(func, types, entry);

    let handed = incoming[ENVIRONMENT_PARAM];
    // A function that captures nothing adds no link to the chain, so what it
    // passes on is exactly what it was handed — which is why the hop counts
    // below only grow for a function that builds one.
    let environment = if builds_environment {
        let fresh = expr::call(&mut builder, ctx, RuntimeOp::ObjectNew, &[])?[0];
        let outer = binding::outer_link(ctx);
        expr::emit_write(&mut builder, ctx, fresh, outer, handed)?;
        fresh
    } else {
        handed
    };

    // Everything the enclosing functions put in their environments, one link
    // further away for each environment between here and there. A function that
    // builds none adds no link, so what it passes on is what it was handed.
    let further = u32::from(builds_environment);
    let reachable: Vec<_> = enclosing
        .reachable()
        .into_iter()
        .map(|(name, hops)| (name, hops + further))
        .collect();

    let mut scope = Scope::for_function(Some(environment), captured, &reachable);
    scope.set_this(incoming[THIS_PARAM], captures_this);

    for (position, name) in parameters.iter().enumerate() {
        binding::declare(&mut builder, &mut scope, ctx, *name, incoming[2 + position])?;
    }

    hoist(&mut builder, &mut scope, ctx, body)?;

    let mut loops = Loops::default();
    let mut terminated = false;
    for statement in body {
        if super::emit_stmt(&mut builder, &mut scope, ctx, &mut loops, statement)? {
            terminated = true;
            break;
        }
    }
    if !terminated {
        let undefined = expr::undefined(&mut builder, ctx);
        builder.ret(&[undefined]);
    }
    Ok(())
}

/// Binds every function declared directly in a body, before the body runs.
///
/// # Why this is not just emitting the declaration where it was written
///
/// `function f() { return f(); }` reads `f` inside `f`, and mutual recursion
/// reads the second function before the first has been written. Both are
/// ordinary JavaScript, and both are the reason declarations are hoisted rather
/// than evaluated in order.
///
/// Hoisting is done per block rather than to the top of the function, which is
/// not quite what the specification says for `var`-like function scoping. It is
/// what makes the cases above work, and the difference shows only for a
/// declaration inside a nested block referenced before that block — named here
/// so the gap is a sentence rather than a surprise.
pub fn hoist(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    body: &[Stmt],
) -> EmitResult<()> {
    // Two passes, and the first one is the whole point: every name is bound
    // before any body is emitted, so a closure made in the second pass can
    // already see the ones declared after it.
    for statement in body {
        if let StmtKind::Function(function) = &statement.kind {
            let Some(name) = function.name else {
                continue;
            };
            let placeholder = expr::undefined(builder, ctx);
            binding::declare(builder, scope, ctx, name, placeholder)?;
        }
    }
    for statement in body {
        if let StmtKind::Function(function) = &statement.kind {
            let Some(name) = function.name else {
                continue;
            };
            let closure = emit_closure(builder, scope, ctx, function)?;
            binding::write(builder, scope, ctx, name, closure)?;
        }
    }
    Ok(())
}
