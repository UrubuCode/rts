//! Call lowering + truthiness for the whole-program path.
//!
//! Split out of [`super::expr`] (the <500-line module rule). Covers the two
//! Tagged-boundary call shapes the engine runs — `console.log(...)` and
//! cross-function calls — plus the JS `ToBoolean` reduction
//! ([`Lowerer::as_bool_value`]) used by `if`/`while`/ternary conditions.

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{types, InstBuilder, Value};
use cranelift_module::Module;

use rts_hir::ir::HirExprKind;
use rts_hir::HirExpr;

use crate::repr::Repr;
use crate::value;
use crate::value::{abi_adapter, emit_marshal};

use crate::front::error::{unsupported, FrontResult, Unsupported};

use super::lower::{HeapShape, JsKind, Lowerer, Val};
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
        if is_console_ident(object) && method == "log" {
            return self.lower_console_log(module, args);
        }
        // GLOBAL static `Array.m(..)` / `String.m(..)` (P5.2).
        if let Some(val) = self.try_global_static_call(module, object, method, args)? {
            return Ok(val);
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
            // GLOBAL static `Array.m(..)` / `String.m(..)` (P5.2) — before the
            // class-instance/primordial dispatch, since `Array`/`String` are not
            // user classes.
            if let Some(val) = self.try_global_static_call(module, object, prop, args)? {
                return Ok(val);
            }
            // `recv.method(args)` lowered as `Call(Member)` — route to dispatch.
            if let Some(val) = self.try_method_dispatch(module, object, prop, args)? {
                return Ok(val);
            }
            return unsupported!("call of member `.{prop}()` (receiver class not statically dispatchable)");
        }
        let name = match &callee.kind {
            HirExprKind::Ident(n) => n.clone(),
            _ => return unsupported!("call of a non-identifier callee"),
        };
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

    /// Reify a user function `name` (referenced as a VALUE) into a `TAG_FUNCTION`
    /// PolyValue: `func_addr` of its uniform-ABI THUNK → `__rtsadp_fn_reify(addr,
    /// nparams, has_rest)` → box the returned 48-bit slot+shard as `TAG_FUNCTION`.
    /// The GC marks it (a real heap handle behind the tag), so it is GC-safe.
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
        let nparams = sig.params.len() as i64;
        let thunk_id = *self
            .thunks
            .get(name)
            .ok_or_else(|| Unsupported::new(format!("no thunk for function `{name}`")))?;

        // Relocatable address of the THUNK (available at JIT time). `func_addr`
        // points at the thunk so every indirect call uses the fixed uniform ABI.
        let func_ref = module.declare_func_in_func(thunk_id, self.builder.func);
        let addr = self.builder.ins().func_addr(types::I64, func_ref);

        let nparams_v = self.builder.ins().iconst(types::I64, nparams);
        // No `...rest` in this increment's reify surface (variadic arrows are
        // rejected at extraction); has_rest is always 0.
        let has_rest_v = self.builder.ins().iconst(types::I64, 0);
        let payload = self
            .call_runtime(module, "__rtsadp_fn_reify", &[addr, nparams_v, has_rest_v])?
            .expect("__rtsadp_fn_reify returns a value");

        // Box the bare 48-bit payload as a TAG_FUNCTION PolyValue word:
        // BOX_BASE | (TAG_FUNCTION<<48) | (payload & PAYLOAD_MASK).
        let header = value::encode(value::TAG_FUNCTION, 0) as i64;
        let mask = self.builder.ins().iconst(types::I64, value::PAYLOAD_MASK as i64);
        let masked = self.builder.ins().band(payload, mask);
        let header_v = self.builder.ins().iconst(types::I64, header);
        let word = self.builder.ins().bor(masked, header_v);
        Ok(Val::tagged_kind(word, JsKind::Function))
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
    fn lower_console_log(
        &mut self,
        module: &mut dyn Module,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
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

    /// Lower a cross-function call: coerce each argument to the callee's param
    /// repr (box/unbox/widen per `FnSig`), emit the Cranelift `call`, and tag the
    /// result with the callee's return repr.
    fn lower_user_call(
        &mut self,
        module: &mut dyn Module,
        sig: &FnSig,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        if args.len() != sig.params.len() {
            return unsupported!(
                "call to `{}` expects {} args, got {}",
                sig.name,
                sig.params.len(),
                args.len()
            );
        }
        let mut lowered = Vec::with_capacity(args.len());
        for (a, &want) in args.iter().zip(&sig.params) {
            let v = self.lower_expr(module, a)?;
            lowered.push(self.coerce(v, want)?);
        }

        let cl_sig = sig.to_cranelift(module);
        let callee = module
            .declare_function(&sig.name, cranelift_module::Linkage::Local, &cl_sig)
            .map_err(|e| Unsupported::new(format!("declare callee `{}`: {e}", sig.name)))?;
        let func_ref = module.declare_func_in_func(callee, self.builder.func);
        let call = self.builder.ins().call(func_ref, &lowered);

        match sig.ret {
            Some(ret) => {
                let v = self.builder.inst_results(call)[0];
                Ok(Val::new(v, ret))
            }
            None => {
                let v = self
                    .builder
                    .ins()
                    .iconst(types::I64, value::PolyValue::undefined().raw() as i64);
                Ok(Val::tagged_kind(v, JsKind::Undefined))
            }
        }
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

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Whether `e` evaluates to a WHOLE OBJECT value (object literal, or an
    /// identifier bound to a local of proven OBJECT shape). Object inspect needs
    /// runtime key recovery (a later increment) — these BAIL.
    pub(super) fn is_whole_object_value(&self, e: &HirExpr) -> bool {
        match &e.kind {
            HirExprKind::Object(_) => true,
            // A class instance (`new C()`) is an OBJECT — its slot-0 global shape-id
            // lets the inspect trampoline render `{ field: value }` (P4.9).
            HirExprKind::New { class, .. } => self.classes.get(class).is_some(),
            HirExprKind::Ident(name) => {
                matches!(self.local_shapes.get(name), Some(HeapShape::Object(_)))
            }
            _ => false,
        }
    }

    /// Whether `e` evaluates to a WHOLE object OR array value. Used where BOTH
    /// kinds must bail (binary `+`/`==` ToPrimitive, method dispatch on a literal).
    pub(super) fn is_whole_heap_value(&self, e: &HirExpr) -> bool {
        self.is_whole_object_value(e) || self.is_whole_array_value(e)
    }

    /// Whether `e` is an array literal that (transitively) contains an OBJECT
    /// element — which would render as a keyless array (`[ 1 ]` for `{a:1}`), a
    /// near-miss vs bun's `{ a: 1 }`. Such logs BAIL until object inspect lands.
    /// Conservative: only array LITERALS are inspected statically (an array local's
    /// elements are opaque, but they can only become objects via paths that already
    /// bail), so this static walk covers the reachable near-miss.
    pub(super) fn array_arg_has_object_element(&self, e: &HirExpr) -> bool {
        match &e.kind {
            HirExprKind::Array(elems) => elems.iter().any(|el| self.is_object_producing(el)),
            _ => false,
        }
    }

    /// Whether `e` (an array element) statically produces an OBJECT value: an
    /// object literal, an array literal that itself contains an object, or an
    /// identifier bound to an object-shaped local.
    fn is_object_producing(&self, e: &HirExpr) -> bool {
        match &e.kind {
            HirExprKind::Object(_) => true,
            HirExprKind::Array(_) => self.array_arg_has_object_element(e),
            HirExprKind::Ident(name) => {
                matches!(self.local_shapes.get(name), Some(HeapShape::Object(_)))
            }
            _ => false,
        }
    }

    /// Whether `e` evaluates to a WHOLE ARRAY value (array literal, or an
    /// identifier bound to a local of proven ARRAY shape). These render via
    /// `__rtsadp_inspect` (`[ … ]`). A scalar member/index access is NOT one.
    pub(super) fn is_whole_array_value(&self, e: &HirExpr) -> bool {
        match &e.kind {
            HirExprKind::Array(_) => true,
            HirExprKind::Ident(name) => {
                matches!(self.local_shapes.get(name), Some(HeapShape::Array))
            }
            _ => false,
        }
    }
}

/// Whether an expr is the bare `console` identifier (the object of `console.log`).
fn is_console_ident(e: &HirExpr) -> bool {
    matches!(&e.kind, HirExprKind::Ident(n) if n == "console")
}
