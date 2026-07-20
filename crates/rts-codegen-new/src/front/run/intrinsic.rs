//! INTRINSIC emission — a Registry member lowered as Cranelift IR instead of an
//! `extern "C"` call.
//!
//! `rts-engine::abi::Intrinsic` and the `Some(Intrinsic::…)` registrations have
//! existed for a long time, but the current engine never read them: every
//! `math.sqrt(x)` was a `call`. (The docs claiming otherwise describe the deleted
//! old engine.) This module is the consumer.
//!
//! ## Why it matters
//!
//! A call is a register spill, an optimisation barrier the e-graph cannot see
//! through, and — in a hot loop — the dominant cost. `sqrt` is ONE machine
//! instruction; wrapping it in a call is pure overhead.
//!
//! ## Fallback, never a failure
//!
//! [`emit`] returns `None` whenever it cannot handle a site (an argument whose
//! repr is not the scalar the instruction needs, a wrong arg count). The caller
//! then emits the ordinary call. So adding an intrinsic can make a site faster,
//! never broken.

use cranelift_codegen::ir::{InstBuilder, types};

use rts_engine::abi::Intrinsic;

use super::lower::{Lowerer, Val};
use crate::repr::Repr;

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Emit `intr` over already-lowered `args` as inline IR, or `None` to let the
    /// caller fall back to the extern call.
    pub(super) fn emit_intrinsic(&mut self, intr: Intrinsic, args: &[Val]) -> Option<Val> {
        match intr {
            // ── f64 unary ────────────────────────────────────────────────────
            Intrinsic::Sqrt => self.f64_unary(args, |b, x| b.ins().sqrt(x)),
            Intrinsic::AbsF64 => self.f64_unary(args, |b, x| b.ins().fabs(x)),
            // ── f64 binary ───────────────────────────────────────────────────
            Intrinsic::MinF64 => self.f64_binary(args, |b, x, y| b.ins().fmin(x, y)),
            Intrinsic::MaxF64 => self.f64_binary(args, |b, x, y| b.ins().fmax(x, y)),
            // ── i64 ──────────────────────────────────────────────────────────
            // No `iabs` in Cranelift: branchless |x| = (x ^ (x>>63)) - (x>>63).
            Intrinsic::AbsI64 => self.i64_unary(args, |b, x| {
                let sign = b.ins().sshr_imm(x, 63);
                let flipped = b.ins().bxor(x, sign);
                b.ins().isub(flipped, sign)
            }),
            Intrinsic::MinI64 => self.i64_binary(args, |b, x, y| {
                let lt = b.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::SignedLessThan,
                    x,
                    y,
                );
                b.ins().select(lt, x, y)
            }),
            Intrinsic::MaxI64 => self.i64_binary(args, |b, x, y| {
                let gt = b.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::SignedGreaterThan,
                    x,
                    y,
                );
                b.ins().select(gt, x, y)
            }),
            // `ReceiverIdentity` is a METHOD-dispatch tag (return the receiver
            // unchanged); it is not a namespace-call intrinsic and is handled by
            // the class-method emitter, not here.
            Intrinsic::ReceiverIdentity => None,
        }
    }

    /// One `Float64` arg in, one out — else `None` (caller falls back).
    fn f64_unary(
        &mut self,
        args: &[Val],
        f: impl FnOnce(&mut cranelift_frontend::FunctionBuilder, cranelift_codegen::ir::Value)
            -> cranelift_codegen::ir::Value,
    ) -> Option<Val> {
        let [a] = args else { return None };
        let x = self.as_f64(*a)?;
        Some(Val::new(f(self.builder, x), Repr::Float64))
    }

    fn f64_binary(
        &mut self,
        args: &[Val],
        f: impl FnOnce(
            &mut cranelift_frontend::FunctionBuilder,
            cranelift_codegen::ir::Value,
            cranelift_codegen::ir::Value,
        ) -> cranelift_codegen::ir::Value,
    ) -> Option<Val> {
        let [a, b] = args else { return None };
        let x = self.as_f64(*a)?;
        let y = self.as_f64(*b)?;
        Some(Val::new(f(self.builder, x, y), Repr::Float64))
    }

    fn i64_unary(
        &mut self,
        args: &[Val],
        f: impl FnOnce(&mut cranelift_frontend::FunctionBuilder, cranelift_codegen::ir::Value)
            -> cranelift_codegen::ir::Value,
    ) -> Option<Val> {
        let [a] = args else { return None };
        let x = self.as_i64(*a)?;
        Some(Val::new(f(self.builder, x), Repr::Int64))
    }

    fn i64_binary(
        &mut self,
        args: &[Val],
        f: impl FnOnce(
            &mut cranelift_frontend::FunctionBuilder,
            cranelift_codegen::ir::Value,
            cranelift_codegen::ir::Value,
        ) -> cranelift_codegen::ir::Value,
    ) -> Option<Val> {
        let [a, b] = args else { return None };
        let x = self.as_i64(*a)?;
        let y = self.as_i64(*b)?;
        Some(Val::new(f(self.builder, x, y), Repr::Int64))
    }

    /// A PROVEN `f64` operand, or `None`. Only `Float64` and `Int32`/`Int64`
    /// (widened) qualify: a `Tagged` value could be any JS type, and unboxing it
    /// here would duplicate the coercion authority the call path already owns.
    fn as_f64(&mut self, v: Val) -> Option<cranelift_codegen::ir::Value> {
        match v.repr {
            Repr::Float64 => Some(v.v),
            Repr::Int32 | Repr::Int64 => Some(self.builder.ins().fcvt_from_sint(types::F64, v.v)),
            _ => None,
        }
    }

    /// A PROVEN `i64` operand, or `None` (same reasoning as [`Self::as_f64`]).
    fn as_i64(&mut self, v: Val) -> Option<cranelift_codegen::ir::Value> {
        match v.repr {
            Repr::Int64 => Some(v.v),
            Repr::Int32 => Some(self.builder.ins().sextend(types::I64, v.v)),
            _ => None,
        }
    }
}
