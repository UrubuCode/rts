//! What the builder can answer without emitting anything.
//!
//! Two questions, both asked at construction rather than by a pass afterwards.
//! A pass would have to be run, would have to be run at the right point, and
//! would be skippable; a builder that refuses to emit the instruction never
//! produces the program to clean up. That is the same reasoning as rule 8 —
//! derive what a client would otherwise have to remember — applied to a client
//! that remembered correctly and still paid for it.
//!
//! Neither question is an optimizer. Both are local, both are decided by
//! representations already recorded, and both answer `None` the moment the
//! premise is not exactly met. Anything needing a fixed point, a traversal, or
//! knowledge of a second block belongs in a pass and not here.

use crate::ir::consts::ConstDecl;
use crate::ir::entity::ValueId;
use crate::ir::func::{Function, ValueOrigin};
use crate::ir::inst::{Inst, NumOp};
use crate::repr::Repr;

/// The value a guard would bind, when the guard cannot fail.
///
/// A guard tests what a generic value's representation is. Two shapes make the
/// answer known while the function is still being built:
///
/// - the operand is already in the expected representation, so the widening the
///   builder is about to insert would be undone by the guard immediately after;
/// - the operand is the result of a [`Inst::Widen`] over something already in
///   the expected representation, which is the same round trip written across
///   two instructions by a client that needed the widened form for a second
///   use as well.
///
/// The second shape is the one that matters in practice and the reason this is
/// not simply a check inside `widen_if_needed`: the widened value is live on the
/// failure path too, so it cannot be withheld — only the *test* can be settled.
///
/// Returns `None` for anything else, including a `Widen` over a representation
/// that merely merges with the expected one. Widening is exact and narrowing
/// never is (rule 11), so "the source was `F64` and the guard wants `F64`" is
/// the whole of what is provable here; "the source was some float" is not.
pub(crate) fn guard_answer(func: &Function, input: ValueId, expect: Repr) -> Option<ValueId> {
    if func.repr_of(input) == expect {
        return Some(input);
    }
    let Inst::Widen(source) = *defining_inst(func, input)? else {
        return None;
    };
    (func.repr_of(source) == expect).then_some(source)
}

/// The operand an arithmetic instruction would return unchanged.
///
/// One case only: a proven-`F64` multiplication by exactly `1.0`. It exists
/// because the layer above spells `ToNumeric` as that multiplication — which is
/// correct on a generic operand, where it performs a real conversion, and is a
/// no-op on an operand already proven to be a double. Every `++` and `--` in
/// every loop emits one.
///
/// `x * 1.0` is the identity on every double: it preserves `-0.0`, both
/// infinities, and rounds nothing. A NaN stays a NaN — its payload is permitted
/// to change, which is unobservable to any client that has only quiet NaNs, and
/// is stated here rather than left to be rediscovered.
///
/// **`x + 0.0` is deliberately absent, and is NOT the identity**: `-0.0 + 0.0`
/// is `+0.0`. It is the obvious next case and it is wrong, so the omission is
/// written down instead of read as an oversight.
///
/// Integer multiplication by one is also absent, for a duller reason: nothing
/// emits it. A fold with no producer is a fold nothing tests.
pub(crate) fn arith_answer(
    func: &Function,
    op: NumOp,
    a: ValueId,
    b: ValueId,
) -> Option<ValueId> {
    if op != NumOp::Mul || func.repr_of(a) != Repr::F64 {
        return None;
    }
    if is_f64_one(func, b) {
        return Some(a);
    }
    if is_f64_one(func, a) {
        return Some(b);
    }
    None
}

/// Whether a value is the constant `1.0` held as a double.
///
/// Compares bit patterns rather than parsing a float back out, because the
/// constant table holds bits and `1.0` has exactly one encoding. That also means
/// this can never accidentally match `-0.0`, whose bits differ from `0.0`.
fn is_f64_one(func: &Function, value: ValueId) -> bool {
    let Some(&Inst::Const(id)) = defining_inst(func, value) else {
        return false;
    };
    matches!(
        func.constant(id),
        Some(ConstDecl::Scalar { repr: Repr::F64, bits })
            if bits.0 == 1.0f64.to_bits()
    )
}

/// The instruction that defined a value, or `None` for a block parameter.
///
/// A block parameter has no defining instruction here on purpose: finding what
/// reaches it means looking at every predecessor, which is a traversal, which is
/// a pass. This module answers only what one instruction says about itself.
fn defining_inst(func: &Function, value: ValueId) -> Option<&Inst> {
    match func.value(value)?.origin {
        ValueOrigin::BlockParam(_) => None,
        ValueOrigin::InstResult(inst) => Some(&func.inst(inst)?.inst),
    }
}
