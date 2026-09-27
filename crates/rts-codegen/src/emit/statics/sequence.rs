//! The four `Number` predicates as instructions over a proven double, written
//! once for both emitters — `emit/math/sequence.rs`'s reason.
//!
//! Each answers a [`Repr::Bool`]. What each one gets wrong when written the
//! obvious way is stated on it.

use rts_cranelift::ir::{BuildResult, CmpOp, ConstDecl, FloatOp, FuncBuilder, ScalarBits, ValueId};
use rts_cranelift::repr::Repr;

fn double(builder: &mut FuncBuilder, value: f64) -> ValueId {
    let id = builder.declare_const(ConstDecl::Scalar {
        repr: Repr::F64,
        bits: ScalarBits(value.to_bits()),
    });
    builder.use_const(id)
}

fn boolean(builder: &mut FuncBuilder, value: bool) -> ValueId {
    let id = builder.declare_const(ConstDecl::Scalar {
        repr: Repr::Bool,
        bits: ScalarBits(u64::from(value)),
    });
    builder.use_const(id)
}

/// `Number.isNaN(x)`: a NaN is the one double unequal to itself, and the
/// machine's float `!=` is the unordered comparison, so this is one instruction.
pub(crate) fn is_nan(builder: &mut FuncBuilder, x: ValueId) -> BuildResult<ValueId> {
    builder.compare(CmpOp::Ne, x, x)
}

/// `Number.isFinite(x)`: the magnitude below infinity, which is false for both
/// infinities and, because every comparison with NaN is, for NaN too.
pub(crate) fn is_finite(builder: &mut FuncBuilder, x: ValueId) -> BuildResult<ValueId> {
    let magnitude = builder.float_unary(FloatOp::Abs, x)?;
    let infinity = double(builder, f64::INFINITY);
    builder.compare(CmpOp::Lt, magnitude, infinity)
}

/// `Number.isInteger(x)`: finite, and equal to its own truncation. Not the
/// truncation test alone — `trunc(Infinity) == Infinity` — so the finiteness
/// test gates it through a select, not a branch.
pub(crate) fn is_integer(builder: &mut FuncBuilder, x: ValueId) -> BuildResult<ValueId> {
    let finite = is_finite(builder, x)?;
    let truncated = builder.float_unary(FloatOp::Trunc, x)?;
    let whole = builder.compare(CmpOp::Eq, truncated, x)?;
    let no = boolean(builder, false);
    builder.select(finite, whole, no)
}

/// `Number.isSafeInteger(x)`: an integer whose magnitude does not exceed
/// 2^53 - 1, the largest a double represents exactly with every neighbour.
pub(crate) fn is_safe_integer(builder: &mut FuncBuilder, x: ValueId) -> BuildResult<ValueId> {
    let integer = is_integer(builder, x)?;
    let magnitude = builder.float_unary(FloatOp::Abs, x)?;
    let largest = double(builder, 9_007_199_254_740_991.0);
    let within = builder.compare(CmpOp::Le, magnitude, largest)?;
    let no = boolean(builder, false);
    builder.select(integer, within, no)
}
