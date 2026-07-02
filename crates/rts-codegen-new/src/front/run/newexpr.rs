//! `new C(args)` lowering — instantiate a user class (P4.9).
//!
//! A class instance IS an object in the P3.6 representation. `new C(args)`:
//!
//! 1. allocates a fresh `Entry::Vec` (the instance), pushes slot 0 = the class's
//!    GLOBAL shape-id (a tagged int) so the inspect trampoline recovers the field
//!    keys, then one `undefined` slot per field (zero-init);
//! 2. calls the synthesized constructor `__rtsn_ctor_C(this, args…)` with the
//!    instance word as `this` — the constructor's `this.field = …` writes land in
//!    the instance's slots (same VEC_SET path as an object literal);
//! 3. yields the instance as a `TAG_OBJECT` PolyValue (kind `Object`).
//!
//! The receiver's static class is recorded by the caller (`let c = new C()` →
//! `local_classes[c] = "C"`, `local_shapes[c] = Object(shape)`), so later
//! `c.field` / `c.method(args)` lower directly. A `new` of an unknown class (not
//! collected — e.g. a Registry/global class, or an `extends` class refused up
//! front) BAILS explicitly.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;

use rts_hir::HirExpr;

use crate::repr::Repr;
use crate::shape::ShapeId;
use crate::value::{self, emit_marshal};

use crate::front::error::{FrontResult, unsupported};

use super::lower::{JsKind, Lowerer, Val};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Lower `new C(args)`. Returns the instance `Val` (kind `Object`) plus the
    /// class name + the OBJECT shape id (interned in THIS fn's ShapeTable), so a
    /// `let` can record the local's class/shape for later member/method access.
    pub(super) fn lower_new(
        &mut self,
        module: &mut dyn Module,
        class: &str,
        args: &[HirExpr],
    ) -> FrontResult<(Val, String, ShapeId)> {
        // A local that REFERENCES a class as a value (`const C = Box; new C(..)`):
        // resolve `C` to the real class `Box` and construct it on the static path.
        // A local SHADOWS the bare class name, so this ref map is checked first.
        // Owned so the name outlives the later `&mut self` lowering calls.
        let class_owned: String = self
            .local_class_refs
            .get(class)
            .cloned()
            .unwrap_or_else(|| class.to_string());
        let class: &str = &class_owned;
        let Some(desc) = self.classes.get(class).cloned() else {
            return unsupported!(
                "`new {class}(..)` — class `{class}` is not a user class in this program \
                 (a global/Registry class or `extends` class is a later increment)"
            );
        };
        // `new AbstractC()` — a TS compile error, re-checked (the class lowers
        // normally for its concrete subclasses, but direct instantiation bails).
        if desc.is_abstract {
            return unsupported!("cannot instantiate abstract class `{class}`");
        }
        // NOTE: arity is validated by `marshal_call_args` against the ctor's
        // `FnSig` (which knows the rest param), not against `desc.ctor_arity` — a
        // variadic ctor accepts a variable count, so the exact `ctor_arity` gate is
        // wrong here.

        // ---- 1. allocate the instance Vec + slot 0 = global shape-id ----
        let obj_word = emit_marshal::emit_new_vec_object(module, self.builder);
        let id_word = self.builder.ins().iconst(
            types::I64,
            value::PolyValue::from_i32(desc.global_shape as i32).raw() as i64,
        );
        emit_marshal::emit_vec_push(module, self.builder, obj_word, id_word);
        // ---- one `undefined` field slot per field (zero-init) ----
        let undef = self
            .builder
            .ins()
            .iconst(types::I64, value::PolyValue::undefined().raw() as i64);
        for _ in &desc.fields {
            emit_marshal::emit_vec_push(module, self.builder, obj_word, undef);
        }

        // ---- 2. run the constructor with `this` = the instance ----
        // Lower each ctor arg, box to its param repr, prepend the instance as `this`.
        let sig = self
            .sigs
            .get(&desc.ctor)
            .cloned()
            .expect("class constructor must be a registered user function");
        // `this` is the constructor's first param: pass the instance word; the
        // explicit args are marshaled to the remaining params (packing a trailing
        // rest param's tail into a fresh array, F3b).
        let call_args = self.marshal_call_args(module, &sig, Some(obj_word), args)?;

        let cl_sig = sig.to_cranelift(module);
        let callee = module
            .declare_function(&sig.name, cranelift_module::Linkage::Local, &cl_sig)
            .map_err(|e| {
                crate::front::error::Unsupported::new(format!("declare ctor `{}`: {e}", sig.name))
            })?;
        let func_ref = module.declare_func_in_func(callee, self.builder.func);
        self.builder.ins().call(func_ref, &call_args);
        // P5.13: a `throw` inside the constructor must propagate (the ctor's sentinel
        // return left the pending-error slot set) — route the unwind here.
        self.emit_post_call_error_check(module)?;

        // ---- 2b. link the instance to the class's SHARED prototype object, so a
        //          later `C.prototype.m = v` reaches it through the dynamic-get
        //          prototype walk (own-key reads never touch this). The one-time
        //          init also wires `constructor.name` + the extends chain up to
        //          the Object.prototype root. ----
        let k = crate::value::abi_adapter::intern_poly(class);
        let k_word = self.builder.ins().iconst(types::I64, k.raw() as i64);
        let parent_word = match &desc.parent {
            Some(p) => {
                let pk = crate::value::abi_adapter::intern_poly(p);
                self.builder.ins().iconst(types::I64, pk.raw() as i64)
            }
            None => self
                .builder
                .ins()
                .iconst(types::I64, value::PolyValue::undefined().raw() as i64),
        };
        let proto = self
            .call_runtime(module, "__rtsadp_class_proto_init", &[k_word, parent_word])?
            .expect("__rtsadp_class_proto_init returns a word");
        self.call_runtime(module, "__rtsadp_obj_set_proto", &[obj_word, proto])?;

        // ---- 3. intern this fn's OBJECT shape (key list = the class fields) and
        //         yield the instance word as a TAG_OBJECT PolyValue ----
        let shape_id = self.shapes.intern(&desc.fields);
        Ok((
            Val::tagged_kind(obj_word, JsKind::Object),
            class.to_string(),
            shape_id,
        ))
    }

    /// Whether `name` is a FUNCTION usable as a constructor via `new name()` —
    /// a top-level free function (a `function F(){…}` decl or a hoisted `const F =
    /// function(){…}`) that carries a synthesized leading `this` (`has_this`) and
    /// is NOT a class fn (those have `has_this` cleared in `compile_program`) nor a
    /// closure (an env-capturing fn is a later increment). The DIRECT-call path
    /// then runs `F` with `this` = the fresh instance (Phase 2/3).
    pub(super) fn is_fn_ctor(&self, name: &str) -> bool {
        self.sigs
            .get(name)
            .is_some_and(|s| s.has_this && !s.is_async)
            && !self.captures.contains_key(name)
    }

    /// The runtime CONSTRUCTOR IDENTITY of fn-ctor `name`: its uniform-ABI THUNK
    /// address (a Cranelift `Value`). This is the SAME `fn_ptr` `new F()` records
    /// via `__rtsadp_ctor_mark` and `__rtsadp_fn_ptr` reads for a function VALUE,
    /// so `x instanceof F` compares the right identity.
    pub(super) fn fn_ctor_identity(
        &mut self,
        module: &mut dyn Module,
        name: &str,
    ) -> FrontResult<Value> {
        let thunk_id = *self.thunks.get(name).ok_or_else(|| {
            crate::front::error::Unsupported::new(format!("no thunk for `{name}`"))
        })?;
        let func_ref = module.declare_func_in_func(thunk_id, self.builder.func);
        Ok(self.builder.ins().func_addr(types::I64, func_ref))
    }

    /// Lower `new F(args)` where `F` is a function-as-constructor (Phase 2/3):
    ///
    /// 1. allocate a fresh `Entry::Vec` instance (no class fields — a bare object);
    /// 2. record `instance → F.fn_ptr` in the ctor side-table (so `this instanceof
    ///    F` inside the body, and a later `x instanceof F`, resolve true);
    /// 3. DIRECT-call `F` with `this` = the instance (reusing the Phase-1 has_this
    ///    marshaling, but with the instance instead of `undefined`);
    /// 4. JS `new` result: F's return value if it is an OBJECT, else the instance.
    ///
    /// Returns the result `Val` (kind Unknown — it could be the instance or F's
    /// object return). The DIRECT path requires `F`'s real signature, which is in
    /// `self.sigs`; the truly-opaque function-VALUE ctor (a `let g = ...; new g()`)
    /// is NOT handled here (it would need `__rtsadp_fn_invoke` to route a receiver
    /// — a later increment) and the caller bails for it.
    pub(super) fn lower_new_fn_ctor(
        &mut self,
        module: &mut dyn Module,
        name: &str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        let sig = self
            .sigs
            .get(name)
            .cloned()
            .expect("caller proved `name` is a fn-ctor in `sigs`");

        // ---- 1. fresh instance (a bare TAG_OBJECT, no field slots) ----
        let obj_word = emit_marshal::emit_new_vec_object(module, self.builder);

        // ---- 2. record instance → ctor identity (F's thunk address) ----
        // The identity is F's uniform-ABI THUNK address — the SAME `fn_ptr`
        // `__rtsadp_fn_ptr` reads for a function VALUE of `F` — so a later
        // `x instanceof F` (value form) agrees with the body's `this instanceof F`.
        let thunk_id = *self.thunks.get(name).ok_or_else(|| {
            crate::front::error::Unsupported::new(format!("no thunk for `{name}`"))
        })?;
        let func_ref = module.declare_func_in_func(thunk_id, self.builder.func);
        let fn_ptr = self.builder.ins().func_addr(types::I64, func_ref);
        self.call_runtime(module, "__rtsadp_ctor_mark", &[obj_word, fn_ptr])?;

        // ---- 3. DIRECT-call F with `this` = the instance ----
        let call_args = self.marshal_call_args(module, &sig, Some(obj_word), args)?;
        let ret = self.emit_user_call(module, &sig, &call_args)?;

        // ---- 4. new-result: F's return if it is an OBJECT, else the instance ----
        let ret_word = self.box_value(ret);
        let result = self.emit_new_result(obj_word, ret_word);
        Ok(Val::new(result, Repr::Tagged))
    }

    /// JS `new` result selection: `ret_word` if it is a boxed `TAG_OBJECT`,
    /// otherwise `instance`. Inline IR (an is-object tag check + `select`), no
    /// runtime call — `box(unbox(x))` folds in the egraph where it is monomorphic.
    fn emit_new_result(&mut self, instance: Value, ret_word: Value) -> Value {
        // is_object(ret) = boxed(ret) && tag(ret) == TAG_OBJECT.
        let boxed = value::emit_is_boxed(self.builder, ret_word);
        let shifted = self
            .builder
            .ins()
            .ushr_imm(ret_word, value::TAG_SHIFT as i64);
        let tag = self.builder.ins().band_imm(shifted, value::TAG_MASK as i64);
        let want = self
            .builder
            .ins()
            .iconst(types::I64, value::TAG_OBJECT as i64);
        let is_obj_tag = self.builder.ins().icmp(IntCC::Equal, tag, want);
        let is_object = self.builder.ins().band(boxed, is_obj_tag);
        // select(is_object, ret, instance).
        self.builder.ins().select(is_object, ret_word, instance)
    }
}
