//! The concurrency substrate: promises, continuations, and the order they run in.
//!
//! # Why this is here
//!
//! A promise is a machine-level object with three states, a result, a wait set,
//! and a scheduling discipline that decides when its waiters run relative to
//! everything else. Every client that wants asynchrony needs the same object,
//! and the observable ordering it produces is a property of the machine rather
//! than of whoever wrote the syntax.
//!
//! A continuation is a parked frame plus the thing it is waiting for. That is
//! not a metaphor here — it is [`Continuation`], and the parked frame is the one
//! [`crate::frame`] already knows how to produce. Coroutines, generators,
//! asynchronous functions and promise waiters are four names for
//! suspend-and-resume over a queue, which is the reason to build them once.
//!
//! # What this replaces
//!
//! The arrangement in the current engine buys concurrency by occupying threads:
//! an asynchronous function compiles as an ordinary linear function, its call
//! site spawns it onto a worker, and awaiting blocks that worker. The number of
//! concurrent operations is bounded by the thread pool, ordering is whatever the
//! operating system decides, and the code generator has to know about spawning at
//! call sites.
//!
//! With a parked frame and a queue, an awaiting operation costs a frame rather
//! than a thread, the ordering is stated here, and a client emits one node.
//!
//! # Shape
//!
//! The parked-frame-plus-waiter shape is the one a stackless asynchronous model
//! uses, because it is a compile-time frame transformation — which is what
//! [`crate::frame`] must own anyway for root reporting and unwinding. The
//! drain-to-empty discipline is the one a production JavaScript engine uses,
//! generalized from one global queue to one per region because the threading
//! model requires it. Nothing here needs stack switching, which is what a
//! stackful model would have required and which is unavailable.

mod promise;
mod queue;

pub use promise::{
    AdoptError, Attachment, PromiseId, PromiseState, PromiseTable, SettleOutcome, Settlement,
};
pub use queue::{Queues, Ran};

use crate::frame::ResumeLabel;

/// Identifies one region's scheduler.
///
/// There is one per region-owning thread. A promise records which one runs its
/// waiters, so that settling from elsewhere hands the work over rather than
/// running it on the wrong thread.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct SchedulerId(pub u32);

/// Identifies a parked frame waiting on something.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct ContinuationId(pub u32);

/// Identifies work that is not a resumption — a timer that fired, a readiness
/// notification, an entry point being started.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct TaskId(pub u32);

/// A parked frame and what it is waiting for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Continuation {
    /// Which parked frame this is.
    pub id: ContinuationId,
    /// The promise whose settlement makes it runnable.
    pub waiting_on: PromiseId,
    /// Where control returns inside the frame.
    ///
    /// The same label [`crate::frame`] derives, so a resumption is a jump
    /// selected by a number rather than a second dispatch mechanism.
    pub resume: ResumeLabel,
}

impl Continuation {
    /// A parked frame waiting on a promise, resuming at a label.
    pub fn new(id: ContinuationId, waiting_on: PromiseId, resume: ResumeLabel) -> Self {
        Self {
            id,
            waiting_on,
            resume,
        }
    }
}

/// Where woken waiters must run.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Delivery {
    /// Nothing was waiting.
    Nobody,
    /// The waiters belong to the scheduler that settled the promise.
    Here(Vec<ContinuationId>),
    /// The waiters belong to another region's scheduler.
    ///
    /// Handing them over is a publication: whatever the settlement carries
    /// becomes reachable from a second thread, so it must be promoted before the
    /// hand-off rather than after. Modelling the hand-off explicitly is what
    /// makes that obligation visible instead of implied.
    Elsewhere {
        /// Which scheduler owns them.
        owner: SchedulerId,
        /// The waiters, in the order they attached.
        continuations: Vec<ContinuationId>,
    },
}

/// One region's scheduler.
///
/// Owns its queues and settles promises on behalf of the region it belongs to.
/// Promises themselves live in a shared table, because a promise created in one
/// region can be awaited from another — which is exactly the case [`Delivery`]
/// exists to make explicit.
pub struct Scheduler {
    id: SchedulerId,
    queues: Queues,
}

impl Scheduler {
    /// A scheduler for one region.
    pub fn new(id: SchedulerId) -> Self {
        Self {
            id,
            queues: Queues::new(),
        }
    }

    /// Which region this scheduler serves.
    pub fn id(&self) -> SchedulerId {
        self.id
    }

    /// Its queues, for enqueuing work and for asking what runs next.
    pub fn queues(&mut self) -> &mut Queues {
        &mut self.queues
    }

    /// Settles a promise and reports where its waiters must run.
    ///
    /// Waiters this scheduler owns are enqueued here; waiters another owns are
    /// returned rather than enqueued, because enqueuing them would run them on
    /// the wrong thread — the failure this whole per-region arrangement exists to
    /// prevent.
    pub fn settle(
        &mut self,
        promises: &mut PromiseTable,
        promise: PromiseId,
        settlement: Settlement,
    ) -> Delivery {
        let owner = promises.owner(promise);
        let outcome = promises.settle(promise, settlement);

        if outcome.woken.is_empty() {
            return Delivery::Nobody;
        }
        if owner == self.id {
            self.queues.wake_all(outcome.woken.iter().copied());
            Delivery::Here(outcome.woken)
        } else {
            Delivery::Elsewhere {
                owner,
                continuations: outcome.woken,
            }
        }
    }

    /// Accepts waiters handed over by another region's scheduler.
    pub fn accept(&mut self, continuations: impl IntoIterator<Item = ContinuationId>) {
        self.queues.wake_all(continuations);
    }

    /// Parks a frame on a promise, or reports that it can run immediately.
    ///
    /// A promise that has already settled does not park anything: the waiter is
    /// enqueued rather than run inline, so that the ordering is the same whether
    /// the promise settled before or after the wait. A waiter that ran inline
    /// when it happened to be late would make a program's behaviour depend on
    /// timing that is not otherwise observable.
    /// Takes the whole continuation rather than only its identifier, so that a
    /// frame cannot be parked without saying where it resumes. A parked frame
    /// with no resume point is a frame nothing can bring back, and discovering
    /// that at resumption time is discovering it too late.
    pub fn park(&mut self, promises: &mut PromiseTable, continuation: Continuation) -> Attachment {
        let attachment = promises.attach(continuation.waiting_on, continuation.id);
        if let Attachment::ReadyNow(_) = attachment {
            self.queues.wake(continuation.id);
        }
        attachment
    }

    /// Takes the next thing to run.
    pub fn next(&mut self) -> Option<Ran> {
        self.queues.next()
    }

    /// Runs until nothing is runnable, reporting what ran in order.
    ///
    /// Only useful for reasoning about ordering — a real scheduler blocks
    /// waiting for external readiness instead of stopping. Keeping it means the
    /// ordering contract is executable rather than described.
    pub fn run_until_idle(&mut self) -> Vec<Ran> {
        let mut ran = Vec::new();
        while let Some(next) = self.queues.next() {
            ran.push(next);
        }
        ran
    }
}
