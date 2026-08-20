//! What a remainder is admitted to be, and what it is refused for.
//!
//! `NumOp::Rem` is the first operator in this crate admitted **conditionally**
//! rather than outright, and every claim that rests on is pinned here. The
//! condition is finer than a domain: in the integer domain it is a divisor that
//! cannot make `srem` trap, and in the float domain it is a divisor that makes
//! the algebraic sequence exact. Both are questions about the divisor's VALUE,
//! which is why neither can be settled by a representation.
//!
//! Most of the tests are refusals, which is the proportion the design implies:
//! the instruction is exact for a narrow shape of divisor and wrong for the
//! rest, and the wrong cases are the ones that would ship silently.
//!
//! The refusals are asserted at all THREE layers on purpose â€” builder,
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
fn a_float_remainder_by_an_arbitrary_divisor_is_refused() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::F64, Repr::F64], &[Repr::F64]);
    let (x, y) = (param(&func, 0), param(&func, 1));

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);

    match b.arith(NumOp::Rem, x, y) {
        Err(BuildError::UnsafeRemainder { found }) => assert_eq!(found, Repr::F64),
        other => panic!(
            "a float remainder by a divisor nothing settled must be refused: IEEE \
             remainder is a library call, and `a - trunc(a / b) * b` stops being exact \
             once the quotient passes 2^53. Got {other:?}"
        ),
    }
}

/// Builds `x % <constant>` in the float domain and answers what the builder said.
fn float_remainder_by(constant: f64) -> Result<cranelift_codegen::ir::Function, BuildError> {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::F64], &[Repr::F64]);
    let x = param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let divisor = b.declare_const(ConstDecl::Scalar {
        repr: Repr::F64,
        bits: ScalarBits(constant.to_bits()),
    });
    let divisor = b.use_const(divisor);
    let rest = b.arith(NumOp::Rem, x, divisor)?;
    b.ret(&[rest]);
    Ok(lower_and_verify(&func))
}

#[test]
fn a_float_remainder_by_a_power_of_two_is_instructions_and_not_a_call() {
    // The one divisor shape where the identity IS exact: dividing by 2^k
    // adjusts an exponent and rounds nothing, so no step of the sequence can
    // lose a bit at any magnitude.
    //
    // `4294967296` is not a decorative choice â€” it is the modulus every 32-bit
    // LCG and every `x % 2^32` mask is written with, and it is what made this
    // worth doing.
    let lowered = float_remainder_by(4294967296.0).expect("a power of two is exact");

    assert_eq!(
        count(&lowered, "call"),
        0,
        "the whole point: `% 2^k` stops being a call into the runtime"
    );
    for (opcode, times, why) in [
        (
            "fmul",
            2,
            "the quotient and the multiple back. The quotient is a MULTIPLY by \
             `1 / d`, not a divide: for a power of two the two are the same \
             number exactly, and `fdiv` costs roughly three times as much \
             latency in the middle of a chain a loop carries",
        ),
        (
            "fdiv",
            0,
            "and so there is no division left. Cranelift does not perform this \
             reduction itself — measured 2026-08-20, doing it here took the \
             remainder loop from 74.9 ms to 52.1 ms",
        ),
        (
            "trunc",
            1,
            "toward zero, which is what the language's remainder means",
        ),
        ("fsub", 1, "the remainder itself"),
        (
            "fcopysign",
            1,
            "the sign of a ZERO result, which the algebra loses: `-8 % 4` is `-0`",
        ),
    ] {
        assert_eq!(count(&lowered, opcode), times, "{opcode}: {why}");
    }
}

#[test]
fn the_float_divisors_that_are_not_powers_of_two_stay_refused() {
    for (divisor, why) in [
        (3.0, "not a power of two at all"),
        (
            0.5,
            "a power of two BELOW one â€” `a / b` can overflow, and the sequence \
             would answer infinity where the true remainder is zero",
        ),
        (0.0, "no exact form, and the language answers NaN"),
        (f64::INFINITY, "not finite"),
        (f64::NAN, "not finite"),
        (
            4294967297.0,
            "one past a power of two, which is the case a sloppy bit test would let through",
        ),
    ] {
        assert!(
            matches!(
                float_remainder_by(divisor),
                Err(BuildError::UnsafeRemainder { .. })
            ),
            "{divisor} must be refused: {why}"
        );
    }
}

#[test]
fn a_negative_power_of_two_divisor_is_admitted_because_the_sign_is_the_dividends() {
    // `x % -4` and `x % 4` differ in nothing: the language gives the result the
    // sign of the DIVIDEND. So the divisor's sign is irrelevant to exactness,
    // and excluding it would refuse a case the sequence answers correctly.
    let lowered = float_remainder_by(-4.0).expect("the divisor's sign does not affect exactness");
    assert_eq!(count(&lowered, "call"), 0);
    assert_eq!(count(&lowered, "trunc"), 1);
}

#[test]
fn a_divisor_whose_reciprocal_is_subnormal_is_refused() {
    // `2^1023` is a power of two, is finite, and is at least one — it passes
    // every part of the test except the last. Its reciprocal is subnormal, and
    // the sequence multiplies by that reciprocal, so admitting it would put a
    // subnormal operand in the middle of the hot path: exactly representable,
    // and on many processors the point at which the fast path is abandoned.
    //
    // Refusing it costs a call on a divisor no program writes, and keeps the
    // claim "this sequence is faster than the call" true for every divisor the
    // sequence is used on.
    let huge = 2.0f64.powi(1023);
    assert!(huge.is_finite(), "the fixture itself has to be a real divisor");
    assert!(
        !(1.0 / huge).is_normal(),
        "and its reciprocal has to be the subnormal this test is about"
    );
    assert!(matches!(
        float_remainder_by(huge),
        Err(BuildError::UnsafeRemainder { .. })
    ));

    // One exponent down is admitted, which is what says the boundary is where
    // the reciprocal stops being normal rather than somewhere arbitrary.
    let admitted = float_remainder_by(2.0f64.powi(1022)).expect("its reciprocal is normal");
    assert_eq!(count(&admitted, "call"), 0);
}

#[test]
fn an_integer_remainder_by_an_unsettled_divisor_is_refused() {
    let types = TypeRegistry::new();
    let mut func = function(&[Repr::I64, Repr::I64], &[Repr::I64]);
    let (x, y) = (param(&func, 0), param(&func, 1));

    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);

    match b.arith(NumOp::Rem, x, y) {
        Err(BuildError::UnsafeRemainder { found }) => assert_eq!(found, Repr::I64),
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
    // claim being pinned is that lowering still does not invent an answer â€”
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
            "lowering does not approximate â€” that is this crate's stated discipline, and a \
             float remainder is exactly where approximating would be tempting. Got {other:?}"
        ),
    }
    let _ = types;
}
