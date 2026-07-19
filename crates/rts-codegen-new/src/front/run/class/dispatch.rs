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
                // scope. Gated on `name` being an actual gcell AND on NO same-named
                // LOCAL in the current function: a plain `const c = expr` local (whose
                // class is unknown → absent from `local_classes`) SHADOWS a top-level
                // `const c = new C()` — without the guard the local mis-dispatched on
                // the singleton's class.
                .or_else(|| {
                    self.gcell_classes
                        .get(name)
                        .filter(|_| self.gcells.contains_key(name) && self.local(name).is_none())
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

    /// The user class whose BODY the function being lowered belongs to, if any:
    /// a ctor/method/accessor binds `this` (`local_classes["this"]`); a STATIC
    /// method carries no `this`, so it is recovered from the synthesized name
    /// (`__rtsn_static_<C>_<m>`, matched against the known classes — longest
    /// class-name match wins so `A` never shadows `A_b`).
    pub(in crate::front::run) fn enclosing_class(&self) -> Option<String> {
        if let Some(c) = self.local_classes.get(super::THIS) {
            return Some(c.clone());
        }
        let rest = self.current_fn.strip_prefix("__rtsn_static_")?;
        self.classes
            .iter()
            .filter(|d| {
                rest.len() > d.name.len()
                    && rest.starts_with(&d.name)
                    && rest.as_bytes()[d.name.len()] == b'_'
            })
            .max_by_key(|d| d.name.len())
            .map(|d| d.name.clone())
    }

    /// JS PRIVATE NAMES are LEXICALLY scoped: an `x.#p` access is legal only
    /// inside the body of the class that DECLARES `#p` — real JS makes this a
    /// syntax error receiver-type-free, so the check fires for ANY receiver,
    /// even one of unknown class. (An inheritance-collision-mangled own access
    /// arrives here already rewritten to `#p@Class`, whose declarer IS the
    /// enclosing class — still exact.)
    pub(in crate::front::run) fn check_private_name_lexical(
        &self,
        prop: &str,
    ) -> FrontResult<()> {
        if !prop.starts_with('#') {
            return Ok(());
        }
        let ok = self.enclosing_class().is_some_and(|c| {
            self.classes.get(&c).is_some_and(|d| {
                d.member_access
                    .get(prop)
                    .is_some_and(|(_, declarer)| declarer == &c)
            })
        });
        if ok {
            Ok(())
        } else {
            unsupported!(
                "private name `{prop}` is not accessible here — a `#name` is \
                 lexically scoped to the body of its declaring class (JS spec)"
            )
        }
    }

    /// Enforce the TS/JS member-ACCESS surface for `class`.`member` from the
    /// current lowering context — the TS compile-error re-check, same family as
    /// the `is_abstract` re-check in `lower_new`. `Ok(())` when the member is
    /// public/unknown or the enclosing class is inside the visibility domain:
    /// - `private` / JS `#name`: only the DECLARING class's own body;
    /// - `protected`: the declaring class or any subclass (parent-chain walk).
    pub(in crate::front::run) fn check_member_access(
        &self,
        class: &str,
        member: &str,
    ) -> FrontResult<()> {
        use rts_ast::ast::Visibility;
        let Some((vis, declarer)) = self
            .classes
            .get(class)
            .and_then(|d| d.member_access.get(member))
        else {
            return Ok(());
        };
        let caller = self.enclosing_class();
        let allowed = match vis {
            Visibility::Public => true,
            Visibility::Private => caller.as_deref() == Some(declarer.as_str()),
            Visibility::Protected => {
                // The enclosing class or any ANCESTOR may be the declarer (a
                // subclass touching an inherited protected member is legal).
                let mut cur = caller;
                let mut ok = false;
                while let Some(c) = cur {
                    if &c == declarer {
                        ok = true;
                        break;
                    }
                    cur = self.classes.get(&c).and_then(|d| d.parent.clone());
                }
                ok
            }
        };
        if allowed {
            return Ok(());
        }
        let vis_name = match vis {
            Visibility::Private => "private",
            Visibility::Protected => "protected",
            Visibility::Public => "public",
        };
        unsupported!(
            "`{member}` is {vis_name} in class `{declarer}` and not accessible here \
             (the TS access-modifier surface, re-checked)"
        )
    }

    /// Whether `object` is a bare identifier naming a user CLASS (not a local) —
    /// the receiver of a STATIC member access `C.m(..)` / `C.f`.
    pub(in crate::front::run) fn class_name_receiver(&self, object: &HirExpr) -> Option<String> {
        // Peel a `… as T` cast — `(Foo as any).bar()` is `Foo.bar()` (the common
        // `(Ctor as any).call(this, …)` idiom in constructor-functions).
        let object = match &object.kind {
            HirExprKind::Cast { expr, .. } => expr.as_ref(),
            _ => object,
        };
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
        // `Ctor.call(thisArg, ...rest)` — ES5 super/mixin init: run the
        // constructor BODY on the EXISTING `thisArg` (no new allocation), so a
        // `Base.call(this, …)` inside a derived constructor-function copies Base's
        // fields onto the current instance. The derived class's shape already holds
        // Base's field slots (ctorfn expands `.call` mixins during field discovery).
        if method == "call" && !args.is_empty() {
            let this_v = self.lower_expr(module, &args[0])?;
            let this_word = self.box_value(this_v);
            return self.call_synth_fn(module, &desc.ctor, Some(this_word), &args[1..]);
        }
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
        // `C.prototype` — the class's ONE shared prototype object (lazy, runtime
        // side-table). `C.prototype.m = v` then writes it dynamically, and every
        // instance reaches it through the dynamic-get prototype walk.
        if field == "prototype" {
            let k = crate::value::abi_adapter::intern_poly_const(class);
            let k_word = self
                .builder
                .ins()
                .iconst(cranelift_codegen::ir::types::I64, k.raw() as i64);
            let w = self
                .call_runtime(module, "__rtsadp_class_proto", &[k_word])?
                .expect("__rtsadp_class_proto returns a word");
            return Ok(Val::tagged_kind(w, JsKind::Object));
        }
        let desc = self
            .classes
            .get(class)
            .expect("static receiver must be a known class")
            .clone();
        if let Some(fn_name) = desc.static_fields.get(field).cloned() {
            // A USER class's static field lives in a WRITABLE module-global cell
            // (`__rtsn_sfield_C_f`, promoted in `build_from_program`) — read it.
            // A prelude/ambient class without a cell keeps the zero-arg getter.
            if let Some(id) = self.gcell_id(&fn_name) {
                return self.emit_gcell_get(module, id);
            }
            return self.call_synth_fn(module, &fn_name, None, &[]);
        }
        // A static METHOD read as a VALUE (`if (Error.captureStackTrace)`,
        // `const f = C.staticMethod`): reify the synthesized static fn into a
        // TAG_FUNCTION value (truthy, callable). Falls through to the bail only when
        // `field` is neither a static field nor a static method.
        if let Some(fn_name) = desc.statics.get(field).cloned() {
            return self.reify_function(module, &fn_name);
        }
        unsupported!("`{class}.{field}` — no such static field on class `{class}`")
    }

    /// [`Self::call_synth_fn`] over PRE-LOWERED, PRE-BOXED arg WORDS — the
    /// single-eval virtual-dispatch marshal: each word is a Tagged PolyValue
    /// coerced to the callee's param repr; an omitted FILLABLE param pads
    /// `undefined`; extra words are dropped (JS ignores surplus args); a
    /// trailing REST param packs the surplus words into a fresh array. Lets
    /// the shape-arm emitters share ONE evaluation of every argument instead
    /// of re-lowering per arm (which restricted args to side-effect-free).
    pub(in crate::front::run) fn call_synth_fn_words(
        &mut self,
        module: &mut dyn Module,
        fn_name: &str,
        this_word: Option<Value>,
        arg_words: &[Value],
    ) -> FrontResult<Val> {
        use crate::repr::Repr;
        use crate::value::emit_marshal;
        let sig = self
            .sigs
            .get(fn_name)
            .cloned()
            .expect("synthesized class fn must be a registered user function");
        let mut call_args = Vec::with_capacity(sig.params.len());
        let mut pi = 0usize;
        if let Some(w) = this_word {
            call_args.push(self.coerce(Val::tagged_kind(w, JsKind::Object), sig.params[0])?);
            pi = 1;
        }
        let user_params = &sig.params[pi..];
        let rest_rel = sig.rest_param.map(|r| r.saturating_sub(pi));
        for (i, &want) in user_params.iter().enumerate() {
            if rest_rel == Some(i) {
                // Trailing rest: pack the surplus words into a fresh array.
                let arr = emit_marshal::emit_new_vec_object(module, self.builder);
                for &w in arg_words.iter().skip(i) {
                    emit_marshal::emit_vec_push(module, self.builder, arr, w);
                }
                call_args.push(self.coerce(Val::tagged_kind(arr, JsKind::Object), want)?);
                break;
            }
            if i < arg_words.len() {
                call_args.push(self.coerce(Val::new(arg_words[i], Repr::Tagged), want)?);
            } else if sig.fillable.get(pi + i).copied().unwrap_or(false) {
                call_args.push(self.undefined_coerced(want)?);
            } else {
                return unsupported!("call to `{fn_name}` missing required arg {i}");
            }
        }
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
        self.emit_post_call_error_check(module)?;
        Ok(result)
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

    /// Dispatch a singleton method whose slot was OVERRIDDEN earlier in this
    /// function (`console.table = fn`): read the own prop word; when it is a
    /// FUNCTION, invoke it; else fall back to the class method (the own slot may
    /// hold `undefined` after a restore). Args are lowered once per ARM — only
    /// the branch runtime takes executes.
    fn call_overridden_singleton(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        class: &str,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        use cranelift_codegen::ir::condcodes::IntCC;
        let recv = self.lower_expr(module, object)?;
        let recv_word = self.box_value(recv);
        let key = crate::value::abi_adapter::intern_poly_const(method);
        let key_word = self.builder.ins().iconst(types::I64, key.raw() as i64);
        let own = self
            .call_runtime(module, "__rtsadp_obj_get", &[recv_word, key_word])?
            .expect("__rtsadp_obj_get returns a value");
        let boxed = value::emit_is_boxed(self.builder, own);
        let shifted = self.builder.ins().ushr_imm(own, value::TAG_SHIFT as i64);
        let tag = self.builder.ins().band_imm(shifted, value::TAG_MASK as i64);
        let is_fn_tag =
            self.builder
                .ins()
                .icmp_imm(IntCC::Equal, tag, value::TAG_FUNCTION as i64);
        let is_fn = self.builder.ins().band(boxed, is_fn_tag);

        let b_fn = self.builder.create_block();
        let b_static = self.builder.create_block();
        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, types::I64);
        self.builder.ins().brif(is_fn, b_fn, &[], b_static, &[]);
        self.builder.seal_block(b_fn);
        self.builder.seal_block(b_static);

        self.builder.switch_to_block(b_fn);
        // Invoke through the FULL INVOKE_AUTO machinery with a COUNTED args
        // vec — a `(...args)` REST override needs its tail packed into one
        // array (the raw 6-slot invoke carries no argc and cannot pack; the
        // callbacks then read a bare first arg where the array should be).
        let vec_h = self
            .call_runtime(module, "__RTS_FN_NS_COLLECTIONS_VEC_NEW", &[])?
            .expect("VEC_NEW returns a handle");
        for a in args {
            let v = self.lower_expr(module, a)?;
            let w = self.box_value(v);
            self.call_runtime(module, "__RTS_FN_NS_COLLECTIONS_VEC_PUSH", &[vec_h, w])?;
        }
        let zero = self.builder.ins().iconst(types::I64, 0);
        let w1 = self
            .call_runtime(module, "__rtsadp_invoke_auto_word", &[own, zero, vec_h])?
            .expect("__rtsadp_invoke_auto_word returns a word");
        self.emit_post_call_error_check(module)?;
        self.builder.ins().jump(merge, &[w1.into()]);

        self.builder.switch_to_block(b_static);
        let fn_name = self
            .classes
            .get(class)
            .and_then(|d| d.method_fn(method).map(str::to_string));
        let w2 = match fn_name {
            Some(f) => {
                let v2 = self.call_synth_fn(module, &f, Some(recv_word), args)?;
                self.box_value(v2)
            }
            None => self
                .builder
                .ins()
                .iconst(types::I64, value::PolyValue::undefined().raw() as i64),
        };
        self.builder.ins().jump(merge, &[w2.into()]);

        self.builder.seal_block(merge);
        self.builder.switch_to_block(merge);
        let out = self.builder.block_params(merge)[0];
        Ok(Val::new(out, crate::repr::Repr::Tagged))
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
        // SINGLETON METHOD OVERRIDE (`console.table = fn` earlier in this
        // function): dispatch through the OWN prop word when it is a function,
        // else fall back to the class method (covers the restore-to-original
        // pattern where the own slot holds `undefined`).
        if let HirExprKind::Ident(n) = &object.kind {
            if self
                .singleton_overrides
                .contains(&(n.clone(), method.to_string()))
            {
                return self.call_overridden_singleton(module, object, class, method, args);
            }
        }
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
        // TS/JS ACCESS SURFACE (re-check): a `private`/`protected`/`#name` method
        // call outside its visibility domain refuses at compile time.
        self.check_member_access(class, method)?;
        let Some(fn_name) = desc.method_fn(method).map(str::to_string) else {
            // An ABSTRACT-declared method (no base impl, defined by descendants):
            // resolve VIRTUALLY by the instance's runtime shape. The last target
            // doubles as the default arm — TS guarantees the receiver is a
            // concrete subclass, so the default is exercised only by that class.
            if let Some(targets) = self.virtual_targets_abstract(class, method) {
                let (last, rest) = targets.split_last().expect("non-empty targets");
                let base_fn = last.1.clone();
                return self.emit_virtual_dispatch(module, object, rest, &base_fn, args);
            }
            // The Object-PROTOCOL surface every JS object inherits
            // (`hasOwnProperty`/`isPrototypeOf`/`propertyIsEnumerable`): a class
            // instance answers these like any object when its class chain does
            // not define them.
            if let Some(val) = self.try_object_protocol_method(module, object, method, args)? {
                return Ok(val);
            }
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
