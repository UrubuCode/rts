//! The promise object.
//!
//! Three states, a result, and a list of things waiting. Every rule about it is
//! enforced here, at the object, rather than re-checked wherever one is settled
//! — a promise that can be settled twice by one caller and not another is a
//! promise whose invariants are a convention.
//!
//! # Why this is machine-level
//!
//! A promise is not a language feature. It is an object with a state machine, a
//! wait set, and a scheduling discipline that decides when the waiters run
//! relative to everything else. Every client that wants asynchrony needs the same
//! object, and the observable ordering it produces is a property of the machine,
//! not of whoever declared the syntax.

use super::{ContinuationId, SchedulerId};

/// Identifies a promise.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct PromiseId(pub(crate) u32);

impl PromiseId {
    /// The table index.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// How a promise ended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Settlement {
    /// It produced a value.
    Fulfilled,
    /// It failed.
    ///
    /// A rejection resumes its waiter through the unwinding path rather than
    /// delivering a value, which is why this is one settlement with two
    /// outcomes and not two unrelated mechanisms.
    Rejected,
}

/// What a promise is doing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PromiseState {
    /// Not yet settled.
    Pending,
    /// Settled, one way or the other.
    Settled(Settlement),
}

/// Something waiting on a promise.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Waiter {
    /// A parked frame.
    Continuation(ContinuationId),
    /// Another promise, which settles the same way this one does.
    Promise(PromiseId),
}

/// What attaching a waiter did.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Attachment {
    /// The promise is pending; the waiter will be woken when it settles.
    Parked,
    /// The promise had already settled; the waiter is runnable now.
    ReadyNow(Settlement),
}

/// Why an adoption was refused.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AdoptError {
    /// A promise cannot wait on itself, directly or through a chain.
    ///
    /// Nothing could ever settle it, so this is a program that hangs. Refusing
    /// is the only outcome that says so.
    Cycle,
}

/// What settling a promise produced.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SettleOutcome {
    /// Whether this settlement took effect.
    ///
    /// A second settlement is dropped rather than rejected: the specification
    /// says the first result wins, and a caller that settles twice is usually
    /// two paths racing rather than a mistake worth failing on.
    pub accepted: bool,
    /// Continuations that became runnable, in the order they attached.
    pub woken: Vec<ContinuationId>,
}

struct PromiseCell {
    state: PromiseState,
    waiters: Vec<Waiter>,
    owner: SchedulerId,
}

/// Every promise one compilation knows about.
#[derive(Default)]
pub struct PromiseTable {
    cells: Vec<PromiseCell>,
}

impl PromiseTable {
    /// An empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a pending promise owned by a scheduler.
    ///
    /// Ownership decides where its waiters run when it settles, which matters
    /// because a waiter resumed on the wrong thread is a waiter that has to be
    /// moved there — and moving it is the expensive operation the whole
    /// per-region design exists to avoid.
    pub fn create(&mut self, owner: SchedulerId) -> PromiseId {
        let id = PromiseId(self.cells.len() as u32);
        self.cells.push(PromiseCell {
            state: PromiseState::Pending,
            waiters: Vec::new(),
            owner,
        });
        id
    }

    /// What a promise is doing.
    pub fn state(&self, id: PromiseId) -> PromiseState {
        self.cell(id).state
    }

    /// Which scheduler runs this promise's waiters.
    pub fn owner(&self, id: PromiseId) -> SchedulerId {
        self.cell(id).owner
    }

    /// Attaches a parked frame to a promise.
    pub fn attach(&mut self, id: PromiseId, waiter: ContinuationId) -> Attachment {
        match self.cell(id).state {
            PromiseState::Settled(settlement) => Attachment::ReadyNow(settlement),
            PromiseState::Pending => {
                self.cell_mut(id).waiters.push(Waiter::Continuation(waiter));
                Attachment::Parked
            }
        }
    }

    /// Makes one promise settle however another does.
    ///
    /// Adoption is not a separate mechanism: the adopting promise becomes an
    /// ordinary waiter on the other one. That is what makes a chain of them
    /// terminate without special handling — each link wakes the next exactly
    /// once, through the same path a parked frame takes.
    pub fn adopt(&mut self, id: PromiseId, other: PromiseId) -> Result<Attachment, AdoptError> {
        if self.would_cycle(id, other) {
            return Err(AdoptError::Cycle);
        }

        match self.cell(other).state {
            PromiseState::Settled(settlement) => {
                self.settle(id, settlement);
                Ok(Attachment::ReadyNow(settlement))
            }
            PromiseState::Pending => {
                self.cell_mut(other).waiters.push(Waiter::Promise(id));
                Ok(Attachment::Parked)
            }
        }
    }

    /// Settles a promise, waking everything that waited on it.
    ///
    /// Adopting promises are settled transitively here rather than left to a
    /// later pass, so that by the time this returns, every promise that could
    /// have settled has.
    pub fn settle(&mut self, id: PromiseId, settlement: Settlement) -> SettleOutcome {
        if self.cell(id).state != PromiseState::Pending {
            return SettleOutcome {
                accepted: false,
                woken: Vec::new(),
            };
        }

        let mut woken = Vec::new();
        let mut pending = vec![(id, settlement)];

        while let Some((current, settlement)) = pending.pop() {
            if self.cell(current).state != PromiseState::Pending {
                continue;
            }
            self.cell_mut(current).state = PromiseState::Settled(settlement);

            for waiter in std::mem::take(&mut self.cell_mut(current).waiters) {
                match waiter {
                    Waiter::Continuation(continuation) => woken.push(continuation),
                    Waiter::Promise(adopter) => pending.push((adopter, settlement)),
                }
            }
        }

        SettleOutcome {
            accepted: true,
            woken,
        }
    }

    /// How many promises exist.
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// Whether none exist.
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Whether `id` already reaches `other` through an adoption chain.
    fn would_cycle(&self, id: PromiseId, other: PromiseId) -> bool {
        if id == other {
            return true;
        }
        // Walk what `id` would eventually settle: if `other` is among them,
        // adopting it closes a loop nothing outside could break.
        let mut seen = vec![id];
        let mut frontier = vec![id];
        while let Some(current) = frontier.pop() {
            for waiter in &self.cell(current).waiters {
                if let Waiter::Promise(next) = waiter {
                    if *next == other {
                        return true;
                    }
                    if !seen.contains(next) {
                        seen.push(*next);
                        frontier.push(*next);
                    }
                }
            }
        }
        false
    }

    fn cell(&self, id: PromiseId) -> &PromiseCell {
        self.cells
            .get(id.index())
            .expect("promise belongs to this table")
    }

    fn cell_mut(&mut self, id: PromiseId) -> &mut PromiseCell {
        self.cells
            .get_mut(id.index())
            .expect("promise belongs to this table")
    }
}
