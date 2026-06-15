//! Call lowering + truthiness for the whole-program path.
//!
//! Split out of [`super::expr`] (the <500-line module rule). Covers the two
//! Tagged-boundary call shapes the engine runs — `console.log(...)` and
//! cross-function calls — plus the JS `ToBoolean` reduction
//! ([`Lowerer::as_bool_value`]) used by `if`/`while`/ternary conditions.

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;

use rts_hir::HirExpr;
use rts_hir::ir::HirExprKind;

use crate::repr::Repr;
use crate::value;
use crate::value::{abi_adapter, emit_marshal};

use crate::front::error::{FrontResult, Unsupported, unsupported};

use super::lower::{JsKind, Lowerer, Val};
use super::sig::FnSig;

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// `console.log(...)` arrives as a `MethodCall` on `console`; any other
    /// method call is a later increment.
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
        if method == super::desugar::OPT_CALL {
            return self.lower_opt_call(module, object, args);
        }
        if is_console_ident(object) && method == "log" {
            return self.lower_console_log(module, args);
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
        // GLOBAL static `Date.now()` / `Date.UTC(..)` / `Date.parse(s)` (P5.16).
        if let Some(val) = self.try_date_static_call(module, object, method, args)? {
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
        if self.fn_value_word(module, object)?.is_some() {
            let prop_val = self.lower_member(module, object, method)?;
            let prop_word = self.box_value(prop_val);
            return self.lower_value_call_word(module, prop_word, args);
        }
        // Data-driven instance-method dispatch (String/Number) via the Registry
        // mirror. `Ok(None)` ⇒ not a dispatchable receiver; fall through to bail.
        if let Some(val) = self.try_method_dispatch(module, object, method, args)? {
            return Ok(val);
        }
        unsupported!("method call `.{method}()` (receiver class not statically dispatchable)")
    }

    /// A `Call` node: either `console.log(...)` (callee is a `console.log`
    /// Member) or a cross-function call to a user function by name.
    pub(super) fn lower_call(
        &mut self,
        module: &mut dyn Module,
        callee: &HirExpr,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        if let HirExprKind::Member { object, prop } = &callee.kind {
            if is_console_ident(object) && prop == "log" {
                return self.lower_console_log(module, args);
            }
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
            // GLOBAL static `Date.now()` / `Date.UTC(..)` / `Date.parse(s)` (P5.16).
            if let Some(val) = self.try_date_static_call(module, object, prop, args)? {
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
            _ => return unsupported!("call of a non-identifier callee"),
        };
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
        // A GLOBAL coercion/predicate function (`Number`/`parseInt`/`isNaN`/…) or
        // `Array(n)` (P5.2) — resolved last, so a same-named user fn/local wins.
        if let Some(val) = self.try_global_fn_call(module, &name, args)? {
            return Ok(val);
        }
        unsupported!("call to unknown function `{name}`")
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
    fn lower_builtin_call(
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
        let mut argvals: Vec<Val> = Vec::with_capacity(args.len());
        for a in args {
            argvals.push(self.lower_expr(module, a)?);
        }
        self.emit_registry_call(module, &resolved, None, &argvals, JsKind::Str)
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

    /// Lower `console.log(a, b, …)` through the REAL runtime: ToString each arg to
    /// a string PolyValue (`__rtsadp_to_string`, interning in the real pool), join
    /// them with a single space via `__rtsadp_add` (real `STRING_CONCAT`), then
    /// print the joined line via [`emit_marshal::emit_print_string_poly`]
    /// (table-load → `STRING_PTR`/`STRING_LEN` → `__rtsadp_print_line`, which
    /// forwards to the REAL `__RTS_FN_NS_IO_PRINT(ptr, len)` — the newline is the
    /// runtime's). Returns `undefined` (console.log's JS result).
    ///
    /// No arity cap (the old fixed-arity `console_logN` family is gone): the line
    /// is folded left-to-right with space separators, so any number of args works.
    fn lower_console_log(&mut self, module: &mut dyn Module, args: &[HirExpr]) -> FrontResult<Val> {
        // Build the joined-line string PolyValue. Empty `console.log()` prints a
        // blank line: start from the empty string.
        let space = {
            let pv = abi_adapter::intern_poly(" ");
            self.builder.ins().iconst(types::I64, pv.raw() as i64)
        };
        let mut line: Option<Value> = None;
        for a in args {
            // A WHOLE OBJECT value (object literal / object-shaped local) now
            // renders via `__rtsadp_inspect_object` (P3.6): box the object word,
            // then the trampoline reads the slot-0 global shape-id, recovers the
            // keys, and renders `{ k: v }`. A whole ARRAY renders via
            // `__rtsadp_inspect` (`[ 1, 2, 3 ]`); a SCALAR pulled from a collection
            // (`o.a`, `arr[0]`, `arr.length`) is a normal PolyValue → ToString.
            // An array literal carrying an OBJECT element (`[{a:1}]`) still bails:
            // bun prints it MULTI-LINE while our object inspect is single-line, a
            // near-miss vs bun — kept bailed (honesty floor) until the formats
            // reconcile.
            if self.array_arg_has_object_element(a) {
                return unsupported!(
                    "console.log of an array containing an OBJECT element (object inspect format differs from bun's multi-line — a later increment)"
                );
            }
            // A whole ARRAY arg renders with the Bun/Node inspect form; a whole
            // OBJECT arg renders with the object form. Both box the heap word and
            // call the matching trampoline (top_level=1 — a top-level string stays
            // bare, nested strings are quoted inside the trampoline).
            let s = if self.is_whole_object_value(a) {
                let v = self.lower_expr(module, a)?;
                let boxed = self.box_value(v);
                let top = self.builder.ins().iconst(types::I64, 1);
                self.call_runtime(module, "__rtsadp_inspect_object", &[boxed, top])?
                    .expect("__rtsadp_inspect_object returns a value")
            } else if self.is_whole_array_value(a) {
                let v = self.lower_expr(module, a)?;
                let boxed = self.box_value(v);
                let top = self.builder.ins().iconst(types::I64, 1);
                self.call_runtime(module, "__rtsadp_inspect", &[boxed, top])?
                    .expect("__rtsadp_inspect returns a value")
            } else {
                let v = self.lower_expr(module, a)?;
                let boxed = self.box_value(v);
                // ToString this arg (real pool) → a string PolyValue word.
                self.call_runtime(module, "__rtsadp_to_string", &[boxed])?
                    .expect("__rtsadp_to_string returns a value")
            };
            line = Some(match line {
                None => s,
                Some(prev) => {
                    // prev + " " + s, both joins through the generic `+` (real
                    // STRING_CONCAT for string operands).
                    let with_space = self
                        .call_runtime(module, "__rtsadp_add", &[prev, space])?
                        .expect("__rtsadp_add returns a value");
                    self.call_runtime(module, "__rtsadp_add", &[with_space, s])?
                        .expect("__rtsadp_add returns a value")
                }
            });
        }
        let line = match line {
            Some(v) => v,
            None => {
                // console.log() with no args → print an empty line.
                let pv = abi_adapter::intern_poly("");
                self.builder.ins().iconst(types::I64, pv.raw() as i64)
            }
        };
        emit_marshal::emit_print_string_poly(module, self.builder, line);

        let v = self
            .builder
            .ins()
            .iconst(types::I64, value::PolyValue::undefined().raw() as i64);
        Ok(Val::tagged_kind(v, JsKind::Undefined))
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
                // Too many positional args (no rest to absorb them) → bail.
                if args.len() > user_params.len() {
                    return unsupported!(
                        "call to `{}` expects {} args, got {}",
                        sig.name,
                        user_params.len(),
                        args.len()
                    );
                }
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
            // For a non-rest fn, a spread mixed with other args (or anywhere but the
            // sole arg) is a later increment → bail rather than mis-marshal. A rest
            // fn handles mixed spread via `marshal_call_args`, so it skips this bail.
            if args
                .iter()
                .any(|a| matches!(a.kind, HirExprKind::Spread(_)))
            {
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

/// Whether an expr is the bare `console` identifier (the object of `console.log`).
fn is_console_ident(e: &HirExpr) -> bool {
    matches!(&e.kind, HirExprKind::Ident(n) if n == "console")
}
