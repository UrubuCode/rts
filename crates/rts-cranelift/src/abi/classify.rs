//! Classification: turning a signature into machine slots.
//!
//! One algorithm, parameterized by the target description. It answers two
//! questions the compiler underneath does not answer at all: how an aggregate
//! travels by value, and what happens when returns outgrow their registers.

use super::target::{AggregatePolicy, TargetAbi};
use super::{AbiType, Signature};
use crate::repr::Repr;
use crate::types::{AggregateLayout, TypeId, TypeRegistry};

/// One machine slot.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SlotSpec {
    /// What the slot holds.
    pub repr: Repr,
}

impl SlotSpec {
    /// A slot holding the given representation.
    pub fn of(repr: Repr) -> Self {
        Self { repr }
    }
}

/// How a parameter reaches the callee.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ParamClass {
    /// Directly, in the given slots.
    Direct(Vec<SlotSpec>),
    /// Through a pointer the caller supplies.
    ByReference,
}

/// How returns reach the caller.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ReturnClass {
    /// Nothing is returned.
    None,
    /// Directly, in the given slots.
    Direct(Vec<SlotSpec>),
    /// Through a pointer the caller supplies as a leading parameter.
    ///
    /// The slots describe the buffer's contents, so the caller knows how much to
    /// reserve and the callee knows where to write.
    OutPointer(Vec<SlotSpec>),
}

/// A signature reduced to machine slots.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LoweredSignature {
    /// One entry per declared parameter, in order.
    pub params: Vec<ParamClass>,
    /// How returns travel.
    pub returns: ReturnClass,
}

impl LoweredSignature {
    /// Whether the callee receives a leading pointer for its return value.
    pub fn has_out_pointer(&self) -> bool {
        matches!(self.returns, ReturnClass::OutPointer(_))
    }
}

/// Reduces a signature to machine slots for a target.
pub fn lower_signature(
    signature: &Signature,
    types: &TypeRegistry,
    target: &TargetAbi,
) -> LoweredSignature {
    let params = signature
        .params
        .iter()
        .map(|&ty| classify_param(ty, types, target))
        .collect();

    LoweredSignature { params, returns: classify_returns(&signature.returns, types, target) }
}

/// How one parameter travels.
fn classify_param(ty: AbiType, types: &TypeRegistry, target: &TargetAbi) -> ParamClass {
    match ty {
        AbiType::Scalar(repr) => ParamClass::Direct(vec![SlotSpec::of(repr)]),
        AbiType::Slice => {
            ParamClass::Direct(vec![SlotSpec::of(Repr::I64), SlotSpec::of(Repr::I64)])
        }
        AbiType::Aggregate(id) => match classify_aggregate(id, types, target) {
            Some(slots) => ParamClass::Direct(slots),
            None => ParamClass::ByReference,
        },
    }
}

/// How returns travel.
///
/// Two independent limits apply. The count of returns must be one a fixture has
/// proven on this target, and the slots must fit the return registers. Failing
/// either sends the whole return list through a pointer, which is the form that
/// is unambiguously supported everywhere.
fn classify_returns(
    returns: &[AbiType],
    types: &TypeRegistry,
    target: &TargetAbi,
) -> ReturnClass {
    if returns.is_empty() {
        return ReturnClass::None;
    }

    let mut slots = Vec::new();
    let mut forced_indirect = returns.len() > usize::from(target.max_direct_returns);

    for &ty in returns {
        match classify_param(ty, types, target) {
            ParamClass::Direct(mut direct) => slots.append(&mut direct),
            ParamClass::ByReference => forced_indirect = true,
        }
    }

    if forced_indirect || !slots_fit_return_registers(&slots, target) {
        // Even indirect, the buffer's contents must be described: the caller
        // reserves by them and the callee writes by them.
        let described = describe_returns(returns, types);
        ReturnClass::OutPointer(described)
    } else {
        ReturnClass::Direct(slots)
    }
}

/// Whether the slots fit the target's return registers.
fn slots_fit_return_registers(slots: &[SlotSpec], target: &TargetAbi) -> bool {
    let floats = slots.iter().filter(|s| s.repr.is_float()).count();
    let others = slots.len() - floats;
    floats <= usize::from(target.float_return_registers)
        && others <= usize::from(target.int_return_registers)
}

/// The contents of an out-pointer buffer, flattened.
fn describe_returns(returns: &[AbiType], types: &TypeRegistry) -> Vec<SlotSpec> {
    let mut described = Vec::new();
    for &ty in returns {
        match ty {
            AbiType::Scalar(repr) => described.push(SlotSpec::of(repr)),
            AbiType::Slice => {
                described.push(SlotSpec::of(Repr::I64));
                described.push(SlotSpec::of(Repr::I64));
            }
            AbiType::Aggregate(id) => {
                described.extend(
                    types.layout(id).fields.iter().map(|f| SlotSpec::of(f.repr)),
                );
            }
        }
    }
    described
}

/// The slots an aggregate travels in, or `None` when it must travel by reference.
///
/// One rule precedes every target policy: **an aggregate containing a value the
/// collector must trace always travels by reference.** Registers are not
/// addressable, so a traced value passed in one has no address for a root map to
/// name, and it would be invisible at the next safepoint. That is a machine
/// fact, not a target's choice, so it is checked before the policy is consulted.
fn classify_aggregate(
    id: TypeId,
    types: &TypeRegistry,
    target: &TargetAbi,
) -> Option<Vec<SlotSpec>> {
    let layout = types.layout(id);
    if layout.has_references() {
        return None;
    }
    if layout.size > target.aggregate_register_limit() {
        return None;
    }

    match target.aggregate {
        AggregatePolicy::Pieces { piece_bytes, .. } => Some(split_into_pieces(layout, piece_bytes)),

        AggregatePolicy::SingleRegister { .. } => {
            if !layout.size.is_power_of_two() {
                return None;
            }
            Some(vec![SlotSpec::of(piece_repr(layout, 0, layout.size))])
        }

        AggregatePolicy::Homogeneous { max_float_members, .. } => {
            if let Some(slots) = homogeneous_float_slots(layout, max_float_members) {
                return Some(slots);
            }
            Some(split_into_pieces(layout, 8))
        }
    }
}

/// Splits an aggregate into register-sized pieces, each classified by content.
fn split_into_pieces(layout: &AggregateLayout, piece_bytes: u32) -> Vec<SlotSpec> {
    let piece_count = layout.size.div_ceil(piece_bytes).max(1);
    (0..piece_count)
        .map(|i| {
            let start = i * piece_bytes;
            SlotSpec::of(piece_repr(layout, start, piece_bytes))
        })
        .collect()
}

/// What a piece of an aggregate holds.
///
/// A piece containing only floating-point fields travels in a floating-point
/// register; a piece containing anything else, or nothing at all, travels in an
/// integer register.
fn piece_repr(layout: &AggregateLayout, start: u32, length: u32) -> Repr {
    let end = start + length;
    let mut saw_field = false;

    for field in &layout.fields {
        let field_end = field.offset + field.repr.bit_width() / 8;
        if field.offset >= end || field_end <= start {
            continue;
        }
        saw_field = true;
        if !field.repr.is_float() {
            return Repr::I64;
        }
    }

    if saw_field { Repr::F64 } else { Repr::I64 }
}

/// Slots for an aggregate whose fields are all the same floating-point type.
fn homogeneous_float_slots(
    layout: &AggregateLayout,
    max_members: u32,
) -> Option<Vec<SlotSpec>> {
    let first = layout.fields.first()?.repr;
    if !first.is_float() {
        return None;
    }
    if layout.fields.len() as u32 > max_members {
        return None;
    }
    if !layout.fields.iter().all(|f| f.repr == first) {
        return None;
    }
    Some(layout.fields.iter().map(|f| SlotSpec::of(f.repr)).collect())
}
