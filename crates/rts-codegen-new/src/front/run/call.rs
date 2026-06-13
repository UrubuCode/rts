//! Call lowering + truthiness for the whole-program path.
//!
//! Split out of [`super::expr`] (the <500-line module rule). Covers the two
//! Tagged-boundary call shapes the engine runs — `console.log(...)` and
//! cross-function calls — plus the JS `ToBoolean` reduction
//! ([`Lowerer::as_bool_value`]) used by `if`/`while`/ternary conditions.

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{types, InstBuilder, Value};
use cranelift_module::Module;

use rts_hir::ir::HirExprKind;
use rts_hir::HirExpr;

use crate::repr::Repr;
use crate::value;

use crate::front::error::{unsupported, FrontResult, Unsupported};

use super::lower::{JsKind, Lowerer, Val};
use super::sig::FnSig;

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// `console.log(...)` arrives as a `MethodCall` on `console`; any other
    /// method call is a later increment.
    pub(super) fn lower_method_call(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        if is_console_ident(object) && method == "log" {
            return self.lower_console_log(module, args);
        }
        unsupported!("method call `.{method}()` (only `console.log` in this increment)")
    }

    /// A `Call` node: either `console.log(...)` (callee is a `console.log`
    /// Member) or a cross-function call to a user function by name.
    pub(super) fn lower_call(
        &mut self,
        module: &mut dyn Module,
        callee: &HirExpr,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        if let HirExprKind::Member { object, prop } = &callee.kind {
            if is_console_ident(object) && prop == "log" {
                return self.lower_console_log(module, args);
            }
            return unsupported!("call of member `.{prop}()`");
        }
        let name = match &callee.kind {
            HirExprKind::Ident(n) => n.clone(),
            _ => return unsupported!("call of a non-identifier callee"),
        };
        let sig = self
            .sigs
            .get(&name)
            .ok_or_else(|| Unsupported::new(format!("call to unknown function `{name}`")))?
            .clone();
        self.lower_user_call(module, &sig, args)
    }

    /// Lower `console.log(a, b, …)`: box each arg to a PolyValue and call the
    /// fixed-arity entry. Returns `undefined` (console.log's JS result).
    fn lower_console_log(
        &mut self,
        module: &mut dyn Module,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        let sym = crate::runtime::console::console_log_symbol(args.len()).ok_or_else(|| {
            Unsupported::new(format!("console.log with {} args (max 6 supported)", args.len()))
        })?;
        let mut boxed = Vec::with_capacity(args.len());
        for a in args {
            let v = self.lower_expr(module, a)?;
            boxed.push(self.box_value(v));
        }
        self.call_runtime(module, sym, &boxed)?;
        let v = self
            .builder
            .ins()
            .iconst(types::I64, value::PolyValue::undefined().raw() as i64);
        Ok(Val::tagged_kind(v, JsKind::Undefined))
    }

    /// Lower a cross-function call: coerce each argument to the callee's param
    /// repr (box/unbox/widen per `FnSig`), emit the Cranelift `call`, and tag the
    /// result with the callee's return repr.
    fn lower_user_call(
        &mut self,
        module: &mut dyn Module,
        sig: &FnSig,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        if args.len() != sig.params.len() {
            return unsupported!(
                "call to `{}` expects {} args, got {}",
                sig.name,
                sig.params.len(),
                args.len()
            );
        }
        let mut lowered = Vec::with_capacity(args.len());
        for (a, &want) in args.iter().zip(&sig.params) {
            let v = self.lower_expr(module, a)?;
            lowered.push(self.coerce(v, want)?);
        }

        let cl_sig = sig.to_cranelift(module);
        let callee = module
            .declare_function(&sig.name, cranelift_module::Linkage::Local, &cl_sig)
            .map_err(|e| Unsupported::new(format!("declare callee `{}`: {e}", sig.name)))?;
        let func_ref = module.declare_func_in_func(callee, self.builder.func);
        let call = self.builder.ins().call(func_ref, &lowered);

        match sig.ret {
            Some(ret) => {
                let v = self.builder.inst_results(call)[0];
                Ok(Val::new(v, ret))
            }
            None => {
                let v = self
                    .builder
                    .ins()
                    .iconst(types::I64, value::PolyValue::undefined().raw() as i64);
                Ok(Val::tagged_kind(v, JsKind::Undefined))
            }
        }
    }

    /// Reduce `val` to an i64 0/1 condition usable by `brif`/`select`, applying
    /// JS `ToBoolean`. A `Bool` is already 0/1. A proven number folds inline
    /// (non-zero & non-NaN is truthy). A Tagged value goes through the runtime
    /// `__rtsn_to_boolean` (which resolves the empty-string case on the heap).
    pub(super) fn as_bool_value(
        &mut self,
        module: &mut dyn Module,
        val: Val,
    ) -> FrontResult<Value> {
        match val.repr {
            Repr::Bool => Ok(val.v),
            Repr::Int32 | Repr::Int64 => {
                let zero = self.builder.ins().iconst(types::I64, 0);
                let b = self.builder.ins().icmp(IntCC::NotEqual, val.v, zero);
                Ok(self.builder.ins().uextend(types::I64, b))
            }
            Repr::Float64 => {
                // truthy iff x != 0 and x == x (NaN compares unequal to itself).
                let zero = self.builder.ins().f64const(0.0);
                let nonzero = self.builder.ins().fcmp(FloatCC::NotEqual, val.v, zero);
                let ordered = self.builder.ins().fcmp(FloatCC::Equal, val.v, val.v);
                let both = self.builder.ins().band(nonzero, ordered);
                Ok(self.builder.ins().uextend(types::I64, both))
            }
            Repr::Tagged => {
                let res = self
                    .call_runtime(module, "__rtsn_to_boolean", &[val.v])?
                    .expect("__rtsn_to_boolean returns a value");
                Ok(res)
            }
            other => unsupported!("condition of repr {other:?}"),
        }
    }
}

/// Whether an expr is the bare `console` identifier (the object of `console.log`).
fn is_console_ident(e: &HirExpr) -> bool {
    matches!(&e.kind, HirExprKind::Ident(n) if n == "console")
}
