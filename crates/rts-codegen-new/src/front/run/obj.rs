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

use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;

use rts_hir::HirExpr;
use rts_hir::ir::HirExprKind;

use crate::repr::Repr;
use crate::shape::ShapeId;
use crate::value::{self, abi_adapter, emit_marshal};

use crate::front::error::{FrontResult, Unsupported, unsupported};

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
    ) -> FrontResult<(Val, ShapeId, Option<String>)> {
        // P5.15: a leading `__rtsl_class__` marker field carries the recovered
        // literal-class name (the object-literal method recovery prepended it). Strip
        // it BEFORE shape interning / slot fill — it is never a real property — and
        // surface the class so the caller records the local in `local_classes`. A
        // sentinel marker (an UNSUPPORTED member: getter/setter/computed/generator/
        // async/spread) bails the literal rather than degrade to a partial object.
        let (lit_class, fields) = strip_lit_class_marker(fields);
        if lit_class.as_deref() == Some(crate::front::run::desugar::LIT_UNSUPPORTED) {
            return unsupported!(
                "object literal with an unsupported member (getter/setter/computed/generator/async method or spread)"
            );
        }

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
        let id_word = self.builder.ins().iconst(
            types::I64,
            value::PolyValue::from_i32(global_id as i32).raw() as i64,
        );
        emit_marshal::emit_vec_push(module, self.builder, obj_word, id_word);
        // ---- slots 1.. : property values in key order, each a boxed PolyValue ----
        for (_, value_expr) in fields {
            let v = self.lower_expr(module, value_expr)?;
            let word = self.box_value(v);
            emit_marshal::emit_vec_push(module, self.builder, obj_word, word);
        }
        Ok((
            Val::tagged_kind(obj_word, JsKind::Unknown),
            shape,
            lit_class,
        ))
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
        let arr_word = emit_marshal::emit_new_vec_object(module, self.builder);
        for e in elems {
            // A spread element `[...src]` (P5.6): `src` must be a proven ARRAY, whose
            // elements are appended onto the new array via `__rtsadp_arr_spread_append`.
            // A non-array spread (object/iterable/string) is a later increment → bail.
            if let HirExprKind::Spread(inner) = &e.kind {
                // Resolve the spread source to an element-ARRAY word, then append.
                // A proven array rides itself; an ITERABLE (Set/Map/generator/string)
                // is materialized via the SAME machinery for-of uses (the
                // `Symbol.iterator` protocol / generator DRAIN / `to_iter_array`), so
                // `[...set]` / `[...gen()]` / `[..."abc"]` work like JS.
                let src_word = self.spread_source_array_word(module, inner)?;
                self.call_runtime(module, "__rtsadp_arr_spread_append", &[arr_word, src_word])?;
                continue;
            }
            let v = self.lower_expr(module, e)?;
            let word = self.box_value(v);
            emit_marshal::emit_vec_push(module, self.builder, arr_word, word);
        }
        Ok(Val::tagged_kind(arr_word, JsKind::Unknown))
    }

    /// Resolve a spread source `[...inner]` to an element-ARRAY word. A proven array
    /// is used directly; an iterable class (`[Symbol.iterator]()`), a lazy generator,
    /// a proven string, or a generic Tagged value is materialized to an array via the
    /// same paths for-of uses. Bails on a value that is provably NOT iterable
    /// (number/bool) or a known non-iterable class instance.
    fn spread_source_array_word(
        &mut self,
        module: &mut dyn Module,
        inner: &HirExpr,
    ) -> FrontResult<cranelift_codegen::ir::Value> {
        if self.is_array_valued(inner) {
            let src = self.lower_expr(module, inner)?;
            return Ok(self.box_value(src));
        }
        // Iterable class (Set/Map/user with `[Symbol.iterator]`) → call it + collect.
        if let Some(w) = self.try_class_iterator_source_word(module, inner)? {
            return Ok(w);
        }
        // Lazy generator call → DRAIN to an array.
        if let Some(w) = self.try_lazy_gen_source_word(module, inner)? {
            return Ok(w);
        }
        // Proven string / generic Tagged (param/binding) → runtime array coercion
        // (string → chars, array → self). A proven number/bool, or a known
        // runtime-class instance with no iterator, bails (not iterable).
        let v = self.lower_expr(module, inner)?;
        if matches!(v.kind, JsKind::Str) {
            let word = self.box_value(v);
            return self
                .call_runtime(module, "__rtsadp_str_chars", &[word])?
                .ok_or_else(|| Unsupported::new("__rtsadp_str_chars returns a value".to_string()));
        }
        if matches!(v.repr, Repr::Tagged)
            && self.static_instance_class(inner).is_none()
            && self.global_instance_class(inner).is_none()
        {
            let word = self.box_value(v);
            return self
                .call_runtime(module, "__rtsadp_to_iter_array", &[word])?
                .ok_or_else(|| {
                    Unsupported::new("__rtsadp_to_iter_array returns a value".to_string())
                });
        }
        unsupported!(
            "spread of a non-iterable value in an array literal (number/bool, or a class with no iterator)"
        )
    }

    /// Lower a member access `obj.prop`. Resolves the object's proven heap shape
    /// from the ctx:
    /// - object shape with `prop` present → `VEC_GET(obj, const slot)` (a Tagged
    ///   PolyValue word — the stored slot);
    /// - object shape WITHOUT `prop` → JS `undefined` (a missing key reads
    ///   `undefined`, statically known here);
    /// - array `.length` → `VEC_LEN` (an unboxed `Int64`);
    /// - anything else (unknown-shape object, array non-`.length` member) bails.
    /// Resolve a NAMESPACE-OBJECT constant read (`math.PI`) to a value: the
    /// constant is a zero-arg `MemberKind::Constant` getter in the Registry, so we
    /// resolve it (`registry::namespace_const`) and emit the zero-arg call through
    /// the generic marshal. `Ok(None)` ⇒ the namespace has no such constant (the
    /// caller falls through to its other member handlers / bails).
    fn try_namespace_const(
        &mut self,
        module: &mut dyn Module,
        ns: &str,
        name: &str,
    ) -> FrontResult<Option<Val>> {
        let Some(call) = super::registry::namespace_const(ns, name) else {
            return Ok(None);
        };
        self.emit_registry_call(module, &call, None, &[], JsKind::Number)
            .map(Some)
    }

    pub(super) fn lower_member(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        prop: &str,
    ) -> FrontResult<Val> {
        // ---- `globalThis.prop` read: the singleton global object is a keyed
        // OBJECT of runtime-only shape, so resolve the property dynamically (a
        // missing key reads `undefined`, JS-correct). ----
        if self.is_globalthis_receiver(object) {
            return self.lower_dynamic_get_expr(module, object, prop);
        }
        // ---- NAMESPACE-OBJECT constant read (`math.PI`, `math.E`): `math` was
        // imported from bare `"rts"` (bound as a namespace object, member empty).
        // The constant is a zero-arg `MemberKind::Constant` getter in the Registry
        // (`__RTS_FN_NS_MATH_PI()`); resolve + call it. Mirrors the namespace-object
        // METHOD path in `try_method_dispatch`. An unknown const falls through. ----
        if let HirExprKind::Ident(obj) = &object.kind {
            if let Some((ns, member)) = self.builtins.get(obj).cloned() {
                if member.is_empty() {
                    if let Some(val) = self.try_namespace_const(module, &ns, prop)? {
                        return Ok(val);
                    }
                }
            }
        }
        // ---- `Math.CONST` / `Number.CONST` (P5.4): a namespace-constant read ----
        if let Some(val) = self.try_math_number_const(object, prop)? {
            return Ok(val);
        }
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
        // ---- `.length` on an `Object.keys/values/entries(o)` result expression
        // (a Call(Member) that always yields a freshly-built engine array): lower
        // the array word directly and read `VEC_LEN`. Restricted to the Object
        // statics (never `Array.from`, whose result may be an unsupported-source
        // sentinel that must bail at use, not silently read length 0).
        if prop == "length" && self.is_object_static_array_expr(object) {
            let arr = self.lower_expr(module, object)?;
            let arr_word = self.box_value(arr);
            let len = emit_marshal::emit_vec_len(module, self.builder, arr_word);
            return Ok(Val::new(len, Repr::Int64));
        }
        // ---- DATA PROPERTY on a FUNCTION value `F.foo` (Phase 4): the receiver is
        // a function value (a top-level/hoisted fn, incl. a `this`-using fn-ctor).
        // Read the recorded property from the function-property side-table at
        // RUNTIME via `__rtsadp_fn_get_prop` (absent → undefined). Routed BEFORE the
        // shape/array paths, which a function receiver has no static shape for. ----
        if let Some(fn_word) = self.fn_value_word(module, object)? {
            let key_word = self.intern_key_word(prop);
            let word = self
                .call_runtime(module, "__rtsadp_fn_get_prop", &[fn_word, key_word])?
                .expect("__rtsadp_fn_get_prop returns a value");
            return Ok(Val::new_with_kind(word, Repr::Tagged, JsKind::Unknown));
        }
        // ---- DYNAMIC object property read (P5.5): the receiver is a proven keyed
        // OBJECT whose exact shape is NOT statically known (a reassigned object
        // local). Resolve the key index at RUNTIME via `__rtsadp_obj_get`. ----
        if let HirExprKind::Ident(name) = &object.kind {
            if self.object_locals.contains(name) {
                return self.lower_dynamic_get(module, name, prop);
            }
        }
        // ---- DYNAMIC `.length` (P5.9): the receiver is a Tagged value of unproven
        // shape (a param / call return / re-`let` local). `.length` is defined on
        // strings (length) and arrays (element count); dispatch on the receiver's
        // PolyValue tag AT RUNTIME via `__rtsadp_dyn_length` (any other tag reads
        // `undefined`, the JS-correct missing-property result). Only `.length` is
        // routed here — other dynamic property reads keep their existing bail. ----
        if prop == "length" && self.is_dynamic_length_receiver(object) {
            let recv = self.lower_expr(module, object)?;
            let recv_word = self.box_value(recv);
            let word = self
                .call_runtime(module, "__rtsadp_dyn_length", &[recv_word])?
                .expect("__rtsadp_dyn_length returns a value");
            // The result is a number PolyValue word (or undefined); kind Number so
            // it ToStrings / arithmetics as a number.
            return Ok(Val::new_with_kind(word, Repr::Tagged, JsKind::Number));
        }
        // ---- string FIELD read (CHANGE 2): `obj.<field>` where the receiver's
        // class proves `<field>` is string-typed. Read the slot word (VEC_GET, as
        // for any field) but tag the resulting `Val` `JsKind::Str` so downstream
        // `.length`/`[i]`/string methods (e.g. `this.#value.charCodeAt(i)`) see a
        // string. Distinct from the `prop == "length"` fast-path below, which reads
        // the LENGTH of a string field/literal. ----
        if let Some(class) = self.static_instance_class(object) {
            if self
                .classes
                .get(&class)
                .map(|d| d.field_is_string(prop))
                .unwrap_or(false)
            {
                let (recv_word, shape) = self.resolve_heap_receiver(module, object)?;
                let HeapShape::Object(shape_id) = shape else {
                    return unsupported!("string field `.{prop}` on a non-object receiver");
                };
                let Some(slot) = self.shapes.slot_of(shape_id, prop) else {
                    return unsupported!("string field `.{prop}` not in the receiver's shape");
                };
                let idx = self.builder.ins().iconst(types::I64, 1 + slot as i64);
                let word = emit_marshal::emit_vec_get(module, self.builder, recv_word, idx);
                return Ok(Val::tagged_kind(word, JsKind::Str));
            }
        }
        // ---- native STRING receiver fast-path (CHANGE 3): `s.length` on a proven
        // string — a literal, a string-typed field (CHANGE 2), or a string param.
        // `.length` reuses the existing `__rtsadp_dyn_length` op (which reads the
        // string's UTF-16 code-unit count from its tag); a string method call
        // (`s.charCodeAt(i)`) is handled by the dispatch path once the receiver is a
        // Str `Val`. Any other string member read keeps the existing bail. ----
        if prop == "length" && self.receiver_is_proven_string(object) {
            let recv = self.lower_string_receiver(module, object)?;
            let recv_word = self.box_value(recv);
            let word = self
                .call_runtime(module, "__rtsadp_dyn_length", &[recv_word])?
                .expect("__rtsadp_dyn_length returns a value");
            return Ok(Val::new_with_kind(word, Repr::Tagged, JsKind::Number));
        }
        match self.resolve_heap_receiver(module, object) {
            Ok((recv_word, HeapShape::Object(shape_id))) => {
                match self.shapes.slot_of(shape_id, prop) {
                    Some(slot) => {
                        // Property values live at slot 1 + slot_index (slot 0 is the
                        // shape-id header). Arrays are NOT shifted (handled below).
                        let idx = self.builder.ins().iconst(types::I64, 1 + slot as i64);
                        let word = emit_marshal::emit_vec_get(module, self.builder, recv_word, idx);
                        Ok(Val::new(word, Repr::Tagged))
                    }
                    // Missing key → `undefined` (statically proven by the shape).
                    None => Ok(self.undefined_val()),
                }
            }
            Ok((recv_word, HeapShape::Array)) => {
                if prop == "length" {
                    let len = emit_marshal::emit_vec_len(module, self.builder, recv_word);
                    Ok(Val::new(len, Repr::Int64))
                } else {
                    // A non-`.length` member on a PROVEN array (e.g. `arr.foo`) has no
                    // own property — read it dynamically (JS-correct `undefined` for an
                    // absent key) rather than bail.
                    self.lower_dynamic_get_expr(module, object, prop)
                }
            }
            // ---- DYNAMIC FALLBACK: the receiver's shape is NOT statically proven (a
            // param, a call return, a reassigned local, a non-identifier object). Read
            // the property by key AT RUNTIME via `__rtsadp_obj_get`, which returns the
            // stored slot for a keyed object and `undefined` for an absent key OR a
            // non-object receiver (JS: `(5).foo` is `undefined`). Correct for any
            // receiver; the proven-shape fast paths above kept their constant-slot
            // load and took priority.
            //
            // EXCEPTION: `.length` is NOT a stored slot — on an array it is the element
            // count, on a string the code-unit count — and `__rtsadp_obj_get` would
            // wrongly read `undefined` for it. The proven-receiver `.length` paths ran
            // above; an UNPROVEN `.length` is the dynamic-length feature (the existing
            // `__rtsadp_dyn_length` path, gated by `is_dynamic_length_receiver`) and is
            // intentionally left to bail here rather than read a wrong `undefined`. ----
            Err(_) if prop == "length" => unsupported!(
                "`.length` on a receiver of unproven shape (param/return/reassigned) \
                 — dynamic-length is a separate path, not a property slot"
            ),
            Err(_) => self.lower_dynamic_get_expr(module, object, prop),
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
        // ---- DYNAMIC computed object key `obj[k]` (P5.5): a STRING-keyed index on
        // a proven keyed OBJECT (known OR unknown shape). ToString the key and
        // resolve at runtime via `__rtsadp_obj_get`. A NUMERIC index on an object
        // bails (number-index-into-object is out of scope). ----
        if let Some(name) = self.object_receiver_name(object) {
            if let Some(key_word) = self.dynamic_key_word(module, index)? {
                let obj_word = self.load_local_word(&name);
                let word = self
                    .call_runtime(module, "__rtsadp_obj_get", &[obj_word, key_word])?
                    .expect("__rtsadp_obj_get returns a value");
                return Ok(Val::new(word, Repr::Tagged));
            }
        }
        // ---- native STRING receiver fast-path (CHANGE 3): `s[i]` on a proven
        // string yields a 1-char string. Reuses the existing `__rtsadp_dyn_char_at`
        // op (PolyValue-native: takes the string word + a boxed index word, returns
        // a 1-char string word) — the same primitive `s.charAt(i)` resolves to. ----
        if self.receiver_is_proven_string(object) {
            let recv = self.lower_string_receiver(module, object)?;
            let recv_word = self.box_value(recv);
            let idx_val = self.lower_expr(module, index)?;
            let idx_word = self.box_value(idx_val);
            let word = self
                .call_runtime(module, "__rtsadp_dyn_char_at", &[recv_word, idx_word])?
                .expect("__rtsadp_dyn_char_at returns a value");
            return Ok(Val::tagged_kind(word, JsKind::Str));
        }
        if let Ok((recv_word, HeapShape::Array)) = self.resolve_heap_receiver(module, object) {
            // A PROVEN array with a numeric index → fast-path `VEC_GET`. A non-numeric
            // index on a proven array falls through to the dynamic path below.
            if let Ok(idx) = self.lower_index_value(module, index) {
                let word = emit_marshal::emit_vec_get(module, self.builder, recv_word, idx);
                return Ok(Val::new(word, Repr::Tagged));
            }
        }
        // ---- DYNAMIC FALLBACK: an unproven receiver, or a proven object with a
        // computed key. Coerce the key ToString (JS: `o[0]` keys on "0") and read at
        // runtime via `__rtsadp_obj_get` (absent / non-object → `undefined`). ----
        if self.receiver_is_unbound_this(object) {
            return unsupported!("indexed read on a captured `this` (arrow at top level — #195)");
        }
        // Computed index on an UNPROVEN receiver → the generic `__rtsadp_idx_get`,
        // which detects at RUNTIME: array (numeric element) / string (char) / object
        // (ToString-keyed `obj_get`). This handles a NUMERIC index (`p[0]` on a
        // nested array), a Tagged numeric index (`this.#a[this.#i]` where the index
        // is a `number` FIELD lowered as Tagged → the array branch coerces it), AND a
        // string/dynamic key (object branch — `o[k]`). One path, no static index-kind
        // proof needed.
        let recv = self.lower_expr(module, object)?;
        let recv_word = self.box_value(recv);
        let idx_val = self.lower_expr(module, index)?;
        let idx_word = self.box_value(idx_val);
        let word = self
            .call_runtime(module, "__rtsadp_idx_get", &[recv_word, idx_word])?
            .expect("__rtsadp_idx_get returns a value");
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
        // ---- `globalThis.prop = v` write: the singleton global object's dynamic
        // property. `__rtsadp_obj_set` writes an existing slot or appends a new key
        // (shape transition), so a brand-new global property works. ----
        if self.is_globalthis_receiver(object) {
            return self.lower_dynamic_set_expr(module, object, prop, value);
        }
        // ---- a bare class NAME target (`C.n = 5`) is a STATIC FIELD WRITE, a later
        // increment. Bail BEFORE the function-value-property path: `C` now reifies
        // to a `TAG_FUNCTION` class-value, so without this guard the write would be
        // silently recorded as a data property on the class-value instead. ----
        if self.class_name_receiver(object).is_some() {
            return unsupported!(
                "static field write `.{prop}` (read-only static getter synthesis — a later increment)"
            );
        }
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
        // ---- DATA PROPERTY on a FUNCTION value `F.foo = v` (Phase 4): record the
        // value in the function-property side-table at RUNTIME via
        // `__rtsadp_fn_set_prop` (returns the assigned value). Routed BEFORE the
        // shape paths — a function receiver has no static object shape. ----
        if let Some(fn_word) = self.fn_value_word(module, object)? {
            let key_word = self.intern_key_word(prop);
            let v = self.lower_expr(module, value)?;
            let word = self.box_value(v);
            let ret = self
                .call_runtime(module, "__rtsadp_fn_set_prop", &[fn_word, key_word, word])?
                .expect("__rtsadp_fn_set_prop returns a value");
            return Ok(Val::new(ret, Repr::Tagged));
        }
        // ---- DYNAMIC object property write (P5.5): a proven keyed OBJECT of
        // unknown shape. `__rtsadp_obj_set` writes an EXISTING key (a key not in the
        // shape is a runtime no-op; adding a brand-new key is the transition tree,
        // still a compile-time bail on a KNOWN shape — an unknown-shape local cannot
        // prove the key is new, so we route it and the trampoline writes iff present).
        if let HirExprKind::Ident(name) = &object.kind {
            if self.object_locals.contains(name) {
                return self.lower_dynamic_set(module, name, prop, value);
            }
        }
        if let Ok((recv_word, HeapShape::Object(shape_id))) =
            self.resolve_heap_receiver(module, object)
        {
            // A PROVEN-shape object: a key already in the shape writes its constant
            // slot directly. A provably-NEW key still bails (the transition tree is a
            // later increment) — the dynamic fallback below cannot add a key either.
            if let Some(slot) = self.shapes.slot_of(shape_id, prop) {
                let v = self.lower_expr(module, value)?;
                let word = self.box_value(v);
                // Property values live at slot 1 + slot_index (slot 0 is the shape-id).
                let idx = self.builder.ins().iconst(types::I64, 1 + slot as i64);
                emit_marshal::emit_vec_set(module, self.builder, recv_word, idx, word);
                return Ok(Val::new(word, Repr::Tagged));
            }
            // A brand-NEW key on a known-shape object: write it at RUNTIME (the
            // `__rtsadp_obj_set` shape transition appends the key + value) and DEMOTE
            // the local to dynamic-shape so subsequent reads see the grown object. A
            // non-ident receiver (no local to demote) still routes the dynamic set.
            let v = self.lower_expr(module, value)?;
            let word = self.box_value(v);
            let key_word = self.intern_key_word(prop);
            self.call_runtime(module, "__rtsadp_obj_set", &[recv_word, key_word, word])?;
            if let HirExprKind::Ident(name) = &object.kind {
                self.demote_local_to_dynamic(name);
            }
            return Ok(Val::new(word, Repr::Tagged));
        }
        // ---- DYNAMIC FALLBACK: an unproven receiver (param / return / reassigned /
        // non-identifier object). Write the EXISTING property by key AT RUNTIME via
        // `__rtsadp_obj_set` (a key absent from the runtime shape, or a non-object
        // receiver, is a no-op returning the value — adding a brand-new key is the
        // transition tree, still out of scope). ----
        self.lower_dynamic_set_expr(module, object, prop, value)
    }

    /// Lower `arr[index] = value` (index-write) to `VEC_SET`.
    pub(super) fn lower_index_assign(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        index: &HirExpr,
        value: &HirExpr,
    ) -> FrontResult<Val> {
        // ---- DYNAMIC computed object key write `obj[k] = v` (P5.5): a STRING key
        // on a proven keyed OBJECT. ToString the key, box the value, write the
        // existing slot at runtime via `__rtsadp_obj_set`. A NUMERIC index on an
        // object bails. ----
        if let Some(name) = self.object_receiver_name(object) {
            if let Some(key_word) = self.dynamic_key_word(module, index)? {
                let v = self.lower_expr(module, value)?;
                let word = self.box_value(v);
                let obj_word = self.load_local_word(&name);
                self.call_runtime(module, "__rtsadp_obj_set", &[obj_word, key_word, word])?;
                // The runtime set may ADD a key (shape transition), so the local's
                // compile-time shape is no longer authoritative — route its future
                // reads through the DYNAMIC path (which reads the live runtime shape).
                self.demote_local_to_dynamic(&name);
                return Ok(Val::new(word, Repr::Tagged));
            }
        }
        if let Ok((recv_word, HeapShape::Array)) = self.resolve_heap_receiver(module, object) {
            // A PROVEN array with a numeric index → fast-path `VEC_SET`.
            if let Ok(idx) = self.lower_index_value(module, index) {
                let v = self.lower_expr(module, value)?;
                let word = self.box_value(v);
                emit_marshal::emit_vec_set(module, self.builder, recv_word, idx, word);
                return Ok(Val::new(word, Repr::Tagged));
            }
        }
        // ---- DYNAMIC FALLBACK: an unproven receiver, or a proven object with a
        // computed key. Coerce the key ToString and write the EXISTING slot at runtime
        // via `__rtsadp_obj_set` (absent key / non-object → no-op). ----
        if self.receiver_is_unbound_this(object) {
            return unsupported!("indexed write on a captured `this` (arrow at top level — #195)");
        }
        let recv = self.lower_expr(module, object)?;
        let recv_word = self.box_value(recv);
        let key_word = self.index_key_word(module, index)?;
        let v = self.lower_expr(module, value)?;
        let word = self.box_value(v);
        self.call_runtime(module, "__rtsadp_obj_set", &[recv_word, key_word, word])?;
        Ok(Val::new(word, Repr::Tagged))
    }

    // ---- native-string receivers (member / index fast-path) ----

    /// Whether `object` is PROVEN to evaluate to a native `string` value, decidable
    /// statically (no IR emitted). Covers the canonical native-string receivers:
    /// - a string LITERAL (`"abc"`);
    /// - a `Member { inner, prop }` where `inner`'s class proves `prop` is a
    ///   string-typed field (the `this.#value` wrap pattern — CHANGE 2);
    /// - an identifier bound to a string-typed param/local (`string_locals`).
    ///
    /// A `true` here lets `lower_member`/`lower_index` take the native string
    /// fast-path; everything else falls through to the heap-shape path UNCHANGED.
    fn receiver_is_proven_string(&self, object: &HirExpr) -> bool {
        match &object.kind {
            HirExprKind::Lit(rts_hir::ir::HirLit::Str(_)) => true,
            HirExprKind::Ident(name) => self.string_locals.contains(name),
            HirExprKind::Member {
                object: inner,
                prop,
            } => self
                .static_instance_class(inner)
                .and_then(|c| self.classes.get(&c).map(|d| d.field_is_string(prop)))
                .unwrap_or(false),
            _ => false,
        }
    }

    /// Lower a PROVEN-string receiver to a PolyValue `Val` tagged `JsKind::Str`.
    /// A string LITERAL / param lowers directly (already `Str`-kinded); a string
    /// FIELD (`this.#value`) routes through [`Self::lower_member`]'s CHANGE-2 branch
    /// (a `VEC_GET` of the slot re-tagged `Str`). Re-tags `Str` defensively so the
    /// caller's fast-path always sees a string `Val`.
    fn lower_string_receiver(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
    ) -> FrontResult<Val> {
        let v = self.lower_expr(module, object)?;
        Ok(Val::tagged_kind(self.box_value(v), JsKind::Str))
    }

    // ---- helpers ----

    /// The statically-known CLASS of an instance-valued `object` (a `new C()` or a
    /// local recorded in `local_classes`), for accessor resolution. A plain
    /// object literal (no class) yields `None`.
    fn instance_class_of(&self, object: &HirExpr) -> Option<String> {
        self.static_instance_class(object)
    }

    /// Resolve a property/index/method receiver to its heap WORD + proven shape.
    /// - `Ident(name)` with a proven shape → `(load_local_word(name), shape)`;
    /// - `Member { object: inner, prop }` where `inner`'s class proves `prop` is an
    ///   Array field → `(VEC_GET(inner_word, 1 + slot), HeapShape::Array)`.
    /// Anything else (unknown shape) BAILS — never a guess.
    fn resolve_heap_receiver(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
    ) -> FrontResult<(Value, HeapShape)> {
        match &object.kind {
            HirExprKind::Ident(name) => {
                let Some(&shape) = self.local_shapes.get(name) else {
                    return unsupported!(
                        "property/index access on `{name}` whose shape is not statically \
                         proven (param/return/reassigned — a later increment)"
                    );
                };
                Ok((self.load_local_word(name), shape))
            }
            HirExprKind::Member {
                object: inner,
                prop,
            } => {
                // Only a field PROVEN (via its class) to hold an Array is chainable
                // in this increment; everything else bails (the honesty floor).
                let Some(class) = self.static_instance_class(inner) else {
                    return unsupported!(
                        "chained access on `.{prop}` of a receiver with no statically-known class"
                    );
                };
                let is_array = self
                    .classes
                    .get(&class)
                    .map(|d| d.field_is_array(prop))
                    .unwrap_or(false);
                if !is_array {
                    return unsupported!(
                        "chained access on non-array field `.{prop}` (only array-typed \
                         fields are chainable in this increment)"
                    );
                }
                let (inner_word, inner_shape) = self.resolve_heap_receiver(module, inner)?;
                let HeapShape::Object(shape_id) = inner_shape else {
                    return unsupported!("field `.{prop}` on a non-object receiver");
                };
                let Some(slot) = self.shapes.slot_of(shape_id, prop) else {
                    return unsupported!("field `.{prop}` not in the receiver's shape");
                };
                let idx = self.builder.ins().iconst(types::I64, 1 + slot as i64);
                let word = emit_marshal::emit_vec_get(module, self.builder, inner_word, idx);
                Ok((word, HeapShape::Array))
            }
            // A statically-classed receiver EXPRESSION that is not a bare ident
            // (e.g. `new TypeError("x")`): lower it to its instance word and intern
            // the class field shape, so a field read (`new C(..).f`) resolves a
            // constant slot just like a bare-ident local of known class.
            _ if self.static_instance_class(object).is_some() => {
                let class = self
                    .static_instance_class(object)
                    .expect("guard proved Some");
                let desc = self
                    .classes
                    .get(&class)
                    .expect("static_instance_class implies a collected class")
                    .clone();
                let recv = self.lower_expr(module, object)?;
                let recv_word = self.box_value(recv);
                let shape_id = self.shapes.intern(&desc.fields);
                Ok((recv_word, HeapShape::Object(shape_id)))
            }
            _ => unsupported!("property/index access on a non-identifier object (unknown shape)"),
        }
    }

    /// If `object` resolves to a FUNCTION VALUE (a top-level/hoisted user function
    /// referenced by name, NOT shadowed by a local), produce a word identifying it
    /// for the function-property side-table; otherwise `None` (the caller falls
    /// through to the object/array/string paths UNCHANGED).
    ///
    /// Two reach paths, both keyed by the SAME thunk identity in the runtime
    /// (`funcops::fn_identity`):
    /// - a reifiable function (no synthesized `this`) → a real `TAG_FUNCTION`
    ///   PolyValue word via [`Self::reify_function`];
    /// - a `this`-using fn-ctor (`const F = function(this){…}`), which cannot reify
    ///   as a value → its raw THUNK-address word via [`Self::fn_ctor_identity`]
    ///   (the runtime treats a non-function word as the identity directly).
    ///
    /// A name shadowed by a local is NOT a function-value receiver (the local wins),
    /// so the existing object/string local paths handle it.
    pub(super) fn fn_value_word(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
    ) -> FrontResult<Option<Value>> {
        let HirExprKind::Ident(name) = &object.kind else {
            return Ok(None);
        };
        // A local of this name shadows the function — not a function-value receiver.
        if self.local(name).is_some() || !self.sigs.contains_key(name) {
            return Ok(None);
        }
        let name = name.clone();
        // A `this`-using fn-ctor cannot reify (its thunk fills param[0] from a0); use
        // its raw thunk identity word instead. Reifiable functions get a real value.
        if self.is_fn_ctor(&name) {
            return Ok(Some(self.fn_ctor_identity(module, &name)?));
        }
        let v = self.reify_function(module, &name)?;
        Ok(Some(self.box_value(v)))
    }

    /// Load the raw object/array PolyValue word held by a local (the local's repr
    /// is `Tagged`, an i64 register).
    /// Demote a local from STATIC object shape (`local_shapes`) to DYNAMIC shape
    /// (`object_locals`): after a runtime key-add transition its compile-time shape
    /// is stale, so its reads/writes must consult the live runtime shape-id.
    fn demote_local_to_dynamic(&mut self, name: &str) {
        if self.local_shapes.remove(name).is_some() {
            self.object_locals.insert(name.to_string());
        }
    }

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

    // ---- DYNAMIC property access (P5.5) ----

    /// Whether `object` is a `this` reference that has reached the DYNAMIC fallback.
    /// A real method/ctor `this` is always shape-proven (`local_shapes`/`local_classes`
    /// carry its class — see [`Lowerer::lower_function`]) and takes the constant-slot
    /// path, never reaching here; so a `this` arriving at the dynamic fallback is a
    /// CAPTURED arrow `this` (a top-level/free arrow synthesizing `this` to a0 —
    /// env-record, #195). Its value is not an honest receiver, so the dynamic property
    /// fallback declines it (bails) rather than read a property off a bogus word.
    fn receiver_is_unbound_this(&self, object: &HirExpr) -> bool {
        matches!(&object.kind, HirExprKind::Ident(name)
            if name == crate::front::run::class::THIS)
    }

    /// The local NAME of a receiver PROVEN to be a keyed OBJECT (a known-shape
    /// object literal local OR a reassigned-object `object_local`), for the dynamic
    /// computed-key path. `None` for an array / opaque / non-identifier receiver —
    /// the caller then falls to the array/static path or bails.
    fn object_receiver_name(&self, object: &HirExpr) -> Option<String> {
        let HirExprKind::Ident(name) = &object.kind else {
            return None;
        };
        let is_object = self.object_locals.contains(name)
            || matches!(self.local_shapes.get(name), Some(HeapShape::Object(_)));
        is_object.then(|| name.clone())
    }

    /// Lower `obj.prop` on an unknown-shape object local to `__rtsadp_obj_get`:
    /// intern `prop` as a string PolyValue key, then resolve at runtime.
    fn lower_dynamic_get(
        &mut self,
        module: &mut dyn Module,
        name: &str,
        prop: &str,
    ) -> FrontResult<Val> {
        let key_word = self.intern_key_word(prop);
        let obj_word = self.load_local_word(name);
        let word = self
            .call_runtime(module, "__rtsadp_obj_get", &[obj_word, key_word])?
            .expect("__rtsadp_obj_get returns a value");
        Ok(Val::new(word, Repr::Tagged))
    }

    /// Lower `obj.prop = value` on an unknown-shape object local to
    /// `__rtsadp_obj_set` (writes the slot iff `prop` is present; a new key is a
    /// runtime no-op — see the trampoline's soundness notes).
    fn lower_dynamic_set(
        &mut self,
        module: &mut dyn Module,
        name: &str,
        prop: &str,
        value: &HirExpr,
    ) -> FrontResult<Val> {
        let key_word = self.intern_key_word(prop);
        let v = self.lower_expr(module, value)?;
        let word = self.box_value(v);
        let obj_word = self.load_local_word(name);
        self.call_runtime(module, "__rtsadp_obj_set", &[obj_word, key_word, word])?;
        Ok(Val::new(word, Repr::Tagged))
    }

    /// Generalized DYNAMIC READ `recv.prop` for ANY receiver EXPRESSION whose shape
    /// is not statically proven (a param, a call return, a non-identifier object).
    /// Lower the receiver to a Val, box it to a PolyValue word, and read the property
    /// by interned static key AT RUNTIME via `__rtsadp_obj_get` — which returns the
    /// stored slot for a keyed object and `undefined` for an absent key OR a
    /// non-object receiver (the trampoline never faults; see `value/objops.rs`).
    /// The result is a Tagged `Val` of kind `Unknown`.
    /// Whether `object` is the bare `globalThis` language global (not shadowed by a
    /// local/param or a module-level cell of the same name). Its `.prop` get/set
    /// routes to the dynamic-object trampolines on the singleton object word.
    pub(super) fn is_globalthis_receiver(&self, object: &HirExpr) -> bool {
        matches!(&object.kind, HirExprKind::Ident(n)
            if n == "globalThis" && self.local(n).is_none() && self.gcell_id(n).is_none())
    }

    fn lower_dynamic_get_expr(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        prop: &str,
    ) -> FrontResult<Val> {
        if self.receiver_is_unbound_this(object) {
            return unsupported!(
                "property access on a captured `this` (arrow at top level — env-record, #195)"
            );
        }
        let recv = self.lower_expr(module, object)?;
        let recv_word = self.box_value(recv);
        let key_word = self.intern_key_word(prop);
        let word = self
            .call_runtime(module, "__rtsadp_obj_get", &[recv_word, key_word])?
            .expect("__rtsadp_obj_get returns a value");
        Ok(Val::new_with_kind(word, Repr::Tagged, JsKind::Unknown))
    }

    /// Generalized DYNAMIC WRITE `recv.prop = value` for ANY receiver EXPRESSION of
    /// unproven shape. Box the receiver + the value, intern the static key, and write
    /// the EXISTING slot at runtime via `__rtsadp_obj_set` (an absent key or a
    /// non-object receiver is a no-op returning the value — adding a brand-new key is
    /// the transition tree, still out of scope). Returns the assigned value.
    fn lower_dynamic_set_expr(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        prop: &str,
        value: &HirExpr,
    ) -> FrontResult<Val> {
        if self.receiver_is_unbound_this(object) {
            return unsupported!(
                "property write on a captured `this` (arrow at top level — env-record, #195)"
            );
        }
        let recv = self.lower_expr(module, object)?;
        let recv_word = self.box_value(recv);
        let key_word = self.intern_key_word(prop);
        let v = self.lower_expr(module, value)?;
        let word = self.box_value(v);
        self.call_runtime(module, "__rtsadp_obj_set", &[recv_word, key_word, word])?;
        Ok(Val::new(word, Repr::Tagged))
    }

    /// Lower a COMPUTED index expression `obj[index]` to a key PolyValue word for the
    /// dynamic object path. Any index (string, number, bool, Tagged) is boxed and
    /// passed through; the `__rtsadp_obj_get`/`_set` trampoline's `key_text` reads a
    /// string directly or ToStrings a non-string (JS: `o[0]` keys on "0"), so a
    /// numeric index is coerced identically to JS property-key stringification.
    fn index_key_word(&mut self, module: &mut dyn Module, index: &HirExpr) -> FrontResult<Value> {
        let v = self.lower_expr(module, index)?;
        Ok(self.box_value(v))
    }

    /// Intern a STATIC property name as a string-PolyValue key word (an i64
    /// constant — the literal is interned in the real pool at lowering time, like
    /// every string literal).
    fn intern_key_word(&mut self, prop: &str) -> Value {
        let pv = abi_adapter::intern_poly(prop);
        self.builder.ins().iconst(types::I64, pv.raw() as i64)
    }

    /// Lower a COMPUTED index expression to a string-PolyValue key WORD for the
    /// dynamic object path, or `None` when the index is a proven NUMBER (a numeric
    /// index into an object is out of scope — the caller bails). A string-kinded /
    /// Tagged index is ToString'd via the box + the lowering's key coercion: a
    /// proven-string value passes its word straight through; a Tagged index is
    /// boxed and ToString happens inside the trampoline's `key_text`.
    fn dynamic_key_word(
        &mut self,
        module: &mut dyn Module,
        index: &HirExpr,
    ) -> FrontResult<Option<Value>> {
        let v = self.lower_expr(module, index)?;
        match v.repr {
            // A numeric index is NOT an object key in this increment — bail (the
            // caller's array path handles a numeric index on an array).
            Repr::Int32 | Repr::Int64 | Repr::Float64 => Ok(None),
            // A string / Tagged index: box to a PolyValue word; the trampoline's
            // `key_text` reads a string directly or ToStrings a non-string.
            _ => Ok(Some(self.box_value(v))),
        }
    }
}

/// Split off a leading `__rtsl_class__` marker field (P5.15), returning the
/// recovered literal-class name (if present) and the REAL field list (the marker
/// removed). The marker is always the first field and its value is a string literal
/// (the object-literal method recovery prepended it); a literal without methods has
/// no marker and the fields pass through unchanged.
fn strip_lit_class_marker(fields: &[(String, HirExpr)]) -> (Option<String>, &[(String, HirExpr)]) {
    if let Some((key, value)) = fields.first() {
        if key == crate::front::run::desugar::LIT_CLASS_MARKER {
            if let HirExprKind::Lit(rts_hir::ir::HirLit::Str(name)) = &value.kind {
                return (Some(name.clone()), &fields[1..]);
            }
        }
    }
    (None, fields)
}
