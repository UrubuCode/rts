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
//! [`generator`] (its frame parks) with an async object on top (its steps answer promises and its object
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
pub(super) fn generator(ctx: &mut Ctx, inner: FuncId, asynchronous: bool) -> EmitResult<FuncId> {
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
    let outer_throw = ctx.body.enter_nested();

    // The address of the body as it will be PLACED, which is the rewritten form
    // — the host redeclares this identifier once the rewrite has told it the
    // frame's shape. Taking the address here rather than after is what lets the
    // wrapper be emitted in one pass with everything else.
    let code = builder.func_addr(ctx.funcs, inner)?;
    let mut operands = Vec::with_capacity(1 + arguments.len());
    operands.push(code);
    operands.extend(arguments);
    // An `async function*` parks the same frame; only the object differs. Its
    // prototype carries `[Symbol.asyncIterator]` and promise-answering steps, so
    // it is chosen where the object is made rather than patched onto it after.
    let op = match asynchronous {
        false => RuntimeOp::GeneratorNew,
        true => RuntimeOp::AsyncGeneratorNew,
    };
    let made = expr::call(&mut builder, ctx, op, &operands)?[0];
    builder.ret(&[made]);
    ctx.body.leave_nested(outer_throw);
    ctx.pending.push((id, func));
    Ok(id)
}

/// Wraps an async function's body so that calling it answers a promise.
///
/// # What this used to be, and why the shape changed
///
/// It called the body, took what came back, and settled a fresh promise with
/// it. That was correct about the VALUE and wrong about the ordering: the body
/// ran to its end before the call returned, because `Inst::Await` lowers to a
/// call that drains until the promise settles rather than parking anything. So
/// everything after `await` ran before the caller's next statement, which is
/// the one thing an async function exists not to do.
///
/// It is now [`generator`] with a different driver, which is what the language
/// says an async function IS: the body is a parked frame, `await` hands its
/// promise out at a suspension exactly as `yield` hands out a value, and the
/// reaction that resumes it settles the promise this answers. Nothing about the
/// frame machinery is new — it is the one `function*` already uses.
///
/// # Why the two are still two functions here
///
/// They differ in one operand and in nothing else, and merging them would put a
/// `RuntimeOp` parameter on a wrapper whose whole content is which operation it
/// calls. Kept apart because the DOC is the difference: what a caller receives
/// is the fact each of them exists to state.
pub(super) fn async_function(ctx: &mut Ctx, inner: FuncId) -> EmitResult<FuncId> {
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
    let outer_throw = ctx.body.enter_nested();
    // The address of the body as it will be PLACED — the rewritten form, for
    // the reason [`generator`] states. The body is not called: `AsyncStart`
    // builds its frame, drives it as far as the first `await`, and answers the
    // promise it will settle.
    let code = builder.func_addr(ctx.funcs, inner)?;
    let mut operands = Vec::with_capacity(1 + arguments.len());
    operands.push(code);
    operands.extend(arguments);
    let made = expr::call(&mut builder, ctx, RuntimeOp::AsyncStart, &operands)?[0];
    builder.ret(&[made]);
    ctx.body.leave_nested(outer_throw);
    ctx.pending.push((id, func));
    Ok(id)
}
