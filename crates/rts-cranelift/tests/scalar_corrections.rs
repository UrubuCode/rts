//! The three small operations a client uses to CORRECT an instruction's answer
//! — a select, a leading-zero count, a rounding to single precision — and the
//! claim each one makes: no call, and one or two instructions.
//!
//! They exist because the corrections a language has to make around `floor`
//! (a tie rounded the other way, the sign of a zero) or around a multiply (the
//! 32-bit wrapping view) cost more as branches than the instruction they
//! correct. A client that could not spell a select spelled a block.

use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::verifier::verify_function;
use rts_cranelift::ir::{
    BuildError, CmpOp, FloatOp, FuncBuilder, Function, IntUnaryOp, Signature, ValueId,
};
use rts_cranelift::lower::lower_function;
use rts_cranelift::repr::Repr;
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

fn count(lowered: &cranelift_codegen::ir::Function, opcode: &str) -> usize {
    lowered
        .to_string()
        .lines()
        .filter(|line| line.split_whitespace().any(|word| word == opcode))
        .count()
}

/// As [`count`], for an opcode the code generator prints with a type suffix —
/// `fdemote.f32`, `fpromote.f64` — which a whole-word match misses.
fn count_prefixed(lowered: &cranelift_codegen::ir::Function, opcode: &str) -> usize {
    let dotted = format!("{opcode}.");
    lowered
        .to_string()
        .lines()
        .filter(|line| line.split_whitespace().any(|word| word.starts_with(&dotted)))
        .count()
}

#[test]
fn a_select_over_two_doubles_is_one_instruction_and_no_block() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::F64, Repr::F64], &[Repr::F64]);
    let (x, y) = (param(&func, 0), param(&func, 1));
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let less = b.compare(CmpOp::Lt, x, y).expect("two doubles compare");
    let chosen = b.select(less, x, y).expect("a select over one representation");
    b.ret(&[chosen]);
    let lowered = lower_and_verify(&func);
    assert_eq!(count(&lowered, "select"), 1, "the choice is the machine's own `select`");
    assert_eq!(count(&lowered, "brif"), 0, "and not a branch");
}

#[test]
fn a_select_refuses_a_condition_that_is_not_a_boolean_and_arms_that_disagree() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::F64, Repr::F64, Repr::I64], &[Repr::F64]);
    let (x, y, n) = (param(&func, 0), param(&func, 1), param(&func, 2));
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    assert!(
        matches!(b.select(n, x, y), Err(BuildError::WrongDomain { operation: "select", .. })),
        "an integer is not a condition"
    );
    let less = b.compare(CmpOp::Lt, x, y).expect("two doubles compare");
    assert!(
        b.select(less, x, n).is_err(),
        "a double and an integer are not two values one select can choose between"
    );
}

#[test]
fn leading_zeros_is_one_instruction() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::I32], &[Repr::I32]);
    let x = param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let zeros = b
        .int_unary(IntUnaryOp::LeadingZeros, x)
        .expect("an integer operation over an integer");
    b.ret(&[zeros]);
    let lowered = lower_and_verify(&func);
    assert_eq!(count(&lowered, "clz"), 1);
    assert_eq!(count(&lowered, "call"), 0);
}

#[test]
fn rounding_to_single_precision_is_a_demote_and_a_promote() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::F64], &[Repr::F64]);
    let x = param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let narrowed = b
        .float_unary(FloatOp::RoundToSingle, x)
        .expect("a float operation over a double");
    b.ret(&[narrowed]);
    let lowered = lower_and_verify(&func);
    assert_eq!(count_prefixed(&lowered, "fdemote"), 1);
    assert_eq!(count_prefixed(&lowered, "fpromote"), 1);
    assert_eq!(count(&lowered, "call"), 0);
}
