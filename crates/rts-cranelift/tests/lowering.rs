//! Lowering, checked by the code generator's own verifier.
//!
//! Our verifier says a program is well formed in our vocabulary. That is not the
//! same claim as "the code generator accepts what we emitted for it", and the
//! second is the one that matters here — so every lowered function in this file
//! is handed to the code generator's verifier, which is a reader with no stake in
//! our being right.

use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::verifier::verify_function;
use rts_cranelift::ir::{
    CmpOp, ConstDecl, FuncBuilder, Function, GenericOp, NumOp, Region, ScalarBits, Signature,
    ValueId,
};
use rts_cranelift::lower::{Capability, LowerError, lower_function};
use rts_cranelift::repr::{RefKind, Repr};
use rts_cranelift::types::TypeRegistry;

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

/// Lowers, then asks the code generator whether it accepts the result.
fn lower_and_verify(func: &Function) -> cranelift_codegen::ir::Function {
    let lowered = lower_function(func, CallConv::SystemV).expect("lowering succeeds");

    let mut flags = settings::builder();
    flags
        .set("enable_verifier", "true")
        .expect("a real setting");
    let flags = settings::Flags::new(flags);

    if let Err(errors) = verify_function(&lowered, &flags) {
        panic!("the code generator rejected our output:\n{errors}\n{lowered}");
    }
    lowered
}

/// How many times an instruction appears in the emitted code.
fn count(lowered: &cranelift_codegen::ir::Function, opcode: &str) -> usize {
    lowered
        .to_string()
        .lines()
        .filter(|line| line.split_whitespace().any(|word| word == opcode))
        .count()
}

#[test]
fn proven_arithmetic_lowers_to_arithmetic() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::F64, Repr::F64], &[Repr::F64]);
    let (x, y) = (param(&func, 0), param(&func, 1));

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let sum = b.arith(NumOp::Add, x, y).expect("proven");
    b.ret(&[sum]);

    let lowered = lower_and_verify(&func);
    assert_eq!(count(&lowered, "fadd"), 1);
    assert_eq!(
        count(&lowered, "call"),
        0,
        "arithmetic on proven values is not a call"
    );
}

#[test]
fn widening_is_instructions_not_a_call() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::I32], &[Repr::Tagged]);
    let x = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let widened = b.widen(x);
    b.ret(&[widened]);

    let lowered = lower_and_verify(&func);
    assert_eq!(
        count(&lowered, "call"),
        0,
        "lowering this to a call would destroy the argument for having a generic form"
    );
}

#[test]
fn widening_a_double_normalizes_only_the_values_that_would_collide() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::F64], &[Repr::Tagged]);
    let x = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let widened = b.widen(x);
    b.ret(&[widened]);

    let lowered = lower_and_verify(&func);
    assert_eq!(
        count(&lowered, "select"),
        1,
        "a real value keeps its own bits"
    );
    assert_eq!(count(&lowered, "call"), 0);
}

#[test]
fn widening_a_boolean_uses_the_numbers_this_layer_reserved() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::Bool], &[Repr::Tagged]);
    let x = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let widened = b.widen(x);
    b.ret(&[widened]);

    lower_and_verify(&func);
}

#[test]
fn a_sixty_four_bit_integer_cannot_be_widened_and_says_so() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::I64], &[Repr::Tagged]);
    let x = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let widened = b.widen(x);
    b.ret(&[widened]);

    assert_eq!(
        lower_function(&func, CallConv::SystemV),
        Err(LowerError::CannotWiden { from: Repr::I64 }),
        "truncating or reinterpreting it would lose values silently"
    );
}

#[test]
fn a_guard_lowers_to_a_test_and_a_branch() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::Tagged], &[Repr::Tagged]);
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
    let doubled = b.arith(NumOp::Add, narrowed, narrowed).expect("narrowed");
    let widened = b.widen(doubled);
    b.ret(&[widened]);

    let mut b = FuncBuilder::new(&mut func, &types, fail);
    b.ret(&[unknown]);

    let lowered = lower_and_verify(&func);
    assert_eq!(count(&lowered, "brif"), 1);
    assert_eq!(
        count(&lowered, "call"),
        0,
        "a representation check is not a slow path"
    );
}

#[test]
fn a_guard_cannot_establish_which_kind_of_reference_a_value_holds() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::Tagged], &[]);
    let unknown = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let ok = b.create_block();
    let fail = b.create_block();
    b.add_block_param(ok, Repr::Ref(RefKind::Bytes));
    b.guard(unknown, Repr::Ref(RefKind::Bytes), (ok, &[]), (fail, &[]))
        .expect("the builder accepts it; the encoding is what cannot");

    for block in [ok, fail] {
        let mut b = FuncBuilder::new(&mut func, &types, block);
        b.ret(&[]);
    }

    assert_eq!(
        lower_function(&func, CallConv::SystemV),
        Err(LowerError::CannotProveReferenceKind {
            expect: Repr::Ref(RefKind::Bytes)
        }),
        "the tag says a value is a reference and nothing more"
    );
}

#[test]
fn a_guard_can_establish_that_a_value_is_a_reference() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::Tagged], &[]);
    let unknown = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let ok = b.create_block();
    let fail = b.create_block();
    b.add_block_param(ok, Repr::Ref(RefKind::Opaque));
    b.guard(unknown, Repr::Ref(RefKind::Opaque), (ok, &[]), (fail, &[]))
        .expect("well formed");

    for block in [ok, fail] {
        let mut b = FuncBuilder::new(&mut func, &types, block);
        b.ret(&[]);
    }

    lower_and_verify(&func);
}

#[test]
fn a_loop_with_a_carried_value_lowers_and_verifies() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::I32], &[Repr::I32]);
    let initial = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let header = b.create_block();
    let exit = b.create_block();
    let carried = b.add_block_param(header, Repr::I32);
    b.jump(header, &[initial]).expect("matching representation");

    let mut b = FuncBuilder::new(&mut func, &types, header);
    let zero = b.declare_const(ConstDecl::Scalar {
        repr: Repr::I32,
        bits: ScalarBits(0),
    });
    let zero = b.use_const(zero);
    let done = b.compare(CmpOp::Eq, carried, zero).expect("proven");
    let next = b.arith(NumOp::Sub, carried, zero).expect("proven");
    b.branch(done, (exit, &[]), (header, &[next]))
        .expect("proven boolean");

    let mut b = FuncBuilder::new(&mut func, &types, exit);
    b.ret(&[carried]);

    let lowered = lower_and_verify(&func);
    assert_eq!(count(&lowered, "brif"), 1);
}

#[test]
fn a_merge_that_widens_emits_the_widening_on_the_edge_that_needs_it() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::I32], &[Repr::Tagged]);
    let x = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let merge = b.create_block();
    b.add_block_param(merge, Repr::Tagged);
    b.jump(merge, &[x]).expect("widening is inserted for us");

    let merged = func.block(merge).expect("exists").params[0];
    let mut b = FuncBuilder::new(&mut func, &types, merge);
    b.ret(&[merged]);

    lower_and_verify(&func);
}

#[test]
fn a_trap_lowers_and_ends_its_block() {
    let types = TypeRegistry::new();
    let mut func = function(&[], &[Repr::I32]);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.trap(rts_cranelift::ir::TrapCode::Unreachable);

    lower_and_verify(&func);
}

#[test]
fn what_needs_the_heap_is_refused_by_name() {
    let mut types = TypeRegistry::new();
    let ty = types.declare(&[Repr::I64]);
    let mut func = function(&[], &[]);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.alloc(ty, Region::Local);
    b.ret(&[]);

    let error = lower_function(&func, CallConv::SystemV).expect_err("no heap yet");
    assert!(
        matches!(
            error,
            LowerError::NotYetLowered {
                needs: Capability::Memory,
                ..
            }
        ),
        "named rather than skipped, so incomplete code is not produced quietly"
    );
}

#[test]
fn a_scheduler_operation_is_a_call_and_needs_a_module_to_name_it() {
    let types = TypeRegistry::new();
    let mut func = function(&[], &[]);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.promise_new();
    b.ret(&[]);

    let error = lower_function(&func, CallConv::SystemV).expect_err("no module here");
    assert!(
        matches!(
            error,
            LowerError::NotYetLowered {
                needs: Capability::Calls,
                ..
            }
        ),
        "creating a promise is asking the runtime to, and asking needs somewhere to \n         name what is being asked"
    );
}

#[test]
fn a_generic_operation_still_needs_a_runtime_to_call() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::Tagged, Repr::Tagged], &[Repr::Tagged]);
    let (a, c) = (param(&func, 0), param(&func, 1));

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let sum = b.generic(GenericOp::Add, a, c);
    b.ret(&[sum]);

    assert!(matches!(
        lower_function(&func, CallConv::SystemV),
        Err(LowerError::NotYetLowered {
            needs: Capability::Calls,
            ..
        })
    ));
}
