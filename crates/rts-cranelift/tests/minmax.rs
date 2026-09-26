//! What a minimum and a maximum are admitted to be: one instruction per domain.
//!
//! `NumOp::Min` and `NumOp::Max` are admitted outright, like `Add`, because
//! every target here has `smin`/`smax` and `fmin`/`fmax`. What this file pins is
//! that the lowering keeps that promise — the whole value of the pair is that a
//! client stops paying a call for them, so a lowering that answered with a call
//! would be correct and pointless.
//!
//! The signed-zero and NaN rules the float form carries are pinned where they
//! are observable, by the running fixture `tests/math_min_max_machine.test.ts`,
//! because a text count cannot tell `fmin` from a comparison-and-select.

use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::verifier::verify_function;
use rts_cranelift::ir::{FuncBuilder, Function, NumOp, Signature, ValueId};
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

fn binary(repr: Repr, op: NumOp) -> cranelift_codegen::ir::Function {
    let types = TypeRegistry::new();
    let mut func = function(&[repr, repr], &[repr]);
    let (x, y) = (param(&func, 0), param(&func, 1));
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let answered = b.arith(op, x, y).expect("min and max are admitted in both domains");
    b.ret(&[answered]);
    lower_and_verify(&func)
}

#[test]
fn a_float_minimum_and_maximum_are_the_machines_own_instructions() {
    for (op, opcode) in [(NumOp::Min, "fmin"), (NumOp::Max, "fmax")] {
        let lowered = binary(Repr::F64, op);
        assert_eq!(count(&lowered, opcode), 1, "{op:?} over doubles is `{opcode}`");
        assert_eq!(
            count(&lowered, "call"),
            0,
            "the reason to admit {op:?} at all is that it is not a call"
        );
    }
}

#[test]
fn an_integer_minimum_and_maximum_are_signed_and_one_instruction() {
    for (op, opcode) in [(NumOp::Min, "smin"), (NumOp::Max, "smax")] {
        let lowered = binary(Repr::I64, op);
        assert_eq!(count(&lowered, opcode), 1, "{op:?} over integers is `{opcode}`");
    }
}
