//! Classification, exercised per target.
//!
//! The rules differ enough between targets that a single expectation would hide
//! a bug in two of them, so each claim names the target it holds for.

use rts_cranelift::abi::{
    AbiType, Convention, ParamClass, ReturnClass, Signature, SlotSpec, TargetAbi, lower_signature,
};
use rts_cranelift::repr::{RefKind, Repr};
use rts_cranelift::types::TypeRegistry;

fn slots(reprs: &[Repr]) -> Vec<SlotSpec> {
    reprs.iter().copied().map(SlotSpec::of).collect()
}

#[test]
fn a_scalar_parameter_is_one_slot() {
    let types = TypeRegistry::new();
    let sig = Signature::internal(vec![AbiType::Scalar(Repr::F64)], vec![]);
    let lowered = lower_signature(&sig, &types, &TargetAbi::x86_64_sysv());

    assert_eq!(
        lowered.params,
        vec![ParamClass::Direct(slots(&[Repr::F64]))]
    );
    assert_eq!(lowered.returns, ReturnClass::None);
}

#[test]
fn a_slice_is_two_slots_as_one_argument() {
    let types = TypeRegistry::new();
    let sig = Signature::internal(vec![AbiType::Slice], vec![]);
    let lowered = lower_signature(&sig, &types, &TargetAbi::x86_64_sysv());

    assert_eq!(
        lowered.params,
        vec![ParamClass::Direct(slots(&[Repr::I64, Repr::I64]))]
    );
}

#[test]
fn a_small_float_pair_travels_in_float_registers_on_sysv() {
    let mut types = TypeRegistry::new();
    let point = types.declare(&[Repr::F64, Repr::F64]);
    let sig = Signature::internal(vec![AbiType::Aggregate(point)], vec![]);
    let lowered = lower_signature(&sig, &types, &TargetAbi::x86_64_sysv());

    assert_eq!(
        lowered.params,
        vec![ParamClass::Direct(slots(&[Repr::F64, Repr::F64]))],
        "each eight-byte piece holds only floats, so each travels in a float register"
    );
}

#[test]
fn a_mixed_piece_travels_in_an_integer_register() {
    let mut types = TypeRegistry::new();
    // Two 32-bit fields of different domains share one eight-byte piece.
    let mixed = types.declare(&[Repr::I32, Repr::F32]);
    let sig = Signature::internal(vec![AbiType::Aggregate(mixed)], vec![]);
    let lowered = lower_signature(&sig, &types, &TargetAbi::x86_64_sysv());

    assert_eq!(
        lowered.params,
        vec![ParamClass::Direct(slots(&[Repr::I64]))]
    );
}

#[test]
fn the_same_aggregate_travels_by_reference_on_windows() {
    let mut types = TypeRegistry::new();
    let point = types.declare(&[Repr::F64, Repr::F64]);
    let sig = Signature::internal(vec![AbiType::Aggregate(point)], vec![]);
    let lowered = lower_signature(&sig, &types, &TargetAbi::x86_64_windows());

    assert_eq!(
        lowered.params,
        vec![ParamClass::ByReference],
        "there is no two-register case here; sixteen bytes exceeds one register"
    );
}

#[test]
fn a_windows_aggregate_of_a_non_power_of_two_size_travels_by_reference() {
    let mut types = TypeRegistry::new();
    let three_bytes = types.declare(&[Repr::Bool, Repr::Bool, Repr::Bool]);
    let sig = Signature::internal(vec![AbiType::Aggregate(three_bytes)], vec![]);
    let lowered = lower_signature(&sig, &types, &TargetAbi::x86_64_windows());

    assert_eq!(lowered.params, vec![ParamClass::ByReference]);
}

#[test]
fn a_homogeneous_float_aggregate_keeps_one_register_per_member_on_aarch64() {
    let mut types = TypeRegistry::new();
    let quad = types.declare(&[Repr::F32, Repr::F32, Repr::F32, Repr::F32]);
    let sig = Signature::internal(vec![AbiType::Aggregate(quad)], vec![]);
    let lowered = lower_signature(&sig, &types, &TargetAbi::aarch64());

    assert_eq!(
        lowered.params,
        vec![ParamClass::Direct(slots(&[
            Repr::F32,
            Repr::F32,
            Repr::F32,
            Repr::F32
        ]))]
    );
}

#[test]
fn an_aggregate_holding_a_reference_never_travels_in_registers() {
    let mut types = TypeRegistry::new();
    let boxed = types.declare(&[Repr::Ref(RefKind::Bytes)]);
    let sig = Signature::internal(vec![AbiType::Aggregate(boxed)], vec![]);

    for target in [
        TargetAbi::x86_64_sysv(),
        TargetAbi::x86_64_windows(),
        TargetAbi::aarch64(),
    ] {
        let lowered = lower_signature(&sig, &types, &target);
        assert_eq!(
            lowered.params,
            vec![ParamClass::ByReference],
            "{}: a register has no address, so a traced value in one is invisible to a root map",
            target.name
        );
    }
}

#[test]
fn a_generic_value_inside_an_aggregate_is_traced_too() {
    let mut types = TypeRegistry::new();
    let maybe = types.declare(&[Repr::Tagged]);
    let sig = Signature::internal(vec![AbiType::Aggregate(maybe)], vec![]);
    let lowered = lower_signature(&sig, &types, &TargetAbi::x86_64_sysv());

    assert_eq!(
        lowered.params,
        vec![ParamClass::ByReference],
        "a generic value may hold a reference, and nothing here can prove otherwise"
    );
}

#[test]
fn one_return_travels_in_a_register() {
    let types = TypeRegistry::new();
    let sig = Signature::internal(vec![], vec![AbiType::Scalar(Repr::F64)]);
    let lowered = lower_signature(&sig, &types, &TargetAbi::x86_64_sysv());

    assert_eq!(lowered.returns, ReturnClass::Direct(slots(&[Repr::F64])));
    assert!(!lowered.has_out_pointer());
}

#[test]
fn an_unproven_return_count_falls_back_to_a_pointer() {
    let types = TypeRegistry::new();
    let sig = Signature::internal(
        vec![],
        vec![AbiType::Scalar(Repr::I64), AbiType::Scalar(Repr::F64)],
    );
    let lowered = lower_signature(&sig, &types, &TargetAbi::x86_64_sysv());

    assert_eq!(
        lowered.returns,
        ReturnClass::OutPointer(slots(&[Repr::I64, Repr::F64])),
        "the count is not proven on this target, so it fails as an indirection"
    );
    assert!(lowered.has_out_pointer());
}

#[test]
fn a_proven_return_count_travels_in_registers() {
    let types = TypeRegistry::new();
    let target = TargetAbi::x86_64_sysv().with_verified_direct_returns(2);
    let sig = Signature::internal(
        vec![],
        vec![AbiType::Scalar(Repr::I64), AbiType::Scalar(Repr::F64)],
    );
    let lowered = lower_signature(&sig, &types, &target);

    assert_eq!(
        lowered.returns,
        ReturnClass::Direct(slots(&[Repr::I64, Repr::F64]))
    );
}

#[test]
fn returns_that_outgrow_their_registers_use_a_pointer_even_when_the_count_is_proven() {
    let types = TypeRegistry::new();
    // The count is allowed, but three integers exceed two integer return registers.
    let target = TargetAbi::x86_64_sysv().with_verified_direct_returns(4);
    let sig = Signature::internal(
        vec![],
        vec![
            AbiType::Scalar(Repr::I64),
            AbiType::Scalar(Repr::I64),
            AbiType::Scalar(Repr::I64),
        ],
    );
    let lowered = lower_signature(&sig, &types, &target);

    assert!(
        lowered.has_out_pointer(),
        "the register budget is a second, independent limit"
    );
}

#[test]
fn an_out_pointer_still_describes_what_it_holds() {
    let mut types = TypeRegistry::new();
    let pair = types.declare(&[Repr::I32, Repr::F64]);
    let sig = Signature::internal(vec![], vec![AbiType::Aggregate(pair), AbiType::Slice]);
    let lowered = lower_signature(&sig, &types, &TargetAbi::x86_64_windows());

    assert_eq!(
        lowered.returns,
        ReturnClass::OutPointer(slots(&[Repr::I32, Repr::F64, Repr::I64, Repr::I64])),
        "the caller reserves by this description and the callee writes by it"
    );
}

#[test]
fn a_tail_call_needs_both_sides_to_permit_it_and_the_returns_to_match() {
    let ret = vec![AbiType::Scalar(Repr::I64)];
    let caller = Signature::internal(vec![], ret.clone()).with_tail_calls();
    let callee =
        Signature::internal(vec![AbiType::Scalar(Repr::I64)], ret.clone()).with_tail_calls();
    let ordinary = Signature::internal(vec![], ret.clone());
    let other_returns =
        Signature::internal(vec![], vec![AbiType::Scalar(Repr::F64)]).with_tail_calls();

    assert!(caller.permits_tail_call_to(&callee));
    assert!(
        !caller.permits_tail_call_to(&ordinary),
        "the group compiles as a unit or not at all"
    );
    assert!(!ordinary.permits_tail_call_to(&callee));
    assert!(!caller.permits_tail_call_to(&other_returns));
}

#[test]
fn only_the_foreign_convention_is_stable_across_a_boundary() {
    assert!(Convention::Foreign.is_stable());
    assert!(!Convention::Internal.is_stable());
    assert!(!Convention::InternalTail.is_stable());
    assert!(Convention::InternalTail.permits_tail_calls());
    assert!(!Convention::Internal.permits_tail_calls());
}
