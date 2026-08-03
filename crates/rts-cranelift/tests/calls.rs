//! Calls, exercised end to end.
//!
//! The claims under test are the four facts a call site must not restate: the
//! shape it calls, what that shape returns, whether the callee is code at all,
//! and whether the frame still exists when control leaves.

use rts_cranelift::abi::Convention;
use rts_cranelift::gc::describe_frames;
use rts_cranelift::ir::{
    BuildError, FuncBuilder, FuncRegistry, Function, Region, Signature, ValueId,
};
use rts_cranelift::repr::{RefKind, Repr};
use rts_cranelift::types::TypeRegistry;
use rts_cranelift::unwind::Tag;
use rts_cranelift::verify::{CallSite, VerifyError, verify};

fn function(params: &[Repr], returns: &[Repr]) -> Function {
    Function::new(Signature {
        params: params.to_vec(),
        returns: returns.to_vec(),
        ..Signature::default()
    })
}

/// A function that permits tail calls.
fn tail_function(params: &[Repr], returns: &[Repr]) -> Function {
    Function::new(Signature {
        params: params.to_vec(),
        returns: returns.to_vec(),
        convention: Convention::InternalTail,
        ..Signature::default()
    })
}

fn param(func: &Function, index: usize) -> ValueId {
    func.block(func.entry).expect("entry exists").params[index]
}

/// Declares a callee of the given shape.
fn declare(
    funcs: &mut FuncRegistry,
    params: &[Repr],
    returns: &[Repr],
) -> rts_cranelift::ir::FuncId {
    let sig = funcs.declare_signature(Signature {
        params: params.to_vec(),
        returns: returns.to_vec(),
        ..Signature::default()
    });
    funcs.declare_function(sig)
}

#[test]
fn a_call_binds_what_its_callee_returns() {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let callee = declare(&mut funcs, &[Repr::F64], &[Repr::F64]);

    let mut func = function(&[Repr::F64], &[Repr::F64]);
    let x = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let results = b.call(&funcs, callee, &[x]).expect("shape matches");
    assert_eq!(results.len(), 1);
    b.ret(&results);

    assert_eq!(
        func.repr_of(results[0]),
        Repr::F64,
        "the shape says so, not the site"
    );
    assert_eq!(verify(&func, &types, &funcs), vec![]);
}

#[test]
fn a_call_can_return_more_than_one_value() {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let callee = declare(&mut funcs, &[], &[Repr::I64, Repr::F64]);

    let mut func = function(&[], &[Repr::I64, Repr::F64]);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let results = b
        .call(&funcs, callee, &[])
        .expect("no arguments to mismatch");
    b.ret(&results);

    assert_eq!(
        results.len(),
        2,
        "an instruction bound to one result would need rebuilding"
    );
    assert_eq!(verify(&func, &types, &funcs), vec![]);
}

#[test]
fn a_call_returning_nothing_binds_nothing() {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let callee = declare(&mut funcs, &[], &[]);

    let mut func = function(&[], &[]);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    assert!(b.call(&funcs, callee, &[]).expect("well formed").is_empty());
    b.ret(&[]);

    assert_eq!(verify(&func, &types, &funcs), vec![]);
}

#[test]
fn the_wrong_number_of_arguments_is_refused_at_construction() {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let callee = declare(&mut funcs, &[Repr::F64, Repr::F64], &[]);

    let mut func = function(&[Repr::F64], &[]);
    let x = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    assert_eq!(
        b.call(&funcs, callee, &[x]),
        Err(BuildError::CallArity {
            expected: 2,
            found: 1
        })
    );
}

#[test]
fn an_argument_of_the_wrong_representation_is_refused_rather_than_converted() {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let callee = declare(&mut funcs, &[Repr::Tagged], &[]);

    let mut func = function(&[Repr::F64], &[]);
    let x = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    assert_eq!(
        b.call(&funcs, callee, &[x]),
        Err(BuildError::CallArgumentRepr {
            position: 0,
            expected: Repr::Tagged,
            found: Repr::F64,
        }),
        "a callee's parameters are its own interface; converting quietly is how two \
         sides come to disagree about what was passed"
    );
}

#[test]
fn an_indirect_call_needs_a_callee_proven_to_be_code() {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let sig = funcs.declare_signature(Signature::default());

    let mut func = function(&[Repr::Tagged], &[]);
    let unknown = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    assert_eq!(
        b.call_indirect(&funcs, unknown, sig, &[]),
        Err(BuildError::IndirectCalleeNotCallable {
            found: Repr::Tagged
        }),
        "it might be code, and a guard is how one finds out"
    );
}

#[test]
fn an_indirect_call_through_a_proven_callable_is_accepted() {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let sig = funcs.declare_signature(Signature {
        params: vec![Repr::I64],
        returns: vec![Repr::I64],
        ..Signature::default()
    });

    let mut func = function(&[Repr::Ref(RefKind::Callable), Repr::I64], &[Repr::I64]);
    let (callee, arg) = (param(&func, 0), param(&func, 1));

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let results = b
        .call_indirect(&funcs, callee, sig, &[arg])
        .expect("proven callable");
    b.ret(&results);

    assert_eq!(verify(&func, &types, &funcs), vec![]);
}

#[test]
fn a_call_is_a_point_the_collector_must_be_able_to_read() {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let callee = declare(&mut funcs, &[], &[]);

    let mut func = function(&[Repr::Ref(RefKind::Bytes)], &[Repr::Ref(RefKind::Bytes)]);
    let held = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.call(&funcs, callee, &[]).expect("well formed");
    b.ret(&[held]);

    let table = describe_frames(&func);
    let frame = table.iter().next().expect("the call is described");
    assert_eq!(
        frame.roots.iter().map(|r| r.value).collect::<Vec<_>>(),
        vec![held],
        "the callee can allocate, so what outlives the call must be findable"
    );
}

#[test]
fn a_tail_call_is_not_a_point_the_collector_can_act_at() {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let sig = funcs.declare_signature(Signature {
        convention: Convention::InternalTail,
        ..Signature::default()
    });
    let callee = funcs.declare_function(sig);

    let mut func = tail_function(&[Repr::Ref(RefKind::Bytes)], &[]);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.tail_call(&funcs, callee, &[])
        .expect("conventions and returns match");

    assert!(
        describe_frames(&func).is_empty(),
        "there is no frame left to scan once control transfers"
    );
    assert_eq!(verify(&func, &types, &funcs), vec![]);
}

#[test]
fn a_tail_call_from_an_ordinary_function_is_refused() {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let sig = funcs.declare_signature(Signature {
        convention: Convention::InternalTail,
        ..Signature::default()
    });
    let callee = funcs.declare_function(sig);

    let mut func = function(&[], &[]);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);

    assert_eq!(
        b.tail_call(&funcs, callee, &[]),
        Err(BuildError::TailCallNotPermitted),
        "a tail-recursive group compiles as a unit or not at all"
    );
}

#[test]
fn a_tail_call_whose_returns_differ_is_refused() {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let sig = funcs.declare_signature(Signature {
        returns: vec![Repr::F64],
        convention: Convention::InternalTail,
        ..Signature::default()
    });
    let callee = funcs.declare_function(sig);

    let mut func = tail_function(&[], &[Repr::I64]);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);

    assert_eq!(
        b.tail_call(&funcs, callee, &[]),
        Err(BuildError::TailCallNotPermitted)
    );
}

#[test]
fn a_tail_call_inside_a_protected_region_is_refused() {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let sig = funcs.declare_signature(Signature {
        convention: Convention::InternalTail,
        ..Signature::default()
    });
    let callee = funcs.declare_function(sig);

    let mut func = tail_function(&[Repr::Tagged], &[]);
    let value = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let cleanup = b.create_block();
    let region = b.declare_region(None, vec![], Some(cleanup));
    b.place_in_region(entry, region);

    assert_eq!(
        b.tail_call(&funcs, callee, &[]),
        Err(BuildError::TailCallInProtectedRegion),
        "returning a call's result directly and catching its exception are exclusive"
    );

    b.throw(Tag(0), value);
    let mut b = FuncBuilder::new(&mut func, &types, cleanup);
    b.ret(&[]);
}

#[test]
fn the_verifier_catches_a_call_the_builder_did_not_make() {
    let types = TypeRegistry::new();
    let mut theirs = FuncRegistry::new();
    let callee = declare(&mut theirs, &[], &[]);
    let ours = FuncRegistry::new();

    let mut func = function(&[], &[]);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.call(&theirs, callee, &[])
        .expect("well formed against theirs");
    b.ret(&[]);

    assert_eq!(verify(&func, &types, &theirs), vec![]);
    assert!(
        verify(&func, &types, &ours).contains(&VerifyError::UnknownCallee {
            at: CallSite::Inst(first_inst(&func))
        }),
        "a shape read from the wrong registry is plausible and wrong"
    );
}

#[test]
fn a_plain_function_may_call_one_that_can_suspend() {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let sig = funcs.declare_signature(Signature {
        returns: vec![Repr::Ref(RefKind::Opaque)],
        may_suspend: true,
        ..Signature::default()
    });
    let callee = funcs.declare_function(sig);

    let mut func = function(&[], &[Repr::Ref(RefKind::Opaque)]);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let results = b.call(&funcs, callee, &[]).expect("well formed");
    b.ret(&results);

    assert_eq!(
        verify(&func, &types, &funcs),
        vec![],
        "suspension here is stackless: the callee returns a promise, and the caller \
         parks at its own await or not at all"
    );
}

#[test]
fn a_guarded_call_is_a_guard_containing_a_call_not_a_node_of_its_own() {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let sig = funcs.declare_signature(Signature {
        params: vec![Repr::I32],
        ..Signature::default()
    });
    let callee = funcs.declare_function(sig);

    let mut func = function(&[Repr::Tagged], &[]);
    let unknown = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let ok = b.create_block();
    let fail = b.create_block();
    b.add_block_param(ok, Repr::I32);
    b.guard(unknown, Repr::I32, (ok, &[]), (fail, &[]))
        .expect("well formed");

    let narrowed = func.block(ok).expect("exists").params[0];
    let mut b = FuncBuilder::new(&mut func, &types, ok);
    b.call(&funcs, callee, &[narrowed])
        .expect("the guard proved the shape fits");
    b.ret(&[]);

    let mut b = FuncBuilder::new(&mut func, &types, fail);
    b.ret(&[]);

    assert_eq!(
        verify(&func, &types, &funcs),
        vec![],
        "composing the two costs nothing; a dedicated node would cost a product of kinds"
    );
}

#[test]
fn an_allocation_and_a_call_are_both_described() {
    let mut types = TypeRegistry::new();
    let ty = types.declare(&[Repr::I64]);
    let mut funcs = FuncRegistry::new();
    let callee = declare(&mut funcs, &[], &[]);

    let mut func = function(&[Repr::Ref(RefKind::Bytes)], &[Repr::Ref(RefKind::Bytes)]);
    let held = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.alloc(ty, Region::Local);
    b.call(&funcs, callee, &[]).expect("well formed");
    b.ret(&[held]);

    assert_eq!(describe_frames(&func).len(), 2);
}

/// The first instruction of a function's entry block.
fn first_inst(func: &Function) -> rts_cranelift::ir::InstId {
    func.block(func.entry).expect("entry exists").insts[0]
}
