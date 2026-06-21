//! Array instance-method dispatch lowering (P4.5 + P4.7 + P5.2) — split out of
//! [`super::method`] to keep both files under the 500-line module rule.
//!
//! Covers the proven-array receiver path: the non-callback `__rtsadp_arr_*`
//! trampolines (indexOf/includes/at/join/push/pop/slice/reverse/fill/concat/flat/
//! lastIndexOf/shift/unshift) and the callback methods (map/filter/forEach/find/
//! findIndex/some/every/reduce). The element convention is the engine's own
//! (boxed PolyValue words), so these resolve to codegen-owned trampolines, never
//! the runtime's raw-i64 Array methods.

use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;

use rts_hir::HirExpr;
use rts_hir::ir::HirExprKind;

use rts_runtime::abi::AbiType;

use crate::dispatch::{MethodSpec, RecvAbi, RecvClass, resolve_method};
use crate::repr::Repr;
use crate::value::{self, emit_marshal};

use crate::front::error::{FrontResult, unsupported};

use super::lower::{HeapShape, JsKind, Lowerer, Val};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Whether `object` is statically a proven ARRAY dispatch receiver:
    /// - a bare identifier bound to a local recorded as [`HeapShape::Array`]
    ///   (P4.5), or
    /// - an array LITERAL (`[1,2,3].map(..)`), or
    /// - a CHAINED array-returning method call (`a.filter(..).map(..)`,
    ///   `a.slice(..).map(..)`): the inner `MethodCall`/`Call` resolves to an
    ///   Array method whose return is itself an array (`map`/`filter`/`slice`).
    ///
    /// Everything else (an object, a param, a non-array call result) falls through
    /// to the non-array path.
    pub(super) fn is_array_receiver(&self, object: &HirExpr) -> bool {
        match &object.kind {
            HirExprKind::Ident(name) => {
                matches!(self.local_shapes.get(name), Some(HeapShape::Array))
            }
            // A class FIELD proven to hold an array (`this.items.push(x)`): the
            // receiver's class proves the field is array-typed.
            HirExprKind::Member {
                object: inner,
                prop,
            } => self
                .static_instance_class(inner)
                .and_then(|c| self.classes.get(&c).map(|d| d.field_is_array(prop)))
                .unwrap_or(false),
            HirExprKind::Array(_) => true,
            // `new Array(n)` chained (`new Array(3).fill(0)`).
            HirExprKind::New { class, .. } => self.is_builtin_array_ctor(class),
            // A chained instance method whose result is an array
            // (`a.filter(..).map(..)`, `"x".split(",").map(..)`).
            HirExprKind::MethodCall {
                object: inner,
                method,
                ..
            } => is_array_returning_method(method) || self.is_array_static_or_split(inner, method),
            // A chained call result: either an array-returning instance method
            // lowered as `Call(Member)`, or an Array/String static / global that
            // yields an array (`Array.of(..).join(..)`, `Array.from(..).join(..)`,
            // `Array(n).fill(..)`).
            HirExprKind::Call { callee, .. } => match &callee.kind {
                HirExprKind::Member {
                    object: inner,
                    prop,
                } => is_array_returning_method(prop) || self.is_array_static_or_split(inner, prop),
                HirExprKind::Ident(name) => self.is_builtin_array_ctor(name),
                _ => false,
            },
            _ => false,
        }
    }

    /// Whether a `obj.method(..)` call (with `obj` the inner receiver expr and
    /// `method` the called member) statically produces an ARRAY value: an `Array`
    /// static that builds one (`Array.of`/`Array.from`), or `str.split(..)` (whose
    /// result is the engine's PolyValue-word array). `obj` is the static `Array`
    /// global for the statics; for split we accept any receiver (the split lowering
    /// itself bails a non-string receiver).
    fn is_array_static_or_split(&self, inner: &HirExpr, method: &str) -> bool {
        if method == "split" {
            return true;
        }
        if matches!(method, "of" | "from")
            && matches!(&inner.kind, HirExprKind::Ident(n)
                if n == "Array" && self.classes.get(n).is_none())
        {
            return true;
        }
        // `Object.keys/values/entries/getOwnPropertyNames(o)` produce an engine
        // array word (P5.4), so a chained `.join`/`.length`/`[i]`/array method on
        // the result dispatches as an array.
        matches!(
            method,
            "keys" | "values" | "entries" | "getOwnPropertyNames"
        ) && matches!(&inner.kind, HirExprKind::Ident(n)
                if n == "Object" && self.local(n).is_none() && self.classes.get(n).is_none())
    }

    /// Lower `arr.method(args)` for a proven-array receiver via the codegen-owned
    /// `__rtsadp_arr_*` trampolines. The receiver marshals as the array's REAL Vec
    /// handle (`POLY_TO_HANDLE`); args/return follow the Array row's `AbiType`s
    /// (`U64` = raw PolyValue word, `I64` = index, `Handle` = string sep). An
    /// unknown `(method, arity)` BAILS explicitly (never a guess).
    pub(super) fn try_array_dispatch(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        // `[obj].join(..)` ToStrings each element in the runtime trampoline, which
        // renders a method-bearing OBJECT element as the default `[object Object]`
        // (it cannot call the JIT'd `toString`) — a wrong value vs bun's element
        // ToPrimitive. Bail rather than diverge (array-of-object ToPrimitive is a
        // later increment). Only the array-LITERAL-with-object case is statically
        // detectable here; an opaque array's elements can only become objects via
        // paths that already bail.
        if method == "join" && self.array_arg_has_object_element(object) {
            return Err(crate::front::error::Unsupported::new(
                "Array.join on an array containing an object element \
                 (element ToPrimitive is a later increment)"
                    .to_string(),
            ));
        }

        let argc = args.len();

        // VARIADIC `push`/`unshift` (N args): the runtime trampolines take ONE value;
        // a multi-arg call folds into N single-arg calls in `.ts`-equivalent order.
        // `push(a, b)` appends a then b (left-to-right). `unshift(a, b)` must yield
        // `[a, b, ...orig]`, so single unshifts run RIGHT-TO-LEFT (unshift b, then a).
        // The result is the final length (push/unshift both return the new length).
        if matches!(method, "push" | "unshift") && argc > 1 {
            return self.array_variadic_insert(module, object, method, args);
        }

        // VARIADIC `concat(x, y, …)`: `__rtsadp_arr_concat` concatenates ONE operand
        // (spreading an array arg, appending a non-array arg as one element) and
        // returns a NEW array. Chain it over the args: acc = concat(acc, arg). 0/1
        // args use the existing rows below.
        if method == "concat" && argc > 1 {
            return self.array_concat_variadic(module, object, args);
        }

        let Some(spec) = resolve_method(RecvClass::Array, method, argc) else {
            return Err(crate::front::error::Unsupported::new(format!(
                "no Registry entry for `Array.{method}({argc} args)` \
                 (callback methods like .map/.filter/.reduce bail by design)"
            )));
        };
        debug_assert!(matches!(spec.recv_abi, RecvAbi::ArrayVec));

        // Callback methods (P4.7) take a dedicated marshaling path.
        if let Some(cb) = spec.cb {
            return self.try_array_callback(module, object, method, args, &spec, cb);
        }

        let mut call_args: Vec<Value> = Vec::with_capacity(argc + 1);

        // ---- receiver (slot 0): the array word → its real Vec handle ----
        let arr = self.lower_expr(module, object)?;
        let arr_word = self.box_value(arr);
        let handle = emit_marshal::emit_table_load(module, self.builder, arr_word);
        call_args.push(handle);

        // ---- explicit args ----
        for (a, &want) in args.iter().zip(spec.args) {
            let v = self.lower_expr(module, a)?;
            call_args.push(self.marshal_array_arg(module, v, want)?);
        }

        // ---- emit + marshal the result ----
        let ret = emit_marshal::emit_call(module, self.builder, spec.symbol, &call_args);
        self.marshal_array_ret(module, method, spec.ret, ret)
    }

    /// Variadic `push(a, b, …)` / `unshift(a, b, …)` as N single-element runtime
    /// calls (the trampolines are 1-element). Each arg is boxed to a PolyValue word
    /// (the array element convention). `push` folds left-to-right; `unshift` folds
    /// RIGHT-to-left so `unshift(a, b)` yields `[a, b, …orig]`. Returns the final
    /// length (an `I64` from the last call) as a proven `Int64`.
    fn array_variadic_insert(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        let sym = if method == "push" {
            "__rtsadp_arr_push"
        } else {
            "__rtsadp_arr_unshift"
        };
        // Lower the receiver ONCE → its real Vec handle (re-loaded per call so each
        // mutation sees the same array; the handle is stable for the same vec word).
        let arr = self.lower_expr(module, object)?;
        let arr_word = self.box_value(arr);
        // Box each arg first (left-to-right evaluation, JS-correct), then apply in
        // the method's fold order.
        let mut words: Vec<Value> = Vec::with_capacity(args.len());
        for a in args {
            let v = self.lower_expr(module, a)?;
            words.push(self.box_value(v));
        }
        if method == "unshift" {
            words.reverse();
        }
        let mut last: Option<Value> = None;
        for w in words {
            let handle = emit_marshal::emit_table_load(module, self.builder, arr_word);
            last = emit_marshal::emit_call(module, self.builder, sym, &[handle, w]);
        }
        // Both return the new length (I64). The last call's result is the final length.
        let len = last.expect("push/unshift returns a length");
        Ok(Val::new(len, Repr::Int64))
    }

    /// Variadic `arr.concat(x, y, …)` — fold `__rtsadp_arr_concat` over the args,
    /// chaining the (NEW array) result of each step as the receiver of the next:
    /// `concat(concat(arr, x), y)`. Each step's `acc` is a TAG_OBJECT array word; it
    /// is table-loaded to a Vec handle for the next call. The args themselves stay
    /// boxed PolyValue words (the trampoline spreads an array arg, appends a scalar).
    fn array_concat_variadic(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        // acc starts as the receiver array word.
        let arr = self.lower_expr(module, object)?;
        let mut acc_word = self.box_value(arr);
        for a in args {
            let v = self.lower_expr(module, a)?;
            let arg_word = self.box_value(v);
            let handle = emit_marshal::emit_table_load(module, self.builder, acc_word);
            acc_word = emit_marshal::emit_call(
                module,
                self.builder,
                "__rtsadp_arr_concat",
                &[handle, arg_word],
            )
            .expect("__rtsadp_arr_concat returns an array word");
        }
        // The final accumulator is a TAG_OBJECT array word.
        Ok(Val::tagged_kind(acc_word, super::lower::JsKind::Array))
    }

    /// Lower an Array CALLBACK method (`map`/`filter`/`forEach`/`find`/`findIndex`/
    /// `some`/`every`/`reduce`) — P4.7. Marshals the receiver as the array's real
    /// Vec handle and the callback argument as a `TAG_FUNCTION` PolyValue word
    /// (reified via the P4.6 path), then calls the codegen-owned `__rtsadp_arr_*`
    /// trampoline that invokes the callback per element.
    ///
    /// BAILS explicitly (never a wrong value) when the callback argument is not a
    /// reifiable function value: a capturing arrow (left as an `Arrow` node by the
    /// P4.6 extraction), a `thisArg` second positional (find/some/… 2nd arg), or
    /// anything that is not an arrow/function-ident.
    fn try_array_callback(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        method: &str,
        args: &[HirExpr],
        spec: &MethodSpec,
        cb: crate::dispatch::CbShape,
    ) -> FrontResult<Val> {
        // ---- receiver (slot 0): the array word → its real Vec handle ----
        let arr = self.lower_expr(module, object)?;
        let arr_word = self.box_value(arr);
        self.emit_array_callback(module, arr_word, method, args, spec, cb)
    }

    /// Lower an Array CALLBACK method on an UNPROVEN receiver (P5.9 extension): the
    /// receiver is a Tagged value of unproven class — an `any`-typed param, a call
    /// return, or a re-`let` local — already lowered to `recv`. When `method` is an
    /// array callback method (`map`/`filter`/`reduce`/…) and the callback argument
    /// is a reifiable function value, dispatch through the SAME codegen-owned
    /// `__rtsadp_arr_*` trampolines as the proven path. This is SAFE on a non-array
    /// receiver: the trampolines reduce the word to a real handle via a HandleTable
    /// LOOKUP (never a raw deref) and `with_vec` yields the default (length 0) for a
    /// non-`Entry::Vec` handle — map/filter→empty, forEach/find→no-op, reduce→init
    /// (the honest no-throw answer for a TypeError-class call), never a fault.
    ///
    /// Returns `Ok(None)` when `method` is not an array callback method or the
    /// callback is not reifiable (a capturing closure → #195) so the caller bails.
    pub(super) fn try_array_callback_dynamic(
        &mut self,
        module: &mut dyn Module,
        recv: Val,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Option<Val>> {
        let argc = args.len();
        let Some(spec) = resolve_method(RecvClass::Array, method, argc) else {
            return Ok(None);
        };
        let Some(cb) = spec.cb else {
            // A non-callback Array method on an unproven receiver is handled by the
            // `__rtsadp_dyn_*` path, not here.
            return Ok(None);
        };
        // The callback must be a reifiable function value (a synthesized non-
        // capturing arrow ident, or a function ident). A capturing arrow stays an
        // `Arrow` node — decline so the caller bails with the closure message
        // (#195), never a wrong value.
        if !matches!(&args[0].kind,
            HirExprKind::Ident(name) if self.sigs.contains_key(name))
        {
            return Ok(None);
        }
        let arr_word = self.box_value(recv);
        self.emit_array_callback(module, arr_word, method, args, &spec, cb)
            .map(Some)
    }

    /// Shared core of an Array CALLBACK lowering, taking the receiver ALREADY
    /// boxed to a PolyValue word (`arr_word`). Used by both the proven-array path
    /// ([`Self::try_array_callback`]) and the DYNAMIC path
    /// ([`Self::try_array_callback_dynamic`], an unproven receiver). The word is
    /// reduced to its real Vec handle via `POLY_TO_HANDLE` — a HandleTable LOOKUP,
    /// never a raw deref — so a non-array word resolves to a handle whose entry is
    /// not an `Entry::Vec`; the `__rtsadp_arr_*` trampolines then see length 0 (the
    /// `with_vec` lookup yields the default), i.e. map/filter→empty array,
    /// forEach/find→no-op, reduce→init/undefined. No fault, no wrong value on a
    /// non-array receiver (the honest no-throw answer for a TypeError-class call).
    #[allow(clippy::too_many_arguments)]
    fn emit_array_callback(
        &mut self,
        module: &mut dyn Module,
        arr_word: Value,
        method: &str,
        args: &[HirExpr],
        spec: &MethodSpec,
        cb: crate::dispatch::CbShape,
    ) -> FrontResult<Val> {
        use crate::dispatch::CbShape;

        // The callback is always the FIRST argument. A `thisArg` (a SECOND
        // positional on the predicate family) is a later increment — bail.
        if matches!(cb, CbShape::Predicate) && args.len() != 1 {
            return Err(crate::front::error::Unsupported::new(format!(
                "Array.{method}() with a thisArg / extra argument is a later increment"
            )));
        }

        let handle = emit_marshal::emit_table_load(module, self.builder, arr_word);

        // ---- callback (slot 1): reify the first arg to a TAG_FUNCTION word ----
        let cb_word = self.reify_callback_arg(module, &args[0], method)?;

        let mut call_args: Vec<Value> = vec![handle, cb_word];

        // ---- reduce's optional initial accumulator + has_init flag ----
        if matches!(cb, CbShape::Reduce) {
            let (init_word, has_init) = if args.len() >= 2 {
                let v = self.lower_expr(module, &args[1])?;
                (self.box_value(v), 1i64)
            } else {
                let undef = self
                    .builder
                    .ins()
                    .iconst(types::I64, value::PolyValue::undefined().raw() as i64);
                (undef, 0i64)
            };
            let has = self.builder.ins().iconst(types::I64, has_init);
            call_args.push(init_word);
            call_args.push(has);
        }

        // ---- emit + marshal the result ----
        let ret = emit_marshal::emit_call(module, self.builder, spec.symbol, &call_args);
        self.marshal_array_ret(module, method, spec.ret, ret)
    }

    /// Reify the callback argument `a` of an Array callback method to a
    /// `TAG_FUNCTION` PolyValue word. After the P4.6 arrow-extraction pre-pass a
    /// non-capturing inline arrow is an `Ident` of a synthesized top-level fn, so
    /// the callback resolves through [`Self::reify_function`]. A capturing arrow
    /// stayed an `Arrow` node and BAILS here (a closure — a later increment); any
    /// other expression (a function value held in a Tagged local, etc.) also bails
    /// this increment, never a guess.
    pub(super) fn reify_callback_arg(
        &mut self,
        module: &mut dyn Module,
        a: &HirExpr,
        method: &str,
    ) -> FrontResult<Value> {
        match &a.kind {
            HirExprKind::Ident(name) if self.sigs.contains_key(name) => {
                let f = self.reify_function(module, name)?;
                Ok(f.v)
            }
            HirExprKind::Arrow { .. } => Err(crate::front::error::Unsupported::new(format!(
                "Array.{method}() with a CAPTURING callback (closures are a later increment)"
            ))),
            _ => Err(crate::front::error::Unsupported::new(format!(
                "Array.{method}() callback is not a reifiable function value"
            ))),
        }
    }

    /// Marshal one array-method arg to its Array-row `AbiType`.
    /// - `U64`: a raw PolyValue WORD (box the value verbatim — element equality
    ///   must match the stored boxed word).
    /// - `I64`: a proven-numeric index, truncated to i64.
    /// - `Handle`: a proven-string separator → its real string handle.
    fn marshal_array_arg(
        &mut self,
        module: &mut dyn Module,
        v: Val,
        want: AbiType,
    ) -> FrontResult<Value> {
        match want {
            AbiType::U64 => Ok(self.box_value(v)),
            AbiType::I64 => self.numeric_to_i64(v),
            AbiType::Handle => {
                if !matches!(v.kind, JsKind::Str) {
                    return unsupported!(
                        "array method arg wants a string but its kind is not statically a string ({:?})",
                        v.repr
                    );
                }
                let word = self.box_value(v);
                Ok(emit_marshal::emit_table_load(module, self.builder, word))
            }
            other => unsupported!("cannot marshal an array-method arg of ABI {other:?}"),
        }
    }

    /// Marshal an array-method result back to a `Val`.
    /// - `U64`: a raw PolyValue word. For an array-producing method it is a fresh
    ///   TAG_OBJECT ARRAY word (kind `Array`, so a `let` records `HeapShape::Array`
    ///   and chaining works); for `at`/`pop`/`shift`/`find`/`reduce` it is an
    ///   opaque element word (kind `Unknown`).
    /// - `Handle` (`join`): a real string handle → re-boxed as a TAG_STR PolyValue.
    /// - `I64`/`Bool`: a proven number / boolean.
    fn marshal_array_ret(
        &mut self,
        module: &mut dyn Module,
        method: &str,
        ret: AbiType,
        value: Option<Value>,
    ) -> FrontResult<Val> {
        match ret {
            AbiType::U64 => {
                let word = value.expect("U64-returning trampoline yields a value");
                let kind = if is_array_returning_method(method) {
                    JsKind::Array
                } else {
                    JsKind::Unknown
                };
                Ok(Val::tagged_kind(word, kind))
            }
            AbiType::Handle => {
                let h = value.expect("Handle-returning trampoline yields a value");
                let word = emit_marshal::emit_box_real_string(module, self.builder, h);
                Ok(Val::tagged_kind(word, JsKind::Str))
            }
            AbiType::I64 => Ok(Val::new(value.expect("int result"), Repr::Int64)),
            AbiType::Bool => Ok(Val::new(value.expect("bool result"), Repr::Bool)),
            other => unsupported!("array method result of ABI {other:?} not marshaled"),
        }
    }
}

/// Whether an Array method name returns a fresh ARRAY (so a chained call on its
/// result is itself an array dispatch receiver): map/filter/slice + the P5.2
/// array-producing methods.
pub(super) fn is_array_returning_method(method: &str) -> bool {
    // DATA-DRIVEN: read the `ret_is_array` bit off the Array dispatch table (the
    // registrar) instead of a parallel hardcoded list. A method whose ANY arity row
    // returns an array word (`slice`/`map`/`toSorted`/…) qualifies; `at`/`pop`/`join`
    // (element/scalar/string returns) do not, even though their ABI is also `U64`.
    crate::dispatch::array_method_returns_array(method)
}
