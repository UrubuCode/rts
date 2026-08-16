//! The driver that makes an `async function` an async function: a parked frame
//! whose resumer is a promise.
//!
//! # Reuse-check, stated before the code
//!
//! Nothing here is a suspension mechanism. `rts_cranelift::frame` parks and
//! resumes frames, `super::super::generator` already holds one per object and
//! re-enters it three ways, and `super::state` already owns promises, their
//! wait sets and the queue their reactions run on. What was missing was the
//! JOIN: something that re-enters a frame when a promise settles, and settles a
//! promise when the frame runs out. That is this file, and it is the fifth
//! reaction `super::react` was documented as lacking.
//!
//! So an `async function` here is exactly what the language says it is — a
//! generator whose `next` is called by the settlement of what it awaited. The
//! object [`start`] makes is the same object a `function*` call answers, made by
//! the same entry point; the program never sees it, and holding a second kind
//! of frame owner for it would be a second answer to "what is parked".
//!
//! # What ordering this buys, and how it was checked
//!
//! ```text
//! async function f() { console.log("a"); await Promise.resolve(); console.log("b"); }
//! f(); console.log("c");
//! ```
//!
//! prints `a c b`, because `await` parks and the resumption is a microtask. It
//! printed `a b c` for as long as `await` drained the queue in place, which is
//! the divergence `super::machine` names in full.
//!
//! # Why an already-settled promise is attached to rather than read
//!
//! Reading it and carrying on would be the blocking form again in a smaller
//! shape: `await p` must cost a turn even when `p` settled long ago, because
//! that is the only thing that keeps two chains in flight in the order they
//! were started. The attachment is what spends it, and it spends exactly ONE —
//! a promise is waited on as it stands rather than wrapped, which is
//! [`super::thenable::waited_on`]'s measured rule and the reason `await
//! Promise.resolve()` resumes on the same turn a `.then` attached beside it
//! would run.

use rts_cranelift::frame::ResumeMode;
use rts_cranelift::sched::{PromiseId, Settlement};

use super::react::Handler;
use super::state;
use crate::entry::generator::advance;
use crate::entry::{Context, objects, throw, with_current};
use crate::value::Value;

/// `RuntimeOp::AsyncStart` — calls an async function's body and answers its
/// promise.
///
/// The parameters are `GeneratorNew`'s, because the frame is the same frame:
/// the address of the rewritten body, then the convention's own arguments,
/// which are written into the frame rather than passed because a resumed body
/// is entered afresh.
///
/// The body is DRIVEN once before this answers, which is the half a caller can
/// observe: everything up to the first `await` has already run when the promise
/// comes back, and everything after it has not.
#[rtse::entry]
pub fn async_start(
    code: i64,
    environment: u64,
    this: u64,
    a0: u64,
    a1: u64,
    a2: u64,
    a3: u64,
) -> u64 {
    let object = super::super::generator_new(code, environment, this, a0, a1, a2, a3);
    // Held from the moment it exists. Everything below allocates — the promise
    // here, hundreds of objects inside the body — and until the reaction is
    // attached this stack is the only thing that names the frame. See
    // `Context::driving`.
    if let Some(cell) = Value(object).as_slot() {
        with_current(|context| context.driving.push(cell));
    }
    let made = with_current(|context| {
        // Registered first, for the reason `machine::promise_new` gives: a
        // program that never writes the word `Promise` has triggered no
        // registration, so the promise comes back with no prototype and
        // therefore no `.then` — and `f().then(…)` is how most programs use an
        // async function. It was measured as exactly that TypeError.
        super::register_promise(context);
        let (cell, id) = state::fresh(context)?;
        Some((cell, id, Value::from_slot(cell).bits()))
    });
    let Some((_, result, promise)) = made else {
        released();
        return with_current(|context| objects::undefined_of(context));
    };
    let Some(frame) = Value(object).as_slot() else {
        // `generator_new` answers `undefined` for a body whose frame nothing
        // registered. Rejecting rather than answering a promise that can never
        // settle: a program awaiting it would hang, and a hang cannot be
        // diagnosed from inside the program.
        with_current(|context| {
            let reason = state::type_error(context, "async function has no registered frame");
            state::reject(context, result, reason);
        });
        // Nothing was pushed for a frame that is not a cell, so nothing is
        // released here.
        return promise;
    };
    // The first turn runs HERE rather than in a microtask, which is the
    // language's own rule and not an optimisation: the body of an async
    // function runs synchronously up to its first `await`.
    drive(frame, result, ResumeMode::Deliver, none());
    released();
    promise
}

/// Drops the innermost frame this stack was holding.
///
/// Paired by hand rather than by a guard type: every push here is inside an
/// `extern "C"` entry point, where an unwind is an abort, so there is no path
/// that skips the pop and nothing for a `Drop` to catch.
fn released() {
    with_current(|context| {
        context.driving.pop();
    });
}

/// Resumes a parked async frame because what it awaited has settled.
///
/// Called from the drain with no borrow held, for the reason everything there
/// is: re-entering the body runs user code.
pub(super) fn resumed(frame: u32, result: PromiseId, settlement: Settlement, value: u64) {
    let mode = match settlement {
        Settlement::Fulfilled => ResumeMode::Deliver,
        // A rejection crossing an `await` is a throw AT the `await`, raised by
        // the frame rewrite inside the regions it was written in — which is
        // what makes `try { await p } catch` catch. Throwing from here would
        // land in the drain's regions, which are nobody's.
        Settlement::Rejected => ResumeMode::Unwind,
    };
    drive(frame, result, mode, value);
}

/// Re-enters the frame once and does whatever it left behind.
///
/// The three outcomes are the whole of what an async function can do: throw
/// (reject), finish (resolve), or park on something (attach and wait).
fn drive(frame: u32, result: PromiseId, mode: ResumeMode, sent: u64) {
    // Held for the whole re-entry. A resumption arrives from the drain, which
    // has already TAKEN the reaction out of the table — so between that and
    // whatever this attaches next, the reaction's root is gone and this stack
    // is the only one left. `advance` runs the body, which allocates.
    with_current(|context| context.driving.push(frame));
    let stepped = advance(frame, sent, mode);
    if stepped.threw {
        // Taken rather than left in flight: the throw belongs to the promise
        // now. Left in flight it would be re-raised by whatever compiled code
        // ran next — the caller of an async function, which is exactly the
        // frame the language says must NOT see it.
        let thrown = throw::caught().unwrap_or(stepped.value);
        with_current(|context| state::reject(context, result, thrown));
        released();
        return;
    }
    if stepped.finished {
        // Through `resolve`, so that `async function f() { return p }` adopts
        // `p` instead of fulfilling with a promise.
        with_current(|context| state::resolve(context, result, stepped.value));
        released();
        return;
    }
    // Parked on an `await`. What it handed out is what it is waiting for.
    with_current(|context| match waited_on(context, stepped.value) {
        Some(source) => state::react(context, source, Handler::Frame { frame, result }),
        // Nothing to attach to means no promise could be made, which is an
        // exhausted heap. Rejecting says so; leaving the frame parked on
        // nothing would be a hang, and a hang cannot be diagnosed from inside
        // the program.
        None => {
            let reason = state::type_error(context, "the heap is exhausted");
            state::reject(context, result, reason);
        }
    });
    released();
}

/// The promise whose settlement resumes the frame.
///
/// `await v` over something that is not a promise still costs a turn, which is
/// what the fresh-and-resolved promise buys. A promise is waited on as it
/// stands — see this module's head for why that is one turn and not two.
fn waited_on(context: &mut Context, awaited: u64) -> Option<PromiseId> {
    if let Some(id) = super::thenable::waited_on(context, awaited) {
        return Some(id);
    }
    let (_, id) = state::fresh(context)?;
    state::resolve(context, id, awaited);
    Some(id)
}

/// `undefined`, for the first resumption, which delivers nothing.
fn none() -> u64 {
    with_current(|context| objects::undefined_of(context))
}
