//! The concurrency substrate, exercised end to end.
//!
//! The claims under test are the ones a client would otherwise have to discover
//! experimentally and then depend on: what settling twice does, what a chain of
//! adopting promises does, what happens when a promise settles before anything
//! waits on it, and where a waiter runs when the promise belongs to another
//! region.

use rts_cranelift::frame::{ResumeLabel, plan_suspension};
use rts_cranelift::gc::describe_frames;
use rts_cranelift::ir::FuncRegistry;
use rts_cranelift::ir::{FuncBuilder, Function, Signature, ValueId};
use rts_cranelift::repr::{RefKind, Repr};
use rts_cranelift::sched::{
    AdoptError, Attachment, Continuation, ContinuationId, Delivery, PromiseState, PromiseTable,
    Ran, Scheduler, SchedulerId, Settlement, TaskId,
};
use rts_cranelift::types::TypeRegistry;
use rts_cranelift::verify::{VerifyError, verify};

fn here() -> SchedulerId {
    SchedulerId(0)
}

fn elsewhere() -> SchedulerId {
    SchedulerId(1)
}

// -- the promise object -----------------------------------------------------

#[test]
fn a_promise_starts_pending_and_settles_once() {
    let mut promises = PromiseTable::new();
    let p = promises.create(here());
    assert_eq!(promises.state(p), PromiseState::Pending);

    let first = promises.settle(p, Settlement::Fulfilled);
    assert!(first.accepted);
    assert_eq!(
        promises.state(p),
        PromiseState::Settled(Settlement::Fulfilled)
    );

    let second = promises.settle(p, Settlement::Rejected);
    assert!(!second.accepted, "the first result wins");
    assert_eq!(
        promises.state(p),
        PromiseState::Settled(Settlement::Fulfilled),
        "and the second does not overwrite it"
    );
}

#[test]
fn waiters_are_woken_in_the_order_they_attached() {
    let mut promises = PromiseTable::new();
    let p = promises.create(here());

    for id in 0..3 {
        assert_eq!(promises.attach(p, ContinuationId(id)), Attachment::Parked);
    }

    let outcome = promises.settle(p, Settlement::Fulfilled);
    assert_eq!(
        outcome.woken,
        vec![ContinuationId(0), ContinuationId(1), ContinuationId(2)]
    );
}

#[test]
fn waiting_on_a_settled_promise_does_not_park() {
    let mut promises = PromiseTable::new();
    let p = promises.create(here());
    promises.settle(p, Settlement::Rejected);

    assert_eq!(
        promises.attach(p, ContinuationId(0)),
        Attachment::ReadyNow(Settlement::Rejected)
    );
}

#[test]
fn an_adopting_promise_settles_the_way_the_one_it_adopted_did() {
    let mut promises = PromiseTable::new();
    let outer = promises.create(here());
    let inner = promises.create(here());

    assert_eq!(promises.adopt(outer, inner), Ok(Attachment::Parked));
    assert_eq!(promises.state(outer), PromiseState::Pending);

    promises.settle(inner, Settlement::Rejected);
    assert_eq!(
        promises.state(outer),
        PromiseState::Settled(Settlement::Rejected),
        "adoption is an ordinary wait, so settling the inner one settles the outer"
    );
}

#[test]
fn a_chain_of_adoptions_settles_all_the_way_out() {
    let mut promises = PromiseTable::new();
    let a = promises.create(here());
    let b = promises.create(here());
    let c = promises.create(here());

    promises.adopt(a, b).expect("no cycle");
    promises.adopt(b, c).expect("no cycle");
    promises.attach(a, ContinuationId(0));

    let outcome = promises.settle(c, Settlement::Fulfilled);
    assert_eq!(
        promises.state(a),
        PromiseState::Settled(Settlement::Fulfilled)
    );
    assert_eq!(
        outcome.woken,
        vec![ContinuationId(0)],
        "each link wakes the next exactly once, through the path a parked frame takes"
    );
}

#[test]
fn adopting_a_settled_promise_settles_immediately() {
    let mut promises = PromiseTable::new();
    let outer = promises.create(here());
    let inner = promises.create(here());
    promises.settle(inner, Settlement::Fulfilled);

    assert_eq!(
        promises.adopt(outer, inner),
        Ok(Attachment::ReadyNow(Settlement::Fulfilled))
    );
    assert_eq!(
        promises.state(outer),
        PromiseState::Settled(Settlement::Fulfilled)
    );
}

#[test]
fn a_promise_cannot_wait_on_itself() {
    let mut promises = PromiseTable::new();
    let p = promises.create(here());
    assert_eq!(promises.adopt(p, p), Err(AdoptError::Cycle));
}

#[test]
fn a_cycle_through_a_chain_is_refused_too() {
    let mut promises = PromiseTable::new();
    let a = promises.create(here());
    let b = promises.create(here());

    promises.adopt(a, b).expect("no cycle yet");
    assert_eq!(
        promises.adopt(b, a),
        Err(AdoptError::Cycle),
        "nothing outside could settle it, so this is a program that hangs"
    );
}

// -- the scheduler ----------------------------------------------------------

#[test]
fn settling_a_promise_this_scheduler_owns_enqueues_its_waiters() {
    let mut promises = PromiseTable::new();
    let mut sched = Scheduler::new(here());
    let p = promises.create(here());
    sched.park(
        &mut promises,
        Continuation::new(ContinuationId(7), p, ResumeLabel(0)),
    );

    let delivery = sched.settle(&mut promises, p, Settlement::Fulfilled);
    assert_eq!(delivery, Delivery::Here(vec![ContinuationId(7)]));
    assert_eq!(sched.next(), Some(Ran::Continuation(ContinuationId(7))));
}

#[test]
fn settling_a_promise_another_region_owns_hands_the_waiters_over() {
    let mut promises = PromiseTable::new();
    let mut sched = Scheduler::new(here());
    let theirs = promises.create(elsewhere());
    promises.attach(theirs, ContinuationId(3));

    let delivery = sched.settle(&mut promises, theirs, Settlement::Fulfilled);
    assert_eq!(
        delivery,
        Delivery::Elsewhere {
            owner: elsewhere(),
            continuations: vec![ContinuationId(3)]
        },
        "running it here would run it on the wrong thread"
    );
    assert_eq!(sched.next(), None, "and it is not queued here");
}

#[test]
fn a_handed_over_waiter_runs_on_the_scheduler_that_owns_it() {
    let mut promises = PromiseTable::new();
    let mut ours = Scheduler::new(here());
    let mut theirs = Scheduler::new(elsewhere());
    let p = promises.create(elsewhere());
    promises.attach(p, ContinuationId(3));

    if let Delivery::Elsewhere { continuations, .. } =
        ours.settle(&mut promises, p, Settlement::Fulfilled)
    {
        theirs.accept(continuations);
    }
    assert_eq!(theirs.next(), Some(Ran::Continuation(ContinuationId(3))));
}

#[test]
fn settling_with_nothing_waiting_delivers_to_nobody() {
    let mut promises = PromiseTable::new();
    let mut sched = Scheduler::new(here());
    let p = promises.create(here());

    assert_eq!(
        sched.settle(&mut promises, p, Settlement::Fulfilled),
        Delivery::Nobody
    );
}

#[test]
fn waiting_on_an_already_settled_promise_is_queued_rather_than_run_inline() {
    let mut promises = PromiseTable::new();
    let mut sched = Scheduler::new(here());
    let p = promises.create(here());
    promises.settle(p, Settlement::Fulfilled);

    let attachment = sched.park(
        &mut promises,
        Continuation::new(ContinuationId(1), p, ResumeLabel(0)),
    );
    assert_eq!(attachment, Attachment::ReadyNow(Settlement::Fulfilled));
    assert_eq!(
        sched.next(),
        Some(Ran::Continuation(ContinuationId(1))),
        "the ordering is the same whether the promise settled before or after the wait"
    );
}

#[test]
fn continuations_drain_completely_before_a_task_runs() {
    let mut promises = PromiseTable::new();
    let mut sched = Scheduler::new(here());
    let first = promises.create(here());
    let second = promises.create(here());

    sched.queues().schedule(TaskId(0));
    sched.park(
        &mut promises,
        Continuation::new(ContinuationId(0), first, ResumeLabel(0)),
    );
    sched.park(
        &mut promises,
        Continuation::new(ContinuationId(1), second, ResumeLabel(0)),
    );
    sched.settle(&mut promises, first, Settlement::Fulfilled);
    sched.settle(&mut promises, second, Settlement::Fulfilled);

    assert_eq!(
        sched.run_until_idle(),
        vec![
            Ran::Continuation(ContinuationId(0)),
            Ran::Continuation(ContinuationId(1)),
            Ran::Task(TaskId(0)),
        ]
    );
}

#[test]
fn a_cancelled_waiter_is_dropped_when_its_promise_settles() {
    let mut promises = PromiseTable::new();
    let mut sched = Scheduler::new(here());
    let p = promises.create(here());

    sched.park(
        &mut promises,
        Continuation::new(ContinuationId(0), p, ResumeLabel(0)),
    );
    sched.park(
        &mut promises,
        Continuation::new(ContinuationId(1), p, ResumeLabel(0)),
    );
    sched.queues().cancel(ContinuationId(0));
    sched.settle(&mut promises, p, Settlement::Fulfilled);

    assert_eq!(
        sched.run_until_idle(),
        vec![Ran::Continuation(ContinuationId(1))]
    );
}

// -- the IR side ------------------------------------------------------------

/// A function that is allowed to park its frame.
fn suspending(params: &[Repr], returns: &[Repr]) -> Function {
    Function::new(Signature {
        params: params.to_vec(),
        returns: returns.to_vec(),
        may_suspend: true,
        ..Signature::default()
    })
}

fn param(func: &Function, index: usize) -> ValueId {
    func.block(func.entry).expect("entry exists").params[index]
}

#[test]
fn awaiting_is_one_node_and_parks_the_frame() {
    let types = TypeRegistry::new();
    let mut func = suspending(&[Repr::Ref(RefKind::Bytes)], &[Repr::Tagged]);
    let held = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let promise = b.promise_new();
    let delivered = b.await_(promise);
    b.ret(&[delivered]);

    assert_eq!(verify(&func, &types, &FuncRegistry::new()), vec![]);

    let plan = plan_suspension(&func);
    assert_eq!(plan.points.len(), 1, "an await is a suspension point");
    assert!(
        plan.spill.slot_of(promise).is_none(),
        "the promise is not read after resuming, so the frame need not keep it"
    );
    let _ = held;
}

#[test]
fn a_reference_live_across_an_await_is_preserved_and_reported() {
    let types = TypeRegistry::new();
    let mut func = suspending(&[Repr::Ref(RefKind::Bytes)], &[Repr::Ref(RefKind::Bytes)]);
    let held = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let promise = b.promise_new();
    b.await_(promise);
    b.ret(&[held]);

    assert!(plan_suspension(&func).spill.slot_of(held).is_some());

    let table = describe_frames(&func);
    let frame = table.iter().next().expect("the await is described");
    assert_eq!(
        frame.roots.iter().map(|r| r.value).collect::<Vec<_>>(),
        vec![held]
    );
    assert!(frame.resume_label.is_some());
}

#[test]
fn settling_widens_what_it_carries() {
    let types = TypeRegistry::new();
    let mut func = suspending(&[Repr::F64], &[]);
    let number = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let promise = b.promise_new();
    b.promise_settle(promise, number, false);
    b.ret(&[]);

    assert_eq!(
        verify(&func, &types, &FuncRegistry::new()),
        vec![],
        "what a settlement carries is decided by whoever awaits it"
    );
}

#[test]
fn awaiting_in_a_function_that_did_not_declare_it_may_suspend_is_rejected() {
    let types = TypeRegistry::new();
    let mut func = Function::new(Signature {
        params: vec![],
        returns: vec![],
        may_suspend: false,
        ..Signature::default()
    });

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let promise = b.promise_new();
    b.await_(promise);
    b.ret(&[]);

    assert!(
        verify(&func, &types, &FuncRegistry::new())
            .iter()
            .any(|e| matches!(e, VerifyError::UndeclaredSuspension { .. })),
        "an await parks the frame, so it is a suspension like any other"
    );
}
