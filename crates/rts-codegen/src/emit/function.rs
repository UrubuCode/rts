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

use super::{Ctx, EmitResult, Loops, Scope, UNPROVEN};
use super::{binding, capture, expr};
use crate::names::Name;
use crate::runtime::{ARGUMENT_SLOTS, RuntimeOp};
use crate::syntax::{
    BindingKind, Expr, ExprKind, ForEachTarget, ForInit, Function, FunctionBody, Stmt, StmtKind,
};

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
    // An ARROW reads `this` as a NAME, from the environment the enclosing
    // function put it in. `Scope::late_this` is the mechanism — it already
    // exists for a derived constructor, where `this` is also a name rather than
    // the receiver — so nothing new is invented here, only pointed at a second
    // case.
    let late_this = match function.captures_this {
        // Only when the enclosing function actually handed one over. It is
        // reachable exactly when that function declared it, which it does when a
        // walk found an arrow inside it reading `this` — so asking the scope is
        // asking the same question from the other side, and it cannot answer
        // yes where the declaration did not happen.
        //
        // Setting it unconditionally made every arrow take the name path,
        // including the ones in bodies where nothing was declared, and 739 files
        // failed with `Unbound("__rts_this")`.
        true => {
            let held = ctx.names.intern("__rts_this");
            scope
                .reachable()
                .iter()
                .any(|(name, _)| *name == held)
                .then_some(held)
        }
        false => late_this,
    };
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
    // A generator is no longer refused here. What is still refused is `yield*`,
    // and it is refused where it is emitted rather than by a pre-pass over the
    // body: it forwards `next`, `throw` and `return` to an inner iterator, which
    // is a loop over the iteration protocol rather than a suspension. See
    // `expr::yielded`.
    let (parameters, mut prologue, parameter_claims) = bind_parameters(ctx, function);
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
    shape.may_suspend = function.is_async || function.is_generator;
    let sig = ctx.funcs.declare_signature(shape);
    let id = ctx.funcs.declare_function(sig);
    let emitted = emit_body(
        ctx,
        enclosing,
        &parameters,
        &parameter_claims,
        body,
        function.captures_this,
        rest,
        late_this,
        // A function EXPRESSION binds its own name for its own body; a
        // declaration binds it outside too, and binding it inside as well is
        // what the language says and is harmless. Passed here rather than
        // decided inside, because only this level has the tree.
        function.name,
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
    emitted.signature.may_suspend = function.is_async || function.is_generator;
    // The name, for a stack trace. Recorded here rather than derived later:
    // this is the one place that has both the identifier and the tree it came
    // from, and a name recovered from anywhere else would be a second answer.
    if let Some(name) = function.name {
        let text = ctx.names.text(name).to_owned();
        ctx.function_names.push((id, text));
    }
    ctx.pending.push((id, emitted));
    if function.is_generator {
        // The body is not called here and is not called by the caller either:
        // the wrapper hands its ADDRESS to the runtime, which parks a frame
        // against it. `ctx.generators` is what tells the host to put this
        // function through `resumable_form` before placing it.
        ctx.generators.push(id);
        return wrap_generator(ctx, id);
    }
    match function.is_async {
        false => Ok(id),
        true => Ok(wrap_async(ctx, id)),
    }
}

/// Wraps a generator's body so that calling it answers a generator object.
///
/// # Why a wrapper, and not a flag the call site reads
///
/// Calling a generator function must not run its body, and the tempting fix is
/// to mark the closure and have the runtime's `invoke` check the mark before
/// every jump. That puts a branch on the path of every ordinary call in the
/// program to express something that is true at ONE site: the definition.
///
/// So the definition is where it is expressed. The wrapper is an ordinary
/// function under the ordinary convention — the closure, the call and the caller
/// are all unchanged — and what it does instead of calling the body is hand its
/// ADDRESS to the runtime, which parks a frame against it. This is the shape
/// [`wrap_async`] already uses, for the same reason: what differs about these
/// functions is what the caller receives.
///
/// The body's arguments are passed on and written into the frame there, because
/// a resumed body is entered afresh with nothing in the registers it had.
fn wrap_generator(ctx: &mut Ctx, inner: FuncId) -> EmitResult<FuncId> {
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

    // The address of the body as it will be PLACED, which is the rewritten form
    // — the host redeclares this identifier once the rewrite has told it the
    // frame's shape. Taking the address here rather than after is what lets the
    // wrapper be emitted in one pass with everything else.
    let code = builder.func_addr(ctx.funcs, inner)?;
    let mut operands = Vec::with_capacity(1 + arguments.len());
    operands.push(code);
    operands.extend(arguments);
    let made = expr::call(&mut builder, ctx, RuntimeOp::GeneratorNew, &operands)?[0];
    builder.ret(&[made]);
    ctx.pending.push((id, func));
    Ok(id)
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
/// `parameter_claims` is what the parameters were annotated with, which the
/// tree cannot be asked for here: a plain name with no default binds directly at
/// entry and has no `Binding` to carry its claim, so `bind_parameters` is the
/// only place it can be read. Empty for the two module-level callers, which have
/// no parameter list to annotate.
pub(super) fn emit_body(
    ctx: &mut Ctx,
    enclosing: &Scope,
    parameters: &[Name],
    parameter_claims: &[(Name, crate::syntax::Claim)],
    body: &[Stmt],
    captures_this: bool,
    rest: Option<Name>,
    late_this: Option<Name>,
    self_name: Option<Name>,
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
    // `...rest` is bound the same way a parameter is — directly, at entry,
    // never through a statement `declared_by_statement` would see — so a
    // closure reading it needs the same listing a parameter gets. Absent
    // here, `values` in `strings.reduce((acc, s, i) => …values[i]…)` inside
    // a function declared `(...values)` was a mention nothing counted as
    // captured, and the arrow found no slot for it: `Unbound("values")`.
    if let Some(name) = rest {
        candidates.push(name);
    }
    for import in imports {
        for binding in &import.bindings {
            candidates.push(match binding {
                crate::syntax::ImportBinding::Named { local, .. } => *local,
                crate::syntax::ImportBinding::Default(local) => *local,
                crate::syntax::ImportBinding::Namespace(local) => *local,
            });
        }
    }
    // `arguments` the same way, and for the same reason one line up: it is bound
    // at entry and an ARROW inside reads the enclosing function's. Without it in
    // the candidate list the arrow finds nothing reachable — which is exactly
    // what happened, and the identical fault the import line above records.
    // A NAMED FUNCTION EXPRESSION binds its own name for its own body. Added as
    // a capture candidate for the same reason a parameter is: a nested function
    // may read it, and a name nothing lists as a candidate is declared as a
    // plain local that no closure can reach.
    //
    // Only when the body mentions it, so the ordinary function pays nothing —
    // the binding costs a call, and a function that never names itself has no
    // use for one.
    let self_name = self_name.filter(|name| capture::mentions(body, *name));
    if let Some(name) = self_name {
        candidates.push(name);
    }
    let named_arguments = ctx.names.intern("arguments");
    let binds_arguments = !captures_this && capture::mentions(body, named_arguments);
    if binds_arguments {
        candidates.push(named_arguments);
    }
    // `this` for the arrows inside, under a name a program cannot write. An
    // arrow takes `this` from where it was written rather than from how it is
    // called, so the only function that can answer is this one — and it answers
    // by handing it over as an ordinary captured name.
    let held_this = ctx.names.intern("__rts_this");
    let hands_this_to_an_arrow = !captures_this && capture::arrow_reads_this(body);
    if hands_this_to_an_arrow {
        candidates.push(held_this);
    }
    let mut captured = capture::captured(body, &candidates);
    // A name this module PUBLISHES is read by the publication, which is emitted
    // after the body and is not a statement the body's own walks can see. So it
    // is forced in here, exactly as `rest` and the imports are one screen up and
    // for the identical reason: a mention the analysis cannot find is still a
    // read.
    //
    // What it costs to leave out: the escape analysis flattens `const x = { a:
    // 1 }` into plain locals when nothing makes the object escape, and a
    // publication was nothing. So `export default { n: 1 }` dissolved the object,
    // left `@@default` bound to nowhere, and the publication read it as a GLOBAL
    // — the module died with `ReferenceError: @@default is not defined`, and an
    // importer saw `cannot resolve module`, naming the wrong problem entirely.
    //
    // Only a literal with no method reached it: one containing a function is not
    // flattenable, which is why every shim in a certain project's `compat/`
    // directory worked and the one written as `{ createAppAt }` did not.
    for publication in publications {
        if let super::module::PublicationSource::Local(name) = &publication.source {
            captured.insert(*name);
        }
    }
    // FORCED into the environment rather than left to the analysis. The arrow
    // reads it through `Scope::late_this`, not through an `Ident`, so nothing
    // the walk can see mentions the name — and a captured set derived from
    // mentions alone would leave the arrow with nothing reachable.
    if hands_this_to_an_arrow {
        captured.insert(held_this);
    }
    // A derived constructor holds `this` in its own environment, so it has one
    // whether or not anything else is captured — the name is added here rather
    // than being special-cased below, so every reader downstream sees an
    // ordinary captured name.
    //
    // `!captures_this` is what keeps that from applying to an ARROW, which also
    // arrives with a `late_this` and does NOT own it. Inserting there put the
    // name in the arrow's own environment at zero hops, shadowing the enclosing
    // function's slot with one nothing ever writes — so every `this` inside an
    // arrow read `undefined` while looking like it worked.
    if let Some(name) = late_this
        && !captures_this
    {
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
    // Beside the proof and AFTER it, because a claim about a name the body
    // already proved is not a second opinion — it is nothing to speculate
    // about, and `analyse` drops it so `Ctx::holds_number` stays the one
    // answer to whether a name holds a number.
    let claims = super::types::analyse(body, parameter_claims, &numeric);
    // The count of what a PROOF made redundant is the difference, and it is
    // taken here because `Facts` no longer holds what it dropped.
    let seeded = parameter_claims.len();
    let redundant = seeded.saturating_sub(claims.len());
    ctx.census.record(&claims, seeded, redundant);
    ctx.census.record(&claims, parameter_claims.len(), 0);

    let outer_numeric = std::mem::replace(&mut ctx.numeric, numeric);
    let outer_claims = std::mem::replace(&mut ctx.claims, claims);
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
        self_name,
        imports,
        module,
        publications,
    );
    ctx.numeric = outer_numeric;
    ctx.flattened = outer_flattened;
    ctx.claims = outer_claims;
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
    self_name: Option<Name>,
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
        // The names this function's inner functions capture, plus the link to
        // the environment outside. This is the count that mattered: a module's
        // scope carries every binding at its top level, and analytic.ts has
        // thirty-two of them against a cell's fifteen — so every read past the
        // fifteenth missed its cache, forever.
        let width = builder.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
            repr: rts_cranelift::repr::Repr::I64,
            bits: rts_cranelift::ir::ScalarBits(captured.len() as u64 + 1),
        });
        let width = builder.use_const(width);
        let fresh = expr::call(&mut builder, ctx, RuntimeOp::ObjectNew, &[width])?[0];
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
        // DECLARED only where this function owns the name. A derived constructor
        // does: `this` does not exist in one until `super()` returns, so the
        // slot is seeded with what the caller passed — `undefined` — rather than
        // left absent, so a read before `super()` answers the same `undefined`
        // the language's ReferenceError would have been about.
        //
        // An ARROW does NOT. It borrows the enclosing function's, and declaring
        // here would seed a fresh local from the arrow's own receiver and shadow
        // the one it came for — which is what it did, and every `this` inside an
        // arrow read `undefined` while looking like it worked.
        if !captures_this {
            binding::declare(&mut builder, &mut scope, ctx, name, incoming[THIS_PARAM])?;
        }
        scope.bind_this_late(name);
    }

    // A named function expression binds its OWN name, BEFORE the parameters.
    //
    // The order is the semantics and it is easy to get backwards: the language
    // puts the function's name in a scope that ENCLOSES the parameter scope, so
    // `function f(f) { return f; }` answers the parameter. Declared after them,
    // it overwrote the parameter instead — `shadow(7)` answered the function.
    //
    // The value is the function currently running rather than anything visible
    // here: `const g = function f() { … f() … }` must reach the function itself,
    // not `g`, which may be reassigned and may not exist.
    if let Some(name) = self_name {
        let running = expr::call(&mut builder, ctx, RuntimeOp::RunningFunction, &[])?[0];
        binding::declare(&mut builder, &mut scope, ctx, name, running)?;
    }

    // The first four come from the convention's slots, which is the whole of the
    // common case and costs nothing.
    for (position, name) in parameters.iter().take(ARGUMENT_SLOTS).enumerate() {
        binding::declare(&mut builder, &mut scope, ctx, *name, incoming[2 + position])?;
    }
    // A FIFTH parameter and beyond has no slot to arrive in, and this used to be
    // refused for that. It does not need one: a call passing more than four
    // arguments already builds a vector and goes through `CallWithArgs`, and
    // `rest_arguments` is what reads that vector back — so the extra parameters
    // are read out of the same array `arguments` is, by position.
    //
    // A caller that passed fewer builds no vector, `rest_arguments` falls back
    // to the four slots, and the read answers `undefined` — which is exactly
    // what a parameter nothing was passed for holds.
    if parameters.len() > ARGUMENT_SLOTS {
        let from = builder.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
            repr: rts_cranelift::repr::Repr::I64,
            bits: rts_cranelift::ir::ScalarBits(0),
        });
        let from = builder.use_const(from);
        let given: Vec<ValueId> = (0..ARGUMENT_SLOTS).map(|at| incoming[2 + at]).collect();
        let mut passed = vec![from];
        passed.extend(given);
        let all = expr::call(&mut builder, ctx, RuntimeOp::RestArguments, &passed)?[0];
        for (position, name) in parameters.iter().enumerate().skip(ARGUMENT_SLOTS) {
            let at = expr::number_constant(&mut builder, position as f64);
            let value = expr::call(&mut builder, ctx, RuntimeOp::GetIndexed, &[all, at])?[0];
            binding::declare(&mut builder, &mut scope, ctx, *name, value)?;
        }
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

    // `arguments`, when the body mentions it and this function has any of its
    // own. An ARROW has none — it sees the enclosing function's, which is why
    // that one binds it as an ordinary local and the capture analysis carries it
    // in like any other name.
    //
    // What this answers is an ARRAY, where the language says an arguments
    // exotic object. `length` and indexing are what a program reads and they
    // agree; `Array.isArray(arguments)` is `true` here and `false` in a real
    // engine, and `arguments.callee` does not exist. Named rather than papered
    // over: the exotic object needs a kind of cell this runtime does not have.
    // `this` handed to the arrows inside, declared before anything can read it.
    // The value is the receiver this function was called with, which is exactly
    // what an arrow written here should see.
    if !captures_this && capture::arrow_reads_this(body) {
        let held_this = ctx.names.intern("__rts_this");
        let receiver = incoming[THIS_PARAM];
        binding::declare(&mut builder, &mut scope, ctx, held_this, receiver)?;
    }

    let named = ctx.names.intern("arguments");
    if !captures_this && capture::mentions(body, named) {
        {
            let from = builder.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
                repr: rts_cranelift::repr::Repr::I64,
                bits: rts_cranelift::ir::ScalarBits(0),
            });
            let from = builder.use_const(from);
            let given: Vec<ValueId> = (0..ARGUMENT_SLOTS).map(|at| incoming[2 + at]).collect();
            let mut passed = vec![from];
            passed.extend(given);
            let all = expr::call(&mut builder, ctx, RuntimeOp::RestArguments, &passed)?[0];
            binding::declare(&mut builder, &mut scope, ctx, named, all)?;
        }
    }

    // Every `var` in the whole body, wherever it is written, exists as
    // `undefined` from here — before the function-declaration hoist below,
    // which is per-block rather than whole-body, and before the first
    // statement, which is what makes reading one before its own `var` line
    // answer `undefined` instead of refusing to compile.
    hoist_vars(&mut builder, &mut scope, ctx, body)?;
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

/// Every name a `var` introduces anywhere in a function body.
///
/// # Why this is a second traversal rather than `capture`'s shared one
///
/// `var` reaches the enclosing FUNCTION, through a block, an `if`, a loop, a
/// `try`, a `switch`, a label and `with` — but not through a nested function or
/// class, which owns its own `var` scope. `capture::walk_stmt`'s own doc says
/// why it cannot answer this alone: it deliberately drops the `BindingKind` off
/// a `Declare`'s bindings and is silent about a `for`-each target, because its
/// two existing callers want different things from both — and a third caller
/// wanting a third thing is not a reason to grow the shared shape, it is
/// `sloppy.rs`'s reason for its own copy, restated.
///
/// # Why declaration order does not matter here
///
/// Unlike [`Pattern::bound_names`], which orders initialisers for their side
/// effects, this just needs the *set* of spellings that must exist as
/// `undefined` before the first statement runs — so duplicates across two
/// `var x` in different blocks collapse for free at the call site, which
/// declares each name once.
fn collect_vars(body: &[Stmt], into: &mut Vec<Name>) {
    for statement in body {
        collect_vars_stmt(statement, into);
    }
}

/// One statement's contribution to [`collect_vars`].
fn collect_vars_stmt(statement: &Stmt, into: &mut Vec<Name>) {
    match &statement.kind {
        StmtKind::Declare {
            kind: BindingKind::Var,
            bindings,
        } => {
            for binding in bindings {
                binding.target.bound_names(into);
            }
        }
        StmtKind::Declare { .. } => {}
        StmtKind::Block(body) => collect_vars(body, into),
        StmtKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            collect_vars_stmt(then_branch, into);
            if let Some(otherwise) = else_branch {
                collect_vars_stmt(otherwise, into);
            }
        }
        StmtKind::While { body, .. } | StmtKind::DoWhile { body, .. } => {
            collect_vars_stmt(body, into)
        }
        StmtKind::For { init, body, .. } => {
            if let Some(ForInit::Declare {
                kind: BindingKind::Var,
                bindings,
            }) = init
            {
                for binding in bindings {
                    binding.target.bound_names(into);
                }
            }
            collect_vars_stmt(body, into);
        }
        StmtKind::ForEach { target, body, .. } => {
            if let ForEachTarget::Declare {
                kind: BindingKind::Var,
                target,
            } = target
            {
                target.bound_names(into);
            }
            collect_vars_stmt(body, into);
        }
        StmtKind::Labelled { body, .. } | StmtKind::With { body, .. } => {
            collect_vars_stmt(body, into)
        }
        StmtKind::Switch { clauses, .. } => {
            for clause in clauses {
                for inner in &clause.body {
                    collect_vars_stmt(inner, into);
                }
            }
        }
        StmtKind::Try {
            body,
            catch,
            finally,
        } => {
            for inner in body {
                collect_vars_stmt(inner, into);
            }
            if let Some(catch) = catch {
                for inner in &catch.body {
                    collect_vars_stmt(inner, into);
                }
            }
            if let Some(finally) = finally {
                for inner in finally {
                    collect_vars_stmt(inner, into);
                }
            }
        }
        // A function or class declaration owns its own `var` scope; `Function`
        // is hoisted separately, by `hoist`, and neither introduces a name into
        // THIS function's. Every other kind — `Expr`, `Return`, `Break`,
        // `Continue`, `Throw`, `Debugger`, `Empty`, `Using` (its binding always
        // behaves as `const`) — introduces no `var`.
        _ => {}
    }
}

/// Declares every `var` in the body as `undefined`, before anything runs.
///
/// # Why this and [`hoist`] are not one function
///
/// A `var` is hoisted ONCE, to the function's own top layer, over the WHOLE
/// body — reading `x` before `var x = 5` anywhere in the function answers
/// `undefined`, not a compile-time refusal. A function declaration is hoisted
/// PER BLOCK, so a nested one can be mutually recursive with a sibling — see
/// `hoist`'s own doc. Folding the two into one traversal would have to carry
/// both rules through one walk, which is the rule-stated-twice failure rule 3
/// warns about, just moved one level down.
///
/// Declared directly into the current (function) layer, which is why this must
/// run before [`Scope::enter`] is called for anything — a `var` inside a block
/// still has to land outside it.
pub fn hoist_vars(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    body: &[Stmt],
) -> EmitResult<()> {
    let mut names = Vec::new();
    collect_vars(body, &mut names);
    for name in names {
        // A name declared twice — `var x; var x;`, or one in each of two
        // sibling blocks — collapses here: `Scope::declare` pushes an entry
        // every time it is called, and a second `undefined` for the same
        // spelling would shadow the first in the SAME layer, which is a
        // different bug from the one this function fixes. Checking first is
        // cheap on a per-function list.
        if scope.lookup(name).is_none() {
            let value = expr::undefined(builder, ctx);
            binding::declare(builder, scope, ctx, name, value)?;
        }
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
/// The third value is the claims. A plain name with no default is
/// short-circuited below and pushes NO prologue statement, so its annotation
/// exists only on the `Parameter` and never reaches a `Binding` — which is
/// where every other claim in a body is read from. Handing it back here is the
/// only place it can be seen.
fn bind_parameters(
    ctx: &mut Ctx,
    function: &Function,
) -> (Vec<Name>, Vec<Stmt>, Vec<(Name, crate::syntax::Claim)>) {
    let mut names = Vec::with_capacity(function.parameters.len());
    let mut prologue = Vec::new();
    let mut claims = Vec::new();
    for (position, parameter) in function.parameters.iter().enumerate() {
        // A plain name with no default is already what the convention wants, so
        // it costs nothing: no synthetic name, no prologue statement, and the
        // common case stays exactly the code it was before this existed.
        if let (crate::syntax::Pattern::Name(name), None) = (&parameter.target, &parameter.default) {
            names.push(*name);
            if let Some(claim) = &parameter.claim {
                claims.push((*name, claim.clone()));
            }
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
    (names, prologue, claims)
}
