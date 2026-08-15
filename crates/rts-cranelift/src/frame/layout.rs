//! What a parked frame looks like in memory.
//!
//! The frame record holds everything the function needs to be picked back up:
//! where it left off, what it was given, what it will return, and every value
//! that has to survive a suspension.
//!
//! It is an ordinary registered aggregate. That is deliberate — a frame that had
//! its own layout rules would be a second set of rules to keep in agreement with
//! the first, and the collector already knows how to trace an aggregate.

use std::collections::HashMap;

use crate::ir::{Function, ValueId};
use crate::repr::Repr;
use crate::types::{TypeId, TypeRegistry};

use super::SuspendPlan;

/// Where each piece of a parked frame lives.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FrameLayout {
    /// The aggregate the record is an instance of.
    pub ty: TypeId,
    /// Where the resume label is kept.
    pub label_field: u32,
    /// Where the value a resumption delivers is left for the frame to read.
    ///
    /// Written by whoever resumes, read by the frame. One slot rather than one
    /// per suspension point: only one suspension can be outstanding at a time,
    /// which is what "parked" means.
    pub resumed_field: u32,
    /// Where the way this resumption was made is left for the frame to read.
    ///
    /// One of [`super::ResumeMode`]'s numbers, written by whoever resumes.
    /// Beside the delivered value rather than encoded into it: the delivered
    /// value is whatever the client's representation says, and a machine layer
    /// that stole bits from it would be deciding what a client may deliver.
    ///
    /// One slot, for the reason [`Self::resumed_field`] is one — only one
    /// suspension can be outstanding.
    pub mode_field: u32,
    /// Where the function's own parameters live.
    ///
    /// In the frame rather than in registers because a resumed frame is entered
    /// afresh, with nothing in the registers it had before — so anything the
    /// function still needs has to have been written down.
    pub param_fields: Vec<u32>,
    /// Where the function's return values are left.
    pub return_fields: Vec<u32>,
    /// Where each value that outlives a suspension lives.
    pub spill_fields: HashMap<ValueId, u32>,
}

impl FrameLayout {
    /// Declares the record a function's parked frame needs.
    ///
    /// The order is fixed — label, resumed value, resume mode, parameters,
    /// returns, spills — so that two builds of one function produce the same
    /// record, and a reader comparing them sees a change only where something
    /// changed.
    pub fn declare(func: &Function, plan: &SuspendPlan, types: &mut TypeRegistry) -> Self {
        let mut fields = vec![Repr::I64, Repr::Tagged, Repr::I64];
        let label_field = 0;
        let resumed_field = 1;
        let mode_field = 2;

        let param_fields = extend(&mut fields, func.signature.params.iter().copied());
        let return_fields = extend(&mut fields, func.signature.returns.iter().copied());

        // A parameter already has a slot, and one that also outlives a
        // suspension does not need a second. Two slots for one value is not
        // merely wasteful: whichever is written last wins, and which that is
        // depends on the order the rewrite happened to visit things in.
        let mut spill_fields: HashMap<ValueId, u32> = func
            .block(func.entry)
            .expect("a function has an entry block")
            .params
            .iter()
            .zip(&param_fields)
            .map(|(&value, &field)| (value, field))
            .collect();

        for slot in plan.spill.slots() {
            if spill_fields.contains_key(&slot.value) {
                continue;
            }
            spill_fields.insert(slot.value, fields.len() as u32);
            fields.push(slot.repr);
        }

        Self {
            ty: types.declare(&fields),
            label_field,
            resumed_field,
            mode_field,
            param_fields,
            return_fields,
            spill_fields,
        }
    }

    /// Where a value lives, if it has to survive a suspension.
    pub fn slot_of(&self, value: ValueId) -> Option<u32> {
        self.spill_fields.get(&value).copied()
    }

    /// Whether a value has to survive a suspension.
    pub fn survives(&self, value: ValueId) -> bool {
        self.spill_fields.contains_key(&value)
    }
}

/// Appends representations and returns the field indices they took.
fn extend(fields: &mut Vec<Repr>, reprs: impl Iterator<Item = Repr>) -> Vec<u32> {
    reprs
        .map(|repr| {
            let index = fields.len() as u32;
            fields.push(repr);
            index
        })
        .collect()
}
