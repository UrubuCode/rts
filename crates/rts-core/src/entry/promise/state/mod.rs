//! The promise machine as this crate holds it — the tables, and nothing else.
//!
//! [`ops`] holds the operations over them, which is the whole of the rest of
//! what this module used to be. The split is the crate's 500-line ceiling
//! applied along the seam the module was already written around: a TABLE is
//! read and written, a verb decides something, and every verb here shares one
//! property worth keeping together — none of them runs user code, so any of
//! them may be reached from inside a borrow.
//!
//! # What is here and what is the machine's
//!
//! The three tables this crate genuinely owns are the value a promise settled
//! with ([`crate::schedule::Settlements`]), which reaction each waiter identifier
//! stands for, and which cell is which promise. Everything else — the states,
//! the waiter lists, the wake order — is read from and written to
//! [`rts_cranelift::sched`] rather than mirrored, because a state kept in two
//! places is a state the two will come to disagree about.
//!
//! # Why a cell and a `PromiseId` are two things
//!
//! A `Promise` **is an object**: `p.tag = 9` has to work and `class Mine extends
//! Promise {}` has to reach `Mine.prototype`, which is the decision
//! `entry::collections` records for `Map` and `array_elements` records
//! for arrays. So the cell carries an ordinary shape, and which promise it is
//! lives beside it — here, in a map, rather than in an [`crate::heap::Aside`],
//! because the reverse direction is needed too: a settler native has to get from
//! a promise back to nothing at all but its index.

use std::collections::HashMap;

use rts_cranelift::sched::{
    ContinuationId, PromiseId, PromiseState, PromiseTable, Ran, Scheduler, SchedulerId,
    Settlement,
};

use crate::schedule::Settlements;
use crate::value::Value;

use super::group::Group;
use super::react::{Handler, Reaction};

mod ops;

pub(super) use ops::{built, fresh, react, reject, resolve, settle, type_error};

/// Every promise this program has, and everything waiting on one.
pub(in crate::entry) struct Machine {
    promises: PromiseTable,
    scheduler: Scheduler,
    settlements: Settlements,
    /// What each waiter identifier stands for, `None` once it has run.
    ///
    /// A reaction runs **once**: taking it out is what says so, where a flag
    /// would be a second fact to keep in step with the queue.
    reactions: Vec<Option<Reaction>>,
    /// The combinators in flight.
    groups: Vec<Group>,
    /// Which promise a cell is.
    of_cell: HashMap<u32, PromiseId>,
    /// Which cell a promise is, by the promise's own index.
    ///
    /// A vector because `PromiseId::index` is dense and issued in order — the
    /// same reason `Settlements` indexes by it.
    cells: Vec<u32>,
    /// Every `(resolve, reject)` pair handed out, and whether it has been used.
    ///
    /// The specification's `alreadyResolved`, which the pair of settlers share:
    /// once either has run, the promise is RESOLVED even though it may not have
    /// settled — `resolve(thenable)` leaves it pending while its fate is already
    /// decided. Nothing else may decide it afterwards, which is what makes
    /// `new Promise(r => { r(x); throw e })` keep the fulfilment.
    ///
    /// Not derivable from the settlement: a pending-but-resolved promise and a
    /// pending one look identical there, and that is exactly the pair this has
    /// to tell apart.
    pairs: Vec<(PromiseId, bool)>,
    /// The `(resolve, reject)` each `NewPromiseCapability` in flight collected.
    ///
    /// A STACK, and bounded by nesting rather than by how many promises a
    /// program builds: the executor writes its slot while the constructor runs
    /// and [`Self::close_capability`] truncates the moment the construction
    /// returns, so a long-running program that derives a million subclass
    /// promises holds no more here than the deepest nesting. Rule 10's
    /// "a root source is BOUNDED, and the bound is about the right thing".
    capabilities: Vec<Option<(u64, u64)>>,
}

impl Default for Machine {
    fn default() -> Self {
        Self::in_region(0)
    }
}

impl Machine {
    /// A machine belonging to one region.
    ///
    /// # Why the scheduler is numbered by the region
    ///
    /// It was `SchedulerId(0)`, unconditionally, because there was one region
    /// and one thread. There are now as many of each as the program was
    /// compiled for, and every thread built its own machine — so every thread
    /// had scheduler zero, and `PromiseTable::owner` answered zero for a
    /// promise whichever thread made it.
    ///
    /// That is the number `Delivery::Elsewhere` compares against to decide
    /// whether a settled promise's waiters belong here or have to be handed
    /// over. With every scheduler called zero it can only ever answer `Here`,
    /// so the one case the machine models for crossing a thread would have
    /// been silently unreachable — and reached, would have run another
    /// thread's reactions on this one.
    ///
    /// Nothing hands waiters over yet. What this buys is that the question is
    /// asked with the real numbers, so the day something does, the answer is
    /// not a constant.
    pub(in crate::entry) fn in_region(region: u32) -> Self {
        Machine {
            promises: PromiseTable::new(),
            scheduler: Scheduler::new(SchedulerId(region)),
            settlements: Settlements::new(),
            reactions: Vec::new(),
            groups: Vec::new(),
            of_cell: HashMap::new(),
            cells: Vec::new(),
            pairs: Vec::new(),
            capabilities: Vec::new(),
        }
    }

    /// Opens a capability's slot, answering the number its executor carries.
    pub(super) fn open_capability(&mut self) -> usize {
        self.capabilities.push(None);
        self.capabilities.len() - 1
    }

    /// What a capability's executor was handed.
    pub(super) fn record_capability(&mut self, at: usize, resolve: u64, reject: u64) {
        if let Some(slot) = self.capabilities.get_mut(at) {
            *slot = Some((resolve, reject));
        }
    }

    /// Takes a capability's answer and gives the slot back.
    ///
    /// Truncating rather than clearing: the slots are opened and closed in
    /// order, so the one being closed is the last, and anything a throw left
    /// above it goes with it.
    pub(super) fn close_capability(&mut self, at: usize) -> Option<(u64, u64)> {
        let taken = self.capabilities.get_mut(at).and_then(Option::take);
        self.capabilities.truncate(at);
        taken
    }

    /// Registers a cell as a fresh pending promise.
    fn create(&mut self, cell: u32) -> PromiseId {
        let id = self.promises.create(self.scheduler.id());
        self.of_cell.insert(cell, id);
        if self.cells.len() <= id.index() {
            self.cells.resize(id.index() + 1, cell);
        }
        self.cells[id.index()] = cell;
        id
    }

    /// A fresh `(resolve, reject)` pair for a promise, and the number the two
    /// carry.
    ///
    /// `CreateResolvingFunctions` makes a pair with ONE `alreadyResolved` cell
    /// between them, and a promise can have several pairs over its life: the
    /// constructor's, and one per thenable it adopts. Keying the flag by the
    /// PROMISE instead was tried and is wrong in a way a test caught at once —
    /// `resolve(thenable)` spends the flag, and the pair handed to the
    /// thenable's own `then` could then never settle anything.
    pub(super) fn open_pair(&mut self, id: PromiseId) -> usize {
        self.pairs.push((id, false));
        self.pairs.len() - 1
    }

    /// The promise a pair settles, once — `None` for a pair already spent.
    pub(super) fn claim_pair(&mut self, at: usize) -> Option<PromiseId> {
        let (id, resolved) = self.pairs.get_mut(at)?;
        if *resolved {
            return None;
        }
        *resolved = true;
        Some(*id)
    }

    /// Whether a pair has been used, without using it.
    pub(super) fn pair_spent(&self, at: usize) -> bool {
        self.pairs.get(at).is_some_and(|(_, resolved)| *resolved)
    }

    /// Which promise a cell is, if it is one.
    pub(super) fn id_of(&self, cell: u32) -> Option<PromiseId> {
        self.of_cell.get(&cell).copied()
    }

    /// What a promise settled with, if it has settled.
    pub(super) fn outcome(&self, id: PromiseId) -> Option<(Settlement, u64)> {
        let PromiseState::Settled(settlement) = self.promises.state(id) else {
            return None;
        };
        Some((settlement, self.settlements.value_of(id)?.bits()))
    }

    /// Marks a promise's settlement as looked at.
    ///
    /// `await` reads a settlement through [`Self::outcome`] directly rather
    /// than through a reaction — there is no `Handler` to `record`, because
    /// nothing is queued for later — so it is the one reader of a rejection
    /// that [`react`](Self::react)'s call to [`Settlements::noticed`] never
    /// reaches. Without this, `await`ing a rejection inside a `try`/`catch`
    /// still reported it as unhandled: the value was read and thrown into the
    /// handler correctly, but the settlement table never learned that anything
    /// had looked, so it aged into `take_unhandled` at the end of the turn
    /// regardless.
    pub(super) fn notice(&mut self, id: PromiseId) {
        self.settlements.noticed(id);
    }

    /// An `await` has parked on a promise that has NOT settled yet.
    ///
    /// [`notice`](Self::notice) is the same statement made after the fact, and
    /// it cannot cover this case: a rejection that arrives while the frame is
    /// parked is recorded with no waiters — an awaiting frame polls, so the
    /// scheduler never sees one — and becomes a candidate the instant before
    /// the poll that reads it.
    pub(super) fn awaiting(&mut self, id: PromiseId) {
        self.settlements.awaiting(id);
    }

    /// An `await` has stopped waiting on one.
    pub(super) fn awaited(&mut self, id: PromiseId) {
        self.settlements.awaited(id);
    }

    /// Records a reaction and answers the identifier it waits under.
    fn record(&mut self, reaction: Reaction) -> ContinuationId {
        let id = ContinuationId(self.reactions.len() as u32);
        self.reactions.push(Some(reaction));
        id
    }

    /// Takes a reaction out, so that it cannot run twice.
    pub(super) fn taken(&mut self, waiter: ContinuationId) -> Option<Reaction> {
        self.reactions.get_mut(waiter.as_index())?.take()
    }

    /// The next reaction to run, or `None` when the queue is empty.
    ///
    /// A task is skipped rather than run: [`Ran::Task`] is the machine's slot
    /// for a timer or a readiness notification, nothing in this engine schedules
    /// one, and running a `TaskId` as if it were a reaction would index this
    /// module's table with a number from a different space.
    pub(super) fn next(&mut self) -> Option<ContinuationId> {
        loop {
            match self.scheduler.next()? {
                Ran::Continuation(waiter) => return Some(waiter),
                Ran::Task(_) => continue,
            }
        }
    }

    /// A group in flight, to read and to advance.
    pub(super) fn group_mut(&mut self, at: usize) -> Option<&mut Group> {
        self.groups.get_mut(at)
    }

    /// Starts a combinator, answering which one it is.
    pub(super) fn open(&mut self, group: Group) -> usize {
        self.groups.push(group);
        self.groups.len() - 1
    }

    /// Every value the machine holds that nothing outside it points at.
    ///
    /// For the collector's roots (phase 2b): a settled value lives only in
    /// [`Settlements`] until a reaction reads it, and a reaction's callback,
    /// its captured thenable and a combinator's collected elements live only
    /// here until the reaction runs. None of that is reachable from a cell —
    /// see the module doc for why the machine's own tables are the only path
    /// to them.
    pub(in crate::entry) fn root_words(&self) -> Vec<u64> {
        let mut out: Vec<u64> = self.settlements.root_words().collect();
        for reaction in self.reactions.iter().flatten() {
            match reaction.handler {
                Handler::Js {
                    on_fulfilled,
                    on_rejected,
                    ..
                } => {
                    out.push(on_fulfilled);
                    out.push(on_rejected);
                }
                Handler::Finally { callback, .. } => out.push(callback),
                Handler::Thenable {
                    thenable, then_fn, ..
                } => {
                    out.push(thenable);
                    out.extend(then_fn);
                }
                // The settlement `finally` is carrying across its callback's own
                // promise. It IS still in `Settlements` under the source that
                // produced it, so this is a second path to one value rather than
                // the only one — kept because the reaction is what makes the
                // value live, and a table that later forgot a settled value
                // would take this with it silently.
                Handler::Restore { value, .. } => out.push(value),
                // The parked frame's owner, and this IS the only path to it: an
                // async call's frame object is never handed to the program, so
                // between the `await` that parked it and the settlement that
                // resumes it, nothing else in the heap names it. Without this
                // the collector frees a body mid-`await`.
                Handler::Frame { frame, .. } => out.push(Value::from_slot(frame).bits()),
                // The two functions a foreign constructor's executor handed
                // over. Nothing else names them: the capability's slot was
                // truncated the moment the construction returned, so between
                // that and the settlement this reaction IS the only path.
                Handler::Forward { resolve, reject } => {
                    out.push(resolve);
                    out.push(reject);
                }
                Handler::Member { .. } => {}
            }
        }
        // A capability still being built: the executor has run and the
        // construction has not returned, so the two functions are named by this
        // stack and by the constructor's own frame — which a conservative scan
        // may or may not reach, and "may" is not an answer rule 10 accepts.
        for (resolve, reject) in self.capabilities.iter().flatten() {
            out.push(*resolve);
            out.push(*reject);
        }
        for group in &self.groups {
            out.extend_from_slice(&group.values);
        }
        out
    }

    /// Every rejection nothing ever waited on, with what it rejected with.
    ///
    /// Drained rather than read, so a rejection is reported once: the turn
    /// after, it is somebody's problem or nobody's, and repeating it every turn
    /// would bury the next one.
    pub(super) fn unhandled(&mut self) -> Vec<u64> {
        // Bound rather than chained, because the draining call borrows the
        // table mutably and the lookups that follow borrow it again.
        let ids = self.settlements.take_unhandled();
        ids.into_iter()
            .filter_map(|id| self.settlements.value_of(id).map(Value::bits))
            .collect()
    }
}

/// `ContinuationId` as a position in this module's side table.
///
/// A trait here rather than a method on the machine's type, because the density
/// it relies on is this module's doing: the identifiers are consecutive because
/// [`Machine::record`] mints them consecutively. That is a fact about the side
/// table, and the machine is right not to promise it.
trait AsIndex {
    fn as_index(self) -> usize;
}

impl AsIndex for ContinuationId {
    fn as_index(self) -> usize {
        self.0 as usize
    }
}

