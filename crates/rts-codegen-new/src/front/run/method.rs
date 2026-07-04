//! Data-driven instance-method dispatch lowering — P4.
//!
//! A `recv.method(args)` whose receiver class is STATICALLY proven (a string /
//! number) and whose `(method, arity)` resolves in the Registry-mirror metadata
//! ([`crate::dispatch`]) lowers to a typed `call` of the REAL `__RTS_FN_GL_*`
//! symbol — no per-method switchboard. ONE generic path: marshal the receiver +
//! each PolyValue arg to the method signature's [`AbiType`], emit the `call`,
//! marshal the result back to a PolyValue.
//!
//! Everything else BAILS EXPLICITLY (never a wrong value):
//! - a receiver whose class is not statically provable (a Tagged var/param/call
//!   result — dynamic receiver-kind dispatch is a later increment);
//! - a `(method, arity)` not in the metadata;
//! - a method taking a callback (`.map`/`.filter`/… need function VALUES — a
//!   later increment): detected as an arrow/function-expression argument;
//! - an argument whose proven kind does not match the slot's `AbiType` (a string
//!   slot wants a string arg, a number slot wants a numeric arg).

use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;

use rts_hir::HirExpr;
use rts_hir::ir::HirExprKind;

use rts_runtime::abi::AbiType;

use crate::dispatch::{MethodSpec, RecvAbi, RecvClass, resolve_method};
use crate::repr::Repr;
use crate::value::{self, emit_marshal};

use crate::front::error::{FrontResult, unsupported};

use super::lower::{JsKind, Lowerer, Val};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// The Object-PROTOCOL methods every JS object inherits from
    /// `Object.prototype`, over ANY object receiver expression:
    /// `proto.isPrototypeOf(obj)` (the `Object.create` chain walk),
    /// `o.hasOwnProperty(k)` (OWN-slot check, no prototype walk) and
    /// `o.propertyIsEnumerable(k)`. Shared by the whole-heap-object receiver
    /// path and the user-class not-found fallback in `try_class_method` (a
    /// class instance inherits these like any object). `Ok(None)` ⇒ not one of
    /// the three (the caller keeps its own fallbacks).
    pub(in crate::front::run) fn try_object_protocol_method(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Option<Val>> {
        let symbol = match (method, args.len()) {
            ("isPrototypeOf", 1) => "__rtsadp_is_prototype_of",
            ("hasOwnProperty", 1) => "__rtsadp_has_own",
            ("propertyIsEnumerable", 1) => "__rtsadp_prop_is_enumerable",
            _ => return Ok(None),
        };
        let recv = self.lower_expr(module, object)?;
        let recv_word = self.box_value(recv);
        let arg = self.lower_expr(module, &args[0])?;
        let arg_word = self.box_value(arg);
        let res = self
            .call_runtime(module, symbol, &[recv_word, arg_word])?
            .expect("an object-protocol trampoline returns a value");
        Ok(Some(self.poly_bool_to_bool(res)))
    }

    /// Try to lower `recv.method(args)` via data-driven dispatch. Returns
    /// `Ok(Some(val))` on success, `Ok(None)` when the receiver class is not a
    /// dispatchable primordial (so the caller falls through to its next handler,
    /// e.g. `console.log`), or `Err(Unsupported)` for an explicit bail.
    pub(super) fn try_method_dispatch(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Option<Val>> {
        // NAMESPACE-OBJECT member call (`gc.string_from_i64(x)`, `io.print(s)`):
        // `gc`/`io` was imported from bare `"rts"` (bound as a namespace object,
        // member empty). Resolve `method` as a namespace function of that namespace
        // through the Registry — the SAME path a direct `rts:<ns>` member-import call
        // uses. An unknown member / unregistered namespace bails honestly.
        if let HirExprKind::Ident(obj) = &object.kind {
            if let Some((ns, member)) = self.builtins.get(obj).cloned() {
                if member.is_empty() {
                    return self.lower_builtin_call(module, &ns, method, args).map(Some);
                }
            }
            // AMBIENT prelude namespace (`test_core.*`/`string.*`/`fmt.*`): a bare
            // ident naming a REGISTERED namespace, used by the embedded `rts:test`
            // bundle prelude (it was NOT import-bound, so it is not in `builtins`).
            // Gated to PRELUDE-origin code + not shadowed by a local/class — the same
            // privacy posture as the `engine.*` global. Resolves through the SAME
            // generic Registry path (`lower_builtin_call`); an unknown member bails.
            if self.is_prelude
                && self.local(obj).is_none()
                && self.classes.get(obj).is_none()
                && super::registry::has_namespace(obj)
            {
                return self.lower_builtin_call(module, obj, method, args).map(Some);
            }
        }

        // STATIC method call `C.m(args)` (P5.1): the receiver is a bare class NAME
        // (not a local/instance). Resolve the static method on that class and emit
        // a direct call (no `this`). An unknown static on a known class BAILS.
        if let Some(class) = self.class_name_receiver(object) {
            return self
                .try_static_method(module, &class, method, args)
                .map(Some);
        }

        // RUNTIME/Registry-class instance receiver (P5.3): a `new Map()`/`Set()`/
        // `Error(..)` instance (direct or a recorded local). Dispatch through the
        // global-class metadata table. `Ok(None)` ⇒ not such an instance.
        if let Some(val) = self.try_global_class_method(module, object, method, args)? {
            return Ok(Some(val));
        }

        // CLASS-INSTANCE receiver (P4.9): a receiver whose class is statically
        // known (`new C()`, a local recorded in `local_classes`, or `this`).
        // Resolve the method on that class at COMPILE TIME and emit a direct call
        // passing the instance as `this`. An unknown method on a known class BAILS.
        if let Some(class) = self.static_instance_class(object) {
            return self
                .try_class_method(module, object, &class, method, args)
                .map(Some);
        }

        // ARRAY receiver (P4.5 non-callback + P4.7 callback): an identifier bound
        // to a local of proven array shape. The element convention is the engine's
        // own (boxed PolyValue words), so these resolve to the codegen-owned
        // `__rtsadp_arr_*` trampolines. Array CALLBACK methods (`.map`/`.reduce`/…)
        // reify the callback argument to a function VALUE here. Must be checked
        // BEFORE the whole-heap-value gate below (an array IS a whole-heap value).
        if self.is_array_receiver(object) {
            return self
                .try_array_dispatch(module, object, method, args)
                .map(Some);
        }

        // A non-array receiver does NOT take a callback in the implemented surface
        // (String/Number methods are all non-callback); a callback arg here is a
        // later increment — bail explicitly.
        for a in args {
            if is_callback_arg(a) {
                return Err(crate::front::error::Unsupported::new(format!(
                    "method `.{method}()` with a callback argument on a non-array receiver"
                )));
            }
        }

        // The receiver must lower AND have a statically-proven class. Lower it
        // first (a whole object/array value is not a dispatch receiver here).
        if self.is_whole_heap_value(object) {
            // The Object-protocol surface (`hasOwnProperty`/`isPrototypeOf`/
            // `propertyIsEnumerable`) — shared with the user-class not-found
            // fallback in `try_class_method` (every JS object inherits these).
            if let Some(val) = self.try_object_protocol_method(module, object, method, args)? {
                return Ok(Some(val));
            }
            if !self.is_array_valued(object) {
                let recv = self.lower_expr(module, object)?;
                if let Some(val) =
                    self.try_primitive_class_method(module, recv, "Object", method, args)?
                {
                    return Ok(Some(val));
                }
                // Not a `class Object` builtin: the receiver may be an object literal
                // carrying its own method (synthetic class) whose static class is not
                // visible here (a free top-level `const o = { m(){…} }` used inside
                // another fn). Dispatch by runtime shape-id (same virtual path the
                // Tagged branch uses). `Ok(None)` ⇒ no class defines it (caller bails).
                if let Some(val) = self.try_user_virtual_dynamic(module, recv, method, args)? {
                    return Ok(Some(val));
                }
            }
            return Ok(None);
        }
        let recv = self.lower_expr(module, object)?;
        let Some(class) = recv_class_of(recv) else {
            // PRIMITIVE BOOL receiver (`true.toString()` / `flag.valueOf()`): route
            // to the ambient prelude `class Boolean` (the `.ts` primitive-method
            // library), passing the primitive BOXED as `this`. This is the
            // primitive → prelude-`.ts`-class dispatch mechanism (the prover for
            // String/Number next). `Ok(None)` ⇒ no such method on `Boolean` (or the
            // prelude class is absent) — falls through to the dynamic/bail paths.
            if matches!(recv.kind, JsKind::Bool) || recv.repr == Repr::Bool {
                if let Some(val) =
                    self.try_primitive_class_method(module, recv, "Boolean", method, args)?
                {
                    return Ok(Some(val));
                }
            }
            // The receiver is a TAGGED value of unproven class (a param, a call
            // return, a re-`let` local). When the METHOD NAME is known (P5.9),
            // dispatch on the receiver's PolyValue tag AT RUNTIME via a
            // `__rtsadp_dyn_*` trampoline. `Ok(None)` ⇒ the method is not
            // dynamically dispatchable (the caller bails — never a guess).
            if recv.repr == Repr::Tagged {
                // An ARRAY CALLBACK method (`map`/`filter`/`reduce`/…) on an
                // unproven receiver with a reifiable (non-capturing) callback:
                // dispatch through the same `__rtsadp_arr_*` trampolines as the
                // proven path (SAFE on a non-array word — the trampolines do a
                // HandleTable lookup and see length 0 for a non-Vec entry). Tried
                // BEFORE the non-callback dyn path. `Ok(None)` ⇒ not an array
                // callback method / a capturing callback — falls through.
                if let Some(val) = self.try_array_callback_dynamic(module, recv, method, args)? {
                    return Ok(Some(val));
                }
                if let Some(val) = self.try_method_dispatch_dynamic(module, recv, method, args)? {
                    return Ok(Some(val));
                }
                // USER-CLASS virtual dispatch on a Tagged receiver of unproven class
                // (a for-of binding, a cast, a reassigned local, a function return):
                // dispatch by the instance's runtime shape-id to whichever user class
                // defines `method` (object-guarded, `undefined` for a non-match — a
                // TypeError-class sentinel, never a wrong value). `Ok(None)` ⇒ no user
                // class defines `method` / an effectful arg — falls through.
                if let Some(val) = self.try_user_virtual_dynamic(module, recv, method, args)? {
                    return Ok(Some(val));
                }
                // FALLBACK: the dynamic table (`DYN_METHODS`) only covers the
                // high-frequency string/array methods at fixed arity. For a method
                // it lacks (`padStart`/`concat`/`includes` with a position arg/…) on
                // a Tagged receiver that is a STRING, route to the ambient prelude
                // `class String` (the `.ts` primitive-method library) — the SAME
                // path a PROVEN-string literal uses, just over the dynamic word. The
                // `.ts` `__str_val(this)` reads the string from the boxed receiver;
                // a non-string Tagged word would mis-dispatch, so gate on a proven
                // string kind. `Ok(None)` ⇒ no such String method (caller bails).
                if matches!(recv.kind, JsKind::Str) {
                    if let Some(val) =
                        self.try_primitive_class_method(module, recv, "String", method, args)?
                    {
                        return Ok(Some(val));
                    }
                }
                // LAST: DYNAMIC Registry-class instance dispatch — the receiver
                // may be a Registry instance (a Date/URL/… reached through an
                // `any` param or a stored slot). Resolved at RUNTIME by each
                // candidate class's `instanceof_predicate` (data-driven; the
                // Registry counterpart of the user-class virtual dispatch).
                if let Some(val) =
                    self.try_registry_virtual_dynamic(module, recv, method, args)?
                {
                    return Ok(Some(val));
                }
                return Ok(None);
            }
            return Ok(None);
        };

        // STRING methods that don't fit the generic typed-row path (P5.2): `split`
        // (returns an ARRAY of boxed string words + a defaulted limit) and the
        // 1-arg `slice`/`substring`/`substr` (a defaulted "to end" bound). Handled
        // here over the proven-string receiver; everything else falls to the row.
        if matches!(class, RecvClass::String) {
            // P5.12: a string-with-regex method (`s.match`/`.replace`/`.split`/
            // `.search` whose first arg is a regex literal or recorded RegExp). This
            // must run BEFORE `try_string_special` (whose `split` bails on a regex
            // separator) and before the generic row (whose `replace` wants a string
            // first arg). `Ok(None)` ⇒ not a regex-first method (falls through).
            if let Some(val) = self.try_string_regex_method(module, recv, method, args)? {
                return Ok(Some(val));
            }
            // `s.match(x)` / `s.replace(x, r)` with an UNPROVEN (Tagged/object)
            // first arg: the runtime hook trampoline — a custom `[Symbol.match]`/
            // `[Symbol.replace]` object runs its hook (spec); hook-less coerces
            // (match → `new RegExp(ToString)`, replace → plain string replace).
            {
                // Only a pattern arg that is STATICALLY an object-ish value (an
                // object literal, or an ident holding a Tagged/object local)
                // routes here — a proven string/regex arg keeps its row, and
                // nothing is double-lowered.
                let arg_objectish = |a: &HirExpr, me: &Self| -> bool {
                    if me.is_regex_value(a) || matches!(a.ty, rts_hir::HirType::Str) {
                        return false;
                    }
                    match &a.kind {
                        HirExprKind::Object(_) => true,
                        HirExprKind::Ident(n) => me
                            .local(n)
                            .is_some_and(|l| matches!(l.repr, crate::repr::Repr::Tagged))
                            && !me.string_locals.contains(n),
                        _ => false,
                    }
                };
                if method == "match" && args.len() == 1 && arg_objectish(&args[0], self) {
                    let recv_word = self.box_value(recv);
                    let a = self.lower_expr(module, &args[0])?;
                    let aw = self.box_value(a);
                    let word = self
                        .call_runtime(module, "__rtsadp_str_match_w", &[recv_word, aw])?
                        .expect("__rtsadp_str_match_w returns a value");
                    self.emit_post_call_error_check(module)?;
                    return Ok(Some(Val::new(word, crate::repr::Repr::Tagged)));
                }
                if method == "split" && (args.len() == 1 || args.len() == 2)
                    && arg_objectish(&args[0], self)
                {
                    let recv_word = self.box_value(recv);
                    let p = self.lower_expr(module, &args[0])?;
                    let pw = self.box_value(p);
                    let lw = match args.get(1) {
                        Some(a) => {
                            let v = self.lower_expr(module, a)?;
                            self.box_value(v)
                        }
                        None => self.builder.ins().iconst(
                            cranelift_codegen::ir::types::I64,
                            crate::value::PolyValue::undefined().raw() as i64,
                        ),
                    };
                    let word = self
                        .call_runtime(module, "__rtsadp_str_split_w", &[recv_word, pw, lw])?
                        .expect("__rtsadp_str_split_w returns a value");
                    self.emit_post_call_error_check(module)?;
                    return Ok(Some(Val::new(word, crate::repr::Repr::Tagged)));
                }
                if method == "replace" && args.len() == 2 && arg_objectish(&args[0], self) {
                    let recv_word = self.box_value(recv);
                    let p = self.lower_expr(module, &args[0])?;
                    let pw = self.box_value(p);
                    let r = self.lower_expr(module, &args[1])?;
                    let rw = self.box_value(r);
                    let word = self
                        .call_runtime(module, "__rtsadp_str_replace_w", &[recv_word, pw, rw])?
                        .expect("__rtsadp_str_replace_w returns a value");
                    self.emit_post_call_error_check(module)?;
                    return Ok(Some(Val::new(word, crate::repr::Repr::Tagged)));
                }
            }
            if let Some(val) = self.try_string_special(module, recv, method, args)? {
                return Ok(Some(val));
            }
            // PROVEN-STRING receiver (`"abc".toUpperCase()`, `s.trim()`): route to
            // the ambient prelude `class String` (the `.ts` primitive-method
            // library), passing the primitive string BOXED as `this`. Same
            // mechanism as the primitive-bool/number paths. The `.ts` method bodies
            // call the private `engine.str_*` helpers (the irreducible Unicode-logic
            // bridge), so the engine no longer hardcodes the migrated String method
            // surface here. Runs AFTER the regex-first + `split`/1-arg-`slice`
            // specials (which the `.ts` class does NOT cover) and BEFORE the
            // (now-narrowed) `STRING_ROWS` table. `Ok(None)` ⇒ no such method on
            // `String` (the prelude class lacks this `(method, arity)`) — falls
            // through to the dispatch table / bail, never a guess.
            if let Some(val) =
                self.try_primitive_class_method(module, recv, "String", method, args)?
            {
                return Ok(Some(val));
            }
        }

        // PROVEN-NUMERIC receiver (`(5).toFixed(2)`, `(255).toString(16)`,
        // `(42).valueOf()`): route to the ambient prelude `class Number` (the `.ts`
        // primitive-method library), passing the primitive number BOXED as `this`.
        // Same mechanism as the primitive-bool path above. The `.ts` method bodies
        // call the private `engine.num_*` formatters (the irreducible-format bridge),
        // so the engine no longer hardcodes the Number method surface. `Ok(None)` ⇒
        // no such method on `Number` (the prelude class is present but lacks this
        // `(method, arity)`) — falls through to the (now-narrowed) dispatch table /
        // bail, never a guess.
        if matches!(class, RecvClass::Number) {
            if let Some(val) =
                self.try_primitive_class_method(module, recv, "Number", method, args)?
            {
                return Ok(Some(val));
            }
        }

        let argc = args.len();
        let Some(spec) = resolve_method(class, method, argc) else {
            return Err(crate::front::error::Unsupported::new(format!(
                "no Registry entry for `{class:?}.{method}({argc} args)`"
            )));
        };
        if spec.args.len() != argc {
            // Defensive: resolve_method already matched arity; keep the invariant.
            return Err(crate::front::error::Unsupported::new(format!(
                "`.{method}()` arity mismatch ({argc} vs {})",
                spec.args.len()
            )));
        }

        let val = self.emit_dispatch_call(module, recv, &spec, args)?;
        Ok(Some(val))
    }

    /// Route a method called on a PRIMITIVE receiver (`recv`, already lowered) to
    /// the ambient prelude `.ts` class `prim_class` (e.g. `"Boolean"`), passing the
    /// primitive BOXED as the method's `this`. This is the primitive →
    /// prelude-`.ts`-class method-dispatch mechanism: the engine resolves the
    /// `(method, arity)` on the ambient class at COMPILE TIME (shape-based, not JS
    /// prototypes) and emits a direct call of the synthesized `__rtsn_method_*`,
    /// reusing the SAME [`Self::call_synth_fn`] path a user-class instance uses.
    ///
    /// The `.ts` method bodies read `this` AS THE PRIMITIVE (the boxed primitive
    /// word — e.g. `this ? "true" : "false"`). Returns `Ok(None)` when the prelude
    /// class is absent or has no such `(method, arity)` (the caller falls through to
    /// the dynamic/bail paths — never a guess). This is the reusable pattern
    /// String/Number will follow (route the proven string/number primitive to a
    /// prelude `class String`/`class Number` once their `.ts` method libs land).
    pub(super) fn try_primitive_class_method(
        &mut self,
        module: &mut dyn Module,
        recv: Val,
        prim_class: &str,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Option<Val>> {
        let Some(desc) = self.classes.get(prim_class).cloned() else {
            return Ok(None);
        };
        let Some(fn_name) = desc.method_fn(method).map(str::to_string) else {
            return Ok(None);
        };
        // The primitive becomes the method's `this` (boxed PolyValue word).
        let this_word = self.box_value(recv);
        let val = self.call_synth_fn(module, &fn_name, Some(this_word), args)?;
        Ok(Some(val))
    }

    /// Lower the STRING methods that need special arg/return handling beyond the
    /// generic typed-row path (P5.2):
    /// - `split(sep[, limit])` → `__rtsadp_str_split(recvHandle, sepHandle, limit)`,
    ///   returning an ARRAY of boxed string words (a regex separator BAILS — the
    ///   sep must be a proven string);
    /// - 1-arg `slice(n)`/`substring(n)`/`substr(n)` → the real 2-arg symbol with a
    ///   defaulted "to end" bound (`i64::MAX`, which the runtime clamps to length).
    ///
    /// Returns `Ok(None)` when `method` is not one of these specials (the caller
    /// falls through to the generic row).
    fn try_string_special(
        &mut self,
        module: &mut dyn Module,
        recv: Val,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Option<Val>> {
        match (method, args.len()) {
            ("split", 1) | ("split", 2) => {
                // sep must be a proven string (a regex separator routes through
                // the regex-first path before this). Lower + marshal it to a
                // real string handle.
                let sep = self.lower_expr(module, &args[0])?;
                // `split(undefined)` — JS: the whole string as the ONE element.
                if matches!(sep.kind, JsKind::Undefined) {
                    let arr = crate::value::emit_marshal::emit_new_vec_object(
                        module,
                        self.builder,
                    );
                    let recv_word = self.box_value(recv);
                    crate::value::emit_marshal::emit_vec_push(
                        module,
                        self.builder,
                        arr,
                        recv_word,
                    );
                    return Ok(Some(Val::tagged_kind(arr, JsKind::Array)));
                }
                if !matches!(sep.kind, JsKind::Str) {
                    return unsupported!(
                        "String.split with a non-string (regex?) separator is a later increment"
                    );
                }
                let sep_word = self.box_value(sep);
                let sep_handle = emit_marshal::emit_table_load(module, self.builder, sep_word);
                let limit = if args.len() == 2 {
                    let l = self.lower_expr(module, &args[1])?;
                    self.numeric_to_i64(l)?
                } else {
                    self.builder.ins().iconst(types::I64, -1)
                };
                let rh = {
                    let word = self.box_value(recv);
                    emit_marshal::emit_table_load(module, self.builder, word)
                };
                let ret = emit_marshal::emit_call(
                    module,
                    self.builder,
                    "__rtsadp_str_split",
                    &[rh, sep_handle, limit],
                );
                let word = ret.expect("__rtsadp_str_split returns a value");
                Ok(Some(Val::tagged_kind(word, JsKind::Array)))
            }
            ("slice", 1) | ("substring", 1) | ("substr", 1) => {
                let start = self.lower_expr(module, &args[0])?;
                let start_i = self.numeric_to_i64(start)?;
                // The "to end" default bound: i64::MAX clamps to length in the
                // runtime (slice/substring) or means "rest" for substr's length.
                let end = self.builder.ins().iconst(types::I64, i64::MAX);
                let symbol = match method {
                    "slice" => "__RTS_FN_GL_STRING_SLICE",
                    "substring" => "__RTS_FN_GL_STRING_SUBSTRING",
                    _ => "__RTS_FN_GL_STRING_SUBSTR",
                };
                let rh = {
                    let word = self.box_value(recv);
                    emit_marshal::emit_table_load(module, self.builder, word)
                };
                let ret =
                    emit_marshal::emit_call(module, self.builder, symbol, &[rh, start_i, end]);
                let h = ret.expect("string slice returns a handle");
                let word = emit_marshal::emit_box_real_string(module, self.builder, h);
                Ok(Some(Val::tagged_kind(word, JsKind::Str)))
            }
            _ => Ok(None),
        }
    }

    /// Marshal the receiver + each arg per `spec`, emit the `call`, marshal the
    /// result back to a PolyValue `Val`.
    fn emit_dispatch_call(
        &mut self,
        module: &mut dyn Module,
        recv: Val,
        spec: &MethodSpec,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        let mut call_args: Vec<Value> = Vec::with_capacity(args.len() + 1);

        // ---- receiver (slot 0) ----
        match spec.recv_abi {
            RecvAbi::Handle => {
                // A string PolyValue → its real GC handle (POLY_TO_HANDLE).
                let word = self.box_value(recv);
                let handle = emit_marshal::emit_table_load(module, self.builder, word);
                call_args.push(handle);
            }
            RecvAbi::F64 => {
                let f = self.coerce(recv, Repr::Float64)?;
                call_args.push(f);
            }
            // Array receivers take the dedicated `try_array_dispatch` path; a
            // String/Number row never carries `ArrayVec`.
            RecvAbi::ArrayVec => {
                return unsupported!("array receiver reached the non-array dispatch path");
            }
        }

        // ---- explicit args ----
        for (a, &want) in args.iter().zip(spec.args) {
            // A REGEX-valued arg into a Handle slot (`s.matchAll(re)` where the
            // pattern rides an `_AUTO` string|RegExp member): the real
            // `Entry::Regex` handle — the runtime dispatches by entry kind.
            if matches!(want, AbiType::Handle) && self.is_regex_value(a) {
                let re = self.lower_regex_value(module, a)?;
                let word = self.box_value(re);
                call_args.push(emit_marshal::emit_table_load(module, self.builder, word));
                continue;
            }
            let v = self.lower_expr(module, a)?;
            let marshaled = self.marshal_arg(module, v, want)?;
            call_args.push(marshaled);
        }

        // ---- emit the typed call to the REAL symbol ----
        let ret = emit_marshal::emit_call(module, self.builder, spec.symbol, &call_args);

        // ---- marshal the result back to a PolyValue ----
        self.marshal_ret(module, spec.ret, spec.ret_is_array, ret)
    }

    /// Marshal one lowered arg `v` to the slot `AbiType` the real symbol wants.
    /// A mismatch (a number where a string handle is wanted, etc.) is an explicit
    /// bail — never a wrong coercion.
    fn marshal_arg(
        &mut self,
        module: &mut dyn Module,
        v: Val,
        want: AbiType,
    ) -> FrontResult<Value> {
        match want {
            // A string handle slot: the arg must be a proven string PolyValue;
            // box it and table-load to the real handle.
            AbiType::Handle => self.marshal_proven_string_handle(module, v, "method"),
            // An index / count: a proven number, truncated to i64.
            AbiType::I64 => {
                let f = self.numeric_to_i64(v)?;
                Ok(f)
            }
            AbiType::F64 => {
                let f = self.coerce(v, Repr::Float64)?;
                Ok(f)
            }
            other => unsupported!("cannot marshal a method arg of ABI {other:?}"),
        }
    }

    /// Coerce a numeric `Val` to an i64 (index/count). A TAGGED value decodes by
    /// TAG at runtime (`emit_tagged_number_to_f64` — int32 or inline double) and
    /// truncates, exactly the `coerce(Tagged → Int)` rule: a non-number word
    /// decodes NaN → saturates to a defined 0, never a trap or a wrong slot.
    pub(super) fn numeric_to_i64(&mut self, v: Val) -> FrontResult<Value> {
        match v.repr {
            Repr::Int32 | Repr::Int64 => Ok(v.v),
            // Bool rides i64 0/1 already (JS ToNumber of a bool index).
            Repr::Bool => Ok(v.v),
            // SATURATING f64→i64: a `number` index/count/depth arg that is
            // `Infinity`/`NaN`/out-of-range (e.g. `arr.flat(Infinity)`) yields a
            // defined clamp (i64::MIN/MAX/0) instead of a Cranelift trap (SIGILL).
            Repr::Float64 => Ok(self.builder.ins().fcvt_to_sint_sat(types::I64, v.v)),
            Repr::Tagged => {
                let f = value::emit_tagged_number_to_f64(self.builder, v.v);
                Ok(self.builder.ins().fcvt_to_sint_sat(types::I64, f))
            }
            _ => unsupported!("method arg wants a number index but got {:?}", v.repr),
        }
    }

    /// Marshal a PROVEN-string `Val` to a real GC string handle (box → table-load).
    /// A value whose kind is not statically a string BAILS (never a mis-marshal /
    /// SIGILL on a bogus slot). Shared by the typed-method and array-method arg
    /// marshalers, whose `Handle` arm means exactly "a proven string"; `ctx` names
    /// the call family for the bail message. NOTE: this is NOT the registry path's
    /// `Handle` arm (registry_call.rs), whose `Handle` ALSO accepts an opaque
    /// integer resource id — that contract is deliberately separate.
    pub(super) fn marshal_proven_string_handle(
        &mut self,
        module: &mut dyn Module,
        v: Val,
        ctx: &str,
    ) -> FrontResult<Value> {
        if !matches!(v.kind, JsKind::Str) {
            return unsupported!(
                "{ctx} arg wants a string but its kind is not statically a string ({:?})",
                v.repr
            );
        }
        let word = self.box_value(v);
        Ok(emit_marshal::emit_table_load(module, self.builder, word))
    }

    /// Marshal a method result (the `call`'s Cranelift value, or `None` for void)
    /// back to a PolyValue `Val` per the return `AbiType`.
    fn marshal_ret(
        &mut self,
        module: &mut dyn Module,
        ret: AbiType,
        ret_is_array: bool,
        value: Option<Value>,
    ) -> FrontResult<Val> {
        match ret {
            // A returned handle: `ret_is_array` routes the ONE heterogeneous-
            // handle authority (`__rtsadp_box_handle_auto`): 0 → null; a String
            // entry → string word (`s.match("p")` returns the match STRING); a
            // Vec → object word with legacy raw-handle elements normalized to
            // words. Otherwise a TAG_STR string (the GL_STRING scalar returns).
            AbiType::Handle if ret_is_array => {
                let h = value.expect("Handle-returning symbol yields a value");
                let word = self
                    .call_runtime(module, "__rtsadp_box_handle_auto", &[h])?
                    .expect("__rtsadp_box_handle_auto returns a word");
                Ok(Val::new(word, Repr::Tagged))
            }
            AbiType::Handle => {
                let h = value.expect("Handle-returning symbol yields a value");
                let word = emit_marshal::emit_box_real_string(module, self.builder, h);
                Ok(Val::tagged_kind(word, JsKind::Str))
            }
            // A returned integer → a proven Int64 number (unboxed fast path).
            AbiType::I64 | AbiType::I32 | AbiType::U64 => {
                let v = value.expect("int-returning symbol yields a value");
                Ok(Val::new(v, Repr::Int64))
            }
            AbiType::F64 => {
                let v = value.expect("f64-returning symbol yields a value");
                Ok(Val::new(v, Repr::Float64))
            }
            // A returned boolean (extern "C" i64 0/1) → a proven Bool.
            AbiType::Bool => {
                let v = value.expect("bool-returning symbol yields a value");
                // The extern returns i64 0/1; narrow to the Bool carrier (i64 0/1
                // already) — keep as-is, repr Bool.
                Ok(Val::new(v, Repr::Bool))
            }
            // A raw PolyValue word returned verbatim → keep it Tagged.
            AbiType::PolyValue => {
                Ok(Val::new(value.expect("PolyValue-returning symbol yields a value"), Repr::Tagged))
            }
            AbiType::Void => {
                let v = self
                    .builder
                    .ins()
                    .iconst(types::I64, value::PolyValue::undefined().raw() as i64);
                Ok(Val::tagged_kind(v, JsKind::Undefined))
            }
            AbiType::StrPtr => unsupported!("a method returning StrPtr is not marshaled yet"),
        }
    }
}

/// The dispatch class implied by a receiver `Val`, when statically provable.
/// `JsKind::Str` ⇒ String; a proven number repr ⇒ Number. Anything else (a
/// Tagged var of unknown kind, a bool, an object) is not a dispatch receiver
/// here — returns `None` so the caller falls through / bails.
fn recv_class_of(recv: Val) -> Option<RecvClass> {
    match recv.kind {
        JsKind::Str => Some(RecvClass::String),
        JsKind::Number => Some(RecvClass::Number),
        _ => match recv.repr {
            Repr::Int32 | Repr::Int64 | Repr::Float64 => Some(RecvClass::Number),
            _ => None,
        },
    }
}

/// Whether an argument expression is a callback (a function/arrow value). On a
/// NON-array receiver such methods bail (function-valued args are only handled
/// for array callback methods). A capturing arrow stays an `Arrow` node here.
fn is_callback_arg(e: &HirExpr) -> bool {
    matches!(&e.kind, HirExprKind::Arrow { .. })
}
