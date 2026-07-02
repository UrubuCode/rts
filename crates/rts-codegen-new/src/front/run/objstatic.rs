//! `Object.*` static methods — P5.4.
//!
//! `Object.keys`/`values`/`entries`/`assign`/`freeze`/`fromEntries`/
//! `getOwnPropertyNames` over a receiver whose OBJECT SHAPE is statically proven
//! (an object literal, or a local recorded `HeapShape::Object(shape_id)`). The
//! engine already has the ordered key list at compile time (the shape), so these
//! are CODEGEN-NATIVE: the lowering builds a fresh `Entry::Vec` (a `TAG_OBJECT`
//! array word) of boxed string/value/sub-array words directly, reusing the SAME
//! VEC + string-pool emit helpers the object/array literals use.
//!
//! A DYNAMIC / unknown-shape receiver (a param, a call return, a reassigned
//! local) BAILS — the engine refuses to guess an object's layout (the honesty
//! floor), exactly like a property access on it.

use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;

use rts_hir::HirExpr;
use rts_hir::ir::HirExprKind;

use crate::repr::Repr;
use crate::value::{abi_adapter, emit_marshal};

use crate::front::error::{FrontResult, unsupported};

use super::lower::{HeapShape, JsKind, Lowerer, Val};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Try to lower an `Object.m(args)` static call. Returns `Ok(None)` when
    /// `object` is not the bare `Object` global (so the caller falls through), or
    /// an explicit bail for an unsupported/dynamic form.
    pub(super) fn try_object_static_call(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Option<Val>> {
        let HirExprKind::Ident(name) = &object.kind else {
            return Ok(None);
        };
        if name != "Object" {
            return Ok(None);
        }
        // A local named `Object` shadows the global. The ambient prelude
        // `class Object` (object.ts) does NOT: it carries INSTANCE methods only, so
        // the STATIC surface (`Object.keys`/…) stays on this path (mirrors the
        // wrapper-primordial-static carve-out for Number/String).
        if self.local(name).is_some() || (self.classes.get(name).is_some() && name != "Object") {
            return Ok(None);
        }
        // DYNAMIC receiver (a param / call-return / reassigned local whose object
        // shape is not statically proven): route keys/values/entries through the
        // RUNTIME trampolines (read slot-0 shape-id at run time). `Object` is a
        // PRIMORDIAL, so this generic enumeration is engine-direct.
        if matches!(method, "keys" | "getOwnPropertyNames" | "values" | "entries")
            && !self.is_static_object_arg(args)
        {
            let sym = match method {
                "values" => "__rtsadp_obj_values",
                "entries" => "__rtsadp_obj_entries",
                // `getOwnPropertyNames` adds an array's non-enumerable `"length"`.
                "getOwnPropertyNames" => "__rtsadp_obj_own_names",
                _ => "__rtsadp_obj_keys",
            };
            return self.dynamic_obj_enum(module, args, sym).map(Some);
        }
        // `Object.getOwnPropertySymbols(o)` — always the runtime walk (symbol
        // entries live under canonical `@@sym:<handle>` keys, #798).
        if method == "getOwnPropertySymbols" {
            return self
                .dynamic_obj_enum(module, args, "__rtsadp_obj_own_symbols")
                .map(Some);
        }
        match method {
            "keys" | "getOwnPropertyNames" => self.object_keys(module, args).map(Some),
            "values" => self.object_values(module, args).map(Some),
            "entries" => self.object_entries(module, args).map(Some),
            "assign" => self.object_assign(module, args).map(Some),
            "freeze" | "seal" | "preventExtensions" => self
                .object_freeze(module, args, method == "freeze")
                .map(Some),
            "isExtensible" => self.object_is_extensible(module, args).map(Some),
            "isFrozen" | "isSealed" => {
                if args.len() != 1 {
                    return unsupported!("Object.{method} expects 1 arg, got {}", args.len());
                }
                let sym = if method == "isFrozen" {
                    "__rtsadp_is_frozen"
                } else {
                    "__rtsadp_is_sealed"
                };
                let v = self.lower_expr(module, &args[0])?;
                let word = self.box_value(v);
                let res = self
                    .call_runtime(module, sym, &[word])?
                    .expect("isFrozen/isSealed returns a flag");
                Ok(Some(Val::new(res, Repr::Bool)))
            }
            "defineProperty" => self.object_define_property(module, args).map(Some),
            "getOwnPropertyDescriptor" => {
                if args.len() != 2 {
                    return unsupported!("Object.getOwnPropertyDescriptor expects 2 args");
                }
                let o = self.lower_expr(module, &args[0])?;
                let o_word = self.box_value(o);
                let k = self.lower_expr(module, &args[1])?;
                let k_word = self.box_value(k);
                let res = self
                    .call_runtime(
                        module,
                        "__rtsadp_obj_get_own_property_descriptor",
                        &[o_word, k_word],
                    )?
                    .expect("descriptor returns a word");
                Ok(Some(Val::new(res, Repr::Tagged)))
            }
            "getOwnPropertyDescriptors" => {
                let o = self.lower_expr(module, &args[0])?;
                let o_word = self.box_value(o);
                let res = self
                    .call_runtime(module, "__rtsadp_obj_get_own_property_descriptors", &[o_word])?
                    .expect("descriptors returns a word");
                Ok(Some(Val::tagged_kind(res, JsKind::Object)))
            }
            "is" => self.object_is(module, args).map(Some),
            "hasOwn" => self.object_has_own(module, args).map(Some),
            "fromEntries" => self.object_from_entries(module, args).map(Some),
            "create" => self.object_create(module, args).map(Some),
            "getPrototypeOf" => self
                .object_proto_unary(module, args, "__rtsadp_obj_proto_of")
                .map(Some),
            "setPrototypeOf" => self.object_set_proto(module, args).map(Some),
            other => unsupported!("Object.{other}(...) static method (later increment)"),
        }
    }

    /// Whether the single receiver arg has a STATICALLY-PROVEN object shape (an
    /// inline object literal, or an ident recorded `HeapShape::Object`). Only then
    /// can the compile-time-keys fast path run; otherwise the dynamic runtime
    /// trampoline is used.
    fn is_static_object_arg(&self, args: &[HirExpr]) -> bool {
        // A shape carrying an ENGINE-OWNED key demotes to the RUNTIME enumeration:
        // `__prim` (a String/Number wrapper box — its own names are the code-unit
        // indices + `length`, not the slot) and `@@sym:*` (a Symbol-keyed entry —
        // filtered from string enumeration). The static path would leak them.
        let needs_runtime =
            |keys: &[String]| keys.iter().any(|k| k == "__prim" || k.starts_with("@@sym:"));
        match args.first().map(|a| &a.kind) {
            Some(HirExprKind::Object(fields)) => {
                !needs_runtime(&fields.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>())
            }
            Some(HirExprKind::Ident(n)) => match self.local_shapes.get(n) {
                Some(HeapShape::Object(shape_id)) => {
                    !needs_runtime(&self.shapes.get(*shape_id).keys)
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// Dynamic `Object.keys/values/entries(x)`: lower `x` to an object word and call
    /// the runtime enumeration trampoline (`sym`), yielding a Tagged array.
    fn dynamic_obj_enum(
        &mut self,
        module: &mut dyn Module,
        args: &[HirExpr],
        sym: &str,
    ) -> FrontResult<Val> {
        if args.len() != 1 {
            return unsupported!("Object.{sym} expects 1 receiver arg, got {}", args.len());
        }
        let v = self.lower_expr(module, &args[0])?;
        let word = self.box_value(v);
        let arr = self
            .call_runtime(module, sym, &[word])?
            .expect("obj enumeration returns an array word");
        Ok(Val::tagged_kind(arr, JsKind::Array))
    }

    /// `Object.create(proto)` → a fresh bare object whose `[[Prototype]]` is `proto`
    /// (recorded in the runtime proto side-table; a null/non-object `proto` ⇒ a null
    /// prototype). A missing arg defaults to `undefined`. The result is a Tagged
    /// OBJECT — `result.key` reads route through the dynamic `__rtsadp_obj_get`,
    /// which walks the prototype chain on an own-slot miss.
    fn object_create(&mut self, module: &mut dyn Module, args: &[HirExpr]) -> FrontResult<Val> {
        let proto_word = match args.first() {
            Some(a) => {
                let v = self.lower_expr(module, a)?;
                self.box_value(v)
            }
            None => self
                .builder
                .ins()
                .iconst(types::I64, crate::value::PolyValue::undefined().raw() as i64),
        };
        let word = self
            .call_runtime(module, "__rtsadp_obj_create", &[proto_word])?
            .expect("__rtsadp_obj_create returns an object word");
        Ok(Val::tagged_kind(word, JsKind::Object))
    }

    /// `Object.setPrototypeOf(obj, proto)` → record `obj`'s prototype (a null/non-
    /// object `proto` clears it), returning `obj`. Both args boxed to words.
    fn object_set_proto(&mut self, module: &mut dyn Module, args: &[HirExpr]) -> FrontResult<Val> {
        if args.len() != 2 {
            return unsupported!("Object.setPrototypeOf expects 2 args, got {}", args.len());
        }
        let o = self.lower_expr(module, &args[0])?;
        let o_word = self.box_value(o);
        let p = self.lower_expr(module, &args[1])?;
        let p_word = self.box_value(p);
        let res = self
            .call_runtime(module, "__rtsadp_obj_set_proto", &[o_word, p_word])?
            .expect("__rtsadp_obj_set_proto returns the object word");
        Ok(Val::tagged_kind(res, JsKind::Object))
    }

    /// A unary `Object.<m>(obj)` over a raw object word → a Tagged result (e.g.
    /// `getPrototypeOf` → the prototype word or `null`).
    fn object_proto_unary(
        &mut self,
        module: &mut dyn Module,
        args: &[HirExpr],
        sym: &str,
    ) -> FrontResult<Val> {
        if args.len() != 1 {
            return unsupported!("Object.{sym} expects 1 arg, got {}", args.len());
        }
        let v = self.lower_expr(module, &args[0])?;
        let word = self.box_value(v);
        let res = self
            .call_runtime(module, sym, &[word])?
            .expect("object proto unary returns a value");
        Ok(Val::new(res, Repr::Tagged))
    }

    /// Resolve the single receiver argument to `(obj_word, keys)`: either a
    /// statically-OBJECT-shaped LOCAL (`Object.keys(o)`) or an inline object
    /// LITERAL (`Object.keys({a:1})` / `Object.keys({})`). Anything else (an array,
    /// a dynamic/unknown-shape value) BAILS — the engine refuses to guess a layout.
    fn receiver(
        &mut self,
        module: &mut dyn Module,
        args: &[HirExpr],
    ) -> FrontResult<(Value, Vec<String>)> {
        if args.len() != 1 {
            return unsupported!("Object static expects 1 receiver arg, got {}", args.len());
        }
        match &args[0].kind {
            // Inline object literal: lower it (records the global shape-id in slot 0)
            // and read its keys from the just-interned compile-time shape.
            HirExprKind::Object(fields) => {
                // Exclude the P5.15 `__rtsl_class__` marker (a synthetic field that is
                // never a real own-enumerable key) from the recovered key list.
                let keys: Vec<String> = fields
                    .iter()
                    .map(|(k, _)| k.clone())
                    .filter(|k| k != crate::front::run::desugar::LIT_CLASS_MARKER)
                    .collect();
                let (val, _shape_id, _lit_class) = self.lower_object_literal(module, fields)?;
                let word = self.box_value(val);
                Ok((word, keys))
            }
            HirExprKind::Ident(name) => match self.local_shapes.get(name) {
                Some(HeapShape::Object(shape_id)) => {
                    let keys = self.shapes.get(*shape_id).keys.clone();
                    let word = self.object_word(name);
                    Ok((word, keys))
                }
                Some(HeapShape::Array) => unsupported!(
                    "Object static on an ARRAY receiver (array key/value enumeration is a later increment)"
                ),
                None => unsupported!(
                    "Object static on `{name}` whose shape is not statically proven (param/return/reassigned)"
                ),
            },
            _ => unsupported!(
                "Object static on a non-identifier, non-literal receiver (dynamic shape) is a later increment"
            ),
        }
    }

    /// Load the raw object PolyValue word held by a shaped local.
    fn object_word(&mut self, name: &str) -> Value {
        let local = self.local(name).expect("shaped object local must exist");
        self.builder.use_var(local.var)
    }

    /// `Object.keys(obj)` / `Object.getOwnPropertyNames(obj)` → a fresh array of
    /// the object's keys as string PolyValue words (the compile-time shape order).
    fn object_keys(&mut self, module: &mut dyn Module, args: &[HirExpr]) -> FrontResult<Val> {
        let (_obj, keys) = self.receiver(module, args)?;
        let arr = emit_marshal::emit_new_vec_object(module, self.builder);
        for k in &keys {
            let pv = abi_adapter::intern_poly(k);
            let word = self.builder.ins().iconst(types::I64, pv.raw() as i64);
            emit_marshal::emit_vec_push(module, self.builder, arr, word);
        }
        Ok(Val::tagged_kind(arr, JsKind::Array))
    }

    /// `Object.values(obj)` → a fresh array of the object's slot VALUES (slots
    /// `1..` of the object Vec; slot 0 is the shape-id header). Each value word is
    /// copied verbatim (it is already a boxed PolyValue).
    fn object_values(&mut self, module: &mut dyn Module, args: &[HirExpr]) -> FrontResult<Val> {
        let (obj, keys) = self.receiver(module, args)?;
        let arr = emit_marshal::emit_new_vec_object(module, self.builder);
        for i in 0..keys.len() {
            // Property values live at slot 1 + slot_index (slot 0 = shape-id).
            let idx = self.builder.ins().iconst(types::I64, 1 + i as i64);
            let word = emit_marshal::emit_vec_get(module, self.builder, obj, idx);
            emit_marshal::emit_vec_push(module, self.builder, arr, word);
        }
        Ok(Val::tagged_kind(arr, JsKind::Array))
    }

    /// `Object.entries(obj)` → a fresh array of `[key, value]` 2-element sub-arrays.
    /// Each entry is its own `Entry::Vec` (a plain array word; nested-array
    /// indexing / `.join` on it works like any engine array).
    fn object_entries(&mut self, module: &mut dyn Module, args: &[HirExpr]) -> FrontResult<Val> {
        let (obj, keys) = self.receiver(module, args)?;
        let outer = emit_marshal::emit_new_vec_object(module, self.builder);
        for (i, k) in keys.iter().enumerate() {
            // value at slot 1+i.
            let idx = self.builder.ins().iconst(types::I64, 1 + i as i64);
            let value_word = emit_marshal::emit_vec_get(module, self.builder, obj, idx);
            // key as a string word.
            let pv = abi_adapter::intern_poly(k);
            let key_word = self.builder.ins().iconst(types::I64, pv.raw() as i64);
            // inner = [key, value].
            let inner = emit_marshal::emit_new_vec_object(module, self.builder);
            emit_marshal::emit_vec_push(module, self.builder, inner, key_word);
            emit_marshal::emit_vec_push(module, self.builder, inner, value_word);
            emit_marshal::emit_vec_push(module, self.builder, outer, inner);
        }
        Ok(Val::tagged_kind(outer, JsKind::Array))
    }

    /// `Object.assign(target, ...sources)` — supported for the common
    /// `Object.assign({}, src)` / 2-arg form where BOTH target and the single
    /// source have a statically-known object shape: copy each source slot value
    /// into the target Vec at the target's slot for that key. A key on the source
    /// not present in the target's shape BAILS (a transition-tree add is a later
    /// increment). Returns the target word.
    fn object_assign(&mut self, module: &mut dyn Module, args: &[HirExpr]) -> FrontResult<Val> {
        // `Object.assign(target, …sources)` — the DYNAMIC path for every form: lower
        // the target word, then chain `__rtsadp_obj_assign(t, sᵢ)` per source (runtime
        // key iteration + `obj_set`, which ADDS new keys via a shape transition, JS
        // last-write-wins). The target local is DEMOTED to dynamic access — added keys
        // live in runtime slots its compile-time shape doesn't know. Handles N
        // sources, key-add, and a non-statically-shaped target/source uniformly (the
        // old static fast path only did 2-arg, keys-already-present, shaped idents).
        if args.is_empty() {
            return unsupported!("Object.assign with no target");
        }
        if let HirExprKind::Ident(name) = &args[0].kind {
            self.demote_local_to_dynamic(name);
        }
        let tval = self.lower_expr(module, &args[0])?;
        let mut target = self.box_value(tval);
        for src in &args[1..] {
            let sval = self.lower_expr(module, src)?;
            let source = self.box_value(sval);
            target = self
                .call_runtime(module, "__rtsadp_obj_assign", &[target, source])?
                .expect("__rtsadp_obj_assign returns the target word");
        }
        Ok(Val::tagged_kind(target, JsKind::Object))
    }

    /// `Object.freeze(obj)` / `seal` / `preventExtensions` — a SEMANTIC no-op in
    /// this increment (the engine does not yet throw on a frozen mutation, #217-
    /// adjacent): return the receiver verbatim. A program that RELIES on a frozen
    /// mutation throwing is out of scope; the common `const x = Object.freeze({…})`
    /// idiom (freeze-then-read) works correctly.
    fn object_freeze(
        &mut self,
        module: &mut dyn Module,
        args: &[HirExpr],
        deep_freeze: bool,
    ) -> FrontResult<Val> {
        let (word, _keys) = self.receiver(module, args)?;
        // `freeze` marks the object non-extensible AND every current key
        // `writable:false` (an existing-key write becomes a silent no-op);
        // `seal`/`preventExtensions` only prevent extension (existing keys stay
        // writable). Returns the object (JS `Object.freeze(o)` returns `o`).
        let sym = if deep_freeze {
            "__rtsadp_freeze"
        } else {
            "__rtsadp_prevent_ext"
        };
        self.call_runtime(module, sym, &[word])?;
        Ok(Val::new(word, Repr::Tagged))
    }

    /// `Object.defineProperty(obj, key, descriptor)` — apply the DATA descriptor's
    /// `value` + flags to `obj` (adds the key via a shape transition). The target
    /// local is DEMOTED to dynamic access (the new key lives in a runtime slot its
    /// compile-time shape doesn't track). Returns the object.
    fn object_define_property(
        &mut self,
        module: &mut dyn Module,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        if args.len() != 3 {
            return unsupported!("Object.defineProperty expects 3 args, got {}", args.len());
        }
        if let HirExprKind::Ident(name) = &args[0].kind {
            self.demote_local_to_dynamic(name);
        }
        let o = self.lower_expr(module, &args[0])?;
        let o_word = self.box_value(o);
        let k = self.lower_expr(module, &args[1])?;
        let k_word = self.box_value(k);
        let d = self.lower_expr(module, &args[2])?;
        let d_word = self.box_value(d);
        let res = self
            .call_runtime(module, "__rtsadp_obj_define_property", &[o_word, k_word, d_word])?
            .expect("__rtsadp_obj_define_property returns the object word");
        Ok(Val::tagged_kind(res, JsKind::Object))
    }

    /// `Object.isExtensible(o)` — the REAL extensibility flag (`preventExtensions`/
    /// `freeze`/`seal` mark it). Returns a `Bool`.
    fn object_is_extensible(
        &mut self,
        module: &mut dyn Module,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        let (word, _keys) = self.receiver(module, args)?;
        let res = self
            .call_runtime(module, "__rtsadp_is_extensible", &[word])?
            .expect("__rtsadp_is_extensible returns a value");
        Ok(Val::new(res, Repr::Bool))
    }

    /// `Object.is(a, b)` — JS SameValue (`===` but NaN is NaN, +0 is not -0). Box
    /// both args and call the `__rtsadp_same_value` trampoline; result is a Bool.
    /// `Object.fromEntries(entries)` — build an object from an array of `[k, v]`
    /// pairs via `__rtsadp_obj_from_entries`. Requires a PROVEN-array source (a Map
    /// source needs iteration — a later increment — so it BAILS honestly rather than
    /// building an empty/wrong object).
    fn object_from_entries(
        &mut self,
        module: &mut dyn Module,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        if args.len() != 1 {
            return unsupported!("Object.fromEntries expects 1 arg, got {}", args.len());
        }
        // A proven array source: an array-valued expr OR an `Object.entries(...)`
        // call (whose result is always a freshly-built array of pairs — the common
        // round-trip `fromEntries(entries(o))`).
        if !self.is_array_valued(&args[0]) && !self.is_object_static_array_expr(&args[0]) {
            return unsupported!(
                "Object.fromEntries with a non-array source (Map/iterator is a later increment)"
            );
        }
        let v = self.lower_expr(module, &args[0])?;
        let entries = self.box_value(v);
        let res = self
            .call_runtime(module, "__rtsadp_obj_from_entries", &[entries])?
            .expect("__rtsadp_obj_from_entries returns an object word");
        Ok(Val::tagged_kind(res, super::lower::JsKind::Object))
    }

    /// `Object.hasOwn(obj, key)` — own-property membership (ES2022). Same runtime
    /// path as the `in` operator / `engine.obj_has`: box obj + key and call
    /// `__rtsadp_obj_has`, returning a Bool.
    fn object_has_own(&mut self, module: &mut dyn Module, args: &[HirExpr]) -> FrontResult<Val> {
        if args.len() != 2 {
            return unsupported!("Object.hasOwn expects 2 args, got {}", args.len());
        }
        let o = self.lower_expr(module, &args[0])?;
        let o_word = self.box_value(o);
        let k = self.lower_expr(module, &args[1])?;
        let k_word = self.box_value(k);
        let res = self
            .call_runtime(module, "__rtsadp_obj_has", &[o_word, k_word])?
            .expect("__rtsadp_obj_has returns a bool");
        Ok(Val::new(res, Repr::Bool))
    }

    fn object_is(&mut self, module: &mut dyn Module, args: &[HirExpr]) -> FrontResult<Val> {
        if args.len() != 2 {
            return unsupported!("Object.is expects 2 args, got {}", args.len());
        }
        let a = self.lower_expr(module, &args[0])?;
        let aw = self.box_value(a);
        let b = self.lower_expr(module, &args[1])?;
        let bw = self.box_value(b);
        let res = self
            .call_runtime(module, "__rtsadp_same_value", &[aw, bw])?
            .expect("__rtsadp_same_value returns a value");
        Ok(Val::tagged_kind(res, super::lower::JsKind::Bool))
    }

    /// Whether `e` is an `Object.keys/values/entries/getOwnPropertyNames(o)` call
    /// expression — its result is ALWAYS a freshly-built engine array (never a
    /// bail sentinel), so chaining `.length` on it is safe. Distinct from the
    /// general `is_array_receiver` (which also matches `Array.from`, whose result
    /// may be an unsupported-source sentinel that must bail at use).
    pub(super) fn is_object_static_array_expr(&self, e: &HirExpr) -> bool {
        let callee = match &e.kind {
            HirExprKind::MethodCall { object, method, .. } => Some((object, method)),
            HirExprKind::Call { callee, .. } => match &callee.kind {
                HirExprKind::Member { object, prop } => Some((object, prop)),
                _ => None,
            },
            _ => None,
        };
        match callee {
            Some((object, method)) => {
                matches!(
                    method.as_str(),
                    "keys" | "values" | "entries" | "getOwnPropertyNames"
                ) && matches!(&object.kind, HirExprKind::Ident(n)
                        if n == "Object" && self.local(n).is_none())
            }
            None => false,
        }
    }

}
