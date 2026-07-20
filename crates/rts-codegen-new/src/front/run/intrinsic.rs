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
    /// Run a member's OWN native emitter over already-lowered `args`, or `None`
    /// to let the caller emit the ordinary call.
    ///
    /// This is the generic replacement for [`Self::emit_intrinsic`]: instead of
    /// the engine matching a closed enum, the member carries its emission and the
    /// engine just invokes it. Adding a natively-emitted operation becomes a
    /// change to a SPEC, never to the engine.
    ///
    /// The operands are handed over as PROVEN scalars only. A `Tagged` value
    /// could be any JS type and its coercion is owned by the marshalling
    /// authority (Pilar 3); reproducing that per emitter would fork it, so such
    /// sites fall through to the call.
    pub(super) fn emit_native(
        &mut self,
        emit: Option<rts_engine::NativeEmit>,
        arg_abis: &[rts_engine::abi::AbiType],
        args: &[Val],
    ) -> Option<Val> {
        use rts_engine::abi::AbiType;

        let emit = emit?;
        if args.len() != arg_abis.len() {
            return None;
        }
        // Hand the emitter operands ALREADY in the representation its own `Sig`
        // declares, coerced through `Lowerer::coerce` — the same authority
        // `marshal_reg_arg` uses for a real call. Without this an `Int32` operand
        // would reach an f64 instruction and produce invalid IR; with it, an
        // emitter can assume its declared types and nothing else.
        let mut vals = Vec::with_capacity(args.len());
        for (a, abi) in args.iter().zip(arg_abis) {
            // A Tagged operand is not provably numeric — its marshalling
            // dispatches on the runtime tag, so it takes the call.
            if !matches!(a.repr, Repr::Float64 | Repr::Int32 | Repr::Int64 | Repr::Bool) {
                return None;
            }
            let want = match abi {
                AbiType::F64 => Repr::Float64,
                AbiType::I64 | AbiType::U64 | AbiType::I32 | AbiType::Bool => Repr::Int64,
                // Handle / StrPtr / Void are not scalars an emitter can take.
                _ => return None,
            };
            vals.push(self.coerce(*a, want).ok()?);
        }
        // The emitter reports its own result repr through the value it builds;
        // we read it back off the builder rather than trusting a declaration.
        let out = emit(self.builder, &vals)?;
        let repr = match self.builder.func.dfg.value_type(out) {
            types::F64 => Repr::Float64,
            types::I32 => Repr::Int32,
            _ => Repr::Int64,
        };
        Some(Val::new(out, repr))
    }

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
            // ── `num` bit ops ────────────────────────────────────────────────
            Intrinsic::CountOnes => self.i64_unary(args, |b, x| b.ins().popcnt(x)),
            Intrinsic::CountZeros => self.i64_unary(args, |b, x| {
                let inv = b.ins().bnot(x);
                b.ins().popcnt(inv)
            }),
            Intrinsic::LeadingZeros => self.i64_unary(args, |b, x| b.ins().clz(x)),
            Intrinsic::TrailingZeros => self.i64_unary(args, |b, x| b.ins().ctz(x)),
            Intrinsic::SwapBytes => self.i64_unary(args, |b, x| b.ins().bswap(x)),
            Intrinsic::WrappingNeg => self.i64_unary(args, |b, x| b.ins().ineg(x)),
            Intrinsic::RotateLeft => self.i64_binary(args, |b, x, n| b.ins().rotl(x, n)),
            Intrinsic::RotateRight => self.i64_binary(args, |b, x, n| b.ins().rotr(x, n)),
            Intrinsic::WrappingAdd => self.i64_binary(args, |b, x, y| b.ins().iadd(x, y)),
            Intrinsic::WrappingSub => self.i64_binary(args, |b, x, y| b.ins().isub(x, y)),
            Intrinsic::WrappingMul => self.i64_binary(args, |b, x, y| b.ins().imul(x, y)),
            Intrinsic::WrappingShl => self.i64_binary(args, |b, x, n| b.ins().ishl(x, n)),
            Intrinsic::WrappingShr => self.i64_binary(args, |b, x, n| b.ins().sshr(x, n)),
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
        f: impl FnOnce(
            &mut cranelift_frontend::FunctionBuilder,
            cranelift_codegen::ir::Value,
        ) -> cranelift_codegen::ir::Value,
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
        f: impl FnOnce(
            &mut cranelift_frontend::FunctionBuilder,
            cranelift_codegen::ir::Value,
        ) -> cranelift_codegen::ir::Value,
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

    /// A PROVEN-number operand as `f64`, or `None`.
    ///
    /// Only `Float64` / `Int32` / `Int64` qualify. A `Tagged` value could be any
    /// JS type and its marshalling dispatches on the RUNTIME tag
    /// (`__rtsadp_word_to_abi_i64`) — reproducing that here would fork the
    /// coercion authority (Pilar 3), so those sites fall back to the call. The
    /// conversion itself is [`Lowerer::coerce`] — the SAME function
    /// `marshal_reg_arg` uses for an `AbiType::F64` slot, so an intrinsic site
    /// and a call site coerce identically.
    fn as_f64(&mut self, v: Val) -> Option<cranelift_codegen::ir::Value> {
        match v.repr {
            Repr::Float64 | Repr::Int32 | Repr::Int64 => self.coerce(v, Repr::Float64).ok(),
            _ => None,
        }
    }

    /// A PROVEN-number operand as `i64` (same reasoning as [`Self::as_f64`]).
    ///
    /// `Float64` is accepted and TRUNCATED, matching `marshal_reg_arg`'s
    /// `AbiType::I64` path exactly — TS `number` is an f64, so rejecting it would
    /// leave every `num.*` call site on the slow path for no soundness gain.
    fn as_i64(&mut self, v: Val) -> Option<cranelift_codegen::ir::Value> {
        match v.repr {
            Repr::Float64 | Repr::Int32 | Repr::Int64 => self.coerce(v, Repr::Int64).ok(),
            _ => None,
        }
    }
}
