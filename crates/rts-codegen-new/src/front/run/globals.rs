//! GLOBAL constant-FUNCTION + Array/String STATIC call lowering (P5.2).
//!
//! Three call shapes the histogram flagged as "unknown function/global" (51 bails)
//! lower here, each to a codegen-owned `__rtsadp_*` trampoline (or, where a REAL
//! `__RTS_FN_*` already has a PolyValue-compatible ABI, that symbol):
//!
//! 1. `Number(x)` / `String(x)` / `Boolean(x)` / `parseInt(s[,radix])` /
//!    `parseFloat(s)` / `isNaN(x)` / `isFinite(x)` — `Call{ Ident("Number"/…) }`.
//! 2. `Array(n)` — `Call{ Ident("Array") }` (and `new Array(n)`, via `newexpr`).
//! 3. `Array.isArray(x)` / `Array.of(…)` / `Array.from(x)` and
//!    `String.fromCharCode(…)` / `String.fromCodePoint(…)` —
//!    `Call{ Member{ Ident("Array"/"String"), prop } }`.
//!
//! Everything else stays an explicit bail (a global with no value model here, an
//! unsupported `Array.from` source, etc.) — never a guess. Each helper returns
//! `Ok(Some(val))` on a handled call, `Ok(None)` when the callee is not one of
//! these globals (so the caller falls through to user-fn / method dispatch), or an
//! `Err(Unsupported)` for an explicit bail of a recognized-but-unsupported form.

use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;

use rts_hir::HirExpr;
use rts_hir::ir::HirExprKind;

use crate::repr::Repr;
use crate::value::{self, emit_marshal};

use crate::front::error::{FrontResult, Unsupported, unsupported};

use super::lower::{JsKind, Lowerer, Val};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Try to lower a `Call{ Ident(name) }` global FUNCTION (`Number`/`String`/
    /// `Boolean`/`parseInt`/`parseFloat`/`isNaN`/`isFinite`/`Array`). Returns
    /// `Ok(None)` when `name` is not a recognized global (the caller then tries
    /// user functions / value calls / bail). A local shadowing the name is the
    /// caller's concern — it checks `sigs`/`local` before reaching here only for
    /// USER calls; we additionally guard against a same-named local below.
    pub(super) fn try_global_fn_call(
        &mut self,
        module: &mut dyn Module,
        name: &str,
        args: &[HirExpr],
    ) -> FrontResult<Option<Val>> {
        // A local/user-fn of the same name shadows the global — let the caller
        // handle it (we only get here when neither matched, but stay defensive).
        if self.local(name).is_some() || self.sigs.contains_key(name) {
            return Ok(None);
        }
        match name {
            "Number" => {
                // `Number()` with NO arg is `+0` (JS spec).
                if args.is_empty() {
                    let zero = self.builder.ins().iconst(types::I64, 0);
                    return Ok(Some(Val::new(zero, Repr::Int64)));
                }
                // `Number(x)` (no `new`) → the prelude `.ts` `NumberFactory` (a
                // PRIMITIVE number result), so the `.ts` class owns BOTH JS forms
                // (this + `new Number(x)`). Falls back to the codegen coercion
                // trampoline only if the prelude factory is absent (defensive).
                if args.len() == 1 && self.sigs.contains_key("NumberFactory") {
                    let v = self.call_synth_fn(module, "NumberFactory", None, args)?;
                    return Ok(Some(v));
                }
                self.coerce_call(module, "__rtsadp_g_number", args, JsKind::Number)
                    .map(Some)
            }
            "String" => {
                // ToPrimitive (issue #304): `String(obj)` of a STATICALLY-KNOWN-CLASS
                // object calls its `toString`/`valueOf` (string hint), not the default
                // `[object Object]`. A plain object / array / dynamic receiver falls
                // through to the runtime `__rtsadp_g_string` (array-join / default).
                if args.len() == 1 {
                    if let Some(word) = self.coerce_object_to_string_word(
                        module,
                        &args[0],
                        super::toprimitive::Hint::String,
                    )? {
                        return Ok(Some(Val::tagged_kind(word, JsKind::Str)));
                    }
                    // Non-#304 case → the prelude `.ts` `StringFactory` (PRIMITIVE
                    // string via the engine's own ToString). Falls back to the
                    // codegen coercion trampoline only if the factory is absent.
                    if self.sigs.contains_key("StringFactory") {
                        let v = self.call_synth_fn(module, "StringFactory", None, args)?;
                        return Ok(Some(v));
                    }
                }
                self.coerce_call(module, "__rtsadp_g_string", args, JsKind::Str)
                    .map(Some)
            }
            "Boolean" => {
                // `Boolean(x)` (no `new`) → the prelude `.ts` `BooleanFactory` (a
                // PRIMITIVE boolean result), so the `.ts` class owns BOTH JS forms.
                // Falls back to the codegen ToBoolean trampoline only if the prelude
                // factory is absent (defensive).
                if args.len() == 1 && self.sigs.contains_key("BooleanFactory") {
                    let v = self.call_synth_fn(module, "BooleanFactory", None, args)?;
                    return Ok(Some(v));
                }
                self.bool_returning_call(module, "__rtsadp_g_boolean", args)
                    .map(Some)
            }
            "parseFloat" => self
                .coerce_call(module, "__rtsadp_g_parse_float", args, JsKind::Number)
                .map(Some),
            "isNaN" => self
                .bool_returning_call(module, "__rtsadp_g_is_nan", args)
                .map(Some),
            "isFinite" => self
                .bool_returning_call(module, "__rtsadp_g_is_finite", args)
                .map(Some),
            // `Symbol(desc?)` CALL form (no `new`) — Symbol is PRIMORDIAL; the
            // engine may name it. The call constructs a unique symbol (≡ the ctor),
            // returning an opaque handle (`===`/`!==` compare it by identity).
            "Symbol" => {
                let word = self.emit_registry_ctor(module, "Symbol", args)?;
                Ok(Some(Val::tagged_kind(word, JsKind::Object)))
            }
            "parseInt" => self.parse_int_call(module, args).map(Some),
            "Array" => self.array_ctor_call(module, args).map(Some),
            "Object" => {
                // `Object(x)` (no `new`) → the prelude `.ts` `ObjectFactory`
                // (null/undefined → a fresh object; an object passes through).
                // 0-arg `Object()` → `ObjectFactory()` (→ `{}`). `Ok(None)` if the
                // prelude factory is absent (defensive) — the caller bails.
                if args.len() <= 1 && self.sigs.contains_key("ObjectFactory") {
                    let v = self.call_synth_fn(module, "ObjectFactory", None, args)?;
                    return Ok(Some(v));
                }
                Ok(None)
            }
            // String→string global functions (one StrPtr arg → string Handle). The
            // URI codecs are UNIVERSAL (rts-shared `global_this`); btoa/atob are
            // backend (rts-std `text_encoding`). The arg is ToString-coerced at this
            // untrusted boundary.
            "encodeURIComponent" => self
                .lower_str_global(module, "__RTS_FN_GL_ENCODE_URI_COMPONENT", args)
                .map(Some),
            "decodeURIComponent" => self
                .lower_str_global(module, "__RTS_FN_GL_DECODE_URI_COMPONENT", args)
                .map(Some),
            "encodeURI" => self
                .lower_str_global(module, "__RTS_FN_GL_ENCODE_URI", args)
                .map(Some),
            "decodeURI" => self
                .lower_str_global(module, "__RTS_FN_GL_DECODE_URI", args)
                .map(Some),
            "btoa" => self
                .lower_str_global(module, "__RTS_FN_GL_TEXTENC_BTOA", args)
                .map(Some),
            "atob" => self
                .lower_str_global(module, "__RTS_FN_GL_TEXTENC_ATOB", args)
                .map(Some),
            _ => Ok(None),
        }
    }

    /// Lower a string→string GLOBAL function (`encodeURIComponent`/`btoa`/…): the
    /// single arg is ToString-coerced (untrusted boundary), marshaled to the
    /// extern's `StrPtr` (ptr+len) shape, and the returned string Handle is reboxed
    /// as a `TAG_STR` PolyValue. The symbol must be JIT-linked + have a `StrPtr →
    /// Handle` sig (see `abi_sig`/`runtime_link`), or it is a runtime SIGILL.
    fn lower_str_global(
        &mut self,
        module: &mut dyn Module,
        symbol: &'static str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        if args.len() != 1 {
            return unsupported!("{symbol} expects 1 arg, got {}", args.len());
        }
        let v = self.lower_expr(module, &args[0])?;
        // ToString-coerce at the boundary (a non-string arg → its JS string form);
        // a proven string rides its word directly.
        let s_word = if matches!(v.kind, JsKind::Str) {
            self.box_value(v)
        } else {
            let w = self.box_value(v);
            self.call_runtime(module, "__rtsadp_to_string", &[w])?
                .expect("__rtsadp_to_string returns a string word")
        };
        let handle = crate::value::emit_marshal::emit_table_load(module, self.builder, s_word);
        let (ptr, len) =
            crate::value::emit_marshal::emit_string_ptr_len(module, self.builder, handle);
        let res = self
            .call_runtime(module, symbol, &[ptr, len])?
            .expect("string→string global returns a string handle");
        let w = crate::value::emit_marshal::emit_box_real_string(module, self.builder, res);
        Ok(Val::tagged_kind(w, JsKind::Str))
    }

    /// Try to lower a STATIC member call `Array.m(args)` / `String.m(args)` (the
    /// callee is `Member{ Ident("Array"/"String"), prop }`). Returns `Ok(None)`
    /// when the object is not the `Array`/`String` global (so the caller falls
    /// through). A user class/local named `Array`/`String` shadows the global.
    pub(super) fn try_global_static_call(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Option<Val>> {
        let HirExprKind::Ident(obj) = &object.kind else {
            return Ok(None);
        };
        // A user LOCAL named `Array`/`String` shadows the global. A prelude
        // wrapper-primordial CLASS (`class String`) does NOT: its `.ts` carries
        // INSTANCE methods only — the STATIC surface (`String.fromCharCode`, …)
        // stays on this global path (mirrors `is_wrapper_primordial_static` /
        // `globalclass::is_wrapper_primordial`). So only a non-wrapper class shadow
        // bails here.
        if self.local(obj).is_some()
            || (self.classes.get(obj).is_some() && !is_global_static_class(obj))
        {
            return Ok(None);
        }
        match obj.as_str() {
            "Array" => self.array_static_call(module, method, args).map(Some),
            "String" => self.string_static_call(module, method, args).map(Some),
            // `Promise` is PRIMORDIAL — the engine may name it. Its statics
            // (`resolve`/`reject`/`all`/…) are registry-backed metadata, so route
            // through the SAME generic static-resolution machinery as Date.
            "Promise" => self
                .resolve_static_via_registry(module, "Promise", method, args)
                .map(Some),
            _ => Ok(None),
        }
    }

    // ---- global coercion / predicate calls (one arg in, PolyValue out) ----

    /// `Number(x)`/`String(x)`/`parseFloat(x)` — one arg, box it, call the
    /// trampoline, tag the result kind. Zero args → `undefined`-ish: bail (JS
    /// `Number()` is 0, `String()` is "", but those are corner cases — explicit).
    fn coerce_call(
        &mut self,
        module: &mut dyn Module,
        symbol: &str,
        args: &[HirExpr],
        kind: JsKind,
    ) -> FrontResult<Val> {
        let boxed = self.single_boxed_arg(module, symbol, args)?;
        let res = self
            .call_runtime(module, symbol, &[boxed])?
            .expect("global coercion returns a value");
        Ok(match kind {
            JsKind::Number => Val::new(res, Repr::Tagged),
            other => Val::tagged_kind(res, other),
        })
    }

    /// `Boolean(x)`/`isNaN(x)`/`isFinite(x)` — one arg → a PolyValue bool word.
    /// The result is Tagged (a boolean singleton word), kind Bool.
    fn bool_returning_call(
        &mut self,
        module: &mut dyn Module,
        symbol: &str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        let boxed = self.single_boxed_arg(module, symbol, args)?;
        let res = self
            .call_runtime(module, symbol, &[boxed])?
            .expect("global predicate returns a value");
        // The trampoline returns a PolyValue bool WORD (Tagged), kind Bool.
        Ok(Val::tagged_kind(res, JsKind::Bool))
    }

    /// `parseInt(s[, radix])` — box the value; the radix is a SECOND optional arg
    /// boxed as a PolyValue word too (the trampoline ToNumbers it). Missing radix →
    /// `0` (auto). Result kind Number (Tagged).
    pub(super) fn parse_int_call(
        &mut self,
        module: &mut dyn Module,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        if args.is_empty() || args.len() > 2 {
            return unsupported!("parseInt expects 1 or 2 args, got {}", args.len());
        }
        let v = self.lower_expr(module, &args[0])?;
        let value_word = self.box_value(v);
        let radix_word = if args.len() == 2 {
            let r = self.lower_expr(module, &args[1])?;
            self.box_value(r)
        } else {
            self.builder
                .ins()
                .iconst(types::I64, value::PolyValue::from_i32(0).raw() as i64)
        };
        let res = self
            .call_runtime(module, "__rtsadp_g_parse_int", &[value_word, radix_word])?
            .expect("parseInt returns a value");
        Ok(Val::new(res, Repr::Tagged))
    }

    /// `Array(n)` — a fresh array of length `n` (holes → `undefined`). Only the
    /// single-numeric-arg form is supported; `Array(a, b, …)` (an element list) is
    /// `Array.of`-shaped and BAILS here (a later increment). Result kind Array.
    fn array_ctor_call(&mut self, module: &mut dyn Module, args: &[HirExpr]) -> FrontResult<Val> {
        // A single SPREAD of a LITERAL array (`new Array(...[1,2,3])`) is
        // statically the argument list `[1,2,3]` — re-dispatch on those real args
        // so JS's own arity rule applies: `Array(...[10])` becomes `Array(10)` (a
        // SIZED array of 10 holes, NOT `[10]`), `Array(...[1,2,3])` an element
        // list, `Array(...[])` empty. Only a literal source can be expanded at
        // lowering time; a runtime spread (`Array(...someVar)`) stays a bail.
        if args.len() == 1 {
            if let HirExprKind::Spread(inner) = &args[0].kind {
                if let HirExprKind::Array(elems) = &inner.kind {
                    // A hole inside the literal (`[, 1]`) would change the argument
                    // count in a way the flat element-list re-dispatch cannot model
                    // (`Array(undefined, 1)`); only expand a hole-free literal.
                    if elems.iter().all(|e| !matches!(e.kind, HirExprKind::Spread(_))) {
                        let expanded = elems.clone();
                        return self.array_ctor_call(module, &expanded);
                    }
                }
            }
        }
        // The ELEMENT-LIST form (lower exactly like the array literal `[a, b, …]`):
        // 0 or 2+ args (JS: never a size); any remaining non-literal SPREAD; or a
        // single bare array value `Array(arr)` (the JS single-element form `[arr]`,
        // NOT a size). Only a single NON-array, NON-spread scalar falls through to
        // the sized `Array(n)` path below.
        let has_spread = args.iter().any(|a| matches!(a.kind, HirExprKind::Spread(_)));
        if has_spread || args.len() != 1 || self.is_array_valued(&args[0]) {
            let arr_expr =
                HirExpr::new(HirExprKind::Array(args.to_vec()), rts_hir::HirType::Unknown);
            return self.lower_expr(module, &arr_expr);
        }
        let n = self.lower_expr(module, &args[0])?;
        let n_word = self.box_value(n);
        let res = self
            .call_runtime(module, "__rtsadp_arr_new_sized", &[n_word])?
            .expect("Array(n) returns a value");
        Ok(Val::tagged_kind(res, JsKind::Array))
    }

    /// Whether `class` names the built-in `Array` constructor (not shadowed by a
    /// user class of the same name) — so `new Array(n)` routes to the array path.
    pub(super) fn is_builtin_array_ctor(&self, class: &str) -> bool {
        class == "Array" && self.classes.get(class).is_none()
    }

    /// Lower `new Array(n)` to a sized array (holes → `undefined`), returned as a
    /// `JsKind::Array` value (so a `let` records `HeapShape::Array`). Only the
    /// single-numeric-arg form is supported; an element list bails.
    pub(super) fn lower_new_array(
        &mut self,
        module: &mut dyn Module,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        self.array_ctor_call(module, args)
    }

    // ---- Array statics: isArray / of / from ----

    fn array_static_call(
        &mut self,
        module: &mut dyn Module,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        match (method, args.len()) {
            ("isArray", 1) => {
                let v = self.lower_expr(module, &args[0])?;
                let boxed = self.box_value(v);
                let res = self
                    .call_runtime(module, "__rtsadp_arr_is_array", &[boxed])?
                    .expect("Array.isArray returns a value");
                Ok(Val::tagged_kind(res, JsKind::Bool))
            }
            ("of", _) => {
                // Build a fresh array from the args (each boxed as a PolyValue word).
                // A spread arg (`Array.of(...xs)`, flattened to a bare array) would
                // wrongly nest as a single element — bail.
                if args.iter().any(|a| self.is_array_valued(a)) && args.len() == 1 {
                    return unsupported!(
                        "Array.of(...spread) — a spread argument is a later increment"
                    );
                }
                let arr = emit_marshal::emit_new_vec_object(module, self.builder);
                for a in args {
                    let v = self.lower_expr(module, a)?;
                    let word = self.box_value(v);
                    emit_marshal::emit_vec_push(module, self.builder, arr, word);
                }
                Ok(Val::tagged_kind(arr, JsKind::Array))
            }
            ("from", 1) => {
                // An ITERABLE-CLASS source (`Array.from(new Set(..))` — any class
                // with a `[Symbol.iterator]` method) collects through the SAME
                // hook the spread uses; the runtime trampoline below only covers
                // strings/arrays/array-likes.
                if let Some(w) = self.try_class_iterator_source_word(module, &args[0])? {
                    return Ok(Val::tagged_kind(w, JsKind::Array));
                }
                // A LAZY generator source (`Array.from(fibonacci())` / a gen
                // local): DRAIN via the same hook for-of uses.
                if let Some(w) = self.try_lazy_gen_source_word(module, &args[0])? {
                    return Ok(Val::tagged_kind(w, JsKind::Array));
                }
                let v = self.lower_expr(module, &args[0])?;
                let boxed = self.box_value(v);
                let res = self
                    .call_runtime(module, "__rtsadp_arr_from", &[boxed])?
                    .expect("Array.from returns a value");
                // The trampoline returns a sentinel for unsupported sources (Map/
                // Set/iterator); guard it at RUNTIME via a tag-check would need a
                // branch — instead we accept the array word and rely on the
                // trampoline's sentinel never being a valid array (an empty
                // singleton). A Map/Set source therefore yields a non-array word; a
                // later `.length`/`.join` on it would bail. To stay honest we keep
                // the result kind Array (the supported string/array cases) — the
                // sentinel path is unreachable from the proven-array/string corpus.
                Ok(Val::tagged_kind(res, JsKind::Array))
            }
            ("from", 2) => {
                // `Array.from(source, mapFn)` == `Array.from(source).map(mapFn)`:
                // build the array from the source, then map each element through the
                // callback `(elem, index)`. The mapper is reified to a TAG_FUNCTION
                // word (same path as the array callback methods); a capturing arrow
                // bails there.
                //
                // An ITERABLE-CLASS source (`Array.from(new Set(..), mapFn)`)
                // collects through the same hook the 1-arg form / spread use,
                // then maps.
                if let Some(w) = self.try_class_iterator_source_word(module, &args[0])? {
                    let cb_word = self.reify_callback_arg(module, &args[1], "from")?;
                    let handle = emit_marshal::emit_table_load(module, self.builder, w);
                    let mapped = self
                        .call_runtime(module, "__rtsadp_arr_map", &[handle, cb_word])?
                        .expect("__rtsadp_arr_map returns an array word");
                    return Ok(Val::tagged_kind(mapped, JsKind::Array));
                }
                // `__rtsadp_arr_from` builds from an ARRAY, a STRING, or an ARRAY-LIKE
                // keyed object with a numeric `length` (`{ length: n }`); a keyed
                // object WITHOUT a numeric length (Map/Set/plain) returns the
                // FROM_UNSUPPORTED sentinel runtime-side. Allow array/string/object
                // sources through; reject only a proven primitive (number/bool) source
                // up front (it can never be array-like).
                let v = self.lower_expr(module, &args[0])?;
                // Reject only a PROVEN primitive source (number/bool/undefined/
                // null — never array-like). A Tagged/Unknown source (a
                // `matchAll` result, a call return) goes through: the runtime
                // `__rtsadp_arr_from` tag-dispatches (array/string/view/
                // array-like) and yields the honest FROM_UNSUPPORTED sentinel
                // for anything it cannot build from.
                let proven_primitive = matches!(
                    v.kind,
                    super::lower::JsKind::Number
                        | super::lower::JsKind::Bool
                        | super::lower::JsKind::Undefined
                        | super::lower::JsKind::Null
                ) || matches!(
                    v.repr,
                    crate::repr::Repr::Int32
                        | crate::repr::Repr::Int64
                        | crate::repr::Repr::Float64
                        | crate::repr::Repr::Bool
                );
                if proven_primitive {
                    return unsupported!(
                        "Array.from(source, mapFn) with a primitive source \
                         (a primitive source is not array-like)"
                    );
                }
                let src_boxed = self.box_value(v);
                let arr_word = self
                    .call_runtime(module, "__rtsadp_arr_from", &[src_boxed])?
                    .expect("Array.from returns an array word");
                let cb_word = self.reify_callback_arg(module, &args[1], "from")?;
                let handle = emit_marshal::emit_table_load(module, self.builder, arr_word);
                let mapped = self
                    .call_runtime(module, "__rtsadp_arr_map", &[handle, cb_word])?
                    .expect("__rtsadp_arr_map returns an array word");
                Ok(Val::tagged_kind(mapped, JsKind::Array))
            }
            ("from", 3) => {
                // `Array.from(source, mapFn, thisArg)` — bind the mapper to
                // `thisArg` (`__rtsadp_fn_bind` with no pre-bound args; a
                // this-first fn receives it as its `this` slot), then run the
                // 2-arg path over the bound function.
                let is_arr = self.is_array_valued(&args[0]);
                let v = self.lower_expr(module, &args[0])?;
                let is_objlike = matches!(v.kind, super::lower::JsKind::Object)
                    || matches!(&args[0].kind, HirExprKind::Object(_));
                if !is_arr && !matches!(v.kind, super::lower::JsKind::Str) && !is_objlike {
                    return unsupported!(
                        "Array.from(source, mapFn, thisArg) with a non-array/non-string/\
                         non-object source (a primitive source is not array-like)"
                    );
                }
                let src_boxed = self.box_value(v);
                let arr_word = self
                    .call_runtime(module, "__rtsadp_arr_from", &[src_boxed])?
                    .expect("Array.from returns an array word");
                let cb_word = self.reify_callback_arg(module, &args[1], "from")?;
                let this_v = self.lower_expr(module, &args[2])?;
                let this_word = self.box_value(this_v);
                let empty_args = emit_marshal::emit_new_vec_object(module, self.builder);
                let empty_handle =
                    emit_marshal::emit_table_load(module, self.builder, empty_args);
                let bound = self
                    .call_runtime(
                        module,
                        "__rtsadp_fn_bind",
                        &[cb_word, this_word, empty_handle],
                    )?
                    .expect("__rtsadp_fn_bind returns a function word");
                let handle = emit_marshal::emit_table_load(module, self.builder, arr_word);
                let mapped = self
                    .call_runtime(module, "__rtsadp_arr_map", &[handle, bound])?
                    .expect("__rtsadp_arr_map returns an array word");
                Ok(Val::tagged_kind(mapped, JsKind::Array))
            }
            (m, n) => unsupported!("Array.{m}({n} args) static method (later increment)"),
        }
    }

    // ---- String statics: fromCharCode / fromCodePoint ----

    fn string_static_call(
        &mut self,
        module: &mut dyn Module,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        // `String.raw(callSite, ...subs)` — the FUNCTION-call form (the tag
        // form `String.raw\`…\`` is desugared at compile time): pack the
        // substitutions into one array word and interleave at runtime.
        if method == "raw" {
            if args.is_empty() {
                return unsupported!("String.raw() requires a call-site object");
            }
            let cs = self.lower_expr(module, &args[0])?;
            let cs_w = self.box_value(cs);
            let subs = emit_marshal::emit_new_vec_object(module, self.builder);
            for a in &args[1..] {
                let v = self.lower_expr(module, a)?;
                let w = self.box_value(v);
                emit_marshal::emit_vec_push(module, self.builder, subs, w);
            }
            let res = self
                .call_runtime(module, "__rtsadp_string_raw", &[cs_w, subs])?
                .expect("__rtsadp_string_raw returns a string word");
            return Ok(Val::tagged_kind(res, JsKind::Str));
        }
        let symbol = match method {
            "fromCharCode" => "__rtsadp_str_from_char_code",
            "fromCodePoint" => "__rtsadp_str_from_code_point",
            other => {
                return unsupported!("String.{other}(...) static method (later increment)");
            }
        };
        if args.is_empty() {
            // `String.fromCharCode()` → "" (the empty string).
            let pv = crate::value::abi_adapter::intern_poly("");
            let v = self.builder.ins().iconst(types::I64, pv.raw() as i64);
            return Ok(Val::tagged_kind(v, JsKind::Str));
        }
        // `String.fromCharCode(...codes)` (P5.6): a SINGLE spread of a proven array
        // → fold `fromCharCode` over the array at runtime via the `_arr` trampoline.
        // (fromCodePoint spread is a later increment — `_arr` is char-code only.)
        if method == "fromCharCode" && args.len() == 1 {
            if let HirExprKind::Spread(inner) = &args[0].kind {
                if !self.is_array_valued(inner) {
                    return unsupported!(
                        "String.fromCharCode(...spread) of a non-array value (later increment)"
                    );
                }
                let src = self.lower_expr(module, inner)?;
                let arr_word = self.box_value(src);
                let res = self
                    .call_runtime(module, "__rtsadp_str_from_char_code_arr", &[arr_word])?
                    .expect("__rtsadp_str_from_char_code_arr returns a value");
                return Ok(Val::tagged_kind(res, JsKind::Str));
            }
        }
        // Each code → a one-char string; concatenate via the generic `+`
        // (real STRING_CONCAT), so the variadic form falls out of the monadic
        // primitive — exactly the documented `globalops` design. A SPREAD arg
        // (`fromCharCode(...codes)`) is lost by the HIR (call args drop the spread
        // flag), surfacing as a single ARRAY arg here — bail rather than coerce an
        // array to a bogus char code (the honesty floor).
        let mut acc: Option<Value> = None;
        for a in args {
            if self.is_array_valued(a) {
                return unsupported!(
                    "String.{method}(...spread) — a spread argument is a later increment"
                );
            }
            let v = self.lower_expr(module, a)?;
            let code = self.box_value(v);
            let piece = self
                .call_runtime(module, symbol, &[code])?
                .expect("String.fromCharCode piece returns a value");
            acc = Some(match acc {
                None => piece,
                Some(prev) => self
                    .call_runtime(module, "__rtsadp_add", &[prev, piece])?
                    .expect("__rtsadp_add returns a value"),
            });
        }
        Ok(Val::tagged_kind(acc.expect("non-empty args"), JsKind::Str))
    }

    // ---- shared helpers ----

    /// Whether `e` statically denotes an ARRAY value (an array literal, an
    /// array-shaped local, or an array-returning method chain). Used to REFUSE a
    /// spread argument the HIR has flattened to a bare array (call args lose their
    /// spread flag) where a scalar is required — never coerce an array to a bogus
    /// scalar (the honesty floor).
    pub(super) fn is_array_valued(&self, e: &HirExpr) -> bool {
        match &e.kind {
            HirExprKind::Array(_) => true,
            HirExprKind::Ident(name) => {
                matches!(
                    self.local_shapes.get(name),
                    Some(super::lower::HeapShape::Array)
                )
            }
            // A CALL of a user function whose declared return type is an array
            // (`function ks(): string[] {…}` → `for (const k of ks())`).
            HirExprKind::Call { callee, .. } => match &callee.kind {
                HirExprKind::Ident(f) => self.sigs.get(f).is_some_and(|s| s.ret_array),
                _ => false,
            },
            // A METHOD CALL whose receiver class is known and whose method's declared
            // return type is an array (`m.values()`/`o.keys()`/`o.entries()`).
            HirExprKind::MethodCall { object, method, .. } => {
                self.static_instance_class(object)
                    .and_then(|c| {
                        self.classes
                            .get(&c)
                            .and_then(|d| d.methods.get(method).cloned())
                    })
                    .and_then(|fname| self.sigs.get(&fname))
                    .is_some_and(|s| s.ret_array)
                    || self.is_array_receiver(e)
            }
            _ => self.is_array_receiver(e),
        }
    }

    /// Lower exactly one arg and box it for a single-arg global. A wrong arity is
    /// an explicit bail (the JS zero-arg defaults are corner cases handled by the
    /// caller where it matters).
    fn single_boxed_arg(
        &mut self,
        module: &mut dyn Module,
        name: &str,
        args: &[HirExpr],
    ) -> FrontResult<Value> {
        if args.len() != 1 {
            return Err(Unsupported::new(format!(
                "global `{name}` expects 1 arg, got {}",
                args.len()
            )));
        }
        let v = self.lower_expr(module, &args[0])?;
        Ok(self.box_value(v))
    }
}

/// Whether `name` is a wrapper-primordial whose STATIC surface stays on the global
/// path even though a prelude `.ts` class of the same name exists (it carries
/// instance methods only). Mirrors `mathobj::is_wrapper_primordial_static` and
/// `globalclass::is_wrapper_primordial`. Only `String` (and `Array`, which has no
/// prelude class today) reaches `try_global_static_call`'s class-shadow gate.
fn is_global_static_class(name: &str) -> bool {
    matches!(name, "String" | "Array")
}
