//! What a promise settled *with*.
//!
//! # Almost all of this phase belongs to the machine
//!
//! [`rts_cranelift::sched`] owns the promise state machine, the waiter lists,
//! cycle detection when one promise adopts another, the queues, and handing
//! woken continuations to the scheduler that owns them. None of it is repeated
//! here.
//!
//! What it deliberately does **not** hold is a value. A `PromiseCell` is a
//! state, a waiter list and an owner; `Settlement` is `Fulfilled` or `Rejected`
//! and carries nothing. That is the right split — the machine models *control*,
//! and which value a promise resolved with is data the language chose — and it
//! leaves exactly two things for this module.
//!
//! # One: the value
//!
//! A side table from promise to value. Kept beside the machine's table rather
//! than inside it, because a second language on this machine settles promises
//! with its own kind of value and the machine should keep not caring.
//!
//! # Two: whether a rejection was noticed
//!
//! An unhandled rejection is a language policy, not a scheduling mechanism, and
//! it has a rule that is easy to implement wrongly in a way nothing catches:
//!
//! > A rejection is unhandled if nothing is waiting on it **when the turn ends**
//! > — not when it settles.
//!
//! `const p = Promise.reject(); p.catch(f);` attaches a handler after the
//! rejection and is not an unhandled rejection. Reporting at settle time flags
//! it; reporting at the end of the turn does not. Getting this wrong produces a
//! warning for correct code, which teaches people to ignore the warning.

use rts_cranelift::sched::{PromiseId, Settlement};

use crate::value::Value;

/// The value each settled promise carries, and which rejections nobody has
/// looked at.
#[derive(Default)]
pub struct Settlements {
    /// Indexed by the promise's own index, so a lookup is an array read.
    values: Vec<Option<Value>>,
    /// Rejections with nothing waiting on them *so far this turn*.
    ///
    /// A list rather than a set, because `PromiseId` keeps its field private —
    /// nothing outside the machine may invent one — so it is neither `Hash` nor
    /// `Ord`. That is the machine being right, and a list is the honest shape:
    /// unhandled rejections are rare, so scanning a handful beats routing
    /// around a constraint that exists on purpose.
    unnoticed: Vec<PromiseId>,
}

impl Settlements {
    /// A table that has seen no settlements.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record what a promise settled with.
    ///
    /// `had_waiters` is whether anything was already waiting — which the
    /// machine's `SettleOutcome` reports, so this is not re-derived. A rejection
    /// nobody was waiting for becomes a candidate; it stops being one the moment
    /// something attaches, and it is only *reported* when the turn ends.
    pub fn record(
        &mut self,
        promise: PromiseId,
        settlement: Settlement,
        value: Value,
        had_waiters: bool,
    ) {
        let index = promise.index();
        if self.values.len() <= index {
            self.values.resize(index + 1, None);
        }

        // The first settlement wins, matching the machine's own rule: a second
        // is dropped rather than refused, because two paths racing is usually
        // what produced it.
        if self.values[index].is_some() {
            return;
        }
        self.values[index] = Some(value);

        if settlement == Settlement::Rejected && !had_waiters {
            self.unnoticed.push(promise);
        }
    }

    /// What a promise settled with, if it has.
    pub fn value_of(&self, promise: PromiseId) -> Option<Value> {
        self.values.get(promise.index()).copied().flatten()
    }

    /// Note that something is now waiting on a promise.
    ///
    /// Called when a handler attaches, whether the promise had already settled
    /// or not. A rejection that was a candidate stops being one — which is the
    /// whole of why the report waits for the end of the turn.
    pub fn noticed(&mut self, promise: PromiseId) {
        self.unnoticed
            .retain(|waiting| waiting.index() != promise.index());
    }

    /// Every rejection still unnoticed, and forget them.
    ///
    /// Called once at the end of a turn, which is what makes
    /// `Promise.reject().catch(f)` not a warning. Draining rather than reading
    /// means a rejection is reported once: the turn after, it is somebody's
    /// problem or nobody's, and repeating it every turn would bury the next one.
    pub fn take_unhandled(&mut self) -> Vec<PromiseId> {
        let mut ids: Vec<PromiseId> = core::mem::take(&mut self.unnoticed);
        // Sorted so a program's diagnostics come out in a stable order — the
        // same determinism rule the machine applies wherever a human reads the
        // output.
        ids.sort_unstable_by_key(|id| id.index());
        ids
    }

    /// Whether any rejection is currently unnoticed.
    pub fn has_unhandled(&self) -> bool {
        !self.unnoticed.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rts_cranelift::sched::{PromiseTable, SchedulerId};

    fn table() -> (PromiseTable, SchedulerId) {
        (PromiseTable::new(), SchedulerId(0))
    }

    #[test]
    fn a_promise_carries_the_value_the_machine_declines_to_hold() {
        let (mut promises, owner) = table();
        let id = promises.create(owner);

        let mut settlements = Settlements::new();
        assert_eq!(settlements.value_of(id), None, "nothing yet");

        settlements.record(id, Settlement::Fulfilled, Value::from_i32(42), false);
        assert_eq!(settlements.value_of(id), Some(Value::from_i32(42)));
    }

    #[test]
    fn the_first_settlement_wins() {
        let (mut promises, owner) = table();
        let id = promises.create(owner);

        let mut settlements = Settlements::new();
        settlements.record(id, Settlement::Fulfilled, Value::from_i32(1), false);
        settlements.record(id, Settlement::Fulfilled, Value::from_i32(2), false);

        assert_eq!(
            settlements.value_of(id),
            Some(Value::from_i32(1)),
            "matching the machine's own rule — two paths racing, not a mistake"
        );
    }

    #[test]
    fn a_rejection_caught_later_in_the_same_turn_is_not_unhandled() {
        let (mut promises, owner) = table();
        let id = promises.create(owner);

        let mut settlements = Settlements::new();
        // `const p = Promise.reject()` — nothing waiting yet.
        settlements.record(id, Settlement::Rejected, Value::from_i32(1), false);
        assert!(settlements.has_unhandled(), "a candidate, so far");

        // `p.catch(f)` — later in the same turn.
        settlements.noticed(id);

        assert!(
            settlements.take_unhandled().is_empty(),
            "reporting at settle time would warn about correct code, which \
             teaches people to ignore the warning"
        );
    }

    #[test]
    fn a_rejection_nobody_ever_looks_at_is_reported_once() {
        let (mut promises, owner) = table();
        let id = promises.create(owner);

        let mut settlements = Settlements::new();
        settlements.record(id, Settlement::Rejected, Value::from_i32(1), false);

        assert_eq!(settlements.take_unhandled(), vec![id]);
        assert!(
            settlements.take_unhandled().is_empty(),
            "the turn after, it is somebody's problem or nobody's — repeating \
             it every turn would bury the next one"
        );
    }

    #[test]
    fn a_rejection_something_was_already_waiting_for_is_never_a_candidate() {
        let (mut promises, owner) = table();
        let id = promises.create(owner);

        let mut settlements = Settlements::new();
        settlements.record(id, Settlement::Rejected, Value::from_i32(1), true);

        assert!(
            !settlements.has_unhandled(),
            "the machine's SettleOutcome already reports this, so it is not \
             re-derived"
        );
    }

    #[test]
    fn a_fulfilment_is_never_a_candidate_however_ignored() {
        let (mut promises, owner) = table();
        let id = promises.create(owner);

        let mut settlements = Settlements::new();
        settlements.record(id, Settlement::Fulfilled, Value::from_i32(1), false);
        assert!(!settlements.has_unhandled());
    }

    #[test]
    fn several_unhandled_rejections_come_back_in_a_stable_order() {
        let (mut promises, owner) = table();
        let first = promises.create(owner);
        let second = promises.create(owner);
        let third = promises.create(owner);

        let mut settlements = Settlements::new();
        for id in [third, first, second] {
            settlements.record(id, Settlement::Rejected, Value::from_i32(0), false);
        }

        assert_eq!(
            settlements.take_unhandled(),
            vec![first, second, third],
            "diagnostics must not depend on hash iteration order"
        );
    }
}
