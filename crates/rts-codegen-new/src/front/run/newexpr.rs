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

use cranelift_codegen::ir::{types, InstBuilder, Value};
use cranelift_module::Module;

use rts_hir::HirExpr;

use crate::shape::ShapeId;
use crate::value::{self, emit_marshal};

use crate::front::error::{unsupported, FrontResult};

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
        let Some(desc) = self.classes.get(class).cloned() else {
            return unsupported!(
                "`new {class}(..)` — class `{class}` is not a user class in this program \
                 (a global/Registry class or `extends` class is a later increment)"
            );
        };
        if args.len() != desc.ctor_arity {
            return unsupported!(
                "`new {class}(..)` expects {} constructor args, got {}",
                desc.ctor_arity,
                args.len()
            );
        }

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
        let mut call_args: Vec<Value> = Vec::with_capacity(args.len() + 1);
        // `this` is the constructor's first param (Tagged): pass the instance word.
        call_args.push(self.coerce(Val::tagged_kind(obj_word, JsKind::Unknown), sig.params[0])?);
        for (a, &want) in args.iter().zip(&sig.params[1..]) {
            let v = self.lower_expr(module, a)?;
            call_args.push(self.coerce(v, want)?);
        }

        let cl_sig = sig.to_cranelift(module);
        let callee = module
            .declare_function(&sig.name, cranelift_module::Linkage::Local, &cl_sig)
            .map_err(|e| {
                crate::front::error::Unsupported::new(format!("declare ctor `{}`: {e}", sig.name))
            })?;
        let func_ref = module.declare_func_in_func(callee, self.builder.func);
        self.builder.ins().call(func_ref, &call_args);

        // ---- 3. intern this fn's OBJECT shape (key list = the class fields) and
        //         yield the instance word as a TAG_OBJECT PolyValue ----
        let shape_id = self.shapes.intern(&desc.fields);
        Ok((Val::tagged_kind(obj_word, JsKind::Object), class.to_string(), shape_id))
    }
}
