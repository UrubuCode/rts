//! ASYNC function CALLS — the promise-centric model (#437) on the new engine.
//!
//! An `async function f(…)` COMPILES as a plain function (its body is ordinary
//! code; `await` inside it lowers to the blocking `__rtsadp_await`, which is
//! correct on the spawned worker). What makes it ASYNC is the CALL SITE: a
//! direct call `f(args)` does NOT run the body inline — it packs the marshaled
//! args into a fresh raw `Vec<i64>`, takes `f`'s REAL code address, and calls
//! `promise.create(fn_addr, args_vec)` (`__RTS_FN_NS_PROMISE_CREATE`), which
//! spawns the body on the shared tokio runtime and returns a PENDING
//! `Entry::PromiseAsync` handle immediately. The handle rides the NUMBER
//! surface (`await` / `promise.wait` / `Promise.all` unwrap exactly that
//! shape), so `Promise.all([f(..), f(..)])` runs bodies in PARALLEL — the
//! observable async semantic the suite pins (`promise_all_parallel`).
//!
//! Honest bails (never a wrong value):
//! - an `f64`-typed parameter: `promise.create`'s invoke trampoline passes raw
//!   `i64` words, which would misload an xmm param (the historical #206 class
//!   of bug) — refuse instead;
//! - `this`-using / rest-param async fns — a later increment.
//!
//! The async fn is EXCLUDED from the TCO tail set ([`super::tco`]) — its raw
//! address crosses to the runtime, so it must keep the default callconv.

use cranelift_codegen::ir::{InstBuilder, types};
use cranelift_module::Module;

use rts_hir::HirExpr;

use crate::repr::Repr;

use crate::front::error::{FrontResult, unsupported};

use super::lower::{JsKind, Lowerer, Val};
use super::sig::FnSig;

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Lower a direct CALL of the async function `sig` to a `promise.create`
    /// spawn. Returns the pending Promise handle as an `Int64` NUMBER value.
    pub(super) fn lower_async_spawn(
        &mut self,
        module: &mut dyn Module,
        sig: &FnSig,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        if sig.has_this || sig.rest_param.is_some() {
            return unsupported!(
                "async call to `{}` with `this`/rest params (a later increment)",
                sig.name
            );
        }
        if sig.params.len() > 8 {
            return unsupported!(
                "async call to `{}` with more than 8 params (the packed kinds word \
                 carries 8 — a later increment)",
                sig.name
            );
        }
        // Marshal the args exactly like a direct call (coerced to the callee's
        // native param reprs), then pack them into a fresh RAW `Vec<i64>` — the
        // `promise.create` args convention (`read_promise_vec` reads raw slots).
        // An `f64` param value travels as its BITS (kind 1 below); the typed
        // invoke (`invoke_typed` via the FN_KINDS registry) `from_bits` it back
        // into the xmm register — never the raw-i64 misload (the #206 family).
        let lowered = self.marshal_call_args(module, sig, None, args)?;
        let vec_h = self
            .call_runtime(module, "__RTS_FN_NS_COLLECTIONS_VEC_NEW", &[])?
            .expect("VEC_NEW returns a handle");
        for (v, &repr) in lowered.iter().zip(&sig.params) {
            let slot = match repr {
                // An i32 register widens to the i64 slot.
                Repr::Int32 => self.builder.ins().sextend(types::I64, *v),
                // An f64 register rides as its BITS (pure bitcast).
                Repr::Float64 => self
                    .builder
                    .ins()
                    .bitcast(types::I64, cranelift_codegen::ir::MemFlags::new(), *v),
                _ => *v,
            };
            self.call_runtime(module, "__RTS_FN_NS_COLLECTIONS_VEC_PUSH", &[vec_h, slot])?;
        }
        // The PACKED ABI the spawn registers for the typed invoke: one kind byte
        // per param (0 = i64-class, 1 = f64-bits, 2 = bool) + the return kind.
        let mut kinds_packed: u64 = 0;
        for (i, &repr) in sig.params.iter().enumerate() {
            let k: u64 = match repr {
                Repr::Float64 => 1,
                Repr::Bool => 2,
                _ => 0,
            };
            kinds_packed |= k << (8 * i);
        }
        let ret_kind: u64 = match sig.ret {
            Some(Repr::Float64) => 1,
            Some(Repr::Bool) => 2,
            _ => 0,
        };
        // The RESOLVE-value BOX kind (meta bits 16..23): how the raw invoke
        // return becomes a PolyValue WORD. A Tagged body already returns a word
        // (3 = verbatim); native reprs box by kind — a raw NEGATIVE i64 falls
        // inside the NaN-box quadrant and MUST be boxed, never stored raw.
        let box_kind: u64 = match sig.ret {
            Some(Repr::Float64) => 1,
            Some(Repr::Bool) => 2,
            Some(Repr::Int32) | Some(Repr::Int64) => 0,
            _ => 3,
        };
        let meta = (box_kind << 16) | ((sig.params.len() as u64) << 8) | ret_kind;
        let kinds_word = self.builder.ins().iconst(types::I64, kinds_packed as i64);
        let meta_word = self.builder.ins().iconst(types::I64, meta as i64);
        // The REAL code address of the async body (default callconv — excluded
        // from the TCO tail set by `sig.is_async`).
        let fid = self
            .ids
            .get(&sig.name)
            .copied()
            .ok_or_else(|| crate::front::error::Unsupported::new(format!(
                "async fn `{}` has no declared FuncId",
                sig.name
            )))?;
        let fref = module.declare_func_in_func(fid, self.builder.func);
        let fn_addr = self.builder.ins().func_addr(types::I64, fref);
        let handle = self
            .call_runtime(
                module,
                "__rtsadp_promise_spawn",
                &[fn_addr, vec_h, kinds_word, meta_word],
            )?
            .expect("__rtsadp_promise_spawn returns a handle");
        // The pending-Promise handle rides the NUMBER surface (`await` /
        // `promise.wait` / the combinator collectors unwrap it).
        Ok(Val::new_with_kind(handle, Repr::Int64, JsKind::Number))
    }
}
