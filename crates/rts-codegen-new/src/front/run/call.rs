//! Call lowering + truthiness for the whole-program path.
//!
//! Split out of [`super::expr`] (the <500-line module rule). Covers method calls
//! and cross-function calls — plus the JS `ToBoolean` reduction
//! ([`Lowerer::as_bool_value`]) used by `if`/`while`/ternary conditions.
//! (`console.log(...)` is no longer special here — `console` is a `.ts` prelude
//! object dispatched through the normal class-instance method path.)

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;

use rts_hir::HirExpr;
use rts_hir::ir::HirExprKind;

use crate::repr::Repr;
use crate::value;
use crate::value::emit_marshal;

use crate::front::error::{FrontResult, Unsupported, unsupported};

use super::lower::{JsKind, Lowerer, Val};
use super::sig::FnSig;

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Lower a `recv.method(args)` method call by routing through the dispatch
    /// chain (optional-chaining desugar, engine/global statics, function-value
    /// props, generator protocol, then static/dynamic instance dispatch).
    pub(super) fn lower_method_call(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        // P5.8: reserved optional-chaining ops the desugar emits (never
        // user-reachable — `__rts_*` is not valid TS source).
        if method == super::desugar::OPT_GET && args.len() == 1 {
            return self.lower_opt_get(module, object, &args[0]);
        }
        if method == super::desugar::OPT_INDEX && args.len() == 1 {
            return self.lower_opt_index(module, object, &args[0]);
        }
        if method == super::desugar::OPT_CALL {
            return self.lower_opt_call(module, object, args);
        }
        if method == super::desugar::OPT_METHOD_CALL {
            return self.lower_opt_method_call(module, object, args);
        }
        // `X.prototype.M.call(recv, ...rest)` → `recv.M(...rest)` — the borrowed-method
        // idiom (`Object.prototype.hasOwnProperty.call(o, k)`,
        // `Array.prototype.slice.call(args)`). Matched by SHAPE — a `.prototype.<M>`
        // member chain as the `.call` receiver — NOT by a class name (doctrine: the
        // front names no non-primordial; here it names nothing at all, just the
        // `prototype` key). The receiver `recv` (first arg) becomes the method's `this`.
        if method == "call" && !args.is_empty() {
            if let HirExprKind::Member { object: inner, prop: m } = &object.kind {
                if let HirExprKind::Member { prop: proto, .. } = &inner.kind {
                    if proto == "prototype" {
                        return self.lower_method_call(module, &args[0], m, &args[1..]);
                    }
                }
            }
        }
        // PRIVATE `engine.*` (arch/time/trace) — prelude-only (privacy gate). A
        // user caller bails here; a prelude caller lowers the runtime call.
        if let Some(val) = self.try_engine_call(module, object, method, args)? {
            return Ok(val);
        }
        // GLOBAL static `Math.m(..)` / `Number.m(..)` / `Object.m(..)` (P5.4).
        if let Some(val) = self.try_math_number_call(module, object, method, args)? {
            return Ok(val);
        }
        if let Some(val) = self.try_object_static_call(module, object, method, args)? {
            return Ok(val);
        }
        // GLOBAL static of a pure-Registry class (today `Date.now()` /
        // `Date.UTC(..)` / `Date.parse(s)`, P5.16) — data-driven, no class literal.
        if let Some(val) = self.try_registry_static_call(module, object, method, args)? {
            return Ok(val);
        }
        // GLOBAL static `Array.m(..)` / `String.m(..)` (P5.2).
        if let Some(val) = self.try_global_static_call(module, object, method, args)? {
            return Ok(val);
        }
        // CALL of a function VALUE stored as a property of a FUNCTION value
        // (`F.make(args)`, Phase 4): the receiver `F` is itself a function value and
        // `F.make` holds a function value (recorded via `__rtsadp_fn_set_prop`). Read
        // the stored property (`lower_member` → `__rtsadp_fn_get_prop`, a TAG_FUNCTION
        // word for a stored function), then invoke it through the uniform-ABI
        // function-value path. Routed BEFORE the Registry instance-method dispatch so
        // a function receiver is not misrouted. No `this` is passed (a stdlib-style
        // static does not use `this`; receiver-as-`this` is a later increment).
        // `f.call(thisArg, …)` / `f.apply(thisArg, [args])` — `Function`'s own
        // methods (primordial). Before the fn-property branch, which would read
        // `.call`/`.apply` as a stored property (→ undefined) and invoke nothing.
        if let Some(val) = self.try_fn_call_apply(module, object, method, args)? {
            return Ok(val);
        }
        if self.fn_value_word(module, object)?.is_some() {
            let prop_val = self.lower_member(module, object, method)?;
            let prop_word = self.box_value(prop_val);
            return self.lower_value_call_word(module, prop_word, args);
        }
        // GENERATOR protocol methods on a generator receiver (`it.next()`/
        // `.return(v)`/`.throw(e)` where `it = g()`): route to `GENERATOR_*` and
        // rebuild a NEW-ENGINE `{value, done}` object the engine can read.
        if matches!(method, "next" | "return" | "throw") {
            if let Some(val) = self.try_generator_method(module, object, method, args)? {
                return Ok(val);
            }
        }
        // Data-driven instance-method dispatch (String/Number) via the Registry
        // mirror. `Ok(None)` ⇒ not a dispatchable receiver; fall through to bail.
        if let Some(val) = self.try_method_dispatch(module, object, method, args)? {
            return Ok(val);
        }
        unsupported!("method call `.{method}()` (receiver class not statically dispatchable)")
    }

    /// Lower `fn.call(thisArg, a, b, …)` / `fn.apply(thisArg, [a, b])` on a
    /// function-VALUE receiver, via the uniform-ABI invoke bridge. `Function` is a
    /// PRIMORDIAL, so the engine names `call`/`apply` directly.
    ///
    /// Returns `Ok(None)` when `method` is not `call`/`apply` OR the receiver is not
    /// a function value (the caller falls through). `thisArg` (the first arg) is NOT
    /// bound — the uniform thunk's slot 0 is the closure `env`, not `this` — so this
    /// covers the common `f.call(null, …)` / `f.apply(null, [...])`. `.apply` with a
    /// NON-literal args array (a runtime array variable) is a later increment.
    pub(super) fn try_fn_call_apply(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Option<Val>> {
        if method != "call" && method != "apply" {
            return Ok(None);
        }
        // The function-value receiver word:
        // - a top-level fn name → its reified value (`fn_value_word`);
        // - a statically-known CLASS INSTANCE → fall through (`Ok(None)`): its
        //   `.call`/`.apply` is a real user method, dispatched by `try_class_method`;
        // - otherwise (an `any`/param/value that may hold a function, e.g.
        //   `Reflect.apply`'s `target`) → the lowered receiver word. A non-function
        //   yields `undefined` at `__rtsadp_fn_invoke` (never a crash).
        let fn_word = match self.fn_value_word(module, object)? {
            Some(w) => w,
            None => {
                if self.static_instance_class(object).is_some() {
                    return Ok(None);
                }
                let v = self.lower_expr(module, object)?;
                self.box_value(v)
            }
        };
        if method == "call" {
            let call_args = if args.is_empty() { &[][..] } else { &args[1..] };
            return Ok(Some(self.lower_value_call_word(module, fn_word, call_args)?));
        }
        match args.get(1) {
            None => Ok(Some(self.lower_value_call_word(module, fn_word, &[])?)),
            // A LITERAL array → lower each element directly (the cheap, exact path).
            Some(a) if matches!(a.kind, HirExprKind::Array(_)) => {
                let HirExprKind::Array(elems) = &a.kind else { unreachable!() };
                Ok(Some(self.lower_value_call_word(module, fn_word, elems)?))
            }
            // A RUNTIME array value (`Reflect.apply`'s pass-through, a variable):
            // extract `a0..a3` via `VEC_GET` (OOB → `undefined`, the guard keeps a
            // missing arg `undefined` not `0`). Functions of arity ≤4 read only
            // `a0..a3`; a >4-arity apply (rest tail) is a later increment.
            Some(arr_expr) => {
                let arr_val = self.lower_expr(module, arr_expr)?;
                let arr_word = self.box_value(arr_val);
                let len = emit_marshal::emit_vec_len(module, self.builder, arr_word);
                let undef = self
                    .builder
                    .ins()
                    .iconst(types::I64, value::PolyValue::undefined().raw() as i64);
                let mut slots = [undef; 4];
                for (i, slot) in slots.iter_mut().enumerate() {
                    let idx = self.builder.ins().iconst(types::I64, i as i64);
                    let in_range =
                        self.builder
                            .ins()
                            .icmp(IntCC::SignedLessThan, idx, len);
                    let elem = emit_marshal::emit_vec_get(module, self.builder, arr_word, idx);
                    *slot = self.builder.ins().select(in_range, elem, undef);
                }
                Ok(Some(self.emit_fn_invoke(module, fn_word, slots, undef)?))
            }
        }
    }

    /// A `Call` node: a member-callee call (routed through the same static/global/
    /// instance dispatch as a method call) or a cross-function call by name.
    pub(super) fn lower_call(
        &mut self,
        module: &mut dyn Module,
        callee: &HirExpr,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        if let HirExprKind::Member { object, prop } = &callee.kind {
            // PRIVATE `engine.*` (arch/time/trace) — prelude-only (privacy gate).
            if let Some(val) = self.try_engine_call(module, object, prop, args)? {
                return Ok(val);
            }
            // GLOBAL static `Math.m(..)` / `Number.m(..)` / `Object.m(..)` (P5.4)
            // — before user/instance dispatch (these are not user classes/locals).
            if let Some(val) = self.try_math_number_call(module, object, prop, args)? {
                return Ok(val);
            }
            if let Some(val) = self.try_object_static_call(module, object, prop, args)? {
                return Ok(val);
            }
            // GLOBAL static of a pure-Registry class (today `Date.now()` /
            // `Date.UTC(..)` / `Date.parse(s)`, P5.16) — data-driven, no literal.
            if let Some(val) = self.try_registry_static_call(module, object, prop, args)? {
                return Ok(val);
            }
            // GLOBAL static `Array.m(..)` / `String.m(..)` (P5.2) — before the
            // class-instance/primordial dispatch, since `Array`/`String` are not
            // user classes.
            if let Some(val) = self.try_global_static_call(module, object, prop, args)? {
                return Ok(val);
            }
            // CALL of a function VALUE stored as a property of a FUNCTION value
            // (`F.make(args)`, Phase 4): the receiver `F` is itself a function value
            // and `F.make` holds a function value (recorded via `__rtsadp_fn_set_prop`).
            // Read the stored property (`lower_member` → `__rtsadp_fn_get_prop`, a
            // TAG_FUNCTION word for a stored function), then invoke it through the
            // existing uniform-ABI function-value path. Routed BEFORE the class/global/
            // string method dispatch so a function receiver is not misrouted (those
            // paths key on class-name / proven receiver kinds a function value lacks).
            // No `this` is passed: a stdlib-style static does not use `this`; binding
            // the receiver as `this` is a later increment (Phase-1 limitation).
            // `f.call(thisArg, …)` / `f.apply(thisArg, [args])` — before the
            // fn-property branch (which would read `.call`/`.apply` as a property).
            if let Some(val) = self.try_fn_call_apply(module, object, prop, args)? {
                return Ok(val);
            }
            if self.fn_value_word(module, object)?.is_some() {
                let prop_val = self.lower_member(module, object, prop)?;
                let prop_word = self.box_value(prop_val);
                return self.lower_value_call_word(module, prop_word, args);
            }
            // `recv.method(args)` lowered as `Call(Member)` — route to dispatch.
            if let Some(val) = self.try_method_dispatch(module, object, prop, args)? {
                return Ok(val);
            }
            return unsupported!(
                "call of member `.{prop}()` (receiver class not statically dispatchable)"
            );
        }
        let name = match &callee.kind {
            HirExprKind::Ident(n) => n.clone(),
            // A non-ident callee that is an EXPRESSION producing a function VALUE
            // (`f(x)(y)` curry, `(cond ? f : g)(x)`, `arr[i](x)`): lower it to a value
            // and invoke through the uniform-ABI value-call path. A non-function value
            // yields `undefined` at `__rtsadp_fn_invoke` (never a crash). Member callees
            // were already routed above (method dispatch), so they never reach here.
            _ => {
                let callee_val = self.lower_expr(module, callee)?;
                let callee_word = self.box_value(callee_val);
                return self.lower_value_call_word(module, callee_word, args);
            }
        };
        // GENERATOR sentinels emitted by the parser's eager desugar (a `function* g`
        // becomes a plain fn that builds an array `__gen_buf` then `return
        // __RTS_GEN_FINISH(__gen_buf, ret)`). Map them to the real runtime externs.
        if name == "__RTS_GEN_FINISH" && args.len() == 2 {
            return self.lower_gen_finish(module, &args[0], &args[1]);
        }
        if name == "__RTS_GEN_GET_RET" && args.len() == 1 {
            return self.lower_gen_get_ret(module, &args[0]);
        }
        // LAZY state-machine sentinels (`__RTS_GEN_SM_*` / `__RTS_GEN_DELEGATE_*`,
        // emitted by the parser's state-machine desugar for loops/yield*). Mapped to
        // the real runtime externs.
        if let Some(val) = self.try_gen_sm_sentinel(module, &name, args)? {
            return Ok(val);
        }
        // `getPointer(fn)` — RTS engine builtin: materialize a top-level user
        // function's RAW code address as an i64 (the C-ABI `fn(..)->..` pointer the
        // `thread`/`parallel`/`sync` namespaces pass to `spawn`/`map`). Not a JS
        // primordial — an engine intrinsic, like the private `engine.*` bridges.
        if let Some(val) = self.try_get_pointer(module, &name, args)? {
            return Ok(val);
        }
        // A BUILTIN-IMPORT name (`import { print } from "rts:io"`): resolve the real
        // `__RTS_FN_NS_*` symbol + ABI signature through the Registry and marshal the
        // call via the SAME generic path as a class method (recv = None — a namespace
        // function has no `this`). Checked FIRST: an imported builtin name is the
        // authoritative binding for that local (the module resolver guarantees it is
        // not also a user declaration). Bare `"rts"` (ns == "") imports a namespace
        // OBJECT, not a member — `namespace_member` returns None and we bail honestly.
        if let Some((ns, member)) = self.builtins.get(&name).cloned() {
            return self.lower_builtin_call(module, &ns, &member, args);
        }
        // A direct call to a CLOSURE (a hoisted `let g = (x) => x*k`, P5.7): `g`'s
        // captures must be snapshotted from the CALL-SITE locals, so it cannot take
        // the native fast path. Reify it (builds the env from current locals), then
        // invoke through the uniform-ABI indirect path. Checked before the native
        // fast path so a capturing `g` never calls its raw body (which expects the
        // prepended capture params it would never receive).
        if self.captures.contains_key(&name) {
            let f = self.reify_function(module, &name)?;
            return self.lower_value_call_word(module, f.v, args);
        }
        // A statically-known user function → the native fast path (NOT regressed).
        if let Some(sig) = self.sigs.get(&name).cloned() {
            return self.lower_user_call(module, &sig, args);
        }
        // A local holding a function VALUE (a Tagged param/let) → the indirect
        // uniform-ABI invoke path (P4.6).
        if let Some(local) = self.local(&name) {
            return self.lower_value_call(module, local, args);
        }
        // A MODULE-GLOBAL cell (#195) holding a function VALUE, called as `f()`:
        // the `rts:test` bundle's lifecycle hooks (`let _before_all_fn = 0;` then
        // `if (_before_all_fn !== 0) { _before_all_fn(); }`) are the canonical case.
        // Load the cell word and invoke it through the same uniform-ABI indirect
        // path as a function-valued local. `gcell_id` already returns `None` when a
        // real local shadows the name (handled above), so this never steals a local.
        if let Some(id) = self.gcell_id(&name) {
            let cell = self.emit_gcell_get(module, id)?;
            let word = self.box_value(cell);
            return self.lower_value_call_word(module, word, args);
        }
        // A GLOBAL coercion/predicate function (`Number`/`parseInt`/`isNaN`/…) or
        // `Array(n)` (P5.2) — resolved last, so a same-named user fn/local wins.
        if let Some(val) = self.try_global_fn_call(module, &name, args)? {
            return Ok(val);
        }
        unsupported!("call to unknown function `{name}`")
    }

    /// `__RTS_GEN_FINISH(__gen_buf, ret)` — the end of a desugared eager generator:
    /// register `ret` as the generator's return value and hand back the buffer ARRAY
    /// (so `g()` is an iterable array and a later `gen.next()` cursors it). Lowers to
    /// `GENERATOR_SET_RET(buf_handle, ret_word)` for the side effect, then returns the
    /// buffer word as `JsKind::Array`.
    fn lower_gen_finish(
        &mut self,
        module: &mut dyn Module,
        buf: &HirExpr,
        ret: &HirExpr,
    ) -> FrontResult<Val> {
        let buf_val = self.lower_expr(module, buf)?;
        let buf_word = self.box_value(buf_val);
        let handle = emit_marshal::emit_table_load(module, self.builder, buf_word);
        let ret_val = self.lower_expr(module, ret)?;
        let ret_word = self.box_value(ret_val);
        self.call_runtime(module, "__RTS_FN_NS_GC_GENERATOR_SET_RET", &[handle, ret_word])?;
        Ok(Val::tagged_kind(buf_word, JsKind::Array))
    }

    /// Lower a generator protocol method `it.next()` / `it.return(v)` / `it.throw(e)`
    /// → the `GENERATOR_*` runtime, then build a NEW-ENGINE `{value, done}` object.
    /// The runtime returns the result as an old-model `Entry::Map` the new engine
    /// cannot read with its shape `obj_get`, so we read its fields via the
    /// `ITER_VALUE`/`ITER_DONE` accessors and reconstruct a real shape object
    /// (slot0 = global shape-id for `["value","done"]`, slot1 = value, slot2 = done).
    /// `Ok(None)` when `object` is not a generator receiver.
    fn try_generator_method(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Option<Val>> {
        let Some(handle) = self.generator_receiver_handle(module, object)? else {
            return Ok(None);
        };
        let result_map = match (method, args.first()) {
            ("next", Some(arg)) => {
                let v = self.lower_expr(module, arg)?;
                let w = self.box_value(v);
                self.call_runtime(module, "__RTS_FN_NS_GC_GENERATOR_NEXT_SENT", &[handle, w])?
            }
            ("next", None) => self.call_runtime(module, "__RTS_FN_NS_GC_GENERATOR_NEXT", &[handle])?,
            (m, arg) => {
                // `.return(v)` / `.throw(e)` — pass the (optional) arg word.
                let w = match arg {
                    Some(a) => {
                        let v = self.lower_expr(module, a)?;
                        self.box_value(v)
                    }
                    None => self
                        .builder
                        .ins()
                        .iconst(types::I64, value::PolyValue::undefined().raw() as i64),
                };
                let sym = if m == "return" {
                    "__RTS_FN_NS_GC_GENERATOR_RETURN"
                } else {
                    "__RTS_FN_NS_GC_GENERATOR_THROW"
                };
                self.call_runtime(module, sym, &[handle, w])?
            }
        };
        let result_map = result_map.expect("GENERATOR_* returns a result-Map handle");
        Ok(Some(self.build_iter_result(module, result_map)))
    }

    /// Build a NEW-ENGINE `{value, done}` object from the runtime result-Map handle.
    /// `value` = `ITER_VALUE` (a real word for a yield; the old `UNDEFINED` sentinel
    /// for the done-no-value case is remapped to the engine's `undefined`); `done` =
    /// `ITER_DONE` (a `1`/`0` flag) → a PolyValue bool.
    fn build_iter_result(&mut self, module: &mut dyn Module, result_map: Value) -> Val {
        // value word (+ remap the old-engine UNDEFINED sentinel → new undefined).
        let value_raw = self
            .call_runtime(module, "__RTS_FN_NS_GC_ITER_VALUE", &[result_map])
            .expect("call_runtime ok")
            .expect("ITER_VALUE returns a value");
        let old_undef = self.builder.ins().iconst(types::I64, i64::MIN + 2);
        let new_undef = self
            .builder
            .ins()
            .iconst(types::I64, value::PolyValue::undefined().raw() as i64);
        let is_old_undef = self.builder.ins().icmp(IntCC::Equal, value_raw, old_undef);
        let value_word = self.builder.ins().select(is_old_undef, new_undef, value_raw);
        // done flag → PolyValue bool.
        let done_flag = self
            .call_runtime(module, "__RTS_FN_NS_GC_ITER_DONE", &[result_map])
            .expect("call_runtime ok")
            .expect("ITER_DONE returns a flag");
        let t_word = self
            .builder
            .ins()
            .iconst(types::I64, value::PolyValue::bool(true).raw() as i64);
        let f_word = self
            .builder
            .ins()
            .iconst(types::I64, value::PolyValue::bool(false).raw() as i64);
        let done_is_true = {
            let zero = self.builder.ins().iconst(types::I64, 0);
            self.builder.ins().icmp(IntCC::NotEqual, done_flag, zero)
        };
        let done_word = self.builder.ins().select(done_is_true, t_word, f_word);
        // Build the object `{value, done}`: slot0 = global shape-id, then the values.
        let keys = ["value".to_string(), "done".to_string()];
        self.shapes.intern(&keys);
        let global_id = crate::shape::intern_global_shape(&keys);
        let obj = emit_marshal::emit_new_vec_object(module, self.builder);
        let id_word = self.builder.ins().iconst(
            types::I64,
            value::PolyValue::from_i32(global_id as i32).raw() as i64,
        );
        emit_marshal::emit_vec_push(module, self.builder, obj, id_word);
        emit_marshal::emit_vec_push(module, self.builder, obj, value_word);
        emit_marshal::emit_vec_push(module, self.builder, obj, done_word);
        Val::tagged_kind(obj, JsKind::Object)
    }

    /// `getPointer(fn)` — materialize a top-level user function's raw code address
    /// as an i64. Returns `Ok(None)` when `name` is not `getPointer` (the caller
    /// falls through). The single arg must be an IDENT resolving to a declared
    /// top-level function (in `self.ids`); anything else BAILS (a function VALUE /
    /// closure has no single stable C-ABI entry to hand a thread — sound refusal).
    fn try_get_pointer(
        &mut self,
        module: &mut dyn Module,
        name: &str,
        args: &[HirExpr],
    ) -> FrontResult<Option<Val>> {
        if name != "getPointer" {
            return Ok(None);
        }
        // A user local/closure named `getPointer` shadows the builtin → not us.
        if self.local(name).is_some() || self.captures.contains_key(name) {
            return Ok(None);
        }
        if args.len() != 1 {
            return unsupported!("getPointer expects exactly 1 argument (a function)");
        }
        let HirExprKind::Ident(fname) = &args[0].kind else {
            return unsupported!("getPointer argument must be a top-level function name");
        };
        let Some(fid) = self.ids.get(fname).copied() else {
            return unsupported!(
                "getPointer(`{fname}`) — not a declared top-level function (a function \
                 value / closure has no single C-ABI entry)"
            );
        };
        let fref = module.declare_func_in_func(fid, self.builder.func);
        let addr = self.builder.ins().func_addr(types::I64, fref);
        Ok(Some(Val::new(addr, Repr::Int64)))
    }

    /// Lower a LAZY generator state-machine sentinel call. Returns `Ok(None)` when
    /// `name` is not a `GEN_SM`/`DELEGATE` sentinel. The args marshal uniformly to
    /// i64: a numeric arg (state label / slot index / GenState handle held as an
    /// `Int` local) rides its raw integer; anything else (a yielded/stored VALUE) is
    /// boxed to its PolyValue word. `GEN_SM_NEW`'s first arg is the state-fn IDENT —
    /// it needs the function's RAW code address (`func_addr`), the `extern "C"
    /// fn(u64)->i64` pointer `GEN_SM_NEXT` transmutes + calls.
    fn try_gen_sm_sentinel(
        &mut self,
        module: &mut dyn Module,
        name: &str,
        args: &[HirExpr],
    ) -> FrontResult<Option<Val>> {
        let Some((symbol, ret_kind, word_args)) = gen_sm_sentinel(name, args.len()) else {
            return Ok(None);
        };
        let mut call_args: Vec<Value> = Vec::with_capacity(args.len());
        for (i, a) in args.iter().enumerate() {
            // `GEN_SM_NEW(state_fn, nslots)`: arg0 is the state-fn ident → its raw
            // code address (the C-ABI `fn(u64)->i64` the runtime calls).
            if symbol == "__RTS_FN_NS_GC_GEN_SM_NEW" && i == 0 {
                if let HirExprKind::Ident(fname) = &a.kind {
                    let fid = *self.ids.get(fname).ok_or_else(|| {
                        Unsupported::new(format!("generator state-fn `{fname}` not declared"))
                    })?;
                    let fref = module.declare_func_in_func(fid, self.builder.func);
                    call_args.push(self.builder.ins().func_addr(types::I64, fref));
                    continue;
                }
            }
            let v = self.lower_expr(module, a)?;
            // A VALUE-position arg (a yielded/stored/delegated value) must cross as a
            // boxed PolyValue WORD (the runtime stores/yields words, DRAIN collects
            // words). A handle / state-label / slot-index arg crosses as a RAW i64.
            let w = if word_args.contains(&i) {
                self.box_value(v)
            } else {
                match v.repr {
                    Repr::Int32 | Repr::Int64 => v.v,
                    Repr::Float64 => self.builder.ins().fcvt_to_sint_sat(types::I64, v.v),
                    _ => self.box_value(v),
                }
            };
            call_args.push(w);
        }
        let res = self.call_runtime(module, symbol, &call_args)?;
        Ok(Some(match ret_kind {
            GenRet::Word => Val::new(res.expect("word-returning GEN_SM"), Repr::Tagged),
            GenRet::Int => Val::new(res.expect("int-returning GEN_SM"), Repr::Int64),
            GenRet::Void => {
                let undef = self
                    .builder
                    .ins()
                    .iconst(types::I64, value::PolyValue::undefined().raw() as i64);
                Val::tagged_kind(undef, JsKind::Undefined)
            }
        }))
    }

    /// `__RTS_GEN_GET_RET(vec)` — read the return value a delegated generator
    /// registered (`const r = yield* g()`). Lowers to `GENERATOR_GET_RET(handle)`.
    fn lower_gen_get_ret(&mut self, module: &mut dyn Module, vec: &HirExpr) -> FrontResult<Val> {
        let v = self.lower_expr(module, vec)?;
        let word = self.box_value(v);
        let handle = emit_marshal::emit_table_load(module, self.builder, word);
        let res = self
            .call_runtime(module, "__RTS_FN_NS_GC_GENERATOR_GET_RET", &[handle])?
            .expect("GENERATOR_GET_RET returns a value");
        Ok(Val::new(res, Repr::Tagged))
    }

    /// Lower a BUILTIN-IMPORT call `member(args)` where `member` was imported from
    /// `rts:<ns>` (`Binding::Builtin`). Resolves the real `__RTS_FN_NS_*` symbol +
    /// its `AbiType` signature through the Registry ([`registry::namespace_member`])
    /// and emits the call through the generic marshal ([`Self::emit_registry_call`])
    /// with NO receiver — a namespace function has no implicit `this`, every arg is
    /// explicit. An unknown member, an arity mismatch, or a bare-`"rts"` namespace
    /// object (`ns == ""`) → explicit `Unsupported` (honest bail, never a guess).
    ///
    /// A `Handle` return is treated as a STRING (`JsKind::Str`): the namespace fns
    /// that return a `Handle` (e.g. a dynamic-string result) hand back a gc string
    /// handle; the generic rebox interns it as a `TAG_STR` PolyValue.
    pub(super) fn lower_builtin_call(
        &mut self,
        module: &mut dyn Module,
        ns: &str,
        member: &str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        let resolved = super::registry::namespace_member(ns, member, args.len()).ok_or_else(|| {
            let spec = if ns.is_empty() {
                "rts".to_string()
            } else {
                format!("rts:{ns}")
            };
            Unsupported::new(format!(
                "builtin import `{member}` from `{spec}` (arity {}): no matching namespace function \
                 (bare `rts` namespace-object imports + unknown members are not wired)",
                args.len()
            ))
        })?;
        // Lower each explicit arg to a Val; the generic marshal coerces it to the
        // parameter's AbiType (StrPtr → ptr+len via the pool, numeric → scalar).
        // EXCEPTION — a CALLBACK fn-ptr: a `U64` param receiving a bare TOP-LEVEL
        // function name wants its RAW C-ABI code address (like `getPointer`/
        // `thread.spawn`), NOT the reified `TAG_FUNCTION` value — so `events.on(e, k,
        // onTick)` registers a pointer `events.emit0` can `transmute` + call. Only a
        // non-`this`, non-closure, declared top-level fn (a single C-ABI entry).
        let mut argvals: Vec<Val> = Vec::with_capacity(args.len());
        for (a, &abi) in args.iter().zip(resolved.arg_abis.iter()) {
            if abi == rts_engine::abi::AbiType::U64 {
                if let HirExprKind::Ident(fname) = &a.kind {
                    if self.local(fname).is_none()
                        && !self.captures.contains_key(fname)
                        && self.sigs.get(fname).is_some_and(|s| !s.has_this && !s.is_async)
                    {
                        if let Some(&fid) = self.ids.get(fname) {
                            let fref = module.declare_func_in_func(fid, self.builder.func);
                            let addr = self.builder.ins().func_addr(types::I64, fref);
                            argvals.push(Val::new(addr, Repr::Int64));
                            continue;
                        }
                    }
                }
            }
            argvals.push(self.lower_expr(module, a)?);
        }
        // The rebox kind only matters for a `Handle` return: a HEAP STRING handle
        // (`gc.string_*`, `string.*`) reboxes as `TAG_STR`; an OPAQUE RESOURCE
        // handle (`audio.*`, `buffer.alloc`, `net.*` — a raw `u64` id, TS type
        // `number`) reboxes as a plain INTEGER. Treating every namespace `Handle`
        // as a string/object (the old fixed `JsKind::Str`) NaN-boxed a raw id as a
        // heap pointer → a later `emit_table_load` on it SIGILL'd (audio repro).
        let result_kind = if resolved.ret_is_string_handle {
            JsKind::Str
        } else {
            JsKind::Number
        };
        self.emit_registry_call(module, &resolved, None, &argvals, result_kind)
    }

    /// Reify a user function `name` (referenced as a VALUE) into a `TAG_FUNCTION`
    /// PolyValue: `func_addr` of its uniform-ABI THUNK → `__rtsadp_fn_reify(addr,
    /// nparams, has_rest, env)` → box the returned 48-bit slot+shard as
    /// `TAG_FUNCTION`. The GC marks it (a real heap handle behind the tag), so it
    /// is GC-safe.
    ///
    /// For a CLOSURE (a synthesized fn recorded in `captures`), `env` is a fresh
    /// `Entry::Vec` snapshotting each captured outer-local's CURRENT value (boxed),
    /// in the recorded capture order — capture-BY-VALUE. `nparams` is the closure's
    /// REAL arity (its own declared params, excluding the prepended captures), so
    /// the invoke marshals a0..a3 to the right params.
    pub(super) fn reify_function(
        &mut self,
        module: &mut dyn Module,
        name: &str,
    ) -> FrontResult<Val> {
        let sig = self
            .sigs
            .get(name)
            .ok_or_else(|| Unsupported::new(format!("reify of unknown function `{name}`")))?;
        if sig.is_async {
            return unsupported!(
                "async/generator function `{name}` as a VALUE (it returns a Promise / suspends — a later increment)"
            );
        }
        // Phase 1: a free function with a synthesized `this` (`params[0]`) is not yet
        // reifiable as a VALUE — the uniform-ABI thunk fills `params[0]` from `a0`
        // (the first user arg), shifting every arg by one. The DIRECT call path
        // (`F(args)`) handles the implicit receiver; the value path is a later
        // increment. Bail explicitly rather than mis-bind the receiver.
        if sig.has_this {
            return unsupported!(
                "free function `{name}` that uses `this` as a VALUE (direct calls work; value-invoke is a later increment)"
            );
        }
        // The captured-var list (if `name` is a closure) — clone to drop the borrow
        // on `self` before lowering the env snapshot.
        let capture_names: Vec<String> = self.captures.get(name).cloned().unwrap_or_default();
        // The closure's REAL arity excludes the prepended captures.
        let nparams = (sig.params.len() - capture_names.len()) as i64;
        let thunk_id = *self
            .thunks
            .get(name)
            .ok_or_else(|| Unsupported::new(format!("no thunk for function `{name}`")))?;

        // Build the env: a fresh array snapshotting each captured local's CURRENT
        // value (capture-by-value). A non-capturing fn passes env = 0 (undefined).
        let env_word = if capture_names.is_empty() {
            self.builder.ins().iconst(types::I64, 0)
        } else {
            self.build_closure_env(module, &capture_names)?
        };

        // Relocatable address of the THUNK (available at JIT time). `func_addr`
        // points at the thunk so every indirect call uses the fixed uniform ABI.
        let func_ref = module.declare_func_in_func(thunk_id, self.builder.func);
        let addr = self.builder.ins().func_addr(types::I64, func_ref);

        let nparams_v = self.builder.ins().iconst(types::I64, nparams);
        // No `...rest` in this increment's reify surface (variadic arrows are
        // rejected at extraction); has_rest is always 0.
        let has_rest_v = self.builder.ins().iconst(types::I64, 0);
        let payload = self
            .call_runtime(
                module,
                "__rtsadp_fn_reify",
                &[addr, nparams_v, has_rest_v, env_word],
            )?
            .expect("__rtsadp_fn_reify returns a value");

        // Box the bare 48-bit payload as a TAG_FUNCTION PolyValue word:
        // BOX_BASE | (TAG_FUNCTION<<48) | (payload & PAYLOAD_MASK).
        let header = value::encode(value::TAG_FUNCTION, 0) as i64;
        let mask = self
            .builder
            .ins()
            .iconst(types::I64, value::PAYLOAD_MASK as i64);
        let masked = self.builder.ins().band(payload, mask);
        let header_v = self.builder.ins().iconst(types::I64, header);
        let word = self.builder.ins().bor(masked, header_v);
        Ok(Val::tagged_kind(word, JsKind::Function))
    }

    /// Reify a USER CLASS named `name` as a first-class VALUE (`const C = Box`): a
    /// `TAG_FUNCTION` PolyValue whose code address is the class NEW-THUNK
    /// (`<class>__rtsn_newthunk`) — invoking it (`new C(args)` / a plain call)
    /// allocates an instance and runs the constructor. `nparams` is the ctor arity
    /// (no `this` in the value ABI; the thunk synthesizes `this`). A class without a
    /// real synthesized ctor (a literal-shape placeholder) has no new-thunk → bail.
    /// `typeof` of the result is "function" (JS: `typeof Box === "function"`).
    pub(super) fn reify_class(&mut self, module: &mut dyn Module, name: &str) -> FrontResult<Val> {
        let desc = self
            .classes
            .get(name)
            .ok_or_else(|| Unsupported::new(format!("reify of unknown class `{name}`")))?;
        let nparams = desc.ctor_arity as i64;
        let thunk_key = super::thunk::new_thunk_name(name);
        let Some(&thunk_id) = self.thunks.get(&thunk_key) else {
            return unsupported!(
                "class `{name}` as a VALUE — no synthesized constructor (a literal-shape \
                 / `extends`-only class as a value is a later increment)"
            );
        };
        let func_ref = module.declare_func_in_func(thunk_id, self.builder.func);
        let addr = self.builder.ins().func_addr(types::I64, func_ref);
        // Register this class NEW-THUNK address as a valid `new <value>()` target,
        // so a later dynamic `new` through this class-value constructs (and a
        // non-constructor value throws). Idempotent: the address is stable.
        self.call_runtime(module, "__rtsadp_register_ctor_thunk", &[addr])?;
        let nparams_v = self.builder.ins().iconst(types::I64, nparams);
        let has_rest_v = self.builder.ins().iconst(types::I64, 0);
        let env_word = self.builder.ins().iconst(types::I64, 0);
        let payload = self
            .call_runtime(module, "__rtsadp_fn_reify", &[addr, nparams_v, has_rest_v, env_word])?
            .expect("__rtsadp_fn_reify returns a value");
        let header = value::encode(value::TAG_FUNCTION, 0) as i64;
        let mask = self
            .builder
            .ins()
            .iconst(types::I64, value::PAYLOAD_MASK as i64);
        let masked = self.builder.ins().band(payload, mask);
        let header_v = self.builder.ins().iconst(types::I64, header);
        let word = self.builder.ins().bor(masked, header_v);
        Ok(Val::tagged_kind(word, JsKind::Function))
    }

    /// Build a closure's env array (P5.7): a fresh `Entry::Vec` (a `TAG_OBJECT`
    /// PolyValue) holding a SNAPSHOT of each captured outer-local's CURRENT value
    /// (boxed), in the recorded capture order. Returns the env's `TAG_OBJECT` word.
    ///
    /// Each captured name MUST be a local in the current scope — the capture
    /// analysis only accepted simple outer locals, so a name not found here is a
    /// codegen invariant break, surfaced as an explicit bail rather than a guess.
    fn build_closure_env(
        &mut self,
        module: &mut dyn Module,
        capture_names: &[String],
    ) -> FrontResult<cranelift_codegen::ir::Value> {
        let arr = emit_marshal::emit_new_vec_object(module, self.builder);
        for cap in capture_names {
            let local = self.local(cap).ok_or_else(|| {
                Unsupported::new(format!(
                    "closure captures `{cap}` which is not a simple local in scope"
                ))
            })?;
            let v = self.builder.use_var(local.var);
            let word = self.box_value(Val::new(v, local.repr));
            emit_marshal::emit_vec_push(module, self.builder, arr, word);
        }
        Ok(arr)
    }

    /// Lower `new <local>(args)` where `<local>` holds a runtime VALUE (a class
    /// reified into a local / `globalThis` field — `const G = globalThis.Box; new
    /// G(5)`). The value is invoked through [`__rtsadp_new_invoke`]: if its stored
    /// thunk is a registered class NEW-THUNK it constructs (allocates + runs the
    /// ctor + returns the instance); otherwise a TypeError is thrown (the value is
    /// not a constructor — never mis-constructed). The result is an opaque
    /// PolyValue word (kind Unknown), so a `let c = new G()` records no static class.
    pub(super) fn lower_new_value(
        &mut self,
        module: &mut dyn Module,
        name: &str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        let local = self
            .local(name)
            .expect("caller proved `name` is an in-scope local value");
        let fn_word = self.builder.use_var(local.var);
        // Box the first four positional args; overflow (5th+) into a rest array.
        let undef = || value::PolyValue::undefined().raw() as i64;
        let mut slots: [Value; 4] = [self.builder.ins().iconst(types::I64, undef()); 4];
        for (i, a) in args.iter().take(4).enumerate() {
            let v = self.lower_expr(module, a)?;
            slots[i] = self.box_value(v);
        }
        let rest = if args.len() > 4 {
            let arr = emit_marshal::emit_new_vec_object(module, self.builder);
            for a in &args[4..] {
                let v = self.lower_expr(module, a)?;
                let word = self.box_value(v);
                emit_marshal::emit_vec_push(module, self.builder, arr, word);
            }
            arr
        } else {
            self.builder.ins().iconst(types::I64, undef())
        };
        let res = self
            .call_runtime(
                module,
                "__rtsadp_new_invoke",
                &[fn_word, slots[0], slots[1], slots[2], slots[3], rest],
            )?
            .expect("__rtsadp_new_invoke returns a value");
        // A non-constructor value left a pending TypeError — unwind it here.
        self.emit_post_call_error_check(module)?;
        Ok(Val::new(res, Repr::Tagged))
    }

    /// Lower a call through a function VALUE held in a local (`g(args)` where `g`
    /// is a Tagged param/let bound to a `TAG_FUNCTION` PolyValue): box up to 4
    /// args into `a0..a3` (undefined for missing), pack `args[4..]` into a rest
    /// ARRAY (or undefined when ≤4), then `__rtsadp_fn_invoke(fn_word, a0..a3,
    /// rest)`. The result is an opaque PolyValue word (kind Unknown).
    fn lower_value_call(
        &mut self,
        module: &mut dyn Module,
        local: super::lower::Local,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        // The function value word (the local's raw Tagged register).
        let fn_word = self.builder.use_var(local.var);
        self.lower_value_call_word(module, fn_word, args)
    }

    /// Invoke a function VALUE held in the Cranelift word `fn_word` (a
    /// `TAG_FUNCTION` PolyValue) through the uniform-ABI indirect path. Shared by
    /// the value-local call ([`Self::lower_value_call`]) and the direct closure
    /// call (P5.7, where `fn_word` is a freshly reified closure with its env).
    pub(super) fn lower_value_call_word(
        &mut self,
        module: &mut dyn Module,
        fn_word: Value,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        // Box the first four positional args; missing slots are `undefined`.
        let undef = || value::PolyValue::undefined().raw() as i64;
        let mut slots: [Value; 4] = [self.builder.ins().iconst(types::I64, undef()); 4];
        for (i, a) in args.iter().take(4).enumerate() {
            let v = self.lower_expr(module, a)?;
            slots[i] = self.box_value(v);
        }

        // Overflow args (5th onward) go into a fresh rest ARRAY; ≤4 args → undefined.
        let rest = if args.len() > 4 {
            let arr = emit_marshal::emit_new_vec_object(module, self.builder);
            for a in &args[4..] {
                let v = self.lower_expr(module, a)?;
                let word = self.box_value(v);
                emit_marshal::emit_vec_push(module, self.builder, arr, word);
            }
            arr
        } else {
            self.builder.ins().iconst(types::I64, undef())
        };

        self.emit_fn_invoke(module, fn_word, slots, rest)
    }

    /// Emit `__rtsadp_fn_invoke(fn_word, a0..a3, rest)` from already-boxed arg
    /// words + route the post-call exception edge. Shared by the by-AST value call
    /// ([`Self::lower_value_call_word`]) and the dynamic `.apply` path (which builds
    /// the slots from a runtime array via `VEC_GET`).
    pub(super) fn emit_fn_invoke(
        &mut self,
        module: &mut dyn Module,
        fn_word: Value,
        slots: [Value; 4],
        rest: Value,
    ) -> FrontResult<Val> {
        let res = self
            .call_runtime(
                module,
                "__rtsadp_fn_invoke",
                &[fn_word, slots[0], slots[1], slots[2], slots[3], rest],
            )?
            .expect("__rtsadp_fn_invoke returns a value");
        // P5.13: a function-value invoke may have thrown (its body's manual-unwind
        // sentinel return left the pending-error slot set). Route the unwind before
        // the result is used.
        self.emit_post_call_error_check(module)?;
        // The result is a PolyValue word of unknown static kind.
        Ok(Val::new(res, Repr::Tagged))
    }

    /// Marshal a call's arguments to the callee's params, honoring a trailing REST
    /// param (`...items`): the args beyond the fixed params are packed into a fresh
    /// array passed as the single rest slot (F3b). `this_word` is the receiver of a
    /// method/constructor, pushed as `params[0]`; `None` for a free function.
    /// Returns the Cranelift values in param order, each already coerced to its
    /// param repr.
    ///
    /// Scope: PLAIN trailing args only. A `Spread` arg anywhere (fixed slot or the
    /// rest tail) is a later increment and BAILS here, leaving the existing
    /// dedicated `f(...arr)` fast path / spread bails to own that case.
    pub(super) fn marshal_call_args(
        &mut self,
        module: &mut dyn Module,
        sig: &FnSig,
        this_word: Option<Value>,
        args: &[HirExpr],
    ) -> FrontResult<Vec<Value>> {
        let mut out = Vec::with_capacity(sig.params.len());
        let mut pi = 0usize;
        if let Some(w) = this_word {
            out.push(self.coerce(Val::tagged_kind(w, JsKind::Object), sig.params[0])?);
            pi = 1;
        }
        let user_params = &sig.params[pi..];
        // The rest-param index relative to the USER params (after dropping `this`).
        match sig.rest_param.map(|r| r - pi) {
            None => {
                for (i, &want) in user_params.iter().enumerate() {
                    if i < args.len() {
                        let a = &args[i];
                        if matches!(a.kind, HirExprKind::Spread(_)) {
                            return unsupported!(
                                "spread arg into `{}` (later increment)",
                                sig.name
                            );
                        }
                        let v = self.lower_expr(module, a)?;
                        out.push(self.coerce(v, want)?);
                    } else if sig.fillable[pi + i] {
                        // An omitted FILLABLE trailing param (optional or defaulted):
                        // pass `undefined`. For a defaulted param, the callee prologue
                        // replaces this `undefined` with the default (correct scoping);
                        // for an optional param the body sees `undefined` directly.
                        out.push(self.undefined_coerced(want)?);
                    } else {
                        return unsupported!("call to `{}` missing required arg {}", sig.name, i);
                    }
                }
                // EXTRA positional args (JS ignores args beyond the declared arity,
                // but still EVALUATES them left-to-right for side effects). Lower each
                // and discard — the callee never receives them. A spread among the
                // extras is a later increment.
                if args.len() > user_params.len() {
                    for a in &args[user_params.len()..] {
                        if matches!(a.kind, HirExprKind::Spread(_)) {
                            return unsupported!(
                                "spread arg into `{}` (later increment)",
                                sig.name
                            );
                        }
                        self.lower_expr(module, a)?;
                    }
                }
            }
            Some(ru) => {
                // `ru` = number of FIXED user params before the rest param. A FIXED
                // param may be fillable (optional/defaulted) and omitted; a required
                // one omitted bails.
                for i in 0..ru {
                    if i < args.len() {
                        if matches!(args[i].kind, HirExprKind::Spread(_)) {
                            return unsupported!(
                                "spread arg into `{}` (later increment)",
                                sig.name
                            );
                        }
                        let v = self.lower_expr(module, &args[i])?;
                        out.push(self.coerce(v, user_params[i])?);
                    } else if sig.fillable[pi + i] {
                        out.push(self.undefined_coerced(user_params[i])?);
                    } else {
                        return unsupported!(
                            "call to `{}` expects at least {} args, got {}",
                            sig.name,
                            ru,
                            args.len()
                        );
                    }
                }
                // Pack the remaining args into a fresh REST array (a `TAG_OBJECT`
                // word over an `Entry::Vec` — the array param's `xs[i]`/`xs.length`
                // lower against it via the F3a array-shape path). Empty tail → an
                // empty array (`xs.length === 0`).
                let arr = emit_marshal::emit_new_vec_object(module, self.builder);
                // When some fixed params were filled (args.len() < ru) the rest tail
                // is empty; clamp the slice start so `&args[ru..]` cannot panic.
                let tail_start = ru.min(args.len());
                for a in &args[tail_start..] {
                    // A spread of a PROVEN array contributes all its elements to the
                    // rest array (reusing the array-literal `[...src]` append path:
                    // `__rtsadp_arr_spread_append(dst, src)` walks the src Vec and
                    // pushes each raw element word onto `dst`). A spread of a
                    // non-array is a later increment → bail.
                    if let HirExprKind::Spread(inner) = &a.kind {
                        if !self.is_array_valued(inner) {
                            return unsupported!(
                                "spread of a non-array into rest of `{}` (later increment)",
                                sig.name
                            );
                        }
                        let src = self.lower_expr(module, inner)?;
                        let src_word = self.box_value(src);
                        self.call_runtime(module, "__rtsadp_arr_spread_append", &[arr, src_word])?;
                        continue;
                    }
                    let v = self.lower_expr(module, a)?;
                    let w = self.box_value(v);
                    emit_marshal::emit_vec_push(module, self.builder, arr, w);
                }
                out.push(self.coerce(Val::tagged_kind(arr, JsKind::Array), user_params[ru])?);
            }
        }
        Ok(out)
    }

    /// An `undefined` PolyValue word coerced to `target` (for an omitted fillable
    /// arg). `target` is `Tagged` for any fillable param (set in [`FnSig::of_func`]),
    /// so the coerce is a no-op; the helper stays general for safety.
    fn undefined_coerced(&mut self, target: Repr) -> FrontResult<Value> {
        let w = self
            .builder
            .ins()
            .iconst(types::I64, value::PolyValue::undefined().raw() as i64);
        self.coerce(Val::tagged_kind(w, JsKind::Undefined), target)
    }

    /// Lower a cross-function call: coerce each argument to the callee's param
    /// repr (box/unbox/widen per `FnSig`), emit the Cranelift `call`, and tag the
    /// result with the callee's return repr.
    fn lower_user_call(
        &mut self,
        module: &mut dyn Module,
        sig: &FnSig,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        // Compile-time flatten of an ARRAY-LITERAL spread: `f(a, ...[1, 2], b)` →
        // `f(a, 1, 2, b)`. The element count of a literal is known, so a spread in
        // the MIDDLE (and multiple literal spreads) reduce to plain positional args
        // with no runtime indexing. Runtime-array spreads (`...arr`) are left intact
        // for the unpack paths below. Only allocate when a literal spread is present.
        if args
            .iter()
            .any(|a| matches!(&a.kind, HirExprKind::Spread(inner) if matches!(inner.kind, HirExprKind::Array(_))))
        {
            let mut flat: Vec<HirExpr> = Vec::with_capacity(args.len());
            for a in args {
                match &a.kind {
                    HirExprKind::Spread(inner) => match &inner.kind {
                        HirExprKind::Array(elems) => flat.extend(elems.iter().cloned()),
                        _ => flat.push(a.clone()),
                    },
                    _ => flat.push(a.clone()),
                }
            }
            return self.lower_user_call(module, sig, &flat);
        }
        // `f(...arr)` (P5.6): a SINGLE spread of a proven array unpacks the array's
        // first `params.len()` elements into the native params. The array must
        // supply exactly the arity at runtime (a shorter array reads `undefined`
        // into a missing slot, which an unbox of a numeric param would mis-handle —
        // so we only support the common exact-arity `f(...arr)` shape here).
        // The sole-`...arr` fast path (unpack first `params.len()` elements into
        // native params) is ONLY for NON-rest fns. For a rest fn, `...arr` means
        // "all of arr into the rest array" — route it to `marshal_call_args`, which
        // packs every spread element into the fresh rest array (F3b spread tail).
        if sig.rest_param.is_none() {
            if args.len() == 1 {
                if let HirExprKind::Spread(inner) = &args[0].kind {
                    // The `f(...arr)` fast path unpacks into NATIVE params and does not
                    // model the implicit `this` slot of a Phase-1 free-`this` function.
                    // Bail that rare combo rather than mis-marshal (the receiver slot).
                    if sig.has_this {
                        return unsupported!(
                            "spread call `{}(...)` to a `this`-using free function (later increment)",
                            sig.name
                        );
                    }
                    return self.lower_user_call_spread(module, sig, inner);
                }
            }
            // For a non-rest fn, a TRAILING spread after leading positionals
            // (`f(a, b, ...arr)`) unpacks: the positionals fill `params[0..k]`, the
            // spread fills `params[k..]` from `arr[0..]` (out-of-range → undefined,
            // exactly like the sole-`...arr` path). A `this`-using free fn, a spread
            // that is not last, or more than one spread stays a later increment.
            let spread_positions: Vec<usize> = args
                .iter()
                .enumerate()
                .filter(|(_, a)| matches!(a.kind, HirExprKind::Spread(_)))
                .map(|(i, _)| i)
                .collect();
            if !spread_positions.is_empty() {
                if !sig.has_this
                    && spread_positions.len() == 1
                    && spread_positions[0] == args.len() - 1
                {
                    let leading = &args[..args.len() - 1];
                    let HirExprKind::Spread(inner) = &args[args.len() - 1].kind else {
                        unreachable!("spread position proven above")
                    };
                    return self.lower_user_call_spread_mixed(module, sig, leading, inner);
                }
                return unsupported!(
                    "call to `{}` with a spread mixed with positional args (later increment)",
                    sig.name
                );
            }
        }
        // A free function with a synthesized `this` (Phase 1): a PLAIN call passes
        // `undefined` as the receiver (`params[0]`); the user args fill `params[1..]`.
        // `marshal_call_args` already routes a `Some(this_word)` into `params[0]` and
        // checks arity against the remaining user params.
        let this_word = if sig.has_this {
            Some(
                self.builder
                    .ins()
                    .iconst(types::I64, value::PolyValue::undefined().raw() as i64),
            )
        } else {
            None
        };
        let lowered = self.marshal_call_args(module, sig, this_word, args)?;
        self.emit_user_call(module, sig, &lowered)
    }

    /// Reduce `val` to an i64 0/1 condition usable by `brif`/`select`, applying
    /// JS `ToBoolean`. A `Bool` is already 0/1. A proven number folds inline
    /// (non-zero & non-NaN is truthy). A Tagged value goes through the runtime
    /// `__rtsadp_to_boolean` (which resolves the empty-string case on the heap).
    pub(super) fn as_bool_value(
        &mut self,
        module: &mut dyn Module,
        val: Val,
    ) -> FrontResult<Value> {
        match val.repr {
            Repr::Bool => Ok(val.v),
            Repr::Int32 | Repr::Int64 => {
                let zero = self.builder.ins().iconst(types::I64, 0);
                let b = self.builder.ins().icmp(IntCC::NotEqual, val.v, zero);
                Ok(self.builder.ins().uextend(types::I64, b))
            }
            Repr::Float64 => {
                // truthy iff x != 0 and x == x (NaN compares unequal to itself).
                let zero = self.builder.ins().f64const(0.0);
                let nonzero = self.builder.ins().fcmp(FloatCC::NotEqual, val.v, zero);
                let ordered = self.builder.ins().fcmp(FloatCC::Equal, val.v, val.v);
                let both = self.builder.ins().band(nonzero, ordered);
                Ok(self.builder.ins().uextend(types::I64, both))
            }
            Repr::Tagged => {
                let res = self
                    .call_runtime(module, "__rtsadp_to_boolean", &[val.v])?
                    .expect("__rtsadp_to_boolean returns a value");
                Ok(res)
            }
            other => unsupported!("condition of repr {other:?}"),
        }
    }
}

/// How a `GEN_SM` sentinel's result is reboxed.
#[derive(Clone, Copy)]
enum GenRet {
    /// A PolyValue WORD (a yielded/stored/sent value): `Tagged`.
    Word,
    /// A raw integer (a state label / done flag): `Int64`.
    Int,
    /// No value (a frame/state write): `undefined`.
    Void,
}

/// Map a parser LAZY-generator sentinel name + arg count to its real runtime
/// `__RTS_FN_NS_GC_*` symbol + result kind. `None` for a non-sentinel name. The
/// SYNC set only (async `ASYNC_SM_*`/`AGEN_*` are a later phase — they stay
/// unmapped and bail honestly).
fn gen_sm_sentinel(name: &str, argc: usize) -> Option<(&'static str, GenRet, &'static [usize])> {
    use GenRet::{Int, Void, Word};
    // The third field lists the VALUE-position args (boxed to PolyValue words); all
    // other positions are raw i64 (handle / state-label / slot-index).
    Some(match (name, argc) {
        ("__RTS_GEN_SM_NEW", 2) => ("__RTS_FN_NS_GC_GEN_SM_NEW", Int, &[]),
        ("__RTS_GEN_SM_FGET", 2) => ("__RTS_FN_NS_GC_GEN_SM_FGET", Word, &[]),
        ("__RTS_GEN_SM_FSET", 3) => ("__RTS_FN_NS_GC_GEN_SM_FSET", Void, &[2]),
        ("__RTS_GEN_SM_STATE", 1) => ("__RTS_FN_NS_GC_GEN_SM_STATE", Int, &[]),
        ("__RTS_GEN_SM_SETSTATE", 2) => ("__RTS_FN_NS_GC_GEN_SM_SETSTATE", Void, &[]),
        ("__RTS_GEN_SM_YIELD", 2) => ("__RTS_FN_NS_GC_GEN_SM_YIELD", Word, &[1]),
        ("__RTS_GEN_SM_DONE", 2) => ("__RTS_FN_NS_GC_GEN_SM_DONE", Word, &[1]),
        ("__RTS_GEN_SM_SENT", 1) => ("__RTS_FN_NS_GC_GEN_SM_SENT", Word, &[]),
        ("__RTS_GEN_SM_ENTER_TRY", 2) => ("__RTS_FN_NS_GC_GEN_SM_ENTER_TRY", Void, &[]),
        ("__RTS_GEN_SM_ENTER_TRY_CATCH", 2) => ("__RTS_FN_NS_GC_GEN_SM_ENTER_TRY_CATCH", Void, &[]),
        ("__RTS_GEN_SM_EXIT_TRY_CATCH", 1) => ("__RTS_FN_NS_GC_GEN_SM_EXIT_TRY_CATCH", Void, &[]),
        ("__RTS_GEN_SM_CAUGHT", 1) => ("__RTS_FN_NS_GC_GEN_SM_CAUGHT", Word, &[]),
        ("__RTS_GEN_SM_END_FINALLY", 1) => ("__RTS_FN_NS_GC_GEN_SM_END_FINALLY", Word, &[]),
        ("__RTS_GEN_DELEGATE_START", 1) => ("__RTS_FN_NS_GC_GEN_DELEGATE_START", Int, &[0]),
        ("__RTS_GEN_DELEGATE_NEXT", 1) => ("__RTS_FN_NS_GC_GEN_DELEGATE_NEXT", Word, &[]),
        ("__RTS_GEN_DELEGATE_DONE", 1) => ("__RTS_FN_NS_GC_GEN_DELEGATE_DONE", Int, &[]),
        _ => return None,
    })
}
