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
use crate::ir::inst::{BitOp, Inst, NumOp};
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

/// Whether a value is a CONSTANT of exactly the given singleton.
///
/// Asked by a client about to emit an equality test, so that a comparison
/// against a singleton it wrote down itself becomes one bit test instead of a
/// call into the client's runtime. It answers about the operand's ORIGIN, which
/// is the whole of why it lives here: [`Inst::IsSingleton`] tests a value at run
/// time, and this tests whether the client has already settled the question at
/// build time.
///
/// Takes the singleton's id rather than answering WHICH singleton a word is,
/// and the difference is the rule that type states: a singleton reconstructed
/// from a bit pattern at a call site is a second copy of the registry's
/// numbering. The client holds the id it was issued; this only confirms it.
///
/// False for a block parameter, for anything not a constant, and for a constant
/// in any representation but the generic one — nothing proven is a singleton,
/// so a `true` here for a proven operand would be a lie about a value whose
/// shape the machine already knows.
pub(crate) fn is_constant_singleton(
    func: &Function,
    value: ValueId,
    singleton: crate::tags::SingletonId,
) -> bool {
    if func.repr_of(value) != Repr::Tagged {
        return false;
    }
    let Some(&Inst::Const(id)) = defining_inst(func, value) else {
        return false;
    };
    matches!(
        func.constant(id),
        Some(ConstDecl::Scalar {
            repr: Repr::Tagged,
            bits
        }) if bits.0 == singleton.word()
    )
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
    power_of_two_reciprocal(func, divisor).is_some()
}

/// `1 / d` for a divisor the sequence can take, and `None` for one it cannot.
///
/// The predicate above and the number lowering multiplies by are ONE question
/// asked twice, so they are one function. Rule 3's shape: a second place that
/// decided which divisors qualify would eventually decide it differently, and
/// the disagreement would be a wrong answer rather than a refusal — lowering
/// would emit the sequence for a divisor the builder never checked.
///
/// # Why lowering wants the reciprocal rather than the divisor
///
/// It emits `x * (1 / d)` where the algebra says `x / d`. For a power of two
/// the two are the same number exactly — both are adjustments of an exponent,
/// and neither rounds — and `fmul` costs roughly a third of `fdiv` in latency,
/// in the middle of a chain a loop carries.
///
/// The extra condition that buys is that `1 / d` must be an ordinary double.
/// The one divisor admitted by every other test and refused by this is
/// `2^1023`, whose reciprocal is subnormal: exactly representable, but a
/// subnormal operand is where a processor can leave its fast path entirely,
/// which would make the sequence slower than the call it replaced.
pub(crate) fn power_of_two_reciprocal(func: &Function, divisor: ValueId) -> Option<f64> {
    let &Inst::Const(id) = defining_inst(func, divisor)? else {
        return None;
    };
    let ConstDecl::Scalar {
        repr: Repr::F64,
        bits,
    } = func.constant(id)?
    else {
        return None;
    };
    let value = f64::from_bits(bits.0);
    let magnitude = value.abs();
    // Asked of the FLOAT and not of a cast: the mantissa must be zero and the
    // value finite. Arithmetic on the exponent field would say the same thing
    // in a way a reader has to verify, so this asks the two questions directly.
    //
    // `>= 1.0` is load-bearing rather than tidy: below one, `x / d` can
    // OVERFLOW, and the sequence would then answer infinity where the true
    // remainder is zero.
    let power_of_two = magnitude.is_finite()
        && magnitude >= 1.0
        && magnitude.to_bits() & ((1u64 << 52) - 1) == 0;
    // The DIVISOR's sign is kept, not the magnitude's: `x * (1 / d)` has to be
    // the same number as `x / d`, and the sign of the quotient is part of that.
    // It cancels in the sequence's final multiply, which is why the language's
    // "sign of the dividend" rule survives either way — but getting it wrong
    // here would put a sign error inside `trunc`, where it does not cancel.
    let reciprocal = 1.0 / value;
    (power_of_two && reciprocal.is_normal()).then_some(reciprocal)
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

/// What [`Inst::ToInt32`] answers without converting anything.
///
/// The instruction is not cheap and its cost is not obvious. Since 2026-08-25 it
/// lowers to a range compare, a branch, and two instructions on the taken side —
/// so a `ToInt32` the builder can settle here removes a branch from the emitted
/// block as well as the conversion, and the emitter above produces one for
/// nearly every bitwise operator a program writes.
pub(crate) enum Int32Answer {
    /// The operand already IS this value in the integer domain.
    Reuse(ValueId),
    /// The conversion is decided here, and this is what it answers.
    Settled(i32),
}

/// The value a `ToInt32` would produce, when it is knowable while building.
///
/// Two shapes, and both were found by reading what the layer above actually
/// emits rather than by listing what is theoretically foldable — `fold`'s own
/// header rule, that a fold with no producer is a fold nothing tests:
///
/// - **over a [`Inst::ToF64`]**, which is the round trip `(a << 3) | 0` writes
///   in full: the shift leaves `I32`, `ToF64` carries it back to the
///   representation every numeric guard tests for, and the `| 0` immediately
///   converts it again. `ToF64` only accepts `I32` (the builder refuses
///   anything else), every `i32` is exactly representable as a double, and the
///   result is integral and inside the range — so `ToInt32` over it is the
///   identity, not an approximation of one.
/// - **over a constant double**, which every `a & 255`, `a | 1` and `a ^ 3`
///   emits for its right operand, and which every `| 0` emits for the zero.
///
/// # What is deliberately absent
///
/// A `ToInt32` over a block parameter whose predecessors all carry integers.
/// That is a traversal, which is a pass, which this module's header says does
/// not live here — and it is the shape that would actually pay, because it is
/// the one on the dependency chain a loop carries. Recorded as the next thing
/// rather than half-attempted: `int not` is `ToInt32` + `Xor` + `ToF64` with
/// nothing foldable left in it and still costs 3.28 ns against 0.94 for the
/// empty loop, so the round trip itself is what remains.
pub(crate) fn to_int32_answer(func: &Function, value: ValueId) -> Option<Int32Answer> {
    match *defining_inst(func, value)? {
        Inst::ToF64(source) => Some(Int32Answer::Reuse(source)),
        Inst::Const(id) => match func.constant(id)? {
            ConstDecl::Scalar {
                repr: Repr::F64,
                bits,
            } => Some(Int32Answer::Settled(to_int32_of(f64::from_bits(bits.0)))),
            _ => None,
        },
        _ => None,
    }
}

/// The language's `ToInt32`, computed here rather than at run time.
///
/// Deliberately written as the specification reads instead of as the lowering
/// emits, because the two agreeing is the property worth having and would be
/// unfalsifiable if this were a transcription of that. Both are checked against
/// the same fixture — `tests/cross-runtime/numeric/436_toint32_boundary.ts`,
/// whose values reach the run-time path, and this module's own unit tests, whose
/// values reach this one.
///
/// `%` on doubles is exact for every operand pair (it is `fmod`, which is
/// defined to be), so the reduction below rounds nothing at any magnitude.
fn to_int32_of(x: f64) -> i32 {
    if !x.is_finite() {
        return 0;
    }
    let whole = x.trunc();
    let reduced = whole % 4294967296.0;
    // Into `[0, 2^32)` before the cast, so that the wrap into signed is the
    // cast's and not a second rule written here. `-0.0` fails `< 0.0` and casts
    // to zero, which is the answer for it.
    let unsigned = if reduced < 0.0 {
        reduced + 4294967296.0
    } else {
        reduced
    };
    unsigned as u32 as i32
}

/// The value a bitwise instruction would produce, when both operands are known.
///
/// Only `And`, `Or` and `Xor`, and only over two constants. The shifts are
/// absent because nothing emits one over two constants — the count is masked
/// against `31` by the layer above, so a shift's operands are a variable and the
/// result of THAT mask, which this fold settles instead.
pub(crate) fn bitwise_answer(
    func: &Function,
    op: BitOp,
    a: ValueId,
    b: ValueId,
) -> Option<i32> {
    let (x, y) = (int32_const(func, a)?, int32_const(func, b)?);
    match op {
        BitOp::And => Some(x & y),
        BitOp::Or => Some(x | y),
        BitOp::Xor => Some(x ^ y),
        BitOp::Shl | BitOp::Shr | BitOp::ShrUnsigned => None,
    }
}

/// The operand a bitwise instruction would return unchanged.
///
/// One case, and it has a producer in every program: `x | 0`, which is how the
/// layer above spells "as an int32" and which it emits around the result of an
/// operation that already left one. Disjunction with zero is the identity on
/// every bit pattern.
///
/// `x & -1` and `x ^ 0` are the obvious neighbours and are absent for the dull
/// reason: nothing emits either.
pub(crate) fn bitwise_unchanged(
    func: &Function,
    op: BitOp,
    a: ValueId,
    b: ValueId,
) -> Option<ValueId> {
    if op != BitOp::Or {
        return None;
    }
    if int32_const(func, b) == Some(0) {
        return Some(a);
    }
    if int32_const(func, a) == Some(0) {
        return Some(b);
    }
    None
}

/// The 32-bit integer a value is, when it is a constant one.
fn int32_const(func: &Function, value: ValueId) -> Option<i32> {
    let Some(&Inst::Const(id)) = defining_inst(func, value) else {
        return None;
    };
    match func.constant(id)? {
        ConstDecl::Scalar {
            repr: Repr::I32,
            bits,
        } => Some(bits.0 as u32 as i32),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::to_int32_of;

    /// The compile-time `ToInt32` answers what the lowering answers.
    ///
    /// These are the same inputs as
    /// `tests/cross-runtime/numeric/436_toint32_boundary.ts`, which reaches the
    /// run-time path and is checked against node and bun. Two implementations
    /// of one specification only stay in agreement if something compares them,
    /// and the fixture cannot reach this one: a constant the builder can fold
    /// never becomes a run-time conversion, which is the whole point of the
    /// fold and also the reason it can drift alone.
    #[test]
    fn a_constant_converts_to_what_the_language_says_it_does() {
        for (input, expected) in [
            (0.0, 0),
            (-0.0, 0),
            (1.0, 1),
            (-1.0, -1),
            // Truncation is toward zero, which is not rounding and not floor.
            (0.9, 0),
            (-0.9, 0),
            (1.5, 1),
            (-1.5, -1),
            (2147483647.0, 2147483647),
            // The wrap into signed, which is the cast's and not a rule of ours.
            (2147483648.0, -2147483648),
            (-2147483649.0, 2147483647),
            (4294967295.0, -1),
            // A multiple of 2^32 reduces to zero however large it is.
            (4294967296.0, 0),
            (4294967297.0, 1),
            (1e21, -559939584),
            (9007199254740991.0, -1),
            (-9007199254740991.0, 1),
            // Either side of 2^63, which is where the lowering changes path and
            // where this one does not — so it is the input most likely to make
            // the two disagree.
            (9223372036854774784.0, -1024),
            (9223372036854775808.0, 0),
            (-9223372036854775808.0, 0),
            (18446744073709551616.0, 0),
            (1.9342813113834067e25, 0),
            (f64::MAX, 0),
            // The three the range guard exists for.
            (f64::INFINITY, 0),
            (f64::NEG_INFINITY, 0),
            (f64::NAN, 0),
        ] {
            assert_eq!(
                to_int32_of(input),
                expected,
                "ToInt32({input}) is {expected} in the language, and the \
                 run-time lowering answers that — see \
                 tests/cross-runtime/numeric/436_toint32_boundary.ts"
            );
        }
    }
}
