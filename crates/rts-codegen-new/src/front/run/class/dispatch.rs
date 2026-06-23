//! Static CLASS-INSTANCE method dispatch lowering (P4.9).
//!
//! A `instance.method(args)` whose receiver's CLASS is statically proven (a
//! `new C()` result, a local recorded in `local_classes`, or `this` inside a
//! method) lowers to a DIRECT call of the synthesized `__rtsn_method_C_m(this,
//! args…)` — the receiver word is passed as the implicit `this` first argument.
//! No vtable, no string-compare dispatch: the method is resolved on the class
//! descriptor at compile time. A receiver of unknown class, or an unknown method
//! on a known class, BAILS (never a guess).

use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;

use rts_hir::HirExpr;
use rts_hir::ir::HirExprKind;

use crate::value;

use crate::front::error::{FrontResult, unsupported};

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
            HirExprKind::New { class, .. } => {
                // `new Box()` → `Box`; `new C()` where `C` is a class-reference local
                // (`const C = Box`) → the referenced class, so a chained
                // `new C().method()` dispatches statically.
                let class = self
                    .local_class_refs
                    .get(class)
                    .cloned()
                    .unwrap_or_else(|| class.clone());
                self.classes.get(&class).map(|_| class)
            }
            HirExprKind::Ident(name) => self
                .local_classes
                .get(name)
                .cloned()
                // A top-level SINGLETON INSTANCE (`const x = new C()`) force-promoted
                // to a gcell (so it reaches inside functions) loses the `local_classes`
                // entry a plain `new C()` local would carry. Recover its class from the
                // data-driven `gcell_classes` map (`name → C`, built by SHAPE from the
                // HIR — no hardcoded name) so `x.method(..)` dispatches on `C` in any
                // scope. Gated on `name` being an actual gcell (a same-named local
                // shadows it via `local_classes` above, checked first).
                .or_else(|| {
                    self.gcell_classes
                        .get(name)
                        .filter(|_| self.gcells.contains_key(name))
                        .cloned()
                }),
            // A CALL whose callee is a user function with a provable return class
            // (`expect(x)` → `Matcher`): lets a chained method dispatch statically on
            // the result (`expect(x).toBe(y)`). Only a bare-ident callee is resolved
            // (a method/computed callee's return class is a later increment).
            HirExprKind::Call { callee, .. } => match &callee.kind {
                HirExprKind::Ident(fname) => {
                    self.sigs.get(fname).and_then(|s| s.ret_class.clone())
                }
                _ => None,
            },
            // A METHOD CALL whose receiver's class is known and whose method has a
            // provable return class (`return this` → owning class, or `return new C`):
            // lets a fluent chain dispatch statically (`c.inc().add(5)`). Recurses
            // through the receiver, so an N-deep chain resolves.
            HirExprKind::MethodCall { object, method, .. } => {
                let recv_class = self.static_instance_class(object)?;
                let synth = self.classes.get(&recv_class)?.methods.get(method)?;
                self.sigs.get(synth).and_then(|s| s.ret_class.clone())
            }
            _ => None,
        }
    }

    /// Whether `object` is a bare identifier naming a user CLASS (not a local) —
    /// the receiver of a STATIC member access `C.m(..)` / `C.f`.
    pub(in crate::front::run) fn class_name_receiver(&self, object: &HirExpr) -> Option<String> {
        match &object.kind {
            HirExprKind::Ident(name)
                if self.local(name).is_none()
                    && self.local_classes.get(name).is_none()
                    && self.classes.get(name).is_some() =>
            {
                Some(name.clone())
            }
            _ => None,
        }
    }

    /// Lower a STATIC method call `C.m(args)` to a direct call of the synthesized
    /// `__rtsn_static_C_m(args…)` (no `this`). An unknown static method on the
    /// class BAILS.
    pub(in crate::front::run) fn try_static_method(
        &mut self,
        module: &mut dyn Module,
        class: &str,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        let desc = self
            .classes
            .get(class)
            .expect("static receiver must be a known class")
            .clone();
        let Some(fn_name) = desc.statics.get(method).cloned() else {
            return unsupported!("`{class}.{method}()` — no such static method on class `{class}`");
        };
        // `defineProperty(obj, …)` (Object/Reflect) GROWS `obj`'s shape at runtime
        // (engine.define_prop). A subsequent STATIC `obj.key` read would miss the new
        // key (it reads the compile-time shape) → demote the target local to dynamic
        // so later reads route through `__rtsadp_obj_get`. Keyed on the METHOD name
        // (a known shape-mutator), not the class — no non-primordial name in the front.
        if method == "defineProperty" {
            if let Some(HirExprKind::Ident(name)) = args.first().map(|a| &a.kind) {
                self.demote_local_to_dynamic(name);
            }
        }
        self.call_synth_fn(module, &fn_name, None, args)
    }

    /// Lower a STATIC field READ `C.f` to a call of its synthesized zero-arg getter
    /// (`__rtsn_sfield_C_f()`). An unknown static field BAILS. (A static-field
    /// WRITE is a later increment — handled as a bail at the call site.)
    pub(in crate::front::run) fn try_static_field_read(
        &mut self,
        module: &mut dyn Module,
        class: &str,
        field: &str,
    ) -> FrontResult<Val> {
        let desc = self
            .classes
            .get(class)
            .expect("static receiver must be a known class")
            .clone();
        let Some(fn_name) = desc.static_fields.get(field).cloned() else {
            return unsupported!("`{class}.{field}` — no such static field on class `{class}`");
        };
        self.call_synth_fn(module, &fn_name, None, &[])
    }

    /// Shared emitter for a direct call of a synthesized class fn `fn_name`,
    /// optionally passing a `this` receiver word as the first argument, with the
    /// explicit `args` coerced to the registered signature.
    pub(in crate::front::run) fn call_synth_fn(
        &mut self,
        module: &mut dyn Module,
        fn_name: &str,
        this_word: Option<Value>,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        let sig = self
            .sigs
            .get(fn_name)
            .cloned()
            .expect("synthesized class fn must be a registered user function");
        let call_args = self.marshal_call_args(module, &sig, this_word, args)?;

        let cl_sig = sig.to_cranelift(module);
        let callee = module
            .declare_function(fn_name, cranelift_module::Linkage::Local, &cl_sig)
            .map_err(|e| {
                crate::front::error::Unsupported::new(format!("declare fn `{fn_name}`: {e}"))
            })?;
        let func_ref = module.declare_func_in_func(callee, self.builder.func);
        let call = self.builder.ins().call(func_ref, &call_args);

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
        // P5.13: a `throw` inside the called method/getter/setter must propagate.
        self.emit_post_call_error_check(module)?;
        Ok(result)
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
        // The caller routes here only for a statically-resolved USER class. A
        // receiver whose recorded class is a Registry/global class (e.g. the
        // `WeakRef` result of `w.deref()`) has no `self.classes` entry — BAIL
        // honestly instead of panicking (dynamic/registry-object method dispatch
        // off such a value is a later increment).
        let Some(desc) = self.classes.get(class).cloned() else {
            return unsupported!(
                "`{class}.{method}()` on a receiver whose class `{class}` is not a \
                 statically-resolved user class (registry/dynamic-object method \
                 dispatch is a later increment)"
            );
        };
        let Some(fn_name) = desc.method_fn(method).map(str::to_string) else {
            return unsupported!(
                "`{class}.{method}()` — no such method on class `{class}` \
                 (a field-as-function / dynamic method is a later increment)"
            );
        };
        // VIRTUAL dispatch: when `method` is OVERRIDDEN somewhere in `class`'s
        // subtree, the receiver's runtime class may differ from its STATIC class
        // `class` (a `: C`-typed param, a `this` inside a base method, a base-typed
        // local) — resolve the target at RUNTIME by the instance's shape-id (slot 0).
        // An EXACT `new C()` receiver is monomorphic (its runtime class IS `class`),
        // so it keeps the DIRECT call below (the fast path).
        if !matches!(object.kind, HirExprKind::New { .. }) {
            if let Some(targets) = self.virtual_targets(class, method) {
                return self.emit_virtual_dispatch(module, object, &targets, &fn_name, args);
            }
        }
        // ---- receiver → `this` (first param) ----
        let recv = self.lower_expr(module, object)?;
        let this_word = self.box_value(recv);
        self.call_synth_fn(module, &fn_name, Some(this_word), args)
    }

    /// Lower an accessor GET `obj.x` where `x` is a getter on `class` (or an
    /// ancestor): a direct call of the synthesized getter with the receiver as
    /// `this`. The caller (`lower_member`) has already proven `x` is an accessor.
    pub(in crate::front::run) fn lower_accessor_get(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        class: &str,
        prop: &str,
    ) -> FrontResult<Val> {
        let getter = self
            .classes
            .get(class)
            .and_then(|d| d.accessor(prop))
            .and_then(|a| a.getter.clone());
        let Some(fn_name) = getter else {
            return unsupported!("`{class}.{prop}` is a write-only accessor (no getter)");
        };
        let recv = self.lower_expr(module, object)?;
        let this_word = self.box_value(recv);
        self.call_synth_fn(module, &fn_name, Some(this_word), &[])
    }

    /// Lower an accessor SET `obj.x = v` where `x` is a setter on `class` (or an
    /// ancestor): a direct call of the synthesized setter with the receiver as
    /// `this` and `v` as the single argument. Returns `v` (assignment's value).
    pub(in crate::front::run) fn lower_accessor_set(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        class: &str,
        prop: &str,
        value: &HirExpr,
    ) -> FrontResult<Val> {
        let setter = self
            .classes
            .get(class)
            .and_then(|d| d.accessor(prop))
            .and_then(|a| a.setter.clone());
        let Some(fn_name) = setter else {
            return unsupported!("`{class}.{prop}` is a read-only accessor (no setter)");
        };
        let recv = self.lower_expr(module, object)?;
        let this_word = self.box_value(recv);
        self.call_synth_fn(
            module,
            &fn_name,
            Some(this_word),
            std::slice::from_ref(value),
        )
    }
}
