//! Argument-spread call lowering (P5.6) — split out of [`super::call`] (the
//! <500-line module rule).
//!
//! Covers `f(...arr)` into a user function: unpack the array's first
//! `params.len()` elements into the native params, plus the shared
//! `emit_user_call` helper the non-spread path also uses. rts-hir now preserves
//! the spread flag (`HirExprKind::Spread`) on call args, so the engine can model
//! argument spread instead of bailing.

use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;

use rts_hir::HirExpr;

use crate::repr::Repr;
use crate::value;
use crate::value::emit_marshal;

use crate::front::error::{FrontResult, Unsupported, unsupported};

use super::lower::{JsKind, Lowerer, Val};
use super::sig::FnSig;

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Lower `f(...arr)`: unpack `arr[0..params.len()]` (each a Tagged PolyValue
    /// word via `VEC_GET`) into the native params, coercing each to its param repr.
    /// `inner` is the spread operand, already proven to be an array by the caller.
    pub(super) fn lower_user_call_spread(
        &mut self,
        module: &mut dyn Module,
        sig: &FnSig,
        inner: &HirExpr,
    ) -> FrontResult<Val> {
        if !self.is_array_valued(inner) {
            return unsupported!(
                "call to `{}` with `...spread` of a non-array value (later increment)",
                sig.name
            );
        }
        let arr = self.lower_expr(module, inner)?;
        let arr_word = self.box_value(arr);
        let mut lowered = Vec::with_capacity(sig.params.len());
        for (i, &want) in sig.params.iter().enumerate() {
            let idx = self.builder.ins().iconst(types::I64, i as i64);
            let word = emit_marshal::emit_vec_get(module, self.builder, arr_word, idx);
            lowered.push(self.spread_elem_to_param(module, word, want, &sig.name)?);
        }
        self.emit_user_call(module, sig, &lowered)
    }

    /// Lower `f(a, b, ...arr)`: the `leading` positional args fill `params[0..k]`,
    /// then the trailing spread fills `params[k..]` from `arr[0..]` (out-of-range →
    /// `undefined`, like the sole-`...arr` path). The caller proved the fn is
    /// non-rest, non-`this`, and the spread is the single LAST arg.
    pub(super) fn lower_user_call_spread_mixed(
        &mut self,
        module: &mut dyn Module,
        sig: &FnSig,
        leading: &[HirExpr],
        inner: &HirExpr,
    ) -> FrontResult<Val> {
        if !self.is_array_valued(inner) {
            return unsupported!(
                "call to `{}` with `...spread` of a non-array value (later increment)",
                sig.name
            );
        }
        let mut lowered = Vec::with_capacity(sig.params.len());
        // Leading positionals: lower + coerce to the matching param repr. More
        // positionals than params is a benign JS arity overrun → ignore the extras.
        let k = leading.len().min(sig.params.len());
        for (i, arg) in leading.iter().take(k).enumerate() {
            let val = self.lower_expr(module, arg)?;
            lowered.push(self.coerce(val, sig.params[i])?);
        }
        // Trailing spread: fill the remaining params from `arr[0..]`.
        let arr = self.lower_expr(module, inner)?;
        let arr_word = self.box_value(arr);
        for (j, &want) in sig.params.iter().enumerate().skip(k) {
            let idx = self.builder.ins().iconst(types::I64, (j - k) as i64);
            let word = emit_marshal::emit_vec_get(module, self.builder, arr_word, idx);
            lowered.push(self.spread_elem_to_param(module, word, want, &sig.name)?);
        }
        self.emit_user_call(module, sig, &lowered)
    }

    /// Coerce one spread-element PolyValue `word` to a native param of repr `want`.
    /// A numeric param normalizes the element via `__rtsadp_canon_double` (ToNumber,
    /// since a stored tagged-int32 word would mis-read through the pure double
    /// bitcast) then narrows to int when needed. `Tagged` keeps the raw word; a
    /// `Bool` param is a later increment → bail.
    fn spread_elem_to_param(
        &mut self,
        module: &mut dyn Module,
        word: Value,
        want: Repr,
        sig_name: &str,
    ) -> FrontResult<Value> {
        let arg = match want {
            Repr::Tagged => word,
            Repr::Float64 | Repr::Int32 | Repr::Int64 => {
                let canon = self
                    .call_runtime(module, "__rtsadp_canon_double", &[word])?
                    .expect("__rtsadp_canon_double returns a value");
                let f = value::emit_unbox_double(self.builder, canon);
                if matches!(want, Repr::Float64) {
                    f
                } else {
                    self.builder.ins().fcvt_to_sint(types::I64, f)
                }
            }
            other => {
                return unsupported!(
                    "`...spread` into a `{:?}` param of `{}` (later increment)",
                    other,
                    sig_name
                );
            }
        };
        Ok(arg)
    }

    /// Emit the Cranelift `call` to user function `sig` with already-marshaled
    /// args, and tag the result with the callee's return repr.
    pub(super) fn emit_user_call(
        &mut self,
        module: &mut dyn Module,
        sig: &FnSig,
        lowered: &[Value],
    ) -> FrontResult<Val> {
        let cl_sig = sig.to_cranelift(module);
        let callee = module
            .declare_function(&sig.name, cranelift_module::Linkage::Local, &cl_sig)
            .map_err(|e| Unsupported::new(format!("declare callee `{}`: {e}", sig.name)))?;
        let func_ref = module.declare_func_in_func(callee, self.builder.func);
        let call = self.builder.ins().call(func_ref, lowered);

        let result = match sig.ret {
            Some(ret) => {
                let v = self.builder.inst_results(call)[0];
                Val::new(v, ret)
            }
            None => {
                let v = self
                    .builder
                    .ins()
                    .iconst(types::I64, value::PolyValue::undefined().raw() as i64);
                Val::tagged_kind(v, JsKind::Undefined)
            }
        };
        // P5.13: a user call may have thrown (its manual-unwind sentinel return left
        // the pending-error slot set). Route the unwind before the result is used.
        self.emit_post_call_error_check(module)?;
        Ok(result)
    }
}
