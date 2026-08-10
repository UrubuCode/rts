//! The promise machine as this crate holds it, and every operation over it that
//! runs no user code.
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
    Attachment, ContinuationId, Delivery, PromiseId, PromiseState, PromiseTable, Ran, Scheduler,
    SchedulerId, Settlement,
};

use crate::entry::Context;
use crate::entry::objects::undefined_of;
use crate::schedule::Settlements;
use crate::text::Str;
use crate::value::Value;

use super::group::Group;
use super::react::{Handler, Reaction};

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
        }
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

    /// Which promise a cell is, if it is one.
    pub(super) fn id_of(&self, cell: u32) -> Option<PromiseId> {
        self.of_cell.get(&cell).copied()
    }

    /// The promise at an index, which is what a settler native carries.
    pub(super) fn at(&self, index: usize) -> Option<PromiseId> {
        // Through the cell table rather than by constructing a `PromiseId`:
        // its field is private on purpose, so nothing outside the machine can
        // invent one, and this is the honest way back.
        let cell = *self.cells.get(index)?;
        self.id_of(cell)
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
                    out.push(then_fn);
                }
                Handler::Member { .. } => {}
            }
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

/// A pending promise, with `Promise.prototype` on it.
pub(super) fn fresh(context: &mut Context) -> Option<(u32, PromiseId)> {
    let cell = super::super::native::plain(context)?;
    // Through the class's own registration rather than a field on the context,
    // for the reason `collections::fresh` records: what `.then` answers must
    // itself answer to the methods this module installed.
    if let Some(prototype) = super::super::class_support::prototype(context, "Promise") {
        context.set_prototype(cell, prototype);
    }
    let id = context.promises.create(cell);
    Some((cell, id))
}

/// The object a constructor writes into: the one `new` made, or one made here.
///
/// # Why a plain call is not refused
///
/// `Promise(f)` without `new` is a `TypeError` in the language, and raising one
/// here would end the program — `entry::throw` cannot find a handler in
/// a caller. The same tolerance `Error("x")` and `Map()` settle on, and it fails
/// where the program uses the result rather than at an arbitrary later point.
pub(super) fn built(context: &mut Context, this: u64) -> Option<(u32, PromiseId)> {
    match Value(this).as_slot() {
        // The cell `construct` already made, which carries the prototype of
        // whatever class was named — so `class Mine extends Promise {}` gives an
        // instance with `Mine.prototype` on it and this only records what it is.
        Some(cell) => Some((cell, context.promises.create(cell))),
        None => fresh(context),
    }
}

/// Settles a promise, waking whatever was waiting, and remembers the value.
///
/// Runs no user code: the waiters are queued, and the drain runs them. That is
/// the single most-tested property of a promise implementation —
/// `Promise.resolve(1).then(f)` must not call `f` before `then` returns — and it
/// is a property of the machine's queue rather than of care taken here.
pub(super) fn settle(context: &mut Context, id: PromiseId, settlement: Settlement, value: u64) {
    let machine = &mut context.promises;
    let delivery = machine
        .scheduler
        .settle(&mut machine.promises, id, settlement);
    // Whether anything was already waiting, which is what decides an unhandled
    // rejection. Taken from the machine's own answer rather than re-derived from
    // this module's tables — the second copy is the one that would come to
    // disagree.
    let had_waiters = delivery != Delivery::Nobody;
    machine
        .settlements
        .record(id, settlement, Value(value), had_waiters);
}

/// Resolves a promise, adopting whatever it was resolved with.
///
/// The three cases the language distinguishes, in the order it distinguishes
/// them: the promise itself, another promise or thenable, an ordinary value.
pub(super) fn resolve(context: &mut Context, id: PromiseId, value: u64) {
    if let Some(cell) = Value(value).as_slot() {
        if context.promises.id_of(cell) == Some(id) {
            // `resolve(p)` inside `new Promise(resolve => …)`. Nothing could
            // ever settle it, so the language rejects with a `TypeError` rather
            // than leaving a promise that hangs and says nothing.
            let reason = type_error(context, "Chaining cycle detected for promise");
            settle(context, id, Settlement::Rejected, reason);
            return;
        }
        if let Some(inner) = context.promises.id_of(cell) {
            // Adoption as an ordinary reaction with no handlers: whatever the
            // inner promise settles as, the `Pass` step hands straight on. See
            // the module documentation for why `PromiseTable::adopt` is not
            // what does this.
            let absent = undefined_of(context);
            react(
                context,
                inner,
                Handler::Js {
                    on_fulfilled: absent,
                    on_rejected: absent,
                    derived: id,
                },
            );
            return;
        }
        if let Some(then_fn) = then_of(context, cell) {
            // A foreign thenable. Its `then` is user code and is called from a
            // microtask, which is what the specification says and what stops a
            // getter from running inside this borrow.
            let waiter = context.promises.record(Reaction {
                source: None,
                handler: Handler::Thenable {
                    thenable: value,
                    then_fn,
                    promise: id,
                },
            });
            context.promises.scheduler.queues().wake(waiter);
            return;
        }
    }
    settle(context, id, Settlement::Fulfilled, value);
}

/// Rejects a promise with a reason.
pub(super) fn reject(context: &mut Context, id: PromiseId, reason: u64) {
    settle(context, id, Settlement::Rejected, reason);
}

/// Attaches a reaction to a promise, queueing it if the promise already settled.
///
/// Queued rather than run, even when the promise settled long ago: the ordering
/// must not depend on whether the handler was early or late, which is the one
/// thing a program can observe about a promise that it was never told it
/// depended on.
pub(super) fn react(context: &mut Context, source: PromiseId, handler: Handler) {
    let machine = &mut context.promises;
    let waiter = machine.record(Reaction {
        source: Some(source),
        handler,
    });
    if let Attachment::ReadyNow(_) = machine.promises.attach(source, waiter) {
        machine.scheduler.queues().wake(waiter);
    }
    // Something is waiting on it now, so a rejection it carries is somebody's
    // problem. `Promise.reject(x).catch(f)` attaches after the rejection and is
    // not an unhandled rejection — which is the whole reason the report waits
    // for the end of the turn.
    machine.settlements.noticed(source);
}

/// A settler — the `resolve` or `reject` an executor is handed.
///
/// # A native cannot close over anything, so this closes over the environment
///
/// `native::callable` gives every native the environment
/// `undefined`, because a native closes over nothing. This one has to: two
/// promises' `resolve` functions are the same code and must settle different
/// promises.
///
/// The environment slot is where it goes. It is passed to the callee by
/// `functions::call` untouched, it is not reachable from
/// JavaScript — `callables` is private and `callable_at` answers only inside
/// this crate — and it costs nothing.
///
/// The alternative was a property on the callable's own cell, which a callable
/// being an object makes possible. It was rejected because it is **observable**:
/// `Object.keys(resolve)` would show it, a program could overwrite it, and it
/// would mint a property key for a fact no program named.
///
/// What crosses is the promise's INDEX, not a `PromiseId`: the machine keeps
/// that field private so nothing outside can invent one, and [`Machine::at`] is
/// the honest way back.
pub(super) fn settler(context: &mut Context, id: PromiseId, settlement: Settlement) -> u64 {
    let code = match settlement {
        Settlement::Fulfilled => resolve_native as super::super::native::Native,
        Settlement::Rejected => reject_native as super::super::native::Native,
    };
    let made = super::super::native::callable(context, code);
    if let Some(cell) = Value(made).as_slot() {
        context.mark_callable(cell, code as usize as u64, id.index() as u64);
    }
    made
}

/// `resolve(value)` as an executor receives it.
extern "C" fn resolve_native(environment: u64, _this: u64, a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with(environment, |context, id| resolve(context, id, a0))
}

/// `reject(reason)` as an executor receives it.
extern "C" fn reject_native(environment: u64, _this: u64, a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with(environment, |context, id| reject(context, id, a0))
}

/// The body both settlers share: find the promise, act, answer `undefined`.
fn with(environment: u64, body: impl FnOnce(&mut Context, PromiseId)) -> u64 {
    crate::entry::with_current(|context| {
        if let Some(id) = context.promises.at(environment as usize) {
            body(context, id);
        }
        undefined_of(context)
    })
}

/// A cell's `then`, when it is callable — which is what makes it a thenable.
///
/// Read through the ordinary property path, so an object inheriting `then` is
/// one too. A getter is deliberately not run: `objects::read_property`
/// answers data properties, and running a getter here would be user code inside
/// a borrow.
fn then_of(context: &mut Context, cell: u32) -> Option<u64> {
    let key = context.well_known("then");
    let found = super::super::objects::read_property(context, cell, key)?;
    let callee = found.as_slot()?;
    context.callable_at(callee)?;
    Some(found.bits())
}

/// A `TypeError` with a message, made without running a constructor.
///
/// The class is registered first so that the object inherits the same prototype
/// `new TypeError("x")` gives — a program that catches one and reads `.name`
/// must not be able to tell where it came from.
pub(super) fn type_error(context: &mut Context, message: &str) -> u64 {
    super::super::error::register_type_error(context);
    let Some(cell) = super::super::native::plain(context) else {
        return undefined_of(context);
    };
    if let Some(prototype) = super::super::class_support::prototype(context, "TypeError") {
        context.set_prototype(cell, prototype);
    }
    let text = context.intern_value(Str::from_str(message)).bits();
    let key = context.well_known("message");
    super::super::objects::put(context, cell, key, text);
    Value::from_slot(cell).bits()
}
