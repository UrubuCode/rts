//! Object + array literals, property/index access, and `.length` — P3.
//!
//! The compile-time SHAPES slice of the object model. An object/array value is a
//! REAL `Entry::Vec` of PolyValue words (the inline slot array), reached as a
//! `TAG_OBJECT` [`crate::value::PolyValue`] whose 48-bit payload is the Vec's
//! handle slot — exactly the strings/handles bridge, with the object tag. A
//! property read `obj.a` lowers to `VEC_GET(obj, slot)` with `slot` the
//! COMPILE-TIME constant from the object's interned [`crate::shape::Shape`]; an
//! array index `arr[i]` to `VEC_GET(obj, i)`; writes to `VEC_SET`; `arr.length`
//! to `VEC_LEN`.
//!
//! Everything that needs the not-yet-built dynamic machinery BAILS explicitly:
//! - an object literal with a computed / spread / method / getter/setter key
//!   (the HIR has already dropped non-`KeyValue`/`Shorthand` props, so we can
//!   only see static-string-keyed literals — but a literal whose lowered field
//!   list does not match a clean static shape still bails);
//! - a property/index access whose object's shape is NOT statically proven in the
//!   ctx (a param, a call return, a reassigned local) — the dynamic IC is a later
//!   increment, and guessing is forbidden;
//! - adding a NEW key not already in the object's shape (the transition tree is a
//!   later increment).

use cranelift_codegen::ir::{types, InstBuilder, Value};
use cranelift_module::Module;

use rts_hir::ir::HirExprKind;
use rts_hir::HirExpr;

use crate::repr::Repr;
use crate::shape::ShapeId;
use crate::value::{self, emit_marshal};

use crate::front::error::{unsupported, FrontResult};

use super::lower::{HeapShape, JsKind, Lowerer, Val};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Lower an object literal `{k0: v0, k1: v1, …}` (all keys statically known)
    /// to a fresh `Entry::Vec` filled in shape-slot order, boxed as a `TAG_OBJECT`
    /// PolyValue. Returns the object `Val` plus the interned [`ShapeId`] so the
    /// caller (`let`) can record it for later `obj.key` resolution.
    pub(super) fn lower_object_literal(
        &mut self,
        module: &mut dyn Module,
        fields: &[(String, HirExpr)],
    ) -> FrontResult<(Val, ShapeId)> {
        // A duplicate key keeps the LAST value (JS); de-dup to the last occurrence
        // while preserving first-seen slot order — but if a literal repeats a key
        // we conservatively bail (rare, and the shape/value alignment is subtle).
        let keys: Vec<String> = fields.iter().map(|(k, _)| k.clone()).collect();
        let mut seen = std::collections::HashSet::new();
        for k in &keys {
            if !seen.insert(k) {
                return unsupported!("object literal with a duplicated key `{k}`");
            }
        }
        let shape = self.shapes.intern(&keys);
        // Reserve a GLOBALLY-UNIQUE shape id (indexes the process-global registry
        // the inspect trampoline reads) and bake it into slot 0 of the object so
        // console.log can recover the keys at runtime. Property values live at
        // slot 1 + slot_index (see `lower_member`/`lower_member_assign`).
        let global_id = crate::shape::intern_global_shape(&keys);

        let obj_word = emit_marshal::emit_new_vec_object(module, self.builder);
        // ---- slot 0: the global shape-id, boxed as a tagged int PolyValue ----
        let id_word = self
            .builder
            .ins()
            .iconst(types::I64, value::PolyValue::from_i32(global_id as i32).raw() as i64);
        emit_marshal::emit_vec_push(module, self.builder, obj_word, id_word);
        // ---- slots 1.. : property values in key order, each a boxed PolyValue ----
        for (_, value_expr) in fields {
            let v = self.lower_expr(module, value_expr)?;
            let word = self.box_value(v);
            emit_marshal::emit_vec_push(module, self.builder, obj_word, word);
        }
        Ok((Val::tagged_kind(obj_word, JsKind::Unknown), shape))
    }

    /// Lower an array literal `[e0, e1, …]` to a fresh `Entry::Vec` filled in
    /// order with each element's boxed PolyValue word, boxed as a `TAG_OBJECT`
    /// PolyValue (arrays and objects share the representation; `typeof []` is
    /// `"object"`). Returns the array `Val`.
    pub(super) fn lower_array_literal(
        &mut self,
        module: &mut dyn Module,
        elems: &[HirExpr],
    ) -> FrontResult<Val> {
        // A spread element needs runtime expansion (a later increment) — bail.
        for e in elems {
            if matches!(e.kind, HirExprKind::Spread(_)) {
                return unsupported!("spread in an array literal");
            }
        }
        let arr_word = emit_marshal::emit_new_vec_object(module, self.builder);
        for e in elems {
            let v = self.lower_expr(module, e)?;
            let word = self.box_value(v);
            emit_marshal::emit_vec_push(module, self.builder, arr_word, word);
        }
        Ok(Val::tagged_kind(arr_word, JsKind::Unknown))
    }

    /// Lower a member access `obj.prop`. Resolves the object's proven heap shape
    /// from the ctx:
    /// - object shape with `prop` present → `VEC_GET(obj, const slot)` (a Tagged
    ///   PolyValue word — the stored slot);
    /// - object shape WITHOUT `prop` → JS `undefined` (a missing key reads
    ///   `undefined`, statically known here);
    /// - array `.length` → `VEC_LEN` (an unboxed `Int64`);
    /// - anything else (unknown-shape object, array non-`.length` member) bails.
    pub(super) fn lower_member(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        prop: &str,
    ) -> FrontResult<Val> {
        // ---- static member read `C.f` (C is a class name, not a local) ----
        if let Some(class) = self.class_name_receiver(object) {
            return self.try_static_field_read(module, &class, prop);
        }
        // ---- runtime/Registry-class instance PROPERTY (`m.size`, `e.message`) ----
        if let Some(val) = self.try_global_class_member(module, object, prop)? {
            return Ok(val);
        }
        // ---- accessor GET `obj.x` where x is a getter on the receiver class ----
        if let Some(class) = self.instance_class_of(object) {
            if self
                .classes
                .get(&class)
                .and_then(|d| d.accessor(prop))
                .is_some()
            {
                return self.lower_accessor_get(module, object, &class, prop);
            }
        }
        let (name, shape) = self.shaped_object(object)?;
        match shape {
            HeapShape::Object(shape_id) => {
                match self.shapes.slot_of(shape_id, prop) {
                    Some(slot) => {
                        // Property values live at slot 1 + slot_index (slot 0 is the
                        // shape-id header). Arrays are NOT shifted (handled below).
                        let obj_word = self.load_local_word(&name);
                        let idx = self.builder.ins().iconst(types::I64, 1 + slot as i64);
                        let word = emit_marshal::emit_vec_get(module, self.builder, obj_word, idx);
                        Ok(Val::new(word, Repr::Tagged))
                    }
                    // Missing key → `undefined` (statically proven by the shape).
                    None => Ok(self.undefined_val()),
                }
            }
            HeapShape::Array => {
                if prop == "length" {
                    let arr_word = self.load_local_word(&name);
                    let len = emit_marshal::emit_vec_len(module, self.builder, arr_word);
                    Ok(Val::new(len, Repr::Int64))
                } else {
                    unsupported!("array member `.{prop}` (only `.length` in this increment)")
                }
            }
        }
    }

    /// Lower an index access `obj[index]`. Only an array with a numeric index is
    /// supported (a constant-or-computed integer → `VEC_GET`). A computed object
    /// key `obj[k]` (string/dynamic index) needs the dynamic property path and
    /// BAILS.
    pub(super) fn lower_index(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        index: &HirExpr,
    ) -> FrontResult<Val> {
        let (name, shape) = self.shaped_object(object)?;
        if !matches!(shape, HeapShape::Array) {
            return unsupported!("computed index on a non-array (dynamic object key is a later increment)");
        }
        let idx = self.lower_index_value(module, index)?;
        let arr_word = self.load_local_word(&name);
        let word = emit_marshal::emit_vec_get(module, self.builder, arr_word, idx);
        Ok(Val::new(word, Repr::Tagged))
    }

    /// Lower `obj.prop = value` (member-write) to `VEC_SET` at the constant slot
    /// (object) — adding a NEW key bails. Array `.length = n` bails (resize is a
    /// later increment).
    pub(super) fn lower_member_assign(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        prop: &str,
        value: &HirExpr,
    ) -> FrontResult<Val> {
        // ---- accessor SET `obj.x = v` where x is a setter on the receiver class ----
        if let Some(class) = self.instance_class_of(object) {
            if self
                .classes
                .get(&class)
                .and_then(|d| d.accessor(prop))
                .is_some()
            {
                return self.lower_accessor_set(module, object, &class, prop, value);
            }
        }
        let (name, shape) = self.shaped_object(object)?;
        let HeapShape::Object(shape_id) = shape else {
            return unsupported!("write to array member `.{prop}` (later increment)");
        };
        let Some(slot) = self.shapes.slot_of(shape_id, prop) else {
            return unsupported!("adding a new key `{prop}` to an object (transition tree is a later increment)");
        };
        let v = self.lower_expr(module, value)?;
        let word = self.box_value(v);
        let obj_word = self.load_local_word(&name);
        // Property values live at slot 1 + slot_index (slot 0 is the shape-id).
        let idx = self.builder.ins().iconst(types::I64, 1 + slot as i64);
        emit_marshal::emit_vec_set(module, self.builder, obj_word, idx, word);
        Ok(Val::new(word, Repr::Tagged))
    }

    /// Lower `arr[index] = value` (index-write) to `VEC_SET`.
    pub(super) fn lower_index_assign(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        index: &HirExpr,
        value: &HirExpr,
    ) -> FrontResult<Val> {
        let (name, shape) = self.shaped_object(object)?;
        if !matches!(shape, HeapShape::Array) {
            return unsupported!("indexed write on a non-array (dynamic object key is a later increment)");
        }
        let idx = self.lower_index_value(module, index)?;
        let v = self.lower_expr(module, value)?;
        let word = self.box_value(v);
        let arr_word = self.load_local_word(&name);
        emit_marshal::emit_vec_set(module, self.builder, arr_word, idx, word);
        Ok(Val::new(word, Repr::Tagged))
    }

    // ---- helpers ----

    /// The statically-known CLASS of an instance-valued `object` (a `new C()` or a
    /// local recorded in `local_classes`), for accessor resolution. A plain
    /// object literal (no class) yields `None`.
    fn instance_class_of(&self, object: &HirExpr) -> Option<String> {
        self.static_instance_class(object)
    }

    /// Resolve `object` to `(local_name, proven_heap_shape)`. Only a bare
    /// identifier bound to a local of proven shape qualifies; everything else
    /// (a param, a call result, a nested expression) bails — the engine refuses
    /// to guess an object's layout.
    fn shaped_object(&self, object: &HirExpr) -> FrontResult<(String, HeapShape)> {
        let HirExprKind::Ident(name) = &object.kind else {
            return unsupported!("property/index access on a non-identifier object (unknown shape)");
        };
        match self.local_shapes.get(name) {
            Some(&shape) => Ok((name.clone(), shape)),
            None => unsupported!(
                "property/index access on `{name}` whose shape is not statically proven \
                 (param/return/reassigned — the dynamic inline cache is a later increment)"
            ),
        }
    }

    /// Load the raw object/array PolyValue word held by a local (the local's repr
    /// is `Tagged`, an i64 register).
    fn load_local_word(&mut self, name: &str) -> Value {
        let local = self.local(name).expect("shaped local must exist");
        self.builder.use_var(local.var)
    }

    /// Lower an array index expression to an unboxed i64 index. A proven integer
    /// rides its register; a proven double truncates to i64; a Tagged index
    /// bails (a non-integer / object key is out of this increment).
    fn lower_index_value(
        &mut self,
        module: &mut dyn Module,
        index: &HirExpr,
    ) -> FrontResult<Value> {
        let v = self.lower_expr(module, index)?;
        match v.repr {
            Repr::Int32 | Repr::Int64 => Ok(v.v),
            Repr::Float64 => Ok(self.builder.ins().fcvt_to_sint(types::I64, v.v)),
            _ => unsupported!("non-integer array index"),
        }
    }

    /// The `undefined` PolyValue as a Tagged `Val`.
    fn undefined_val(&mut self) -> Val {
        let v = self
            .builder
            .ins()
            .iconst(types::I64, value::PolyValue::undefined().raw() as i64);
        Val::tagged_kind(v, JsKind::Undefined)
    }
}
