//! Widening and narrowing, as instructions.
//!
//! This is the coupling the whole design rests on. Widening a value into the
//! generic form and narrowing it back are bit operations and selects — never
//! calls — so an optimizer can see through a pair of them and remove both. That
//! is what makes the generic representation cost nothing wherever the value was
//! already uniform. Lowering either to a call destroys the property, and with it
//! the argument for having a generic representation at all.
//!
//! Two cases are refused rather than approximated, and both refusals are findings
//! rather than gaps.
//!
//! A 64-bit integer does not fit the 48-bit payload. Widening it would either
//! truncate or reinterpret it as a double, and a double loses integers past 53
//! bits. Doing it correctly needs a heap box, which is a memory operation and
//! not something this module can emit. So it is refused, loudly.
//!
//! A reference's *kind* is not in the encoding — the tag says a value is a
//! reference and nothing more. Proving which kind would mean reading the object,
//! which is a memory operation for the same reason. So a guard can establish
//! that a value is a reference, and cannot establish which.

use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_frontend::FunctionBuilder;

use super::error::LowerError;
use crate::repr::{RefKind, Repr};
use crate::tags::{
    BOX_BASE, CANONICAL_NAN, PAYLOAD_MASK, SINGLETON_FALSE, SINGLETON_TRUE, TAG_INT32,
    TAG_REFERENCE, TAG_SHIFT, TAG_SINGLETON,
};

/// The encoded word for a tag with a constant payload.
fn encoded(tag: u8, payload: u64) -> i64 {
    (BOX_BASE | (u64::from(tag) << TAG_SHIFT) | (payload & PAYLOAD_MASK)) as i64
}

/// Widens a proven value into the generic form.
pub fn widen(builder: &mut FunctionBuilder, value: Value, from: Repr) -> Result<Value, LowerError> {
    match from {
        Repr::I8 | Repr::I16 | Repr::I32 => {
            // The payload holds the low 32 bits; decoding sign-extends them, so
            // widening must not carry sign bits above bit 31 into the tag.
            let word = builder.ins().uextend(types::I64, value);
            let masked = builder.ins().band_imm(word, 0xFFFF_FFFFu32 as i64);
            let base = builder.ins().iconst(types::I64, encoded(TAG_INT32, 0));
            Ok(builder.ins().bor(base, masked))
        }

        Repr::F32 => {
            let promoted = builder.ins().fpromote(types::F64, value);
            widen(builder, promoted, Repr::F64)
        }

        Repr::F64 => {
            // Every real value keeps its own bits; only a NaN is normalized, so
            // that no double can land in the encoded quadrant and be mistaken
            // for a tagged word.
            let bits =
                builder
                    .ins()
                    .bitcast(types::I64, cranelift_codegen::ir::MemFlags::new(), value);
            let canonical = builder.ins().iconst(types::I64, CANONICAL_NAN as i64);
            let is_nan = builder.ins().fcmp(
                cranelift_codegen::ir::condcodes::FloatCC::NotEqual,
                value,
                value,
            );
            Ok(builder.ins().select(is_nan, canonical, bits))
        }

        Repr::Bool => {
            let f = builder.ins().iconst(
                types::I64,
                encoded(TAG_SINGLETON, u64::from(SINGLETON_FALSE)),
            );
            let t = builder.ins().iconst(
                types::I64,
                encoded(TAG_SINGLETON, u64::from(SINGLETON_TRUE)),
            );
            Ok(builder.ins().select(value, t, f))
        }

        Repr::Ref(_) => {
            let base = builder.ins().iconst(types::I64, encoded(TAG_REFERENCE, 0));
            let payload = builder.ins().band_imm(value, PAYLOAD_MASK as i64);
            Ok(builder.ins().bor(base, payload))
        }

        Repr::I64 => Err(LowerError::CannotWiden { from }),
        Repr::Tagged => Ok(value),
    }
}

/// Emits the test a guard performs, as a machine boolean.
pub fn test(
    builder: &mut FunctionBuilder,
    value: Value,
    expect: Repr,
) -> Result<Value, LowerError> {
    use cranelift_codegen::ir::condcodes::IntCC;

    let base = builder.ins().band_imm(value, BOX_BASE as i64);
    let is_encoded = builder.ins().icmp_imm(IntCC::Equal, base, BOX_BASE as i64);

    let raw = match expect {
        // A double is anything that is *not* in the encoded quadrant, which is
        // why the encoding reserves that quadrant in the first place.
        Repr::F64 => builder.ins().icmp_imm(IntCC::Equal, is_encoded, 0),

        Repr::I8 | Repr::I16 | Repr::I32 => has_tag(builder, value, is_encoded, TAG_INT32),
        Repr::Bool => has_tag(builder, value, is_encoded, TAG_SINGLETON),
        Repr::Ref(RefKind::Opaque) => has_tag(builder, value, is_encoded, TAG_REFERENCE),

        Repr::Ref(_) => return Err(LowerError::CannotProveReferenceKind { expect }),
        Repr::F32 | Repr::I64 | Repr::Tagged => {
            return Err(LowerError::CannotNarrow { to: expect });
        }
    };
    Ok(to_machine_bool(builder, raw))
}

/// Widens a comparison result to the width a boolean occupies.
///
/// Comparisons produce a byte; a boolean is a word here, for the reason
/// [`super::types::machine_type`] gives. Widening once, where booleans are made,
/// keeps every consumer from having to know which form it received.
pub fn to_machine_bool(builder: &mut FunctionBuilder, raw: Value) -> Value {
    builder.ins().uextend(types::I64, raw)
}

/// Narrows a generic value whose representation a guard has established.
///
/// Only correct where [`test`] holds. It is not exposed to clients for that
/// reason: the guard supplies the narrowed value to its success block, so the
/// narrowing exists only on the path that proved it.
pub fn narrow(builder: &mut FunctionBuilder, value: Value, to: Repr) -> Result<Value, LowerError> {
    match to {
        Repr::F64 => {
            Ok(builder
                .ins()
                .bitcast(types::F64, cranelift_codegen::ir::MemFlags::new(), value))
        }

        Repr::I8 | Repr::I16 | Repr::I32 => {
            let low = builder.ins().ireduce(types::I32, value);
            Ok(match to {
                Repr::I32 => low,
                narrower => builder
                    .ins()
                    .ireduce(super::types::machine_type(narrower), low),
            })
        }

        // A singleton's number is its payload, and the machine's booleans are
        // numbered so that the number *is* the truth value.
        Repr::Bool => {
            let payload = builder.ins().band_imm(value, PAYLOAD_MASK as i64);
            let raw = builder.ins().icmp_imm(
                cranelift_codegen::ir::condcodes::IntCC::Equal,
                payload,
                i64::from(SINGLETON_TRUE),
            );
            Ok(to_machine_bool(builder, raw))
        }

        Repr::Ref(RefKind::Opaque) => Ok(builder.ins().band_imm(value, PAYLOAD_MASK as i64)),

        Repr::Ref(_) => Err(LowerError::CannotProveReferenceKind { expect: to }),
        Repr::F32 | Repr::I64 | Repr::Tagged => Err(LowerError::CannotNarrow { to }),
    }
}

/// Whether an encoded word carries a particular tag.
fn has_tag(builder: &mut FunctionBuilder, value: Value, is_encoded: Value, tag: u8) -> Value {
    use cranelift_codegen::ir::condcodes::IntCC;

    let shifted = builder.ins().ushr_imm(value, i64::from(TAG_SHIFT));
    let masked = builder
        .ins()
        .band_imm(shifted, crate::tags::TAG_MASK as i64);
    let matches = builder.ins().icmp_imm(IntCC::Equal, masked, i64::from(tag));
    builder.ins().band(is_encoded, matches)
}
