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

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{types, InstBuilder, MemFlags, Value};
use cranelift_frontend::FunctionBuilder;

use super::{encode, BOX_BASE, TAG_INT32};

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

/// Box an `f64` SSA value into its inline-double PolyValue word: a plain
/// `bitcast` f64→i64.
///
/// NOTE: the pure-Rust model canonicalizes NaN in [`PolyValue::from_f64`](super::PolyValue::from_f64).
/// That requires a compare+select (control-flow-free, but two extra ops) and is
/// left out of this straight-line helper on purpose — the box-double fast path
/// is for values the front-end already proved are not NaN, or where the boxed
/// word is immediately unboxed again. A NaN-canonicalizing variant would be:
/// `select(is_nan(f), iconst(CANONICAL_NAN), bitcast(f))`. Callers that may box
/// an arbitrary NaN-bearing double must insert that select (TODO when the lower
/// pass needs it).
pub fn emit_box_double(builder: &mut FunctionBuilder, f64_val: Value) -> Value {
    builder
        .ins()
        .bitcast(types::I64, MemFlags::new(), f64_val)
}

/// Unbox an inline-double PolyValue word (i64) back to an `f64`: a plain
/// `bitcast` i64→f64.
pub fn emit_unbox_double(builder: &mut FunctionBuilder, v: Value) -> Value {
    builder.ins().bitcast(types::F64, MemFlags::new(), v)
}
