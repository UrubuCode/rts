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
            // A whole OBJECT value receiver (`o.hasOwnProperty(k)`, `o.toString()`):
            // route to the ambient prelude `class Object` (object.ts), passing the
            // object as `this`. The `.ts` bodies use the shape-aware `engine.obj_*`
            // bridge. Arrays were already handled above; guard against an array
            // value slipping through. `Ok(None)` when the method is not on
            // `class Object` (the caller bails — never a guess).
            if !self.is_array_valued(object) {
                let recv = self.lower_expr(module, object)?;
                if let Some(val) =
                    self.try_primitive_class_method(module, recv, "Object", method, args)?
                {
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
                return self.try_method_dispatch_dynamic(module, recv, method, args);
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
                // sep must be a proven string (a regex separator is a later
                // increment). Lower + marshal it to a real string handle.
                let sep = self.lower_expr(module, &args[0])?;
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
            let v = self.lower_expr(module, a)?;
            let marshaled = self.marshal_arg(module, v, want)?;
            call_args.push(marshaled);
        }

        // ---- emit the typed call to the REAL symbol ----
        let ret = emit_marshal::emit_call(module, self.builder, spec.symbol, &call_args);

        // ---- marshal the result back to a PolyValue ----
        self.marshal_ret(module, spec.ret, ret)
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
            AbiType::Handle => {
                if !matches!(v.kind, JsKind::Str) {
                    return unsupported!(
                        "method arg wants a string handle but its kind is not statically a string ({:?})",
                        v.repr
                    );
                }
                let word = self.box_value(v);
                Ok(emit_marshal::emit_table_load(module, self.builder, word))
            }
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

    /// Coerce a proven-numeric `Val` to an i64 (index/count). A Tagged value is
    /// not accepted here (we cannot prove it numeric) — bail.
    pub(super) fn numeric_to_i64(&mut self, v: Val) -> FrontResult<Value> {
        match v.repr {
            Repr::Int32 | Repr::Int64 => Ok(v.v),
            // SATURATING f64→i64: a `number` index/count/depth arg that is
            // `Infinity`/`NaN`/out-of-range (e.g. `arr.flat(Infinity)`) yields a
            // defined clamp (i64::MIN/MAX/0) instead of a Cranelift trap (SIGILL).
            Repr::Float64 => Ok(self.builder.ins().fcvt_to_sint_sat(types::I64, v.v)),
            _ => unsupported!("method arg wants a number index but got {:?}", v.repr),
        }
    }

    /// Marshal a method result (the `call`'s Cranelift value, or `None` for void)
    /// back to a PolyValue `Val` per the return `AbiType`.
    fn marshal_ret(
        &mut self,
        module: &mut dyn Module,
        ret: AbiType,
        value: Option<Value>,
    ) -> FrontResult<Val> {
        match ret {
            // A returned string/object handle → box as a TAG_STR PolyValue (the
            // GL_STRING methods all return strings).
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
