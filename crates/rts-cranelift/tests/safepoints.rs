//! Root derivation, exercised end to end.
//!
//! The claim under test is that the root set is *derived*, so that no program
//! can be built which forgets to report a reference. Each test names the case
//! that would otherwise be reported wrongly.

use rts_cranelift::gc::describe_frames;
use rts_cranelift::ir::{FuncBuilder, Function, NumOp, Region, Signature, ValueId};
use rts_cranelift::repr::{RefKind, Repr};
use rts_cranelift::types::TypeRegistry;

fn function(params: &[Repr], returns: &[Repr]) -> Function {
    Function::new(Signature { params: params.to_vec(), returns: returns.to_vec() })
}

fn param(func: &Function, index: usize) -> ValueId {
    func.block(func.entry).expect("entry exists").params[index]
}

/// The values reported at the only described point of a function.
fn only_frame_roots(func: &Function) -> Vec<ValueId> {
    let table = describe_frames(func);
    assert_eq!(table.len(), 1, "this function has exactly one point that can collect");
    table.iter().next().expect("one frame").roots.iter().map(|r| r.value).collect()
}

#[test]
fn a_function_that_never_allocates_describes_nothing() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::F64, Repr::F64], &[Repr::F64]);
    let (x, y) = (param(&func, 0), param(&func, 1));

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let sum = b.arith(NumOp::Add, x, y).expect("proven");
    b.ret(&[sum]);

    assert!(describe_frames(&func).is_empty(), "no allocation means no point that can collect");
}

#[test]
fn a_reference_live_across_an_allocation_is_reported() {
    let mut types = TypeRegistry::new();
    let ty = types.declare(&[Repr::I64]);

    let mut func = function(&[Repr::Ref(RefKind::Bytes)], &[Repr::Ref(RefKind::Bytes)]);
    let held = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.alloc(ty, Region::Local);
    b.ret(&[held]);

    assert_eq!(
        only_frame_roots(&func),
        vec![held],
        "the parameter is read after the allocation, so it must survive it"
    );
}

#[test]
fn a_reference_nothing_reads_afterwards_is_not_reported() {
    let mut types = TypeRegistry::new();
    let ty = types.declare(&[Repr::I64]);

    let mut func = function(&[Repr::Ref(RefKind::Bytes)], &[]);
    let unused_after = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.alloc(ty, Region::Local);
    b.ret(&[]);
    let _ = unused_after;

    assert_eq!(
        only_frame_roots(&func),
        Vec::<ValueId>::new(),
        "keeping it alive would extend a lifetime for nothing"
    );
}

#[test]
fn a_number_live_across_an_allocation_is_not_a_root() {
    let mut types = TypeRegistry::new();
    let ty = types.declare(&[Repr::I64]);

    let mut func = function(&[Repr::F64], &[Repr::F64]);
    let number = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.alloc(ty, Region::Local);
    b.ret(&[number]);

    assert_eq!(
        only_frame_roots(&func),
        Vec::<ValueId>::new(),
        "a conservative scan cannot tell this from a reference; a derived set can"
    );
}

#[test]
fn a_generic_value_is_reported_without_claiming_what_it_holds() {
    let mut types = TypeRegistry::new();
    let ty = types.declare(&[Repr::I64]);

    let mut func = function(&[Repr::Tagged], &[Repr::Tagged]);
    let unknown = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.alloc(ty, Region::Local);
    b.ret(&[unknown]);

    let table = describe_frames(&func);
    let frame = table.iter().next().expect("one frame");
    assert_eq!(frame.roots.len(), 1);
    assert_eq!(frame.roots[0].value, unknown);
    assert_eq!(
        frame.roots[0].kind, None,
        "it may hold a reference of any kind; a claim here would be a guess the collector trusts"
    );
}

#[test]
fn the_allocations_own_result_is_not_live_across_itself() {
    let mut types = TypeRegistry::new();
    let ty = types.declare(&[Repr::I64]);

    let mut func = function(&[], &[Repr::Ref(RefKind::Aggregate(ty))]);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let obj = b.alloc(ty, Region::Local);
    b.ret(&[obj]);

    assert_eq!(
        only_frame_roots(&func),
        Vec::<ValueId>::new(),
        "it does not exist before the point that produces it"
    );
}

#[test]
fn an_earlier_allocation_is_reported_at_a_later_one() {
    let mut types = TypeRegistry::new();
    let ty = types.declare(&[Repr::I64]);

    let mut func = function(&[], &[Repr::Ref(RefKind::Aggregate(ty))]);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let first = b.alloc(ty, Region::Local);
    b.alloc(ty, Region::Local);
    b.ret(&[first]);

    let table = describe_frames(&func);
    assert_eq!(table.len(), 2, "both allocations can collect");

    let frames: Vec<_> = table.iter().collect();
    assert!(frames[0].roots.is_empty(), "nothing is live across the first");
    assert_eq!(
        frames[1].roots.iter().map(|r| r.value).collect::<Vec<_>>(),
        vec![first],
        "the first allocation's result must survive the second"
    );
}

#[test]
fn a_reference_live_through_a_branch_is_reported_on_both_paths() {
    let mut types = TypeRegistry::new();
    let ty = types.declare(&[Repr::I64]);

    let mut func = function(&[Repr::Bool, Repr::Ref(RefKind::Bytes)], &[Repr::Ref(RefKind::Bytes)]);
    let (cond, held) = (param(&func, 0), param(&func, 1));

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let left = b.create_block();
    let right = b.create_block();
    b.branch(cond, (left, &[]), (right, &[])).expect("proven boolean");

    for block in [left, right] {
        let mut b = FuncBuilder::new(&mut func, &types, block);
        b.alloc(ty, Region::Local);
        b.ret(&[held]);
    }

    let table = describe_frames(&func);
    assert_eq!(table.len(), 2);
    for frame in table.iter() {
        assert_eq!(
            frame.roots.iter().map(|r| r.value).collect::<Vec<_>>(),
            vec![held],
            "liveness crosses block boundaries; a per-block view would miss this"
        );
    }
}

#[test]
fn a_reference_carried_around_a_loop_stays_reported() {
    let mut types = TypeRegistry::new();
    let ty = types.declare(&[Repr::I64]);

    let mut func = function(&[Repr::Bool, Repr::Ref(RefKind::Bytes)], &[]);
    let (cond, initial) = (param(&func, 0), param(&func, 1));

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let header = b.create_block();
    let exit = b.create_block();
    let carried = b.add_block_param(header, Repr::Ref(RefKind::Bytes));
    b.jump(header, &[initial]).expect("matching representation");

    let mut b = FuncBuilder::new(&mut func, &types, header);
    b.alloc(ty, Region::Local);
    b.branch(cond, (header, &[carried]), (exit, &[])).expect("proven boolean");

    let mut b = FuncBuilder::new(&mut func, &types, exit);
    b.ret(&[]);

    assert_eq!(
        only_frame_roots(&func),
        vec![carried],
        "the back edge keeps it live; reaching that needs a fixed point, not one pass"
    );
}

#[test]
fn the_reported_order_does_not_depend_on_hashing() {
    let mut types = TypeRegistry::new();
    let ty = types.declare(&[Repr::I64]);

    let mut func = function(
        &[Repr::Ref(RefKind::Bytes), Repr::Ref(RefKind::Callable), Repr::Tagged],
        &[Repr::Tagged],
    );
    let (a, b_ref, c) = (param(&func, 0), param(&func, 1), param(&func, 2));

    let entry = func.entry;
    let mut builder = FuncBuilder::new(&mut func, &types, entry);
    builder.alloc(ty, Region::Local);
    let joined = builder.generic(rts_cranelift::ir::GenericOp::Add, a, b_ref);
    let _ = builder.generic(rts_cranelift::ir::GenericOp::Add, joined, c);
    builder.ret(&[c]);

    let first = describe_frames(&func);
    let second = describe_frames(&func);
    assert_eq!(
        first, second,
        "a set that depends on hash order cannot be compared between builds"
    );
    assert!(first.iter().all(|f| f.roots.windows(2).all(|w| w[0].value < w[1].value)));
}

#[test]
fn a_described_point_is_found_by_lookup() {
    let mut types = TypeRegistry::new();
    let ty = types.declare(&[Repr::I64]);

    let mut func = function(&[Repr::Ref(RefKind::Bytes)], &[Repr::Ref(RefKind::Bytes)]);
    let held = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.alloc(ty, Region::Local);
    b.ret(&[held]);

    let table = describe_frames(&func);
    let at = table.iter().next().expect("one frame").at;
    assert!(table.lookup(at).is_some(), "the consumer looks up a point it discovered at run time");
}
