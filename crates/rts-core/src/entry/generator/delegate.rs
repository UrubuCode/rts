//! `yield*` — stepping the inner iterator, and forwarding an abrupt resumption
//! to it.
//!
//! # What already answers this, and what does not
//!
//! The machine answers how a parked frame is picked back up:
//! [`rts_cranelift::frame::ResumeMode`] is the one numbering for "carry on",
//! "unwind" and "return", and [`super::resume`] writes it. Nothing there knows
//! what an iterator is, and rule 2 of `rts-cranelift`'s README says it must not
//! — so **which** iterator a parked generator is standing in front of is this
//! crate's question.
//!
//! Nothing here answered it before. The nearest is `entry/iterate.rs`, which
//! drives the same protocol but drains it: it materialises the whole sequence
//! and has nowhere to park, so it never has to remember an iterator across a
//! suspension. Its `member` and `callable` are shared rather than written a
//! second time.
//!
//! # Why the step is a call and not emitted
//!
//! `emit/delegate.rs` used to emit `next(v)` itself, which is one call fewer to
//! read and was right until `outer.throw(e)` had to reach the inner iterator.
//! Forwarding is what the specification says a delegating `yield` does, and it
//! has two consequences a loop of emitted calls cannot express:
//!
//! - the inner iterator is stepped by [`super::Generator::thrown`], from
//!   OUTSIDE the parked frame — so when the outer body resumes, the step for
//!   that turn has already happened and calling `next` again would advance a
//!   finished iterator;
//! - whether the delegation is still standing has to be known while the frame
//!   is parked, which is a fact about the generator and belongs beside its cell.
//!
//! Both land in one place by making the step itself the entry point: it
//! performs the turn, records what it is delegating to, and hands back the
//! pending answer when a forwarded `throw` already produced one. The alternative
//! — an operation the emitter puts before the suspension to declare the
//! delegation, and a second one after it to ask whether it ended — is the same
//! three facts arrived at three times, and gets them right only where the
//! emitter remembered all three.
//!
//! # What is deliberately NOT forwarded
//!
//! A resumption of [`ResumeMode::Return`] whose way out passes through a
//! `finally` that yields. That is not this file's limit — see
//! [`super`]'s note: it is the same one every generator has.

use rts_cranelift::frame::ResumeMode;

use super::State;
use super::resume::{finish, resumable, resume};
use crate::entry::iterate::{callable, member};
use crate::entry::{Context, functions, objects, primitives, throw, with_current};

/// One turn of a delegated iteration: `inner.next(sent)`, and what it answered.
///
/// Answers the iterator result unchanged, so the emitted loop reads `value` and
/// `done` off it exactly as it read them off its own call. What is added is the
/// bookkeeping the emitted loop cannot do: while the answer is not `done`, the
/// generator being resumed is standing in front of `source`, and
/// [`super::Generator::thrown`] and [`super::Generator::returned`] have to be
/// able to find it there.
///
/// `done` is read here as DATA — a getter is not run, the same boundary
/// `entry/iterate.rs` draws and for the same reason. The cost is stated rather
/// than hidden: an iterator whose result carries `done` as an accessor is read
/// as "not done" here, so the delegation is remembered one turn longer than it
/// exists. The emitted loop runs that accessor and leaves correctly; what a
/// later `outer.return(v)` would then do is forward to an iterator that has
/// already finished, which is a call the specification does not make. Reading it
/// as a property here instead would run the accessor TWICE per element, which is
/// observable in every program rather than in that one.
#[rtse::entry]
pub fn delegate_step(step: u64, source: u64, sent: u64) -> u64 {
    // A resumption that arrived through a forwarded `throw` carries its answer
    // with it: the inner iterator was already stepped, by whoever forwarded.
    // Stepping it again here would call `next` after it said `done`.
    if let Some(pending) = taken(|state| state.pending.take()) {
        recorded(None);
        return pending;
    }

    let absent = with_current(|context| objects::undefined_of(context));
    // ONE argument written, which is what the emitted call passed and what lets
    // the callee tell `next(undefined)` from `next()`.
    // Nothing to name: a native calling `next` has no call SITE, which is what
    // the name operand carries. `functions::NO_CALL_NAME` is that, said once.
    let unnamed = functions::NO_CALL_NAME;
    let answered = functions::call_counted(step, source, 1, unnamed, sent, absent, absent, absent);
    // Rule 8: `next` is user code, and a throw leaves `call` answering
    // `undefined`. Reading `done` off that answers `undefined`, which is never
    // true, so the loop above would yield for ever.
    if throw::in_flight() {
        recorded(None);
        return absent;
    }

    let finished = primitives::to_boolean(member(answered, "done"));
    recorded((!finished).then_some(source));
    answered
}

/// Records what the generator now being resumed is delegating to.
///
/// Nothing happens when no generator is being resumed, which is not a defect:
/// `yield*` is only reachable from inside a generator body, and a body only
/// runs from [`resume`].
fn recorded(source: Option<u64>) {
    taken(|state| state.delegating = source);
}

/// Reaches the state of the generator whose body is running right now.
fn taken<T: Default>(with: impl FnOnce(&mut State) -> T) -> T {
    with_current(|context| {
        let Some(&cell) = context.resuming.last() else {
            return T::default();
        };
        match context.generators.get_mut(cell) {
            Some(state) => with(state),
            None => T::default(),
        }
    })
}

/// What a delegating generator is standing in front of, if it is parked at one.
pub(super) fn inner_of(context: &Context, cell: u32) -> Option<u64> {
    context.generators.get(cell)?.delegating
}

/// `outer.throw(e)` while `outer` is parked inside `yield* inner`.
///
/// The specification calls the INNER iterator's `throw` first, and what comes
/// back decides everything: a result that is not `done` is yielded by the outer
/// generator without its frame being touched at all — which is why this answers
/// without resuming — and a `done` one ends the delegation and lets the outer
/// body carry on after the `yield*`.
///
/// An inner iterator with no `throw` is closed and then refused, which is the
/// specification's own order: `return()` runs before the `TypeError`, so an
/// iterator that holds a resource releases it. The refusal is raised AT the
/// delegating `yield` — [`ResumeMode::Unwind`] — rather than from here, so a
/// `try` written around the `yield*` catches it. Raising from here would land it
/// in the caller's regions, which are somebody else's.
pub(super) fn forward_throw(cell: u32, inner: u64, error: u64) -> Option<u64> {
    let method = member(inner, "throw");
    if !callable(method) {
        close(inner);
        if throw::in_flight() {
            return Some(with_current(|context| objects::undefined_of(context)));
        }
        // Built outside `with_current`: raising from inside the borrow aborts
        // the process rather than unwinding.
        throw::type_error("the delegated iterator has no throw()");
        let raised = throw::take_thrown();
        return Some(resume(cell, raised, ResumeMode::Unwind));
    }

    let absent = with_current(|context| objects::undefined_of(context));
    let unnamed = functions::NO_CALL_NAME;
    let answered = functions::call_counted(method, inner, 1, unnamed, error, absent, absent, absent);
    if throw::in_flight() {
        with_current(|context| finish(context, cell));
        return Some(absent);
    }
    delivered(cell, answered)
}

/// `outer.return(v)` while `outer` is parked inside `yield* inner`.
///
/// The inner iterator's `return` runs first and its answer decides the same two
/// ways [`forward_throw`]'s does. An inner iterator with no `return` is not an
/// error: the delegation simply ends and the outer generator's own return
/// completion proceeds, which is what `None` here means.
pub(super) fn forward_return(cell: u32, inner: u64, value: u64) -> Option<u64> {
    let method = member(inner, "return");
    if !callable(method) {
        return None;
    }

    let absent = with_current(|context| objects::undefined_of(context));
    let unnamed = functions::NO_CALL_NAME;
    let answered = functions::call_counted(method, inner, 1, unnamed, value, absent, absent, absent);
    if throw::in_flight() {
        with_current(|context| finish(context, cell));
        return Some(absent);
    }

    let produced = member(answered, "value");
    if primitives::to_boolean(member(answered, "done")) {
        // The delegation is over and the outer owes a return completion —
        // carrying the INNER's value, not the resumer's. That is the whole
        // observable difference between forwarding and not.
        with_current(|context| forget(context, cell));
        return Some(resume(cell, produced, ResumeMode::Return));
    }
    // Still going, so the outer yields what came back and stays exactly where
    // it is: its frame is not re-entered, and the delegation still stands.
    Some(with_current(|context| super::result(context, produced, false)))
}

/// What both forwardings do with an answer the inner iterator produced.
///
/// `done` ends the delegation, and the outer body carries on from the `yield*`
/// with the inner's value — so the frame IS re-entered, ordinarily, and the
/// answer is left where [`delegate_step`] will hand it straight back rather than
/// stepping an iterator that has finished.
fn delivered(cell: u32, answered: u64) -> Option<u64> {
    let produced = member(answered, "value");
    if !primitives::to_boolean(member(answered, "done")) {
        return Some(with_current(|context| super::result(context, produced, false)));
    }
    with_current(|context| {
        if let Some(state) = context.generators.get_mut(cell) {
            state.pending = Some(answered);
        }
    });
    Some(resume(cell, produced, ResumeMode::Deliver))
}

/// `IteratorClose(inner)` — its `return`, if it has one, ignoring the answer.
fn close(inner: u64) {
    let method = member(inner, "return");
    if !callable(method) {
        return;
    }
    let absent = with_current(|context| objects::undefined_of(context));
    // No arguments written: closing an iterator passes none, and the count is
    // what a callee reads to tell that from `return(undefined)`.
    let unnamed = functions::NO_CALL_NAME;
    functions::call_counted(method, inner, 0, unnamed, absent, absent, absent, absent);
}

/// Ends a delegation without ending the generator.
fn forget(context: &mut Context, cell: u32) {
    if let Some(state) = context.generators.get_mut(cell) {
        state.delegating = None;
    }
}

/// The delegation a re-entry starts with: none.
///
/// Cleared on the way IN rather than trusted to be cleared on the way out.
/// `delegate_step` sets it every turn it is reached, and a body that leaves the
/// delegating loop by any other path — a `next` that threw, a `catch` around the
/// `yield*` — would otherwise park with an iterator it is no longer standing in
/// front of, and `outer.return(v)` would call `return` on it.
pub(super) fn entering(context: &mut Context, cell: u32) {
    forget(context, cell);
}

/// Whether a generator parked inside a `yield*` can have a resumption
/// forwarded, and to what.
///
/// Asked before [`resumable`] rather than after, because the two answers are
/// about different things: this is about the delegation, and that is about
/// whether the frame may be entered. A generator that is delegating is by
/// construction parked, so the second question is already answered.
pub(super) fn delegated(this: u64) -> Option<(u32, u64)> {
    let cell = resumable(this)?;
    let inner = with_current(|context| inner_of(context, cell))?;
    Some((cell, inner))
}
