//! `new <RuntimeClass>(args)` + their instance methods + `instanceof` (P5.3).
//!
//! PRIMORDIAL-vs-Registry doctrine. `Date` dispatches through the REAL Registry
//! (Pilar 6 — see [`super::registryclass`] / [`super::registry`]): no row here.
//! `Map`/`Set` are now the faithful TS stdlib (embedded as an engine include in
//! [`super::registry`]); their ambient `class Map`/`class Set` shadow any native
//! dispatch, so there is NO Map/Set row here either. The remaining RUNTIME/
//! Registry class (`RegExp`) and the wrapper/error primordials (`Error`/
//! `Boolean`/`Number`/`String`) still resolve through the small [`class_meta`]
//! table → a `__rtsadp_*` trampoline that OWNS a representation the raw runtime
//! ABI can't express (the wrapper ctors' ToBoolean/ToNumber/ToString coercion;
//! RegExp array-result building). A constructed instance is a real runtime handle boxed
//! `TAG_OBJECT`; the local records its static class so a later `inst.method(args)`
//! / `inst instanceof C` dispatches at compile time. Anything unmodeled BAILS —
//! never a guess (the honesty floor).

use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;

use rts_hir::HirExpr;
use rts_hir::ir::HirExprKind;

use crate::value;

use crate::front::error::{FrontResult, unsupported};

use super::lower::{JsKind, Lowerer, Val};

/// How a runtime/Registry class's CONSTRUCTOR marshals its arguments. The wrapper
/// primordials (`Boolean`/`Number`/`String`) are NO LONGER here — they construct
/// via their `.ts` prelude class (user-class path). Only `RegExp` remains.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CtorKind {
    /// `new RegExp(pattern[, flags])` — one or two string args; compiles via the
    /// regex compile trampoline (P5.12).
    Regex,
}

/// One runtime/Registry class the engine can construct + dispatch on.
struct ClassMeta {
    kind: CtorKind,
    /// The instance-method rows: `(jsName, arity, methodSymbol)`. Each symbol is a
    /// PolyValue-in/out `__rtsadp_*` trampoline; slot 0 is the instance word.
    methods: &'static [(&'static str, usize, &'static str)],
}

/// Resolve a runtime/Registry class NAME to its [`ClassMeta`], or `None` when the
/// engine does not model it (so the caller bails / falls through).
///
/// NOTE: the `Error` family is NO LONGER here — it is a PRIMORDIAL `.ts` class
/// (`rts-primitives/src/error.ts`, included as an engine prelude). A user
/// `new Error("x")` constructs that shape-based class via the normal user-class
/// path; `.message`/`.name`/`.stack`/`toString()`/`instanceof` all ride the
/// user-class machinery. The former `err_meta` rows + `__rtsadp_err_*`
/// trampolines were deleted with this migration.
fn class_meta(name: &str) -> Option<ClassMeta> {
    let m = match name {
        "RegExp" => ClassMeta {
            // The ctor compiles via `__rtsadp_re_compile` (a dedicated emit path
            // that marshals the pattern + optional flags strings).
            kind: CtorKind::Regex,
            methods: REGEX_METHODS,
        },
        _ => return None,
    };
    Some(m)
}

/// RegExp instance methods (P5.12). `.test(s)` is the high-value one: receiver
/// word + subject word → a bool word, the SAME generic shape as Map/Set methods.
/// `.exec` BAILS (capture-group array extraction is a later increment — not a row
/// here, so it is rejected as "no such method").
const REGEX_METHODS: &[(&str, usize, &str)] = &[("test", 1, "__rtsadp_re_test")];

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Whether `class` names a runtime/Registry class the engine constructs (and
    /// is NOT shadowed by a user class of the same name).
    ///
    /// The wrapper primordials (`Boolean`/`Number`/`String`) are NO LONGER global
    /// ctors: their `.ts` prelude `class X` (the primitive-method library) now ALSO
    /// owns construction — `new X(x)` builds a shape-based object via the normal
    /// user-class path (`self.classes` has the ambient class, so the first clause
    /// below is false and we fall through to the user-class `new`). Only true
    /// runtime/Registry classes (`RegExp`, `Date`, …) with no ambient `.ts` class
    /// reach the global-ctor path here.
    pub(super) fn is_global_class_ctor(&self, class: &str) -> bool {
        self.classes.get(class).is_none()
            && (class_meta(class).is_some() || super::registry::has_class(class))
    }

    /// Whether `class` is a PURE runtime/Registry class — constructed and
    /// dispatched ENTIRELY through the REAL Registry (no codegen `class_meta`
    /// trampoline, no ambient `.ts` user class). This is the DATA-DRIVEN routing
    /// predicate that replaced the hardcoded `class == "Date"` checks: today the
    /// set is `{Date}`, and a future `Map`/`Set` migration joins it simply by
    /// dropping its `class_meta`/ambient entry — no new codegen branch. The
    /// per-class lowering lives in [`super::registryclass`].
    pub(super) fn is_pure_registry_class(&self, class: &str) -> bool {
        self.classes.get(class).is_none()
            && class_meta(class).is_none()
            && super::registry::has_class(class)
    }

    /// Lower `new <RuntimeClass>(args)` to its constructor trampoline, returning the
    /// boxed `TAG_OBJECT` instance word + the class name (so a `let` records the
    /// local's static class for method dispatch / instanceof).
    pub(super) fn lower_new_global_class(
        &mut self,
        module: &mut dyn Module,
        class: &str,
        args: &[HirExpr],
    ) -> FrontResult<(Val, String)> {
        // A PURE Registry class (today `Date`) constructs through the REAL
        // Registry (Pilar 6 — the ctor overloads come from the registered class,
        // no codegen ctor table). Data-driven: no class-name literal here.
        if self.is_pure_registry_class(class) {
            let word = self.emit_registry_ctor(module, class, args)?;
            return Ok((Val::tagged_kind(word, JsKind::Object), class.to_string()));
        }
        let meta = class_meta(class).expect("caller proved a global class");
        let word = match meta.kind {
            CtorKind::Regex => self.emit_regex_ctor(module, args)?,
        };
        Ok((Val::tagged_kind(word, JsKind::Object), class.to_string()))
    }

    /// `new RegExp(pattern[, flags])` — both args must be proven strings (a regex
    /// from a non-string pattern is a later increment). Interns nothing extra: the
    /// pattern/flags string PolyValue words go straight to `__rtsadp_re_compile`.
    /// Returns the boxed `TAG_OBJECT` RegExp instance word.
    fn emit_regex_ctor(&mut self, module: &mut dyn Module, args: &[HirExpr]) -> FrontResult<Value> {
        if args.is_empty() || args.len() > 2 {
            return unsupported!("`new RegExp(..)` expects 1 or 2 args, got {}", args.len());
        }
        let pat = self.lower_expr(module, &args[0])?;
        if !matches!(pat.kind, JsKind::Str) {
            return unsupported!(
                "`new RegExp(pattern)` with a non-string pattern (a regex-from-regex \
                 copy / coercion is a later increment)"
            );
        }
        let pat_word = self.box_value(pat);
        let flags_word = if args.len() == 2 {
            let f = self.lower_expr(module, &args[1])?;
            if !matches!(f.kind, JsKind::Str) {
                return unsupported!("`new RegExp(pattern, flags)` with a non-string flags arg");
            }
            self.box_value(f)
        } else {
            let pv = value::abi_adapter::intern_poly("");
            self.builder.ins().iconst(types::I64, pv.raw() as i64)
        };
        Ok(self
            .call_runtime(module, "__rtsadp_re_compile", &[pat_word, flags_word])?
            .expect("regex compile returns a value"))
    }

    /// Try to lower `inst.method(args)` where `inst`'s static class is a recorded
    /// runtime/Registry class (`Map`/`Set`/`Error`/…). Returns `Ok(Some(val))` on a
    /// resolved method, `Ok(None)` when the receiver is not a recorded global-class
    /// instance (caller falls through), or an explicit bail for an unknown method.
    pub(super) fn try_global_class_method(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Option<Val>> {
        let Some(class) = self.global_instance_class(object) else {
            return Ok(None);
        };
        // A PURE Registry class (today `Date`) dispatches its instance methods
        // through the REAL Registry (Pilar 6 — data-driven, no hand-written method
        // table): resolve the method's real `__RTS_FN_*` symbol + `AbiType`
        // signature and marshal generically. No class-name literal here.
        if self.is_pure_registry_class(&class) {
            return self
                .try_registry_instance_method(module, &class, object, method, args)
                .map(Some);
        }
        let meta = class_meta(&class).expect("recorded global class must resolve");
        let Some(&(_, arity, symbol)) = meta
            .methods
            .iter()
            .find(|(n, a, _)| *n == method && *a == args.len())
        else {
            return Err(crate::front::error::Unsupported::new(format!(
                "`{class}.{method}({} args)` — no such method on runtime class `{class}`",
                args.len()
            )));
        };
        debug_assert_eq!(arity, args.len());

        // Receiver word (slot 0) — the instance is a Tagged local; use its raw word.
        let recv = self.lower_expr(module, object)?;
        let recv_word = self.box_value(recv);
        let mut call_args: Vec<Value> = Vec::with_capacity(args.len() + 1);
        call_args.push(recv_word);
        for a in args {
            let v = self.lower_expr(module, a)?;
            call_args.push(self.box_value(v));
        }
        let res = self
            .call_runtime(module, symbol, &call_args)?
            .expect("global-class method returns a value");
        // Every trampoline returns a PolyValue word. The static kind is generally
        // Unknown (a get could be anything); `.set`/`.add` return the SAME instance
        // (kind Object), `.has`/`.delete` a bool — but Unknown is always sound here.
        Ok(Some(Val::new(res, crate::repr::Repr::Tagged)))
    }

    /// An instance PROPERTY of a recorded runtime/Registry instance (a RegExp's
    /// source/flags/…). Returns `Ok(Some(val))` when `object` is such an instance
    /// and `prop` is a known property; else `Ok(None)` (caller falls through to its
    /// normal member handling).
    ///
    /// NOTE: the Error family is gone from here — an `.ts` Error's
    /// `.message`/`.name`/`.stack` are ordinary shape slots read by the normal
    /// object-member path (or, for a thrown-then-caught error, the dynamic
    /// `__rtsadp_obj_get` fallback). No `__rtsadp_err_*` property trampolines.
    pub(super) fn try_global_class_member(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        prop: &str,
    ) -> FrontResult<Option<Val>> {
        let Some(class) = self.global_instance_class(object) else {
            return Ok(None);
        };
        // GENERIC: a property READ on a PURE Registry-class instance (`u.hostname`,
        // `u.pathname`) resolves to a 0-explicit-arg instance member — a getter — via
        // the Registry, exactly like a method call but with no `()`. Data-driven, no
        // per-class literal (the `RegExp` arms below are the primitive `/re/`
        // exception, kept hardcoded). Today this serves `URL`'s getters; any Registry
        // class with a 0-arg instance member gets its property read for free.
        if self.is_pure_registry_class(&class) {
            if let Some(call) = super::registry::class_member(&class, prop, 0) {
                let recv = self.lower_expr(module, object)?;
                let result_kind = match call.ret {
                    rts_engine::abi::AbiType::Handle => JsKind::Str,
                    rts_engine::abi::AbiType::Bool => JsKind::Bool,
                    _ => JsKind::Number,
                };
                let v = self.emit_registry_call(module, &call, Some(recv), &[], result_kind)?;
                return Ok(Some(v));
            }
            // A real `InstanceGetter` member (read with no `()` — e.g. a Symbol's
            // `description`). `class_member` above only matches `InstanceMethod`;
            // this resolves the getter-kind member.
            if let Some(call) = super::registry::class_instance_getter(&class, prop) {
                let recv = self.lower_expr(module, object)?;
                let result_kind = match call.ret {
                    rts_engine::abi::AbiType::Handle => JsKind::Str,
                    rts_engine::abi::AbiType::Bool => JsKind::Bool,
                    _ => JsKind::Number,
                };
                let v = self.emit_registry_call(module, &call, Some(recv), &[], result_kind)?;
                return Ok(Some(v));
            }
            // No registered getter for `prop` → DYNAMIC property read. This serves an
            // EXOTIC instance whose properties are not fixed members (a `Proxy`, whose
            // `get` trap fires inside `__rtsadp_obj_get`); for an ordinary Registry
            // instance with no such key the dynamic read returns `undefined` — the
            // JS-correct value (`new Date().foo` is `undefined`), not a bail. Generic
            // (no per-class name): the proxy trap dispatch lives entirely in the
            // runtime trampoline.
            let recv = self.lower_expr(module, object)?;
            let recv_word = self.box_value(recv);
            let key = self.intern_key_word(prop);
            let v = self
                .call_runtime(module, "__rtsadp_obj_get", &[recv_word, key])?
                .expect("__rtsadp_obj_get returns a value");
            return Ok(Some(Val::new(v, crate::repr::Repr::Tagged)));
        }
        let symbol = match (class.as_str(), prop) {
            ("RegExp", "source") => "__rtsadp_re_source",
            ("RegExp", "flags") => "__rtsadp_re_flags",
            ("RegExp", "global") => "__rtsadp_re_global",
            ("RegExp", "ignoreCase") => "__rtsadp_re_ignore_case",
            ("RegExp", "multiline") => "__rtsadp_re_multiline",
            ("RegExp", "lastIndex") => "__rtsadp_re_last_index",
            ("RegExp", other) => {
                return Err(crate::front::error::Unsupported::new(format!(
                    "`RegExp.{other}` — only source/flags/global/ignoreCase/multiline/\
                     lastIndex are properties on a runtime RegExp"
                )));
            }
            _ => return Ok(None),
        };
        let recv = self.lower_expr(module, object)?;
        let recv_word = self.box_value(recv);
        let res = self
            .call_runtime(module, symbol, &[recv_word])?
            .expect("global-class member returns a value");
        // `.size` is a number; `RegExp.global` is a bool; the error props +
        // `RegExp.source`/`.flags` are strings. Tag accordingly so a later
        // `console.log` / coercion formats them correctly.
        let kind = match prop {
            "lastIndex" => JsKind::Number,
            "global" | "ignoreCase" | "multiline" => JsKind::Bool,
            _ => JsKind::Str,
        };
        Ok(Some(Val::new_with_kind(
            res,
            crate::repr::Repr::Tagged,
            kind,
        )))
    }

    /// The recorded runtime/Registry class of a receiver, if statically known:
    /// - `new C(args)` directly (e.g. a chained `new RegExp(..).test(..)`);
    /// - a bare identifier recorded in `global_instance_classes`.
    pub(super) fn global_instance_class(&self, object: &HirExpr) -> Option<String> {
        // A bare regex LITERAL receiver `/pat/.test(s)` (P5.12): a RegExp instance.
        if super::regex::is_regex_literal(object) {
            return Some(super::regex::REGEX_CLASS.to_string());
        }
        match &object.kind {
            HirExprKind::New { class, .. } if self.is_global_class_ctor(class) => {
                Some(class.clone())
            }
            HirExprKind::Ident(name) => self.global_instance_classes.get(name).cloned(),
            _ => None,
        }
    }

    /// Lower `lhs instanceof <ClassName>` when the engine can decide it via a
    /// runtime class-tag check. Returns `Ok(Some(val))` (a bool) when handled, or
    /// `Ok(None)` when the right side is not an engine-checkable class (caller bails
    /// — never a guess). The check is a real runtime trampoline that inspects the
    /// instance's `Entry` kind / error name, so it is correct for ANY operand.
    pub(super) fn try_instanceof(
        &mut self,
        module: &mut dyn Module,
        lhs: &HirExpr,
        class: &str,
    ) -> FrontResult<Option<Val>> {
        // `x instanceof F` where `F` is a FUNCTION used as a constructor (Phase 3):
        // a REAL runtime check against the ctor side-table. `F`'s identity is its
        // THUNK address (the same `fn_ptr` `new F()` recorded via `__rtsadp_ctor_mark`).
        if self.is_fn_ctor(class) {
            let v = self.lower_expr(module, lhs)?;
            let word = self.box_value(v);
            let fn_ptr = self.fn_ctor_identity(module, class)?;
            let res = self
                .call_runtime(module, "__rtsadp_instanceof_fn", &[word, fn_ptr])?
                .expect("__rtsadp_instanceof_fn returns a value");
            return Ok(Some(Val::tagged_kind(res, JsKind::Bool)));
        }
        // A user class instanceof: a compile-time class-name compare (the local's
        // recorded class equals `class` or a descendant). Only resolvable when the
        // lhs has a statically-known class.
        if self.classes.get(class).is_some() {
            return self.user_instanceof(module, lhs, class).map(Some);
        }
        // A PURE Registry class (today `Date`) resolves instanceof through its
        // REAL Registry predicate symbol (e.g. `__RTS_FN_NS_GC_IS_DATE`) — no
        // codegen `__rtsadp_is_*` trampoline, no class-name literal here.
        if self.is_pure_registry_class(class) {
            if let Some(pred) = super::registry::instanceof_predicate(class) {
                let v = self.lower_expr(module, lhs)?;
                let word = self.box_value(v);
                return self.emit_registry_instanceof(module, word, pred).map(Some);
            }
        }
        // NOTE: `x instanceof Error` (and any error subtype) is handled by the
        // user-class branch above — the Error family is a PRIMORDIAL `.ts` class
        // (prelude), so its `instanceof` rides the shape-id/descendant check in
        // `user_instanceof`/`dynamic_user_instanceof`. No `__rtsadp_is_error`.
        let symbol = match class {
            "Array" => "__rtsadp_arr_is_array",
            // RegExp instanceof: no dedicated runtime tag trampoline yet, so only a
            // STATICALLY-recorded RegExp local/literal can be decided here (compile-
            // time class-name compare). A dynamic operand BAILS (never a guess).
            "RegExp" => {
                if self.global_instance_class(lhs).as_deref() == Some("RegExp") {
                    let word = value::PolyValue::bool(true).raw() as i64;
                    let v = self.builder.ins().iconst(types::I64, word);
                    return Ok(Some(Val::tagged_kind(v, JsKind::Bool)));
                }
                return Ok(None);
            }
            _ => return Ok(None),
        };
        let v = self.lower_expr(module, lhs)?;
        let boxed = self.box_value(v);
        let res = self
            .call_runtime(module, symbol, &[boxed])?
            .expect("instanceof trampoline returns a value");
        Ok(Some(Val::tagged_kind(res, JsKind::Bool)))
    }

    /// `lhs instanceof <UserClass>`. When the lhs has a statically-known class the
    /// result is the compile-time bool `class_is_a(lhsClass, class)` (the cheap
    /// fast path). When the lhs is OPAQUE (a param / return / any value with no
    /// statically-recorded class) we emit a REAL runtime check: the result is
    /// `true` iff the value is a `TAG_OBJECT` instance whose shape-id (slot 0)
    /// equals the `global_shape` of `class` OR of any descendant of `class`.
    fn user_instanceof(
        &mut self,
        module: &mut dyn Module,
        lhs: &HirExpr,
        class: &str,
    ) -> FrontResult<Val> {
        // Fast path: the lhs's STATIC class IS-A `class` → constant `true`. This is
        // sound — an `lhs_class` instance (exact OR a subclass) is always an instance
        // of any ancestor `class`. The CONVERSE is NOT sound: when `lhs_class` is NOT
        // an `class`, the runtime value may still be a SUBCLASS of `lhs_class` that
        // IS-A `class` (a base-typed param/field: `a: Animal` holding a `Dog`, then
        // `a instanceof Dog`). So a `false` result must come from the RUNTIME shape
        // check, never a compile-time constant — otherwise `a instanceof Dog` wrongly
        // folds to `false`.
        if let Some(lhs_class) = self.static_instance_class(lhs) {
            if self.class_is_a(&lhs_class, class) {
                let word = value::PolyValue::bool(true).raw() as i64;
                let v = self.builder.ins().iconst(types::I64, word);
                return Ok(Val::tagged_kind(v, JsKind::Bool));
            }
        }
        self.dynamic_user_instanceof(module, lhs, class)
    }

    /// Runtime `opaqueValue instanceof <UserClass>` (the None case of
    /// [`Self::user_instanceof`]).
    ///
    /// Each user class is allocated a UNIQUE `global_shape` (slot 0 of every
    /// instance, written by [`Self::lower_new`] as a boxed INT32 PolyValue). So
    /// `x instanceof C` ≡ `is_object(x) && shapeId(x) ∈ { global_shape of every
    /// class K where K IS-A C }`. The accepting set is computed entirely at
    /// compile time (C plus all its transitive descendants), so the emitted code
    /// is just an is-object tag check ANDed with a small OR-chain of `icmp eq`.
    ///
    /// SAFETY of the slot-0 read on a non-object word: `emit_vec_get` → a slab
    /// handle-table lookup (`with_vec`), never a raw-pointer dereference. A
    /// non-object word reconstructs a bogus/non-Vec handle, which the runtime
    /// tolerates by returning `0` — it CANNOT fault. We therefore read slot 0
    /// unconditionally and discard the result for a non-object via the final
    /// `band` with `is_object` (the same shape as `emit_registry_instanceof`),
    /// rather than branching.
    fn dynamic_user_instanceof(
        &mut self,
        module: &mut dyn Module,
        lhs: &HirExpr,
        class: &str,
    ) -> FrontResult<Val> {
        // Accepting shape-id set: every collected class K with `K IS-A class`.
        // Snapshot (name, shape) first so the `class_is_a` borrow does not overlap
        // the `self.classes.iter()` borrow.
        let all: Vec<(String, u32)> = self
            .classes
            .iter()
            .map(|d| (d.name.clone(), d.global_shape))
            .collect();
        let accepting: Vec<u32> = all
            .into_iter()
            .filter(|(name, _)| self.class_is_a(name, class))
            .map(|(_, shape)| shape)
            .collect();
        debug_assert!(
            self.classes
                .get(class)
                .is_some_and(|d| accepting.contains(&d.global_shape)),
            "the class itself must be in its own accepting set"
        );

        // Lower the operand to a single PolyValue word.
        let v = self.lower_expr(module, lhs)?;
        let word = self.box_value(v);

        // is_object = boxed(word) && tag(word) == TAG_OBJECT.
        let boxed = value::emit_is_boxed(self.builder, word);
        let shifted = self.builder.ins().ushr_imm(word, value::TAG_SHIFT as i64);
        let tag = self.builder.ins().band_imm(shifted, value::TAG_MASK as i64);
        let want = self
            .builder
            .ins()
            .iconst(types::I64, value::TAG_OBJECT as i64);
        let is_obj_tag =
            self.builder
                .ins()
                .icmp(cranelift_codegen::ir::condcodes::IntCC::Equal, tag, want);
        let is_object = self.builder.ins().band(boxed, is_obj_tag);

        // slot0 = shape-id word of the instance (a boxed INT32 PolyValue). Safe on
        // a non-object (returns 0 — see the doc comment above).
        let zero_idx = self.builder.ins().iconst(types::I64, 0);
        let slot0 = value::emit_marshal::emit_vec_get(module, self.builder, word, zero_idx);

        // shapeId(word) ∈ accepting : OR-chain of `icmp eq slot0, <boxedInt(shape)>`
        // (each `icmp` yields an i8 bool, matching `is_object`'s type).
        let mut matches = self.builder.ins().iconst(types::I8, 0);
        for shape in accepting {
            let shape_word = value::PolyValue::from_i32(shape as i32).raw() as i64;
            let shape_v = self.builder.ins().iconst(types::I64, shape_word);
            let eq = self.builder.ins().icmp(
                cranelift_codegen::ir::condcodes::IntCC::Equal,
                slot0,
                shape_v,
            );
            matches = self.builder.ins().bor(matches, eq);
        }

        // result = is_object && shapeId ∈ accepting (i8 0/1, Repr::Bool).
        let result = self.builder.ins().band(is_object, matches);
        Ok(Val::new(result, crate::repr::Repr::Bool))
    }

    /// Whether `derived` IS `base` or transitively extends it (walking `parent`),
    /// over the user ClassTable. A builtin-error virtual parent participates too.
    fn class_is_a(&self, derived: &str, base: &str) -> bool {
        let mut cur = Some(derived.to_string());
        let mut steps = 0;
        while let Some(name) = cur {
            if name == base {
                return true;
            }
            if steps > 64 {
                break;
            }
            steps += 1;
            cur = self.classes.get(&name).and_then(|d| d.parent.clone());
        }
        false
    }
}
