//! What the builder settles instead of emitting, and what it refuses to settle.
//!
//! Every claim here has a negative twin on purpose. A fold is only as good as
//! the case next to it that it declines: "multiplying by one disappears" is
//! worthless without "multiplying a generic operand by one still happens",
//! because the second is the one that would silently delete a `ToNumber`.

use rts_cranelift::ir::FuncRegistry;
use rts_cranelift::ir::{
    BuildError, ConstDecl, FuncBuilder, Function, Inst, InstId, NumOp, ScalarBits, Signature,
    Terminator, ValueId,
};
use rts_cranelift::repr::Repr;
use rts_cranelift::types::TypeRegistry;
use rts_cranelift::verify::verify;

/// A function taking the given parameters and returning the given values.
fn function(params: &[Repr], returns: &[Repr]) -> Function {
    Function::new(Signature {
        params: params.to_vec(),
        returns: returns.to_vec(),
        ..Signature::default()
    })
}

/// The value bound to the entry block's parameter at `index`.
fn param(func: &Function, index: usize) -> ValueId {
    func.block(func.entry).expect("entry exists").params[index]
}

/// The instructions a block holds, in order.
fn insts(func: &Function, block: rts_cranelift::ir::BlockId) -> Vec<Inst> {
    func.block(block)
        .expect("block exists")
        .insts
        .iter()
        .map(|&id: &InstId| func.inst(id).expect("instruction exists").inst.clone())
        .collect()
}

/// How control leaves a block.
fn terminator(func: &Function, block: rts_cranelift::ir::BlockId) -> Terminator {
    func.block(block)
        .expect("block exists")
        .terminator
        .clone()
        .expect("block is terminated")
}

// ------------------------------------------------------------------- guards

#[test]
fn a_guard_on_a_value_already_in_the_expected_form_costs_nothing() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::F64], &[Repr::F64]);
    let x = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let ok = b.create_block();
    let fail = b.create_block();
    b.add_block_param(ok, Repr::F64);
    b.guard(x, Repr::F64, (ok, &[]), (fail, &[]))
        .expect("well-formed guard");

    assert_eq!(
        terminator(&func, entry),
        Terminator::Jump(rts_cranelift::ir::BlockCall {
            block: ok,
            args: vec![x],
        }),
        "an f64 tested for being an f64 cannot fail, so no test is emitted"
    );
    assert!(
        !insts(&func, entry).iter().any(|i| matches!(i, Inst::Widen(_))),
        "and the widening the guard would have undone is never emitted either"
    );
}

#[test]
fn a_guard_over_a_widening_of_the_same_form_binds_the_source() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::F64], &[Repr::Tagged]);
    let x = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let widened = b.widen(x);
    let ok = b.create_block();
    let fail = b.create_block();
    b.add_block_param(ok, Repr::F64);
    b.guard(widened, Repr::F64, (ok, &[]), (fail, &[]))
        .expect("well-formed guard");

    assert_eq!(
        terminator(&func, entry),
        Terminator::Jump(rts_cranelift::ir::BlockCall {
            block: ok,
            args: vec![x],
        }),
        "the success path receives what was widened, not the box it was widened into"
    );
    assert!(
        insts(&func, entry).iter().any(|i| matches!(i, Inst::Widen(_))),
        "the widening itself stays: the client asked for it and may use it elsewhere"
    );
}

#[test]
fn a_guard_over_a_widening_of_a_different_form_is_kept() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::I32], &[Repr::Tagged]);
    let x = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let widened = b.widen(x);
    let ok = b.create_block();
    let fail = b.create_block();
    b.add_block_param(ok, Repr::F64);
    b.guard(widened, Repr::F64, (ok, &[]), (fail, &[]))
        .expect("well-formed guard");

    assert!(
        matches!(terminator(&func, entry), Terminator::Guard { .. }),
        "an i32 is not an f64; this is exactly the test that must survive"
    );
}

#[test]
fn a_settled_guard_still_refuses_a_success_block_without_the_narrowed_value() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::F64], &[Repr::F64]);
    let x = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let ok = b.create_block();
    let fail = b.create_block();

    assert_eq!(
        b.guard(x, Repr::F64, (ok, &[]), (fail, &[])),
        Err(BuildError::GuardTargetMissingValue { target: ok }),
        "a malformed guard is malformed whether or not its test can fail; \
         reporting it only when the fold misses would make the check a lottery"
    );
}

#[test]
fn a_function_whose_guard_was_settled_still_verifies() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::F64], &[Repr::F64]);
    let x = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let ok = b.create_block();
    let fail = b.create_block();
    b.add_block_param(ok, Repr::F64);
    b.guard(x, Repr::F64, (ok, &[]), (fail, &[]))
        .expect("well-formed guard");

    let narrowed = func.block(ok).expect("ok exists").params[0];
    let mut b = FuncBuilder::new(&mut func, &types, ok);
    b.ret(&[narrowed]);
    let mut b = FuncBuilder::new(&mut func, &types, fail);
    b.ret(&[x]);

    assert_eq!(
        verify(&func, &types, &FuncRegistry::new()),
        vec![],
        "settling a guard must not leave a function the verifier rejects"
    );
}

// --------------------------------------------------------------- arithmetic

/// The constant `1.0`, materialized in the block being built.
fn one(b: &mut FuncBuilder<'_>) -> ValueId {
    let id = b.declare_const(ConstDecl::Scalar {
        repr: Repr::F64,
        bits: ScalarBits(1.0f64.to_bits()),
    });
    b.use_const(id)
}

/// A double constant, materialized in the block being built.
fn double(b: &mut FuncBuilder<'_>, value: f64) -> ValueId {
    let id = b.declare_const(ConstDecl::Scalar {
        repr: Repr::F64,
        bits: ScalarBits(value.to_bits()),
    });
    b.use_const(id)
}

#[test]
fn multiplying_a_proven_double_by_one_emits_nothing() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::F64], &[Repr::F64]);
    let x = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let one = one(&mut b);
    let product = b.arith(NumOp::Mul, x, one).expect("both proven f64");

    assert_eq!(
        product, x,
        "`x * 1.0` is the identity on every double, including -0.0 and the infinities"
    );
    assert!(
        !insts(&func, entry)
            .iter()
            .any(|i| matches!(i, Inst::FloatArith(..))),
        "and nothing is emitted to compute it"
    );
}

#[test]
fn multiplying_by_one_written_on_the_left_is_settled_too() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::F64], &[Repr::F64]);
    let x = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let one = one(&mut b);
    let product = b.arith(NumOp::Mul, one, x).expect("both proven f64");

    assert_eq!(product, x, "multiplication commutes and so does the fold");
}

#[test]
fn multiplying_a_proven_double_by_anything_else_is_kept() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::F64], &[Repr::F64]);
    let x = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let two = double(&mut b, 2.0);
    let product = b.arith(NumOp::Mul, x, two).expect("both proven f64");

    assert_ne!(product, x, "only one is the identity");
    assert!(
        insts(&func, entry)
            .iter()
            .any(|i| matches!(i, Inst::FloatArith(NumOp::Mul, ..))),
        "the multiplication is emitted"
    );
}

#[test]
fn adding_zero_to_a_proven_double_is_kept() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::F64], &[Repr::F64]);
    let x = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let zero = double(&mut b, 0.0);
    let sum = b.arith(NumOp::Add, x, zero).expect("both proven f64");

    assert_ne!(
        sum, x,
        "`x + 0.0` is NOT the identity: `-0.0 + 0.0` is `+0.0`. This is the \
         obvious next fold and it is wrong, which is why the test exists"
    );
}

#[test]
fn multiplying_a_generic_operand_by_one_is_still_refused() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::Tagged], &[Repr::Tagged]);
    let unknown = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let one = one(&mut b);

    assert_eq!(
        b.arith(NumOp::Mul, unknown, one),
        Err(BuildError::GenericOperand { operation: "arith" }),
        "on a generic operand the multiplication performs a real conversion — \
         `\"3\" * 1` is 3 — so folding it away would delete a coercion"
    );
}
