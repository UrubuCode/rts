//! Static CLASS-INSTANCE method dispatch lowering (P4.9).
//!
//! A `instance.method(args)` whose receiver's CLASS is statically proven (a
//! `new C()` result, a local recorded in `local_classes`, or `this` inside a
//! method) lowers to a DIRECT call of the synthesized `__rtsn_method_C_m(this,
//! args…)` — the receiver word is passed as the implicit `this` first argument.
//! No vtable, no string-compare dispatch: the method is resolved on the class
//! descriptor at compile time. A receiver of unknown class, or an unknown method
//! on a known class, BAILS (never a guess).

use cranelift_codegen::ir::{types, InstBuilder, Value};
use cranelift_module::Module;

use rts_hir::ir::HirExprKind;
use rts_hir::HirExpr;

use crate::value;

use crate::front::error::{unsupported, FrontResult};

use super::super::lower::{JsKind, Lowerer, Val};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// The statically-known CLASS of a method-call receiver, if any:
    /// - `new C(args)` → `C` (chained `new C().m()`);
    /// - a bare identifier (a local/param/`this`) recorded in `local_classes`.
    ///
    /// Returns `None` when the receiver's class is not statically proven — the
    /// caller falls through to the primordial-dispatch / bail paths (we never
    /// guess a class).
    pub(in crate::front::run) fn static_instance_class(&self, object: &HirExpr) -> Option<String> {
        match &object.kind {
            HirExprKind::New { class, .. } => self.classes.get(class).map(|_| class.clone()),
            HirExprKind::Ident(name) => self.local_classes.get(name).cloned(),
            _ => None,
        }
    }

    /// Lower `instance.method(args)` for a statically-known class via a DIRECT call
    /// to the synthesized `__rtsn_method_C_m(this, args…)`. The receiver is lowered
    /// (a `new C()` instance, a local, or `this`), boxed to its `this` slot, and
    /// the args coerced to the method signature. An unknown method on the class
    /// BAILS (never a guess); a getter/setter/static was already refused at class
    /// collection.
    pub(in crate::front::run) fn try_class_method(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        class: &str,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        let desc = self
            .classes
            .get(class)
            .expect("class table must contain a statically-resolved class")
            .clone();
        let Some(fn_name) = desc.method_fn(method).map(str::to_string) else {
            return unsupported!(
                "`{class}.{method}()` — no such method on class `{class}` \
                 (a field-as-function / inherited / dynamic method is a later increment)"
            );
        };
        let sig = self
            .sigs
            .get(&fn_name)
            .cloned()
            .expect("synthesized method must be a registered user function");
        // sig.params[0] is `this`; the rest are the method's declared params.
        let want_args = sig.params.len().saturating_sub(1);
        if args.len() != want_args {
            return unsupported!(
                "`{class}.{method}()` expects {want_args} args, got {}",
                args.len()
            );
        }

        // ---- receiver → `this` (first param) ----
        let recv = self.lower_expr(module, object)?;
        let this_word = self.box_value(recv);
        let mut call_args: Vec<Value> =
            vec![self.coerce(Val::tagged_kind(this_word, JsKind::Object), sig.params[0])?];
        // ---- explicit args coerced to the method param reprs ----
        for (a, &want) in args.iter().zip(&sig.params[1..]) {
            let v = self.lower_expr(module, a)?;
            call_args.push(self.coerce(v, want)?);
        }

        // ---- emit the direct call ----
        let cl_sig = sig.to_cranelift(module);
        let callee = module
            .declare_function(&fn_name, cranelift_module::Linkage::Local, &cl_sig)
            .map_err(|e| {
                crate::front::error::Unsupported::new(format!("declare method `{fn_name}`: {e}"))
            })?;
        let func_ref = module.declare_func_in_func(callee, self.builder.func);
        let call = self.builder.ins().call(func_ref, &call_args);

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
}
