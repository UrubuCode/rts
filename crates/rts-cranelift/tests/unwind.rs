//! Unwinding, exercised end to end.
//!
//! The claims under test are that the cleanup chain is derived rather than
//! remembered, that a handler cannot be written without receiving what it
//! catches, and that the region a point belongs to reaches the same record its
//! roots do.

use rts_cranelift::gc::describe_frames;
use rts_cranelift::ir::FuncRegistry;
use rts_cranelift::ir::{FuncBuilder, Function, Region, Signature, ValueId};
use rts_cranelift::repr::{RefKind, Repr};
use rts_cranelift::types::TypeRegistry;
use rts_cranelift::unwind::{Handler, Tag, plan_all_throws};
use rts_cranelift::verify::{VerifyError, verify};

fn function(params: &[Repr], returns: &[Repr]) -> Function {
    Function::new(Signature {
        params: params.to_vec(),
        returns: returns.to_vec(),
        ..Signature::default()
    })
}

fn param(func: &Function, index: usize) -> ValueId {
    func.block(func.entry).expect("entry exists").params[index]
}

#[test]
fn a_throw_reaches_a_handler_that_catches_its_tag() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::Tagged], &[Repr::Tagged]);
    let value = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let caught = b.create_block();
    b.add_block_param(caught, Repr::Tagged);
    b.open_region(
        vec![Handler {
            tag: Tag(1),
            block: caught,
        }],
        None,
    );
    b.throw(Tag(1), value);

    let recovered = func.block(caught).expect("exists").params[0];
    let mut b = FuncBuilder::new(&mut func, &types, caught);
    b.ret(&[recovered]);

    let plans = plan_all_throws(&func);
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].1.handler, Some(caught));
    assert!(!plans[0].1.escapes());
    assert_eq!(verify(&func, &types, &FuncRegistry::new()), vec![]);
}

#[test]
fn a_tag_nothing_catches_leaves_the_function() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::Tagged], &[]);
    let value = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let caught = b.create_block();
    b.add_block_param(caught, Repr::Tagged);
    b.open_region(
        vec![Handler {
            tag: Tag(1),
            block: caught,
        }],
        None,
    );
    b.throw(Tag(2), value);

    let mut b = FuncBuilder::new(&mut func, &types, caught);
    b.ret(&[]);

    let plans = plan_all_throws(&func);
    assert!(
        plans[0].1.escapes(),
        "the caller resumes the search from its own region"
    );
}

#[test]
fn cleanup_runs_on_the_way_out_even_when_nothing_catches() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::Tagged], &[]);
    let value = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let cleanup = b.create_block();
    let _region = b.open_region(vec![], Some(cleanup));
    b.throw(Tag(1), value);

    let mut b = FuncBuilder::new(&mut func, &types, cleanup);
    b.ret(&[]);

    let plans = plan_all_throws(&func);
    assert!(plans[0].1.escapes());
    assert_eq!(
        plans[0].1.cleanups,
        vec![cleanup],
        "an escaping throw still owes every scope it leaves"
    );
}

#[test]
fn nested_regions_clean_up_innermost_first() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::Tagged], &[]);
    let value = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let outer_cleanup = b.create_block();
    let inner_cleanup = b.create_block();
    let caught = b.create_block();
    b.add_block_param(caught, Repr::Tagged);

    b.open_region(
        vec![Handler {
            tag: Tag(1),
            block: caught,
        }],
        Some(outer_cleanup),
    );
    b.open_region(vec![], Some(inner_cleanup));
    b.throw(Tag(1), value);

    for block in [outer_cleanup, inner_cleanup] {
        let mut b = FuncBuilder::new(&mut func, &types, block);
        b.ret(&[]);
    }
    let mut b = FuncBuilder::new(&mut func, &types, caught);
    b.ret(&[]);

    let plans = plan_all_throws(&func);
    assert_eq!(plans[0].1.cleanups, vec![inner_cleanup, outer_cleanup]);
    assert_eq!(plans[0].1.handler, Some(caught));
}

#[test]
fn a_throw_outside_every_region_leaves_immediately() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::Tagged], &[]);
    let value = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.throw(Tag(1), value);

    let plans = plan_all_throws(&func);
    assert!(plans[0].1.escapes());
    assert!(
        plans[0].1.cleanups.is_empty(),
        "nothing to undo is a plan, not a gap"
    );
}

#[test]
fn a_thrown_value_is_widened_rather_than_refused() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::I32], &[]);
    let number = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.throw(Tag(1), number);

    assert_eq!(
        verify(&func, &types, &FuncRegistry::new()),
        vec![],
        "this layer does not know what may be thrown, so what travels is the uniform form"
    );
}

#[test]
fn a_handler_that_does_not_receive_the_value_is_rejected() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::Tagged], &[]);
    let value = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let caught = b.create_block();
    let region = b.open_region(
        vec![Handler {
            tag: Tag(1),
            block: caught,
        }],
        None,
    );
    b.throw(Tag(1), value);

    let mut b = FuncBuilder::new(&mut func, &types, caught);
    b.ret(&[]);

    let errors = verify(&func, &types, &FuncRegistry::new());
    assert!(
        errors.contains(&VerifyError::HandlerMissingPayload {
            region,
            target: caught
        }),
        "finding it elsewhere means a side channel that outlives the frame"
    );
}

#[test]
fn a_point_that_can_collect_records_the_region_protecting_it() {
    let mut types = TypeRegistry::new();
    let ty = types.declare(&[Repr::I64]);

    let mut func = function(&[Repr::Ref(RefKind::Bytes)], &[Repr::Ref(RefKind::Bytes)]);
    let held = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let cleanup = b.create_block();
    let region = b.open_region(vec![], Some(cleanup));
    b.alloc(ty, Region::Local);
    b.ret(&[held]);

    let mut b = FuncBuilder::new(&mut func, &types, cleanup);
    b.ret(&[held]);

    let table = describe_frames(&func);
    let frame = table.iter().next().expect("the allocation is described");
    assert_eq!(
        frame.region,
        Some(region),
        "one record: a value spilled for the collector and a value that must survive \
         cleanup are not two answers to one question"
    );
    assert_eq!(
        frame.roots.iter().map(|r| r.value).collect::<Vec<_>>(),
        vec![held]
    );
}

#[test]
fn a_region_naming_a_block_that_does_not_exist_is_rejected() {
    let types = TypeRegistry::new();
    let mut other = function(&[], &[]);
    let foreign_block = other.push_block();

    let mut func = function(&[], &[]);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    // A handful of blocks exist in `other` that do not exist here.
    for _ in 0..2 {
        other.push_block();
    }
    let _region = b.open_region(
        vec![Handler {
            tag: Tag(1),
            block: foreign_block,
        }],
        None,
    );
    b.ret(&[]);

    let errors = verify(&func, &types, &FuncRegistry::new());
    assert!(errors.iter().any(|e| matches!(
        e,
        VerifyError::UnknownRegionBlock { .. } | VerifyError::HandlerMissingPayload { .. }
    )));
}

/// A client that decides its regions before emitting -- opening each once while it
/// makes its blocks, then emitting with none open -- needs the blocks it makes WHILE
/// emitting to belong where the block being built is. Under the open-region rule
/// they belong to nothing, and a throw from one leaves the function past a handler
/// written to catch it. `inherit_block_regions` derives it; without it, the same
/// sequence leaves the continuation unprotected, which is why it is not the default
/// only for a client that turns it on.
#[test]
fn an_inheriting_builder_keeps_a_continuation_in_the_region_of_the_block_it_continues() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::Tagged], &[Repr::Tagged]);
    let value = param(&func, 0);
    let entry = func.entry;

    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let caught = b.create_block();
    b.add_block_param(caught, Repr::Tagged);
    let region = b.open_region(
        vec![Handler {
            tag: Tag(1),
            block: caught,
        }],
        None,
    );
    let inside = b.create_block();
    b.close_region();
    b.jump(inside, &[]).expect("a jump into the region");

    // EMITTING, with no region open: the continuation is made from inside.
    b.inherit_block_regions();
    b.switch_to(inside);
    assert_eq!(b.current_region(), Some(region));
    let continuation = b.create_block();
    b.jump(continuation, &[])
        .expect("a jump to the continuation");
    b.switch_to(continuation);
    b.throw(Tag(1), value);

    let recovered = func.block(caught).expect("exists").params[0];
    let mut b = FuncBuilder::new(&mut func, &types, caught);
    b.ret(&[recovered]);

    assert_eq!(func.region_of(continuation), Some(region));
    let plans = plan_all_throws(&func);
    assert_eq!(plans.len(), 1);
    assert_eq!(
        plans[0].1.handler,
        Some(caught),
        "the throw from the continuation is caught"
    );
    assert_eq!(verify(&func, &types, &FuncRegistry::new()), vec![]);

    // AND WITHOUT THE MODE the same continuation belongs to nothing -- the reason a
    // translating client has to turn it on, and a client emitting regions in order
    // must not have it forced.
    let mut func = function(&[Repr::Tagged], &[Repr::Tagged]);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let caught = b.create_block();
    b.add_block_param(caught, Repr::Tagged);
    b.open_region(
        vec![Handler {
            tag: Tag(1),
            block: caught,
        }],
        None,
    );
    let inside = b.create_block();
    b.close_region();
    b.switch_to(inside);
    let continuation = b.create_block();
    assert_eq!(func.region_of(continuation), None);
}

/// A block made after stepping out of a region belongs to what encloses it, so a
/// throw from it is not caught by the handler it stepped past -- and stepping back
/// in restores the regions for what is made after.
#[test]
fn a_block_made_after_stepping_out_belongs_to_the_enclosing_region() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::Tagged], &[]);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let outer_caught = b.create_block();
    b.add_block_param(outer_caught, Repr::Tagged);
    let outer = b.open_region(
        vec![Handler {
            tag: Tag(1),
            block: outer_caught,
        }],
        None,
    );
    let depth = b.open_depth();
    let inner_caught = b.create_block();
    b.add_block_param(inner_caught, Repr::Tagged);
    let inner = b.open_region(
        vec![Handler {
            tag: Tag(1),
            block: inner_caught,
        }],
        None,
    );
    let left = b.step_out_to(depth);
    assert_eq!(left, vec![inner]);
    let outside = b.create_block();
    b.step_back_in(left);
    let inside = b.create_block();
    assert_eq!(func.region_of(outside), Some(outer));
    assert_eq!(func.region_of(inside), Some(inner));
}
