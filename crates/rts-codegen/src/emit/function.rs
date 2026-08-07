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
//! function declares, or more. Fewer is padded with `undefined` at the call,
//! because the callee's parameters exist whether or not anything was passed.
//!
//! More goes in a vector the **runtime** holds, reached by `call_with_args`.
//! Not a caller-allocated stack slot, which is what a real engine hands over
//! and what this compiler cannot emit — and choosing something else *because*
//! of that would be rule 2's mistake. This is not that: where the arguments of
//! a running call live is a runtime question, the same kind as where a string's
//! text lives, and this layer says "call with these" without learning the
//! answer. The stack-slot convention is still the end state, and what it buys
//! is that the vector stops being allocated at all.
//!
//! What stays fixed at [`ARGUMENT_SLOTS`] is the **declaration**: a fifth
//! parameter has no slot to arrive in, so it is refused by name rather than
//! reading `undefined` forever.
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
use crate::syntax::{Expr, ExprKind, Function, FunctionBody, Stmt, StmtKind};

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
    emit_closure_with(builder, scope, ctx, function, None)
}

/// The same, for a body that holds `this` rather than being handed it.
///
/// One caller: a derived constructor, where `this` does not exist until
/// `super()` answers one. See [`Scope::bind_this_late`] for why that cannot be
/// the block parameter every other function's `this` is.
pub fn emit_closure_binding_this_late(
    builder: &mut FuncBuilder,
    scope: &Scope,
    ctx: &mut Ctx,
    function: &Function,
    held: Name,
) -> EmitResult<ValueId> {
    emit_closure_with(builder, scope, ctx, function, Some(held))
}

/// Both, differing only in whether `this` is held.
fn emit_closure_with(
    builder: &mut FuncBuilder,
    scope: &Scope,
    ctx: &mut Ctx,
    function: &Function,
    late_this: Option<Name>,
) -> EmitResult<ValueId> {
    let id = emit_function(ctx, scope, function, late_this)?;

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
fn emit_function(
    ctx: &mut Ctx,
    enclosing: &Scope,
    function: &Function,
    late_this: Option<Name>,
) -> EmitResult<FuncId> {
    // Two refusals and not one, because they are two constructs and the
    // measurement that ranks this crate's gaps counts by this string. Merged,
    // they were the largest single entry in that ranking and it was impossible
    // to tell which of the two it was — the corpus turned out to hold 52 async
    // functions across 15 files and 203 generators across 34, which is not the
    // split the merged number suggested.
    if function.is_generator {
        return Err(EmitError::Unsupported {
            construct: "a generator",
        });
    }
    let (parameters, mut prologue) = bind_parameters(ctx, function);
    // A rest parameter that destructures is the same desugaring as a parameter
    // that does: bind the vector to a name, then unpack it with an ordinary
    // declaration. It was refused separately, and the refusal only made sense
    // while a parameter could not be unpacked either.
    let rest = match &function.rest_parameter {
        Some(crate::syntax::Pattern::Name(name)) => Some(*name),
        Some(pattern) => {
            let held = ctx.names.intern("__rts_rest");
            prologue.push(Stmt {
                kind: StmtKind::Declare {
                    kind: crate::syntax::BindingKind::Let,
                    bindings: vec![crate::syntax::Binding {
                        target: pattern.clone(),
                        value: Some(Expr {
                            kind: ExprKind::Ident(held),
                            at: function.at,
                        }),
                        claim: None,
                    }],
                },
                at: function.at,
            });
            Some(held)
        }
        None => None,
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

    // The prologue goes FIRST, so a destructured or defaulted parameter is a
    // binding before any statement can read it. Prepended rather than emitted
    // separately, because it is ordinary code: `capture.rs` has to see these
    // declarations to decide whether a nested function captures one of them.
    let body: Vec<Stmt> = match prologue.is_empty() {
        true => body.to_vec(),
        false => prologue.into_iter().chain(body.iter().cloned()).collect(),
    };
    let body = body.as_slice();

    // An async body may contain `Await`, and the verifier refuses that
    // instruction in a function whose signature does not say so. Declared on the
    // BODY and not on the wrapper: the wrapper only calls and settles, and a
    // signature claiming a capability the function does not use would be a claim
    // nothing checked.
    let mut shape = signature();
    shape.may_suspend = function.is_async;
    let sig = ctx.funcs.declare_signature(shape);
    let id = ctx.funcs.declare_function(sig);
    let emitted = emit_body(
        ctx,
        enclosing,
        &parameters,
        body,
        function.captures_this,
        rest,
        late_this,
        &[],
        // A nested function is not a module: it has no specifier and nothing to
        // publish. Passing the enclosing module's would make every closure
        // re-publish its exports on every call.
        None,
        &[],
    )?;
    // Set on the EMITTED function and not only on the declared signature:
    // `emit_body` builds its own `Function` from `signature()`, so a flag set on
    // the registry copy alone never reached the thing the verifier reads. It
    // refused every `await` with `UndeclaredSuspension` until this line existed.
    let mut emitted = emitted;
    emitted.signature.may_suspend = function.is_async;
    ctx.pending.push((id, emitted));
    match function.is_async {
        false => Ok(id),
        true => Ok(wrap_async(ctx, id)),
    }
}

/// Wraps an async function's body so that calling it answers a promise.
///
/// # Why a wrapper and not a rewritten body
///
/// An `await` here does not park the frame — `Inst::Await` lowers to a call that
/// drains until the promise settles, which is the contract `rts-cranelift`'s own
/// signature doc states for it. So an async function runs to completion the
/// moment it is called, and the only thing that separates it from an ordinary
/// one is what the CALLER receives: a promise rather than the value.
///
/// That is a boundary, and a boundary is a wrapper. The alternative was
/// threading "this body is async" through `emit_body` and rewriting every
/// `return` inside it — more code, in the one place shared with a module's own
/// body, to express something that only happens at the edge.
///
/// # What this does NOT do, by name
///
/// A `throw` escaping the body does not reject the promise; it escapes, and with
/// nothing to catch it the program ends. Rejecting would need the body's
/// unwinding to be visible here, and `try` around a call is refused by this
/// crate anyway — so there is no shape in which the difference is observable
/// yet. It is written down because there will be.
fn wrap_async(ctx: &mut Ctx, inner: FuncId) -> FuncId {
    let sig = ctx.funcs.declare_signature(signature());
    let id = ctx.funcs.declare_function(sig);
    let mut func = MachineFunction::new(signature());
    let entry = func.entry;
    let arguments: Vec<ValueId> = func
        .block(entry)
        .expect("a function always has its entry block")
        .params
        .clone();
    let types = rts_cranelift::types::TypeRegistry::new();
    let mut builder = FuncBuilder::new(&mut func, &types, entry);
    // The body runs first and to its end. Its completion value is what the
    // promise carries, which is what `async function f() { return 1 }`
    // resolving with `1` means.
    let produced = builder
        .call(ctx.funcs, inner, &arguments)
        .map(|results| results.first().copied())
        .ok()
        .flatten();
    let promise = builder.promise_new();
    if let Some(value) = produced {
        builder.promise_settle(promise, value, false);
    }
    // Widened, because the convention returns `Tagged` and a promise is a
    // reference. The verifier caught this rather than a test: a function whose
    // declared return and actual return disagree is refused at build time, which
    // is the point of declaring it.
    let answered = builder.widen(promise);
    builder.ret(&[answered]);
    ctx.pending.push((id, func));
    id
}

/// Emits a body against the convention, with the environment chain set up.
/// `imports` is bound before the first statement and is empty for every body but
/// a module's own: an import introduces a name in the scope it is written in,
/// which is this one, and nothing downstream learns that it came from a module.
pub(super) fn emit_body(
    ctx: &mut Ctx,
    enclosing: &Scope,
    parameters: &[Name],
    body: &[Stmt],
    captures_this: bool,
    rest: Option<Name>,
    late_this: Option<Name>,
    imports: &[crate::syntax::Import],
    module: Option<&str>,
    publications: &[super::module::Publication],
) -> EmitResult<MachineFunction> {
    // An import is bound at entry and may be read from inside a nested function,
    // exactly as a parameter is — so it is a capture CANDIDATE like one. Without
    // this the name is declared as a plain local, a closure that reads it finds
    // nothing reachable, and every `describe(… test(…) …)` in the corpus reported
    // `test` as a name nothing introduces.
    let mut candidates = parameters.to_vec();
    for import in imports {
        for binding in &import.bindings {
            candidates.push(match binding {
                crate::syntax::ImportBinding::Named { local, .. } => *local,
                crate::syntax::ImportBinding::Default(local) => *local,
                crate::syntax::ImportBinding::Namespace(local) => *local,
            });
        }
    }
    let mut captured = capture::captured(body, &candidates);
    // A derived constructor holds `this` in its environment, so it has one
    // whether or not anything else is captured — the name is added here rather
    // than being special-cased below, so every reader downstream sees an
    // ordinary captured name.
    if let Some(name) = late_this {
        captured.insert(name);
    }
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
    // Escape first, because what it replaces changes what there is to prove: a
    // replaced object has no binding of its own, and its properties have
    // bindings the source never named. It takes `captured` because which names
    // nested code can see is decided once — recomputing it here would be a
    // second chance to say "not captured" about a local a closure holds.
    let flattened = super::escape::analyse(body, parameters, &captured);
    let mut numeric = super::analyse(body, &flattened);
    // The properties are proved as `(object, key)` pairs, because `proven` has
    // no `Ctx` to mint a name from. Minting them here rather than there keeps
    // the interning out of a loop that runs to convergence.
    numeric.name_fields(|object, property| super::escape::field_name(ctx, object, property));

    // Both saved and restored around the emission because each is a fact about
    // ONE body, and a nested function emitted in the middle of an outer one
    // would otherwise be read against the outer's answers.
    let outer_numeric = std::mem::replace(&mut ctx.numeric, numeric);
    let outer_flattened = std::mem::replace(&mut ctx.flattened, flattened);

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
        rest,
        late_this,
        imports,
        module,
        publications,
    );
    ctx.numeric = outer_numeric;
    ctx.flattened = outer_flattened;
    result?;
    Ok(func)
}

/// The part that holds the builder, split out so the numeric state above is
/// restored on the failing path as well as the succeeding one.
#[allow(clippy::too_many_arguments)]
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
    rest: Option<Name>,
    late_this: Option<Name>,
    imports: &[crate::syntax::Import],
    module: Option<&str>,
    publications: &[super::module::Publication],
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
        super::property::emit_write(&mut builder, ctx, fresh, outer, handed)?;
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
    if let Some(name) = late_this {
        // Seeded with what the caller passed, which for a derived constructor is
        // `undefined`. Written rather than left absent so that a read before
        // `super()` answers the same `undefined` the language's ReferenceError
        // would have been about, rather than whatever the slot happened to hold.
        binding::declare(&mut builder, &mut scope, ctx, name, incoming[THIS_PARAM])?;
        scope.bind_this_late(name);
    }

    for (position, name) in parameters.iter().enumerate() {
        binding::declare(&mut builder, &mut scope, ctx, *name, incoming[2 + position])?;
    }

    // Before the first statement, and after the parameters: an import is a
    // declaration in this scope, so it is bound where a declaration would be.
    for import in imports {
        super::module::emit_import(&mut builder, &mut scope, ctx, import)?;
    }

    // `...rest` is declared like any other parameter, from an array the runtime
    // builds — out of the vector when the caller allocated one, and out of the
    // four slots when it did not. Which of those happened is not decided here,
    // and could not be: it is a fact about the call, not about the callee.
    if let Some(name) = rest {
        let declared = builder.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
            repr: rts_cranelift::repr::Repr::I64,
            bits: rts_cranelift::ir::ScalarBits(parameters.len() as u64),
        });
        let declared = builder.use_const(declared);
        let given: Vec<ValueId> = (0..ARGUMENT_SLOTS).map(|at| incoming[2 + at]).collect();
        let mut passed = vec![declared];
        passed.extend(given);
        let gathered = expr::call(&mut builder, ctx, RuntimeOp::RestArguments, &passed)?[0];
        binding::declare(&mut builder, &mut scope, ctx, name, gathered)?;
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
    // A module publishes what it exports here: after its last statement, so the
    // value published is the one it finished with, and before the return, so it
    // happens whether or not anything reads the module's answer.
    //
    // Guarded on `terminated` because a body that returned has no reachable
    // point to emit into. A module cannot contain a top-level `return` — that is
    // a syntax error in a module — so this guard is unreachable for a real
    // module and is here because "unreachable" is a claim the emitter should not
    // have to make about IR it is building.
    if !terminated
        && let Some(specifier) = module
    {
        super::module::emit_publications(&mut builder, &scope, ctx, specifier, publications)?;
    }
    if !terminated {
        // A derived constructor answers its `this`, not `undefined`. That is
        // what `construct` takes back — it allocated nothing, so what the callee
        // returns IS the instance — and a body falling off its end is the
        // ordinary way a constructor is written.
        let answer = match late_this {
            Some(name) => binding::read(&mut builder, &scope, ctx, name)?,
            None => expr::undefined(&mut builder, ctx),
        };
        builder.ret(&[answer]);
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

/// A function's parameter list as plain names, with the statements that unpack
/// what was not one.
///
/// # Why this is a desugaring and not an emission
///
/// `function f({a}, b = 1)` means exactly `function f(p0, p1) { let {a} = p0;
/// let b = p1 === undefined ? 1 : p1; … }`. Both halves already emit: a
/// destructuring declaration is `destructure.rs`, and a conditional is
/// `choice.rs`. Writing a second unpacker here that emitted patterns against
/// parameter slots would be the same rule stated twice, which rule 3 forbids —
/// and the two would eventually disagree about a hole, a nested default, or a
/// rest element.
///
/// # The two things the language decides here
///
/// A default fires on `undefined` and NOT on `null`, which is why the test is
/// `=== undefined` rather than a truthiness or a nullish check. And a default is
/// evaluated at the CALL rather than once at the declaration, which is what
/// keeping it as an expression in the prologue gives for free — `f()` twice with
/// `b = []` must not share one array.
fn bind_parameters(ctx: &mut Ctx, function: &Function) -> (Vec<Name>, Vec<Stmt>) {
    let mut names = Vec::with_capacity(function.parameters.len());
    let mut prologue = Vec::new();
    for (position, parameter) in function.parameters.iter().enumerate() {
        // A plain name with no default is already what the convention wants, so
        // it costs nothing: no synthetic name, no prologue statement, and the
        // common case stays exactly the code it was before this existed.
        if let (crate::syntax::Pattern::Name(name), None) = (&parameter.target, &parameter.default) {
            names.push(*name);
            continue;
        }
        // Not a name a program can write: a parameter called `__rts_param_0`
        // would otherwise shadow the slot it is being unpacked from.
        let held = ctx.names.intern(&format!("__rts_param_{position}"));
        names.push(held);
        let read = Expr {
            kind: ExprKind::Ident(held),
            at: function.at,
        };
        let value = match &parameter.default {
            None => read,
            Some(default) => Expr {
                kind: ExprKind::Conditional {
                    condition: Box::new(Expr {
                        kind: ExprKind::Binary {
                            op: crate::syntax::BinaryOp::StrictEqual,
                            left: Box::new(read.clone()),
                            right: Box::new(Expr {
                                kind: ExprKind::Ident(ctx.names.intern("undefined")),
                                at: function.at,
                            }),
                        },
                        at: function.at,
                    }),
                    then_branch: Box::new(default.clone()),
                    else_branch: Box::new(read),
                },
                at: function.at,
            },
        };
        prologue.push(Stmt {
            kind: StmtKind::Declare {
                // `let` and not `var`: a parameter is scoped to the function
                // body, and hoisting one to the top would make a name declared
                // in an inner block collide with it.
                kind: crate::syntax::BindingKind::Let,
                bindings: vec![crate::syntax::Binding {
                    target: parameter.target.clone(),
                    value: Some(value),
                    claim: parameter.claim.clone(),
                }],
            },
            at: function.at,
        });
    }
    (names, prologue)
}
