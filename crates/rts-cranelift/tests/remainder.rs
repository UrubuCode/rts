//! What a remainder is admitted to be, and what it is refused for.
//!
//! `NumOp::Rem` is the first operator in this crate admitted **per domain**
//! rather than outright, and every claim that asymmetry rests on is pinned
//! here. Three of the four tests are refusals, which is the proportion the
//! design implies: the instruction is total for one shape of operand and
//! partial for the rest, and the partial cases are the ones a wrong answer
//! would ship silently.
//!
//! The refusals are asserted at all THREE layers on purpose — builder,
//! verifier, lowering. Rule 7 asks for the builder and the verifier both,
//! because the builder reports the mistake where it was made and the verifier
//! catches a representation that never went through the builder. Lowering is
//! the third because it is the layer with nothing correct to emit.

use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::verifier::verify_function;
use rts_cranelift::ir::{
    BuildError, ConstDecl, FuncBuilder, Function, NumOp, ScalarBits, Signature, ValueId,
};
use rts_cranelift::lower::{LowerError, lower_function};
use rts_cranelift::repr::Repr;
use rts_cranelift::types::TypeRegistry;
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

/// Lowers, then asks the code generator whether it accepts the result.
///
/// The same shape `lowering.rs` uses, and for the same reason: our verifier
/// saying a program is well formed in our vocabulary is a different claim from
/// the code generator accepting what we emitted for it.
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

#[test]
fn an_integer_remainder_by_a_settled_divisor_is_one_instruction() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::I64], &[Repr::I64]);
    let x = param(&func, 0);

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let modulus = b.declare_const(ConstDecl::Scalar {
        repr: Repr::I64,
        bits: ScalarBits(4294967296),
    });
    let modulus = b.use_const(modulus);
    let rest = b
        .arith(NumOp::Rem, x, modulus)
        .expect("a constant divisor that is neither 0 nor -1 settles the trap");
    b.ret(&[rest]);

    let lowered = lower_and_verify(&func);
    assert_eq!(
        count(&lowered, "srem"),
        1,
        "an integer remainder is the machine's own instruction"
    );
    assert_eq!(
        count(&lowered, "call"),
        0,
        "the whole point of admitting it in this domain is that it is not a call"
    );
}

#[test]
fn a_float_remainder_is_refused_because_no_exact_form_exists() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::F64, Repr::F64], &[Repr::F64]);
    let (x, y) = (param(&func, 0), param(&func, 1));

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);

    match b.arith(NumOp::Rem, x, y) {
        Err(BuildError::UnsafeRemainder {
            found,
            domain_admits,
        }) => {
            assert_eq!(found, Repr::F64);
            assert!(
                !domain_admits,
                "the float domain does not admit a remainder at all — the refusal is \
                 about the domain, not about this pair of operands"
            );
        }
        other => panic!(
            "a float remainder must be refused: IEEE remainder is a library call, and \
             `a - trunc(a / b) * b` stops being exact once the quotient passes 2^53. \
             Got {other:?}"
        ),
    }
}

#[test]
fn an_integer_remainder_by_an_unsettled_divisor_is_refused() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::I64, Repr::I64], &[Repr::I64]);
    let (x, y) = (param(&func, 0), param(&func, 1));

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);

    match b.arith(NumOp::Rem, x, y) {
        Err(BuildError::UnsafeRemainder {
            found,
            domain_admits,
        }) => {
            assert_eq!(found, Repr::I64);
            assert!(
                domain_admits,
                "the integer domain does admit a remainder — what is unsettled here is \
                 the divisor, which is a parameter and could be 0 or -1"
            );
        }
        other => panic!(
            "`srem` traps on a zero divisor and on INT_MIN % -1, and a parameter could be \
             either. The language above answers NaN for the first and 0 for the second, so \
             emitting a trap would stop a process where a value was required. Got {other:?}"
        ),
    }
}

#[test]
fn the_two_divisors_that_trap_are_refused_by_value_not_by_representation() {
    // Both are integer constants, which is what makes this the interesting case:
    // nothing in the REPRESENTATION distinguishes them from the divisor the first
    // test accepts. Only the value does, which is why the check reads the constant.
    for (bits, why) in [
        (0i64, "a zero divisor traps rather than answering NaN"),
        (-1i64, "INT_MIN % -1 traps because the quotient is not representable"),
    ] {
        let types = TypeRegistry::new();
        let mut func = function(&[Repr::I64], &[Repr::I64]);
        let x = param(&func, 0);

        let entry = func.entry;
        let mut b = FuncBuilder::new(&mut func, &types, entry);
        let divisor = b.declare_const(ConstDecl::Scalar {
            repr: Repr::I64,
            bits: ScalarBits(bits as u64),
        });
        let divisor = b.use_const(divisor);

        assert!(
            matches!(
                b.arith(NumOp::Rem, x, divisor),
                Err(BuildError::UnsafeRemainder { .. })
            ),
            "{why}"
        );
    }
}

#[test]
fn the_verifier_refuses_a_trapping_remainder_the_builder_never_saw() {
    // Built by pushing the instruction straight onto the function, which is what
    // "a representation that did not come from the builder" means. Rule 7 asks
    // for both checks precisely because this path exists.
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::I64, Repr::I64], &[Repr::I64]);
    let (x, y) = (param(&func, 0), param(&func, 1));
    let entry = func.entry;

    let rest = func.push_inst(entry, rts_cranelift::ir::Inst::IntArith(NumOp::Rem, x, y), &[
        Repr::I64,
    ])[0];
    func.set_terminator(entry, rts_cranelift::ir::Terminator::Return(vec![rest]));

    let funcs = rts_cranelift::ir::FuncRegistry::new();
    let errors = verify(&func, &types, &funcs);
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, VerifyError::UnsafeRemainder { .. })),
        "the builder is not the only way to write this instruction, so it cannot be the \
         only thing that refuses it. Got {errors:?}"
    );
}

#[test]
fn lowering_refuses_a_float_remainder_rather_than_approximating_one() {
    // The third layer. Reaching it means the program was not verified, and the
    // claim being pinned is that lowering still does not invent an answer —
    // there is no inexact identity emitted here as a convenience.
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::F64, Repr::F64], &[Repr::F64]);
    let (x, y) = (param(&func, 0), param(&func, 1));
    let entry = func.entry;

    let rest = func.push_inst(entry, rts_cranelift::ir::Inst::FloatArith(NumOp::Rem, x, y), &[
        Repr::F64,
    ])[0];
    func.set_terminator(entry, rts_cranelift::ir::Terminator::Return(vec![rest]));

    match lower_function(&func, CallConv::SystemV) {
        Err(LowerError::RemainderNotInDomain { found, .. }) => {
            assert_eq!(found, Repr::F64);
        }
        other => panic!(
            "lowering does not approximate — that is this crate's stated discipline, and a \
             float remainder is exactly where approximating would be tempting. Got {other:?}"
        ),
    }
    let _ = types;
}
