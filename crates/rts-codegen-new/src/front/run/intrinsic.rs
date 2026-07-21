//! NATIVE EMISSION — a Registry member emits Cranelift IR at the call site
//! instead of an `extern "C"` call.
//!
//! Each member carries its own emitter (`rts_engine::NativeEmit`), so the
//! emission lives WITH THE SPEC (in `rts-shared`/`rts-primitives`) and the engine
//! has ONE generic entry point here — [`Lowerer::emit_native`]. This replaced the
//! closed `abi::Intrinsic` enum, which forced a variant + a `match` arm per
//! operation inside the engine (engine-side knowledge of a non-primordial, which
//! the doctrine forbids). See `CRANELIFT_IMPLEMENTATION.md` step 8.
//!
//! ## Why it matters
//!
//! A call is a register spill, an optimisation barrier the e-graph cannot see
//! through, and — in a hot loop — the dominant cost. `sqrt` is ONE machine
//! instruction; wrapping it in a call is pure overhead.
//!
//! ## Fallback, never a failure
//!
//! [`Lowerer::emit_native`] returns `None` whenever it cannot handle a site (an
//! operand whose repr is not a proven scalar, a wrong arg count). The caller then
//! emits the ordinary call. So a native emitter can make a site faster, never
//! broken.

use cranelift_codegen::ir::{InstBuilder, types};


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
}
