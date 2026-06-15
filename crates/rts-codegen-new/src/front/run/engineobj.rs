//! `engine.*` — the PRIVATE engine-internal ambient global (arch/time/trace).
//!
//! `engine` re-exposes already-existing runtime functionality to the engine's OWN
//! embedded TS prelude (future `Error.ts`/`Date.ts`): `engine.arch()`,
//! `engine.now_ms()`/`now_ns()`/`unix_ms()`/`unix_ns()`, and the trace passthrough
//! (`trace_capture()`/`trace_print()`/`trace_push(...)`/`trace_pop()`) error
//! stacks need. It is NOT a value (like `Math`/`Number`), so a bare `engine`
//! identifier still bails (no value model).
//!
//! ## Privacy gate
//!
//! The engine has no import resolver — ambient globals are recognized BY NAME at
//! lowering time. `engine` is PRIVATE: only a PRELUDE-origin function (a function
//! that came from the engine's embedded TS includes — [`Lowerer::is_prelude`]) may
//! resolve it. A USER function (including the synthesized `__rtsn_main`) that names
//! `engine.*` gets an EXPLICIT `Unsupported` bail (the honest deny — never a
//! silent allow, never a wrong value). A user local/class named `engine` shadows
//! the global and is handled by the normal local/class paths before this is
//! reached.
//!
//! Each call lowers to a `call_runtime` of the matching `__RTS_FN_NS_ENGINE_*`
//! symbol, marshaled through the same Registry-style ABI path the other runtime
//! calls use: a `Handle` (string) return is boxed as a `TAG_STR` PolyValue; the
//! `I64` timestamps return a native `Int64`; the `Void` trace ops return
//! `undefined`.

use cranelift_codegen::ir::InstBuilder;
use cranelift_codegen::ir::types;
use cranelift_module::Module;

use rts_hir::HirExpr;
use rts_hir::ir::HirExprKind;

use crate::repr::Repr;
use crate::value::emit_marshal;

use crate::front::error::{FrontResult, unsupported};

use super::lower::{JsKind, Lowerer, Val};

/// How an `engine.method` lowers: the real `__RTS_FN_NS_ENGINE_*` symbol + a tag
/// describing how to rebox its result.
enum EngRet {
    /// A GC string handle → box as a `TAG_STR` PolyValue (`arch`, `trace_capture`).
    Str,
    /// A native `i64` timestamp (`now_ms`/`now_ns`/`unix_ms`/`unix_ns`).
    I64,
    /// A `void` op → `undefined` (`trace_pop`, `trace_print`).
    Void,
}

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Try to lower an `engine.m(args)` static call. Returns `Ok(None)` when
    /// `object` is not the bare `engine` global (so the caller falls through to its
    /// next handler), the explicit PRIVACY bail when a USER function names it, or
    /// the lowered call when a PRELUDE function names it.
    pub(super) fn try_engine_call(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Option<Val>> {
        let HirExprKind::Ident(name) = &object.kind else {
            return Ok(None);
        };
        if name != "engine" {
            return Ok(None);
        }
        // A local/user-class named `engine` shadows the private global — let the
        // normal paths own it (this handler only claims the bare global).
        if self.local(name).is_some() || self.classes.get(name).is_some() {
            return Ok(None);
        }
        // PRIVACY GATE: only prelude-origin code may reach the private namespace.
        if !self.is_prelude {
            return unsupported!(
                "`engine.{method}()` is a PRIVATE engine-internal API (usable only from the engine's embedded prelude, not user code)"
            );
        }
        self.lower_engine_call(module, method, args).map(Some)
    }

    /// Lower `engine.method(args)` (already gated to a prelude-origin caller).
    fn lower_engine_call(
        &mut self,
        module: &mut dyn Module,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        // `trace_push(file, fn, line, col)` is the one member taking args; it
        // marshals two strings + two numbers. Everything else is a 0-arg op.
        if method == "trace_push" {
            return self.lower_engine_trace_push(module, args);
        }
        let Some((symbol, ret)) = engine_member(method) else {
            return unsupported!("engine.{method}(...) (unknown engine member)");
        };
        if !args.is_empty() {
            return unsupported!("engine.{method} takes no args (got {})", args.len());
        }
        let res = self.call_runtime(module, symbol, &[])?;
        Ok(self.rebox_engine_ret(module, ret, res))
    }

    /// Lower `engine.trace_push(file: string, fn: string, line: number, col: number)`
    /// — marshal the two string args (each `StrPtr` = ptr+len via the real pool)
    /// and the two numeric args, call the void extern, return `undefined`.
    fn lower_engine_trace_push(
        &mut self,
        module: &mut dyn Module,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        if args.len() != 4 {
            return unsupported!("engine.trace_push expects 4 args, got {}", args.len());
        }
        // file ptr+len, fn ptr+len: box each arg to a string PolyValue word,
        // recover the real handle (table-load), then split into the StrPtr 2-slot
        // (ptr, len) shape the void extern expects.
        let mut slots = Vec::with_capacity(6);
        for i in 0..2 {
            let v = self.lower_expr(module, &args[i])?;
            let boxed = self.box_value(v);
            let handle = emit_marshal::emit_table_load(module, self.builder, boxed);
            let (ptr, len) = emit_marshal::emit_string_ptr_len(module, self.builder, handle);
            slots.push(ptr);
            slots.push(len);
        }
        // line, col → native i64.
        for i in 2..4 {
            let v = self.lower_expr(module, &args[i])?;
            slots.push(self.coerce(v, Repr::Int64)?);
        }
        self.call_runtime(module, "__RTS_FN_NS_ENGINE_TRACE_PUSH", &slots)?;
        let undef = self
            .builder
            .ins()
            .iconst(types::I64, crate::value::PolyValue::undefined().raw() as i64);
        Ok(Val::tagged_kind(undef, JsKind::Undefined))
    }

    /// Rebox a 0-arg `engine.*` call's result per its declared return kind.
    fn rebox_engine_ret(
        &mut self,
        module: &mut dyn Module,
        ret: EngRet,
        res: Option<cranelift_codegen::ir::Value>,
    ) -> Val {
        match ret {
            EngRet::Str => {
                let h = res.expect("engine string member returns a handle");
                let w = emit_marshal::emit_box_real_string(module, self.builder, h);
                Val::tagged_kind(w, JsKind::Str)
            }
            EngRet::I64 => Val::new(res.expect("engine i64 member returns a value"), Repr::Int64),
            EngRet::Void => {
                let undef = self
                    .builder
                    .ins()
                    .iconst(types::I64, crate::value::PolyValue::undefined().raw() as i64);
                Val::tagged_kind(undef, JsKind::Undefined)
            }
        }
    }
}

/// Resolve a 0-arg `engine.method` name to its real symbol + return kind.
/// `trace_push` is handled separately (it takes args).
fn engine_member(method: &str) -> Option<(&'static str, EngRet)> {
    Some(match method {
        "arch" => ("__RTS_FN_NS_ENGINE_ARCH", EngRet::Str),
        "now_ms" => ("__RTS_FN_NS_ENGINE_NOW_MS", EngRet::I64),
        "now_ns" => ("__RTS_FN_NS_ENGINE_NOW_NS", EngRet::I64),
        "unix_ms" => ("__RTS_FN_NS_ENGINE_UNIX_MS", EngRet::I64),
        "unix_ns" => ("__RTS_FN_NS_ENGINE_UNIX_NS", EngRet::I64),
        "trace_capture" => ("__RTS_FN_NS_ENGINE_TRACE_CAPTURE", EngRet::Str),
        "trace_pop" => ("__RTS_FN_NS_ENGINE_TRACE_POP", EngRet::Void),
        "trace_print" => ("__RTS_FN_NS_ENGINE_TRACE_PRINT", EngRet::Void),
        _ => return None,
    })
}
