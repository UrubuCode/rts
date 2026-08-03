//! The queues, and the order they drain in.
//!
//! The order is part of the contract, not an implementation detail. Observable
//! ordering is exactly what makes an asynchronous program deterministic or not,
//! and a client that has to discover it experimentally will encode whatever it
//! observed — after which the order cannot be changed without breaking programs
//! that were never told what they depended on.
//!
//! # The order
//!
//! Continuations first, drained to empty — including continuations enqueued *by*
//! that draining. Then exactly one task. Then back to continuations.
//!
//! Draining to empty rather than round-robin is what makes a chain of settled
//! promises resolve completely before anything else is allowed to observe the
//! world. The cost is that a continuation that keeps enqueuing continuations
//! starves the task queue, which is a real hazard and a deliberate one: the
//! alternative interleaves work whose ordering programs already depend on.
//!
//! # One set of queues per region, not one globally
//!
//! A global queue makes every resumption cross a region boundary, which makes
//! every wait a publication event — and the premise of thread-local regions is
//! that local values need no synchronization. A global queue would make regions
//! decorative. Per promise is worse: a promise's waiter list already is its wait
//! set, and a second queue per object adds state without answering where a
//! runnable waiter actually runs.

use std::collections::VecDeque;

use super::{ContinuationId, TaskId};

/// What the scheduler ran.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ran {
    /// A parked frame resumed.
    Continuation(ContinuationId),
    /// A task ran.
    Task(TaskId),
}

/// One region's queues.
#[derive(Default)]
pub struct Queues {
    continuations: VecDeque<ContinuationId>,
    tasks: VecDeque<TaskId>,
    cancelled: Vec<ContinuationId>,
}

impl Queues {
    /// Empty queues.
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes a parked frame runnable.
    pub fn wake(&mut self, continuation: ContinuationId) {
        self.continuations.push_back(continuation);
    }

    /// Makes several parked frames runnable, preserving their order.
    pub fn wake_all(&mut self, continuations: impl IntoIterator<Item = ContinuationId>) {
        self.continuations.extend(continuations);
    }

    /// Schedules a task.
    pub fn schedule(&mut self, task: TaskId) {
        self.tasks.push_back(task);
    }

    /// Marks a continuation as never to be run.
    ///
    /// Cancellation is a flag consulted when the continuation comes up, not a
    /// removal: a continuation can be cancelled while queued here, while parked
    /// on a promise, or while neither, and one flag covers all three. A separate
    /// cancellation queue would have to be kept in agreement with the other two.
    pub fn cancel(&mut self, continuation: ContinuationId) {
        if !self.cancelled.contains(&continuation) {
            self.cancelled.push(continuation);
        }
    }

    /// Whether a continuation has been cancelled.
    pub fn is_cancelled(&self, continuation: ContinuationId) -> bool {
        self.cancelled.contains(&continuation)
    }

    /// Takes the next thing to run, or `None` when nothing is runnable.
    ///
    /// This is the whole ordering rule, in one place, so that it can be read and
    /// argued with rather than inferred from several call sites.
    pub fn next(&mut self) -> Option<Ran> {
        while let Some(continuation) = self.continuations.pop_front() {
            if !self.is_cancelled(continuation) {
                return Some(Ran::Continuation(continuation));
            }
        }
        self.tasks.pop_front().map(Ran::Task)
    }

    /// Whether anything is waiting to run.
    ///
    /// A cancelled continuation still sitting in the queue does not count: it
    /// will be dropped rather than run, so a scheduler that treated it as work
    /// would report itself busy with nothing to do.
    pub fn has_work(&self) -> bool {
        self.continuations.iter().any(|c| !self.is_cancelled(*c)) || !self.tasks.is_empty()
    }

    /// How many continuations are queued, cancelled or not.
    pub fn queued_continuations(&self) -> usize {
        self.continuations.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continuations_drain_before_any_task() {
        let mut queues = Queues::new();
        queues.schedule(TaskId(0));
        queues.wake(ContinuationId(0));
        queues.wake(ContinuationId(1));

        assert_eq!(queues.next(), Some(Ran::Continuation(ContinuationId(0))));
        assert_eq!(queues.next(), Some(Ran::Continuation(ContinuationId(1))));
        assert_eq!(queues.next(), Some(Ran::Task(TaskId(0))));
    }

    #[test]
    fn a_continuation_enqueued_while_draining_still_precedes_the_task() {
        let mut queues = Queues::new();
        queues.schedule(TaskId(0));
        queues.wake(ContinuationId(0));

        assert_eq!(queues.next(), Some(Ran::Continuation(ContinuationId(0))));
        // Running it settled something, which woke another.
        queues.wake(ContinuationId(1));
        assert_eq!(
            queues.next(),
            Some(Ran::Continuation(ContinuationId(1))),
            "a chain of settled promises resolves completely before anything else observes"
        );
        assert_eq!(queues.next(), Some(Ran::Task(TaskId(0))));
    }

    #[test]
    fn a_cancelled_continuation_is_dropped_rather_than_run() {
        let mut queues = Queues::new();
        queues.wake(ContinuationId(0));
        queues.wake(ContinuationId(1));
        queues.cancel(ContinuationId(0));

        assert_eq!(queues.next(), Some(Ran::Continuation(ContinuationId(1))));
        assert_eq!(queues.next(), None);
    }

    #[test]
    fn a_queue_holding_only_cancelled_work_reports_itself_idle() {
        let mut queues = Queues::new();
        queues.wake(ContinuationId(0));
        queues.cancel(ContinuationId(0));

        assert!(
            !queues.has_work(),
            "reporting busy with nothing to do is a spin"
        );
        assert_eq!(
            queues.queued_continuations(),
            1,
            "it is still there, it just will not run"
        );
    }

    #[test]
    fn waking_several_preserves_the_order_they_attached_in() {
        let mut queues = Queues::new();
        queues.wake_all([ContinuationId(2), ContinuationId(0), ContinuationId(1)]);

        assert_eq!(queues.next(), Some(Ran::Continuation(ContinuationId(2))));
        assert_eq!(queues.next(), Some(Ran::Continuation(ContinuationId(0))));
        assert_eq!(queues.next(), Some(Ran::Continuation(ContinuationId(1))));
    }
}
