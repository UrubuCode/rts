//! The spill record: where a parked frame's state lives.
//!
//! A frame that parks cannot keep its state in registers or on the machine
//! stack, because both are gone the moment it stops running. Everything that
//! must outlive a suspension moves into an explicit record, and the record is
//! what the frame *is* while it is not running.
//!
//! The layout is computed once per function, over the union of every suspension
//! point, rather than once per point. A value live across two suspensions
//! occupies the same slot at both — otherwise resuming at one point and
//! suspending at another would have to copy between two layouts of the same
//! frame, which is work whose only purpose is to undo a choice made here.

use std::collections::HashMap;

use crate::ir::ValueId;
use crate::repr::Repr;

/// One value's place in the record.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SpillSlot {
    /// The value this slot holds across a suspension.
    pub value: ValueId,
    /// Its position in the record.
    pub index: u32,
    /// How it exists on the machine.
    ///
    /// Kept per slot rather than uniform: forcing everything into the generic
    /// form would widen values that were proven, and pay for a representation
    /// change on every suspension of a loop that suspends often.
    pub repr: Repr,
}

/// Every value a function must preserve across a suspension.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct SpillLayout {
    slots: Vec<SpillSlot>,
    by_value: HashMap<ValueId, u32>,
}

impl SpillLayout {
    /// Builds a layout from the values that must survive, in a stable order.
    ///
    /// Ordering is by value rather than by discovery, so two runs over one
    /// function produce the same record. A layout that depends on iteration
    /// order cannot be compared between builds, and a frame record whose shape
    /// changes for no reason is one nobody can reason about.
    pub fn build(mut values: Vec<(ValueId, Repr)>) -> Self {
        values.sort_by_key(|(value, _)| *value);
        values.dedup_by_key(|(value, _)| *value);

        let mut layout = SpillLayout::default();
        for (index, (value, repr)) in values.into_iter().enumerate() {
            let index = index as u32;
            layout.by_value.insert(value, index);
            layout.slots.push(SpillSlot { value, index, repr });
        }
        layout
    }

    /// Where a value lives, if it is preserved at all.
    pub fn slot_of(&self, value: ValueId) -> Option<u32> {
        self.by_value.get(&value).copied()
    }

    /// Every slot, in record order.
    pub fn slots(&self) -> &[SpillSlot] {
        &self.slots
    }

    /// How many values the record holds.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether nothing needs preserving.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Function, Signature};

    fn values(count: usize) -> Vec<ValueId> {
        let func = Function::new(Signature {
            params: vec![Repr::I64; count],
            ..Signature::default()
        });
        let entry = func.entry;
        func.block(entry).expect("entry exists").params.clone()
    }

    #[test]
    fn a_value_appearing_twice_occupies_one_slot() {
        let v = values(2);
        let layout = SpillLayout::build(vec![
            (v[0], Repr::I64),
            (v[1], Repr::F64),
            (v[0], Repr::I64),
        ]);

        assert_eq!(
            layout.len(),
            2,
            "resuming and suspending must agree on one layout"
        );
        assert_eq!(layout.slot_of(v[0]), Some(0));
    }

    #[test]
    fn the_order_does_not_depend_on_discovery() {
        let v = values(3);
        let forward = SpillLayout::build(vec![
            (v[0], Repr::I64),
            (v[1], Repr::I64),
            (v[2], Repr::I64),
        ]);
        let backward = SpillLayout::build(vec![
            (v[2], Repr::I64),
            (v[1], Repr::I64),
            (v[0], Repr::I64),
        ]);

        assert_eq!(
            forward, backward,
            "a record whose shape changes for no reason is unreadable"
        );
    }

    #[test]
    fn a_proven_value_keeps_its_representation() {
        let v = values(1);
        let layout = SpillLayout::build(vec![(v[0], Repr::F64)]);

        assert_eq!(
            layout.slots()[0].repr,
            Repr::F64,
            "widening here would pay for a representation change on every suspension"
        );
    }
}
