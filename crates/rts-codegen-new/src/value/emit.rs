//! Cranelift emit helpers for [`PolyValue`](super::PolyValue).
//!
//! These prove that box/unbox is pure straight-line IR (band/bor/icmp/bitcast/
//! ishl/sshr) — no extern call — that Cranelift's egraph can fold. Each takes a
//! live `FunctionBuilder` and operates on i64-typed SSA `Value`s (the ABI slot
//! for a `PolyValue`) except where noted.
//!
//! Keeping these in their own file (a) honours the <500-line module rule and
//! (b) keeps the Cranelift dependency surface of the value layer isolated to the
//! one place that actually emits IR.

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{InstBuilder, MemFlags, Value, types};
use cranelift_frontend::FunctionBuilder;

use super::{BOX_BASE, CANONICAL_NAN, TAG_INT32, encode};

/// Emit `(v & BOX_BASE) == BOX_BASE`, producing an `i8` boolean (1 = boxed).
///
/// `v` must be the i64 raw word.
pub fn emit_is_boxed(builder: &mut FunctionBuilder, v: Value) -> Value {
    let base = builder.ins().iconst(types::I64, BOX_BASE as i64);
    let masked = builder.ins().band(v, base);
    builder.ins().icmp(IntCC::Equal, masked, base)
}

/// Emit the negation of [`emit_is_boxed`]: `(v & BOX_BASE) != BOX_BASE`,
/// i.e. "is an inline double". Produces an `i8` boolean (1 = double).
pub fn emit_is_double(builder: &mut FunctionBuilder, v: Value) -> Value {
    let base = builder.ins().iconst(types::I64, BOX_BASE as i64);
    let masked = builder.ins().band(v, base);
    builder.ins().icmp(IntCC::NotEqual, masked, base)
}

/// Box an `i32` SSA value (Cranelift type `I32`) into a tagged-`int32`
/// PolyValue word (i64).
///
/// `encode(TAG_INT32, i as u32) = BOX_BASE | (TAG_INT32<<48) | (i as u32)`.
/// We zero-extend the i32 to i64 (so the high payload bits are 0) then OR in the
/// constant header.
pub fn emit_box_int32(builder: &mut FunctionBuilder, i32_val: Value) -> Value {
    // Zero-extend the 32-bit value into the low 32 bits of an i64.
    let widened = builder.ins().uextend(types::I64, i32_val);
    let header = encode(TAG_INT32, 0) as i64; // BOX_BASE | (TAG_INT32<<48)
    let header_v = builder.ins().iconst(types::I64, header);
    builder.ins().bor(widened, header_v)
}

/// Unbox a tagged-`int32` PolyValue word (i64) back to an i64 holding the
/// sign-extended `i32` value (matching [`PolyValue::as_i32`](super::PolyValue::as_i32)
/// cast to `i64`).
///
/// Done branch-free with shifts: mask is implicit because the payload's low
/// 32 bits already hold the int; `ishl 32` pushes the sign bit (bit 31) to bit
/// 63, then `sshr 32` arithmetic-shifts it back, sign-extending. This drops the
/// tag/header bits (they live above bit 48 and are shifted out) for free.
pub fn emit_unbox_int32(builder: &mut FunctionBuilder, v: Value) -> Value {
    let shifted_up = builder.ins().ishl_imm(v, 32);
    builder.ins().sshr_imm(shifted_up, 32)
}

/// Box an `f64` SSA value into its inline-double PolyValue word, mirroring the
/// pure-Rust [`PolyValue::from_f64`](super::PolyValue::from_f64): a `bitcast`
/// f64→i64 **with NaN canonicalization**.
///
/// A runtime-computed NaN on x86 is a *negative* qNaN (`Math.sqrt(-4)`, `0.0/0.0`,
/// `0*Infinity` → `0xFFF8…`), whose top 13 bits are all-ones — i.e. it lands in
/// the negative-qNaN BOXED space. A bare `bitcast` would store that word as-is,
/// and the decode (`is_boxed`) would then MISREAD it as a boxed `TAG_OBJECT`/etc.
/// (rendering "[object Object]" and giving a wrong `typeof`). So we fold every
/// NaN to the single positive [`CANONICAL_NAN`](super::CANONICAL_NAN) (sign bit 0
/// → NOT in the boxed space, still `is_nan`), exactly as the Rust model does.
///
/// Pure straight-line IR (`bitcast` + `fcmp uno` + `iconst` + `select`), so the
/// egraph CONST-FOLDS the whole thing away for any `f64_val` it can prove is not
/// NaN: `fcmp uno x, x` becomes `false`, the `select` collapses to the plain
/// `bitcast`, and the proven-numeric fast path pays nothing.
///
/// INVARIANT: `f64_val` must be a GENUINE double (an arithmetic / `Math` / const
/// result), never a PolyValue word merely `bitcast` to `f64` as an ABI carrier —
/// such a word is itself a NaN pattern and canonicalization would corrupt it. The
/// signature layer ([`crate::front::run::sig`]) enforces this by never typing a
/// `Tagged`-param function's return as `Float64` (which is what created that
/// carrier), so this helper only ever sees real doubles.
pub fn emit_box_double(builder: &mut FunctionBuilder, f64_val: Value) -> Value {
    let bits = builder.ins().bitcast(types::I64, MemFlags::new(), f64_val);
    let is_nan = builder.ins().fcmp(FloatCC::Unordered, f64_val, f64_val);
    let canon = builder.ins().iconst(types::I64, CANONICAL_NAN as i64);
    builder.ins().select(is_nan, canon, bits)
}

/// Unbox an inline-double PolyValue word (i64) back to an `f64`: a plain
/// `bitcast` i64→f64.
pub fn emit_unbox_double(builder: &mut FunctionBuilder, v: Value) -> Value {
    builder.ins().bitcast(types::F64, MemFlags::new(), v)
}

/// Read a NUMBER-tagged PolyValue word (i64) as an `f64`, handling BOTH number
/// representations: a tagged `int32` (`fcvt_from_sint(unbox_int32)`) and an inline
/// double (`bitcast`). The two candidates are computed unconditionally and a
/// `select` on the int32-tag picks the right one — pure straight-line IR the
/// egraph collapses to the surviving branch when the source repr is proven.
///
/// This is the SOUND `Tagged → number` decode: the bare [`emit_unbox_double`]
/// (bitcast) misreads a tagged-int32 word as a NaN double, and [`emit_unbox_int32`]
/// misreads a boxed double's low 32 bits — each is correct only for its own tag.
/// Both number tags reach a numeric coercion (`__rtsadp_add` returns a boxed
/// double for non-int sums, a tagged int32 for exact small ones), so the decode
/// must accept either. A non-number word (string/object) must never reach here —
/// the coercion layer routes only proven numbers to a native numeric target.
pub fn emit_tagged_number_to_f64(builder: &mut FunctionBuilder, v: Value) -> Value {
    // int32 candidate: sign-extend the low 32 payload bits, then int→float.
    let as_int = emit_unbox_int32(builder, v);
    let from_int = builder.ins().fcvt_from_sint(types::F64, as_int);
    // double candidate: bitcast the word directly.
    let from_double = builder.ins().bitcast(types::F64, MemFlags::new(), v);
    // is this word a tagged int32?  (v & BOX_BASE)==BOX_BASE && tag==INT32.
    let boxed = emit_is_boxed(builder, v);
    let tag_shifted = builder.ins().ushr_imm(v, super::TAG_SHIFT as i64);
    let tag = builder.ins().band_imm(tag_shifted, super::TAG_MASK as i64);
    let is_int_tag = builder.ins().icmp_imm(IntCC::Equal, tag, TAG_INT32 as i64);
    let is_int32 = builder.ins().band(boxed, is_int_tag);
    builder.ins().select(is_int32, from_int, from_double)
}
