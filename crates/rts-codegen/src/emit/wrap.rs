//! The functions that stand between a call and a body, when what the CALLER
//! receives is what makes the function special.
//!
//! Three of them, and they are one idea: `function*` must not run its body,
//! `async function` must answer a promise, `async function*` must answer
//! something the async iteration protocol can step. None of those is a fact
//! about the body — each is a fact about the DEFINITION, true at exactly one
//! site — so each is expressed by an extra function emitted there rather than
//! by a flag the calling path reads. A flag would put a branch on every
//! ordinary call in the program to express something one site knows.
//!
//! They stack, and the stacking is the semantics: an `async function*` is
//! [`generator`] (its frame parks) with [`async_generator`] on top (its object
//! is async-iterable).
//!
//! Split out of `function.rs` rather than added to it: that file is already
//! past the crate's 1000-line ceiling, and these three share a reason to exist
//! that nothing else in it shares.

use rts_cranelift::ir::{FuncBuilder, FuncId, Function as MachineFunction, ValueId};

use super::function::signature;
use super::{Ctx, EmitResult, expr};
use crate::runtime::RuntimeOp;

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
/// [`async_function`] already uses, for the same reason: what differs about these
/// functions is what the caller receives.
///
/// The body's arguments are passed on and written into the frame there, because
/// a resumed body is entered afresh with nothing in the registers it had.
pub(super) fn generator(ctx: &mut Ctx, inner: FuncId) -> EmitResult<FuncId> {
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
    // A WRAPPER is its own function, so the enclosing body's throw-flag
    // address is not a value that exists here. Taken rather than left, because
    // `expr::call` below reads it off the context and an SSA id from another
    // function is not refused by anything — it happened to name a `FuncAddr`
    // here, and every generator in the suite loaded the callee's address as if
    // it were the flag.
    let outer_flag = ctx.thrown_flag.take();

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
    ctx.thrown_flag = outer_flag;
    ctx.pending.push((id, func));
    Ok(id)
}

/// Wraps a generator's wrapper so that `async function*` answers something the
/// async iteration protocol can step.
///
/// # What an async generator is here, exactly
///
/// The frame half is already right: an `async function*` body suspends at
/// `yield` and drains at `await`, which is what [`generator`] hands to the
/// runtime. What is missing is the PROTOCOL — `for await (x of xs)` asks `xs`
/// for `[Symbol.asyncIterator]` and nothing else, and a generator object
/// declares only `[Symbol.iterator]`. So this installs the one member that
/// separates the two.
///
/// # Why the member is READ off the object rather than emitted
///
/// `[Symbol.iterator]` on a generator answers the generator itself
/// (`entry/generator.rs`'s `itself`), and `[Symbol.asyncIterator]` on an async
/// generator has to answer exactly the same thing. Emitting a second
/// "return this" function here would be that rule written twice, and the copy is
/// the one that goes stale — so the function the runtime already installed is
/// the value written under the second key.
///
/// # What this deliberately does NOT do
///
/// `next()` answers `{ value, done }` where the language says a PROMISE of one.
/// `await` on a non-thenable answers the value, so `await g.next()` and
/// `for await` both read correctly; what diverges is `g.next().then(…)`, which
/// has no `then` to call. Closing it needs the object's `next` to settle a
/// promise, which is `entry/generator.rs`'s to decide and not this layer's.
///
/// A synchronous `for (const x of asyncGen())` also still iterates, because the
/// inherited `[Symbol.iterator]` is not removed — the language says an async
/// generator has none. Removing it needs a prototype of its own, which is the
/// same file's decision.
pub(super) fn async_generator(ctx: &mut Ctx, inner: FuncId) -> EmitResult<FuncId> {
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
    // Taken for the reason [`generator`] takes it: this is its own function, and
    // an SSA value defined in another one is not a value here.
    let outer_flag = ctx.thrown_flag.take();

    // The generator object, made by the wrapper this one stacks on: the frame is
    // parked exactly as a synchronous generator's is.
    let made = builder.call(ctx.funcs, inner, &arguments)?[0];
    // `@@` is the reserved spelling of a well-known symbol's key — the same one
    // `delegate.rs` reads `@@iterator` under and `class.rs` writes a
    // `[Symbol.iterator]() {}` member under. A key minted here and one interned
    // by the runtime from a symbol's own text are the same number, because there
    // is one `KeyRegistry`.
    let named = ctx.names.intern("@@iterator");
    let key = super::property::key_constant(&mut builder, ctx, named);
    let itself = expr::call(&mut builder, ctx, RuntimeOp::GetProperty, &[made, key])?[0];
    let named = ctx.names.intern("@@asyncIterator");
    let key = super::property::key_constant(&mut builder, ctx, named);
    expr::call(&mut builder, ctx, RuntimeOp::SetProperty, &[made, key, itself])?;
    builder.ret(&[made]);
    ctx.thrown_flag = outer_flag;
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
pub(super) fn async_function(ctx: &mut Ctx, inner: FuncId) -> FuncId {
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
    // A WRAPPER is its own function, so the enclosing body's throw-flag
    // address is not a value that exists here. Taken rather than left, because
    // `expr::call` below reads it off the context and an SSA id from another
    // function is not refused by anything — it happened to name a `FuncAddr`
    // here, and every generator in the suite loaded the callee's address as if
    // it were the flag.
    let outer_flag = ctx.thrown_flag.take();
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
    ctx.thrown_flag = outer_flag;
    ctx.pending.push((id, func));
    id
}
