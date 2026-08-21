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

/// Whether an integer remainder's divisor cannot make the instruction trap.
///
/// # Why the builder has to ask
///
/// `srem` is not total. It traps on a zero divisor, and it traps on
/// `INT_MIN % -1` because the quotient of that pair is not representable. Both
/// are *machine* facts with no counterpart in the language above: `x % 0` is
/// `NaN` there, and no value of `x` makes `x % -1` anything but `0`. So an
/// integer remainder whose divisor is not settled here would turn a program
/// that must answer a number into one that stops the process — which the
/// honesty floor names specifically, and which no verifier downstream could
/// catch because the trapping value only exists at run time.
///
/// # What counts as settled, and why it is only this
///
/// A divisor that is a constant, and is neither `0` nor `-1`. That is the whole
/// of what this module can decide: rule 11's neighbour, that a fact is spent
/// only where it was established. Proving a *variable* non-zero needs a range,
/// which needs a fixed point over the block graph — a pass, which the header of
/// this module says does not live here.
///
/// The consequence is deliberate and is the safe direction: a remainder by a
/// divisor this cannot settle is REFUSED at construction rather than emitted
/// and hoped about. The client's answer to a refusal is to keep the generic
/// form, which is rule 5 above — what cannot be proven becomes generic, visibly.
pub(crate) fn divisor_cannot_trap(func: &Function, divisor: ValueId) -> bool {
    let Some(&Inst::Const(id)) = defining_inst(func, divisor) else {
        return false;
    };
    let Some(ConstDecl::Scalar { repr, bits }) = func.constant(id) else {
        return false;
    };
    if !repr.is_integer() {
        return false;
    }
    // Compared as the signed value the instruction will see, so that `-1` is
    // recognised whatever width the representation is. `bits` holds the pattern;
    // an `I32` `-1` and an `I64` `-1` are different patterns for one number.
    let signed = match repr {
        Repr::I8 => bits.0 as u8 as i8 as i64,
        Repr::I16 => bits.0 as u16 as i16 as i64,
        Repr::I32 => bits.0 as u32 as i32 as i64,
        _ => bits.0 as i64,
    };
    signed != 0 && signed != -1
}

/// Whether a float remainder's divisor makes the exact sequence available.
///
/// # The claim, and why it is not the one `NumOp` rejects
///
/// [`crate::ir::inst::NumOp`] records that `a - trunc(a / b) * b` is **not** an
/// exact remainder in general: past 2^53 the division rounds and the identity
/// stops answering what the language requires. That is true, and it is why the
/// float domain does not admit a remainder outright.
///
/// It stops being true when the divisor is a power of two. Dividing by 2^k is
/// an adjustment of the exponent and nothing else — every bit of the mantissa
/// survives — so `a / b` is exact, `trunc` of it is exact, multiplying that
/// back by 2^k is exact, and the subtraction is of two values within one `b` of
/// each other. No step rounds, at any magnitude.
///
/// # Why `k >= 0`, which is the whole of the restriction
///
/// If `|b| < 1` then `a / b` can OVERFLOW — `b = 2^-1000` with a large `a`
/// answers infinity, and the sequence then computes `inf - inf` where the true
/// answer is zero. Requiring `|b| >= 1` makes `|a / b| <= |a|`, so nothing can
/// overflow that was not already infinite.
///
/// This admits `% 4294967296`, `% 256`, `% 2` — the divisors a hash, a mask or
/// a generator is written with. It excludes `% 0.5`, which keeps the call.
///
/// # What it does NOT settle
///
/// The sign of a zero result. `-8 % 4` is `-0` in the language and the sequence
/// answers `+0`, so lowering finishes with a copy of the dividend's sign; that
/// is a fact about the emitted code rather than about the divisor, so it lives
/// there and not here.
pub(crate) fn divisor_is_power_of_two(func: &Function, divisor: ValueId) -> bool {
    let Some(&Inst::Const(id)) = defining_inst(func, divisor) else {
        return false;
    };
    let Some(ConstDecl::Scalar {
        repr: Repr::F64,
        bits,
    }) = func.constant(id)
    else {
        return false;
    };
    let value = f64::from_bits(bits.0).abs();
    // `is_power_of_two` on the FLOAT, not on a cast: the mantissa must be zero
    // and the value finite. `f64::EXP` arithmetic would say the same thing in a
    // way a reader has to verify, so this asks the two questions directly.
    value.is_finite() && value >= 1.0 && value.to_bits() & ((1u64 << 52) - 1) == 0
}

/// What a value was widened FROM, when it was widened at all.
///
/// A query and not a fold: it emits nothing and decides nothing. It exists
/// because a client that widened a proof and is now asked to consume the proof
/// again has no other way to discover that the two are the same thing — and the
/// round trip it cannot see costs whatever un-widening costs that client, which
/// for the language layer is a call to its runtime.
///
/// Answers about ONE instruction, so a value that reaches a block parameter
/// through two widened predecessors answers `None`. That is a traversal, and a
/// traversal is a pass.
pub(crate) fn widened_source(func: &Function, value: ValueId) -> Option<ValueId> {
    match *defining_inst(func, value)? {
        Inst::Widen(source) => Some(source),
        _ => None,
    }
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
