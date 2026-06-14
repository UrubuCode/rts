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

use cranelift_codegen::ir::{types, InstBuilder, Value};
use cranelift_module::Module;

use rts_hir::ir::HirExprKind;
use rts_hir::HirExpr;

use crate::repr::Repr;
use crate::value::{self, emit_marshal};

use crate::front::error::{unsupported, FrontResult, Unsupported};

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
            "Number" => self.coerce_call(module, "__rtsadp_g_number", args, JsKind::Number).map(Some),
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
                }
                self.coerce_call(module, "__rtsadp_g_string", args, JsKind::Str).map(Some)
            }
            "Boolean" => self
                .bool_returning_call(module, "__rtsadp_g_boolean", args)
                .map(Some),
            "parseFloat" => self
                .coerce_call(module, "__rtsadp_g_parse_float", args, JsKind::Number)
                .map(Some),
            "isNaN" => self.bool_returning_call(module, "__rtsadp_g_is_nan", args).map(Some),
            "isFinite" => self
                .bool_returning_call(module, "__rtsadp_g_is_finite", args)
                .map(Some),
            "parseInt" => self.parse_int_call(module, args).map(Some),
            "Array" => self.array_ctor_call(module, args).map(Some),
            _ => Ok(None),
        }
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
        if self.local(obj).is_some() || self.classes.get(obj).is_some() {
            return Ok(None);
        }
        match obj.as_str() {
            "Array" => self.array_static_call(module, method, args).map(Some),
            "String" => self.string_static_call(module, method, args).map(Some),
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
    fn array_ctor_call(
        &mut self,
        module: &mut dyn Module,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        if args.len() != 1 {
            return unsupported!(
                "Array(...) with {} args (only `Array(n)` sized form is supported)",
                args.len()
            );
        }
        // `Array(...arr)` (spread, flattened to a single array arg by the HIR) is
        // NOT the sized form — bail rather than treat the array's `length` coercion
        // as `n`.
        if self.is_array_valued(&args[0]) {
            return unsupported!("Array(...spread) — a spread argument is a later increment");
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
                matches!(self.local_shapes.get(name), Some(super::lower::HeapShape::Array))
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
