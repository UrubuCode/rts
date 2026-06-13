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

use cranelift_codegen::ir::{types, InstBuilder, Value};
use cranelift_module::Module;

use rts_hir::ir::HirExprKind;
use rts_hir::HirExpr;

use rts_runtime::abi::AbiType;

use crate::dispatch::{resolve_method, MethodSpec, RecvAbi, RecvClass};
use crate::repr::Repr;
use crate::value::{self, emit_marshal};

use crate::front::error::{unsupported, FrontResult};

use super::lower::{HeapShape, JsKind, Lowerer, Val};

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
        // Any callback/function-valued argument needs function VALUES — bail.
        for a in args {
            if is_callback_arg(a) {
                return Err(crate::front::error::Unsupported::new(format!(
                    "method `.{method}()` with a callback argument (function values are a later increment)"
                )));
            }
        }

        // ARRAY receiver (P4.5): an identifier bound to a local of proven array
        // shape. The element convention is the engine's own (boxed PolyValue
        // words), so these resolve to the codegen-owned `__rtsadp_arr_*`
        // trampolines. Must be checked BEFORE the whole-heap-value gate below
        // (an array IS a whole-heap value).
        if self.is_array_receiver(object) {
            return self.try_array_dispatch(module, object, method, args).map(Some);
        }

        // The receiver must lower AND have a statically-proven class. Lower it
        // first (a whole object/array value is not a dispatch receiver here).
        if self.is_whole_heap_value(object) {
            // A whole OBJECT value receiver: object method dispatch is a later
            // increment. Let the caller bail with its own message rather than guess.
            return Ok(None);
        }
        let recv = self.lower_expr(module, object)?;
        let Some(class) = recv_class_of(recv) else {
            return Ok(None);
        };

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

    /// Whether `object` is statically a proven ARRAY: a bare identifier bound to
    /// a local recorded as [`HeapShape::Array`]. Only such an array is a P4.5
    /// dispatch receiver; everything else (an object, a param, a call result)
    /// falls through to the non-array path.
    fn is_array_receiver(&self, object: &HirExpr) -> bool {
        matches!(
            &object.kind,
            HirExprKind::Ident(name) if matches!(self.local_shapes.get(name), Some(HeapShape::Array))
        )
    }

    /// Lower `arr.method(args)` for a proven-array receiver via the codegen-owned
    /// `__rtsadp_arr_*` trampolines. The receiver marshals as the array's REAL Vec
    /// handle (`POLY_TO_HANDLE`); args/return follow the Array row's `AbiType`s
    /// (`U64` = raw PolyValue word, `I64` = index, `Handle` = string sep). An
    /// unknown `(method, arity)` BAILS explicitly (never a guess).
    fn try_array_dispatch(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        let argc = args.len();
        let Some(spec) = resolve_method(RecvClass::Array, method, argc) else {
            return Err(crate::front::error::Unsupported::new(format!(
                "no Registry entry for `Array.{method}({argc} args)` \
                 (callback methods like .map/.filter/.reduce bail by design)"
            )));
        };
        debug_assert!(matches!(spec.recv_abi, RecvAbi::ArrayVec));

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
    /// - `U64`: a raw PolyValue word. For `slice` it is a fresh TAG_OBJECT ARRAY
    ///   word (kind `Array`, so the `let` records `HeapShape::Array`); for
    ///   `at`/`pop` it is an opaque element word (kind `Unknown`).
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
                let kind = if method == "slice" { JsKind::Array } else { JsKind::Unknown };
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
    fn numeric_to_i64(&mut self, v: Val) -> FrontResult<Value> {
        match v.repr {
            Repr::Int32 | Repr::Int64 => Ok(v.v),
            Repr::Float64 => Ok(self.builder.ins().fcvt_to_sint(types::I64, v.v)),
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

/// Whether an argument expression is a callback (a function/arrow value). Such
/// methods need function VALUES — a later increment — so they bail.
fn is_callback_arg(e: &HirExpr) -> bool {
    matches!(&e.kind, HirExprKind::Arrow { .. })
}
