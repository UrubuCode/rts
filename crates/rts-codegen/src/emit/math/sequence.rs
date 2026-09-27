//! The `Math` members that are a SEQUENCE of instructions, written once.
//!
//! Two emitters lower a program — the running one in `emit/` and the MIR stage
//! in `machine/` — and both build the same machine IR through `FuncBuilder`.
//! What `Math.round` means is a fact about the language, so it is stated here
//! once over that builder and both call it, rather than each carrying a copy
//! of a sequence whose whole difficulty is the corner it gets wrong.
//!
//! Every function takes proven operands and answers a proven one; whether an
//! operand IS proven, and what to do when it is not, is each emitter's own
//! question.

use rts_cranelift::ir::{BuildResult, CmpOp, ConstDecl, FloatOp, FuncBuilder, IntUnaryOp, NumOp, ScalarBits, ValueId};
use rts_cranelift::repr::Repr;

fn double(builder: &mut FuncBuilder, value: f64) -> ValueId {
    let id = builder.declare_const(ConstDecl::Scalar {
        repr: Repr::F64,
        bits: ScalarBits(value.to_bits()),
    });
    builder.use_const(id)
}

/// `Math.round(x)` over a proven double.
///
/// # Why not `floor(x + 0.5)`
///
/// Two values break it. `0.49999999999999994 + 0.5` rounds UP to `1.0` in
/// double arithmetic, so the floor answers 1 where the language answers 0. And
/// `-0.3 + 0.5` floors to `0`, where the language answers `-0` — observable
/// through `Object.is` and `1 / x`. So: the floor, the fraction, a select on the
/// fraction reaching one half (a tie rounds toward +∞, which `>=` gives), and
/// then the zero's sign restored from the operand. Seven instructions and no
/// branch; `NaN` and the infinities fall through every select unchanged.
pub(crate) fn round(builder: &mut FuncBuilder, x: ValueId) -> BuildResult<ValueId> {
    let floored = builder.float_unary(FloatOp::Floor, x)?;
    let fraction = builder.arith(NumOp::Sub, x, floored)?;
    let half = double(builder, 0.5);
    let up = builder.compare(CmpOp::Ge, fraction, half)?;
    let one = double(builder, 1.0);
    let next = builder.arith(NumOp::Add, floored, one)?;
    let rounded = builder.select(up, next, floored)?;
    // `-0` for a negative operand that rounded to zero: `-0.3`, `-0.5`. The
    // comparison `rounded == 0` is true of both zeros, which is what makes the
    // select below a no-op for an operand that was `-0` to begin with.
    let zero = double(builder, 0.0);
    let is_zero = builder.compare(CmpOp::Eq, rounded, zero)?;
    let negative = builder.compare(CmpOp::Lt, x, zero)?;
    let negative_zero = double(builder, -0.0);
    let signed = builder.select(is_zero, negative_zero, rounded)?;
    builder.select(negative, signed, rounded)
}

/// `Math.sign(x)` over a proven double: `1`, `-1`, or the operand itself — which
/// is how `+0`, `-0` and `NaN` come back as themselves, since `x > 0` and
/// `x < 0` are both false for all three.
pub(crate) fn sign(builder: &mut FuncBuilder, x: ValueId) -> BuildResult<ValueId> {
    let zero = double(builder, 0.0);
    let positive = builder.compare(CmpOp::Gt, x, zero)?;
    let negative = builder.compare(CmpOp::Lt, x, zero)?;
    let one = double(builder, 1.0);
    let minus_one = double(builder, -1.0);
    let below = builder.select(negative, minus_one, x)?;
    builder.select(positive, one, below)
}

/// `Math.imul(a, b)` over two proven doubles: both through `ToInt32`, a
/// wrapping 32-bit multiply, and the product back as a double. `ToInt32` and
/// `ToUint32` agree on the bit pattern, and a wrapping product over the pattern
/// is what the language defines, so the signed conversion serves.
pub(crate) fn imul(builder: &mut FuncBuilder, a: ValueId, b: ValueId) -> BuildResult<ValueId> {
    let x = builder.to_int32(a)?;
    let y = builder.to_int32(b)?;
    let product = builder.arith(NumOp::Mul, x, y)?;
    builder.to_f64(product)
}

/// `Math.clz32(x)` over a proven double: the leading zero bits of the 32-bit
/// view. `ToInt32` and `ToUint32` are the same bits, and `NaN` converts to zero,
/// whose count is 32 — which is the language's answer.
pub(crate) fn clz32(builder: &mut FuncBuilder, x: ValueId) -> BuildResult<ValueId> {
    let bits = builder.to_int32(x)?;
    let zeros = builder.int_unary(IntUnaryOp::LeadingZeros, bits)?;
    builder.to_f64(zeros)
}
