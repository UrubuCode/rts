//! Suspension, exercised end to end.
//!
//! The claims under test are that what a parked frame preserves is derived
//! rather than declared, that a suspension is also a point the collector must be
//! able to read, and that suspending inside a protected region keeps the region.

use rts_cranelift::frame::plan_suspension;
use rts_cranelift::gc::describe_frames;
use rts_cranelift::ir::{FuncBuilder, Function, NumOp, Region, Signature, ValueId};
use rts_cranelift::repr::{RefKind, Repr};
use rts_cranelift::types::TypeRegistry;
use rts_cranelift::unwind::Tag;
use rts_cranelift::verify::{VerifyError, verify};

/// A function that is allowed to park its frame.
fn suspending(params: &[Repr], returns: &[Repr]) -> Function {
    Function::new(Signature {
        params: params.to_vec(),
        returns: returns.to_vec(),
        may_suspend: true,
    })
}

fn param(func: &Function, index: usize) -> ValueId {
    func.block(func.entry).expect("entry exists").params[index]
}

/// The values a function's parked frame preserves.
fn preserved(func: &Function) -> Vec<ValueId> {
    plan_suspension(func)
        .spill
        .slots()
        .iter()
        .map(|s| s.value)
        .collect()
}

#[test]
fn a_function_that_never_parks_plans_nothing() {
    let types = TypeRegistry::new();
    let mut func = suspending(&[Repr::F64], &[Repr::F64]);
    let x = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.ret(&[x]);

    assert!(plan_suspension(&func).is_empty());
}

#[test]
fn everything_live_across_a_suspension_is_preserved_not_only_references() {
    let types = TypeRegistry::new();
    let mut func = suspending(
        &[Repr::F64, Repr::Ref(RefKind::Bytes)],
        &[Repr::F64, Repr::Tagged],
    );
    let (number, reference) = (param(&func, 0), param(&func, 1));

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.suspend();
    let doubled = b.arith(NumOp::Add, number, number).expect("proven f64");
    let widened = b.widen(reference);
    b.ret(&[doubled, widened]);

    let kept = preserved(&func);
    assert!(
        kept.contains(&number),
        "a number that survives a suspension is as necessary to resuming as a reference is"
    );
    assert!(kept.contains(&reference));
    assert_eq!(
        kept.len(),
        2,
        "and asking a client to tell them apart is asking it to err"
    );
}

#[test]
fn a_value_nothing_reads_after_a_suspension_is_not_preserved() {
    let types = TypeRegistry::new();
    let mut func = suspending(&[Repr::F64], &[]);
    let unused_after = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let _ = b
        .arith(NumOp::Add, unused_after, unused_after)
        .expect("proven");
    b.suspend();
    b.ret(&[]);

    assert!(
        preserved(&func).is_empty(),
        "the record holds what resuming needs, nothing else"
    );
}

#[test]
fn the_resumed_value_is_not_something_the_frame_preserves() {
    let types = TypeRegistry::new();
    let mut func = suspending(&[], &[Repr::Tagged]);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let delivered = b.suspend();
    b.ret(&[delivered]);

    assert!(
        !preserved(&func).contains(&delivered),
        "it is the value resumption delivers, so it does not exist to be preserved"
    );
}

#[test]
fn a_value_live_across_two_suspensions_occupies_one_slot() {
    let types = TypeRegistry::new();
    let mut func = suspending(&[Repr::F64], &[Repr::F64]);
    let carried = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.suspend();
    b.suspend();
    b.ret(&[carried]);

    let plan = plan_suspension(&func);
    assert_eq!(plan.points.len(), 2);
    assert_eq!(
        plan.spill.len(),
        1,
        "resuming at one point and suspending at another must agree on one layout"
    );
}

#[test]
fn a_proven_value_keeps_its_representation_in_the_record() {
    let types = TypeRegistry::new();
    let mut func = suspending(&[Repr::F64], &[Repr::F64]);
    let number = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.suspend();
    b.ret(&[number]);

    let plan = plan_suspension(&func);
    assert_eq!(
        plan.spill.slots()[0].repr,
        Repr::F64,
        "widening would pay for a representation change on every suspension"
    );
}

#[test]
fn resume_labels_number_the_points_in_program_order() {
    let types = TypeRegistry::new();
    let mut func = suspending(&[], &[]);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.suspend();
    b.suspend();
    b.suspend();
    b.ret(&[]);

    let plan = plan_suspension(&func);
    let labels: Vec<_> = plan.points.iter().map(|(_, label)| label.0).collect();
    assert_eq!(
        labels,
        vec![0, 1, 2],
        "a resumption is a jump selected by a number"
    );
}

#[test]
fn a_suspension_is_a_point_the_collector_must_be_able_to_read() {
    let types = TypeRegistry::new();
    let mut func = suspending(&[Repr::Ref(RefKind::Bytes)], &[Repr::Ref(RefKind::Bytes)]);
    let held = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.suspend();
    b.ret(&[held]);

    let table = describe_frames(&func);
    let frame = table.iter().next().expect("the suspension is described");
    assert_eq!(
        frame.roots.iter().map(|r| r.value).collect::<Vec<_>>(),
        vec![held],
        "a parked frame can sit across any number of collections"
    );
    assert!(
        frame.resume_label.is_some(),
        "and it is also where control comes back"
    );
}

#[test]
fn suspending_inside_a_protected_region_keeps_the_region() {
    let types = TypeRegistry::new();
    let mut func = suspending(&[Repr::Tagged], &[Repr::Tagged]);
    let value = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let cleanup = b.create_block();
    let region = b.declare_region(None, vec![], Some(cleanup));
    b.place_in_region(entry, region);
    b.suspend();
    b.ret(&[value]);

    let mut b = FuncBuilder::new(&mut func, &types, cleanup);
    b.throw(Tag(1), value);

    let table = describe_frames(&func);
    let frame = table.iter().next().expect("described");
    assert_eq!(
        (frame.region, frame.resume_label.is_some()),
        (Some(region), true),
        "resuming re-establishes the cleanup chain the frame suspended inside"
    );
}

#[test]
fn an_allocation_and_a_suspension_are_both_described_but_only_one_resumes() {
    let mut types = TypeRegistry::new();
    let ty = types.declare(&[Repr::I64]);

    let mut func = suspending(&[Repr::Ref(RefKind::Bytes)], &[Repr::Ref(RefKind::Bytes)]);
    let held = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.alloc(ty, Region::Local);
    b.suspend();
    b.ret(&[held]);

    let table = describe_frames(&func);
    let frames: Vec<_> = table.iter().collect();
    assert_eq!(frames.len(), 2);
    assert!(
        frames[0].resume_label.is_none(),
        "an allocation is not a resumption target"
    );
    assert!(frames[1].resume_label.is_some());
}

#[test]
fn parking_a_frame_that_did_not_declare_it_may_is_rejected() {
    let types = TypeRegistry::new();
    let mut func = Function::new(Signature {
        params: vec![],
        returns: vec![],
        may_suspend: false,
    });

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.suspend();
    b.ret(&[]);

    let errors = verify(&func, &types);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, VerifyError::UndeclaredSuspension { .. })),
        "a function that suspends while claiming not to makes every caller's decision wrong"
    );
}

#[test]
fn the_record_does_not_depend_on_hashing() {
    let types = TypeRegistry::new();
    let mut func = suspending(
        &[Repr::F64, Repr::Ref(RefKind::Bytes), Repr::Tagged],
        &[Repr::Tagged],
    );
    let (a, b_ref, c) = (param(&func, 0), param(&func, 1), param(&func, 2));

    let entry = func.entry;
    let mut builder = FuncBuilder::new(&mut func, &types, entry);
    builder.suspend();
    let _ = builder.arith(NumOp::Add, a, a).expect("proven");
    let _ = builder.widen(b_ref);
    builder.ret(&[c]);

    assert_eq!(
        plan_suspension(&func),
        plan_suspension(&func),
        "a record whose shape changes for no reason is one nobody can reason about"
    );
}
