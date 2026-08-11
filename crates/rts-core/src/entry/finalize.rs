//! Telling a client that an object it cared about has died.
//!
//! # Why the sweep does not call anything
//!
//! It holds the borrow. `release` runs with `&mut Context`, and this crate's
//! whole discipline — rule 8 of its README, and the abort `native.rs`
//! describes — is that a native must not call out while holding one. A
//! finalizer calls out by definition: an addon's does allocate, and does call
//! back in.
//!
//! So the sweep QUEUES and something else drains, which is also what Node does
//! and for the same reason. The queue is drained where microtasks are, by
//! [`super::drain_microtasks`]: that is the one point every host in this
//! repository already pumps — the JIT loop in `rts-host` and the `main` in
//! `rts-runtime` both — so a finalizer runs without either of them being taught
//! anything.
//!
//! # Why the callback is two words and a function pointer
//!
//! Because this crate must not know what an N-API finalizer is. A registration
//! is `code(data, hint)`, spelled in the C convention, and what those words
//! mean belongs to whoever registered them — the same shape `entry::foreign`
//! takes for the same reason.
//!
//! `rts-napi` registers a trampoline of its own here, exactly as it does
//! for a callable, and recovers its environment from the words. This crate
//! never learns that an environment exists.
//!
//! # What a client must not assume
//!
//! **When.** A finalizer runs at the next drain after the collection that freed
//! the object, which may be much later than the death and is never during it.
//!
//! **Whether.** A program that ends before the next drain runs nothing at all.
//! That is what every JavaScript engine says about finalizers and it is not a
//! gap to be closed here: a finalizer is not a destructor, and code that must
//! run belongs somewhere it can be called.

use super::Context;
use crate::value::Value;

/// What to call, and the two words to call it with.
///
/// `extern "C"` because the registrant is as likely to be a C addon as Rust,
/// and because a Rust `fn` pointer of this shape converts for free while the
/// reverse does not.
pub type OnDeath = extern "C" fn(usize, usize);

/// A registration waiting for its object to die, or waiting to be run.
#[derive(Clone, Copy)]
pub struct Pending {
    /// What to call.
    pub code: OnDeath,
    /// The first word it receives.
    pub data: usize,
    /// The second.
    pub hint: usize,
}

/// Asks to be told when the object `value` names is collected.
///
/// `None` when the value names no cell: a number never dies, so a registration
/// on one would be a subscription to an event that cannot happen — the same
/// refusal, for the same reason, [`super::weak`] makes.
pub fn on_death(context: &mut Context, value: u64, pending: Pending) -> Option<u32> {
    let cell = Value(value).as_slot()?;
    let id = context.next_death;
    context.next_death = context.next_death.wrapping_add(1);
    context.deaths.push((id, cell, pending));
    Some(id)
}

/// Withdraws a registration, answering whether it was still waiting.
///
/// A client that has already run the callback itself — an addon calling
/// `napi_remove_wrap` — withdraws rather than letting it run twice.
pub fn cancel(context: &mut Context, id: u32) -> bool {
    let before = context.deaths.len();
    context.deaths.retain(|(waiting, _, _)| *waiting != id);
    context.deaths.len() != before
}

/// Moves every registration on a dying cell into the run queue.
///
/// Called by the sweep, which must not call them — see the module doc.
pub(super) fn queue_freed(context: &mut Context, cell: u32) {
    let mut at = 0;
    while at < context.deaths.len() {
        match context.deaths[at].1 == cell {
            true => {
                let (_, _, pending) = context.deaths.remove(at);
                context.dying.push(pending);
            }
            false => at += 1,
        }
    }
}

/// Runs a collection now, and answers how many cells it freed.
///
/// The engine collects on its own when the region fills, which is the path
/// every program takes and the reason there is no `gc` surface in the language
/// (`CLAUDE.md` records that decision). This exists for the caller that must
/// OBSERVE one: `rts-napi`'s own tests, which have to see that a wrap's
/// finalizer is queued by a real cycle rather than trust two halves that were
/// each tested apart.
///
/// `stack_low` is a local's address at the call site, and the caller must have
/// set [`Context::stack_high`] — see `collect_cycle::collect` for why a cycle
/// that cannot see the stack refuses instead of running smaller.
pub fn collect_now(stack_low: usize) -> usize {
    super::with_current(|context| super::collect_cycle::collect(context, stack_low))
}

/// Runs what the sweep queued, and answers how many ran.
///
/// Takes the whole queue in one short borrow and then calls with none held,
/// which is the rule this module exists to keep. A finalizer that allocates may
/// trigger a collection that queues more; those run on the next drain rather
/// than in this loop, so one drain cannot be made unbounded by a callback that
/// keeps producing work.
pub fn drain() -> usize {
    let queued: Vec<Pending> = super::with_current(|context| core::mem::take(&mut context.dying));
    for pending in &queued {
        (pending.code)(pending.data, pending.hint);
    }
    queued.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{Kinds, Singletons};
    use core::sync::atomic::{AtomicUsize, Ordering};

    static RAN: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn record(data: usize, hint: usize) {
        RAN.fetch_add(data + hint, Ordering::SeqCst);
    }

    fn context() -> Context {
        Context::new(
            Singletons {
                undefined: 0,
                null: 1,
                hole: 2,
            },
            Kinds::in_declaration_order(),
        )
    }

    fn object(context: &mut Context) -> (u64, u32) {
        let ty = context.types.declare(&[rts_cranelift::repr::Repr::I64]);
        let cell = context
            .region
            .alloc(crate::heap::STRIDE, ty.index() as u32)
            .expect("room");
        (Value::from_slot(cell).bits(), cell)
    }

    #[test]
    fn a_registration_on_something_that_cannot_die_is_refused() {
        let mut context = context();
        let pending = Pending {
            code: record,
            data: 0,
            hint: 0,
        };
        assert_eq!(
            on_death(&mut context, Value::from_f64(1.0).bits(), pending),
            None
        );
    }

    #[test]
    fn a_cancelled_registration_does_not_reach_the_queue() {
        // What `napi_remove_wrap` relies on: the addon ran the finalizer itself
        // and the collector must not run it a second time.
        let mut context = context();
        let (value, cell) = object(&mut context);
        let id = on_death(
            &mut context,
            value,
            Pending {
                code: record,
                data: 1,
                hint: 0,
            },
        )
        .expect("a cell is registrable");

        assert!(cancel(&mut context, id));
        assert!(!cancel(&mut context, id), "and only once");
        queue_freed(&mut context, cell);
        assert!(context.dying.is_empty(), "nothing left to run");
    }

    #[test]
    fn the_sweep_queues_and_the_drain_calls() {
        // The separation this module is: `queue_freed` runs with the borrow
        // held and must not call; `drain` calls with none held.
        let mut context = context();
        let (value, cell) = object(&mut context);
        on_death(
            &mut context,
            value,
            Pending {
                code: record,
                data: 2,
                hint: 3,
            },
        )
        .expect("a cell is registrable");

        queue_freed(&mut context, cell);
        assert_eq!(context.dying.len(), 1, "queued, not called");

        RAN.store(0, Ordering::SeqCst);
        let (_, ran) = super::super::with_context(context, drain);
        assert_eq!(ran, 1);
        assert_eq!(RAN.load(Ordering::SeqCst), 5, "called with both words");
    }
}
