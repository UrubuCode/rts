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
        // The numeric-format bridge (`num_to_fixed`/`num_to_precision`/
        // `num_to_exponential`/`num_to_string_radix`): each takes (n, arg) — a
        // number receiver + an int arg — and returns a GC string handle. They wrap
        // the irreducible Rust formatters (one source of truth); the `.ts`
        // `class Number` methods call them.
        if let Some(symbol) = engine_num_member(method) {
            return self.lower_engine_num(module, symbol, args);
        }
        // The string-method bridge (`str_to_upper`/`str_trim`/`str_char_at`/
        // `str_slice`/`str_index_of`/`str_includes`/`str_pad_start`/`str_concat`/
        // `str_replace`/…): the `.ts` `class String` methods call these with `this`
        // (the boxed primitive string) + 0..2 string/number args. Each wraps the
        // irreducible Rust `__RTS_FN_GL_STRING_*` impl (one source of truth). The
        // arg/return marshaling is uniform (a small descriptor table), so adding a
        // member is a data row, never new Cranelift code.
        if let Some(spec) = engine_str_member(method) {
            return self.lower_engine_str(module, spec, args);
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

    /// Lower an `engine.num_*(n, arg)` numeric-format call: marshal the number
    /// receiver to `F64` and the int arg to `I64`, call the wrapping extern, and
    /// box the returned string handle as a `TAG_STR` PolyValue. Both args come from
    /// the `.ts` `class Number` method (`this` + the digits/radix param); a `this`
    /// boxed-number word coerces to `F64` via the tag-selecting decode, and the
    /// (possibly-defaulted) int param coerces to `I64`.
    fn lower_engine_num(
        &mut self,
        module: &mut dyn Module,
        symbol: &'static str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        if args.len() != 2 {
            return unsupported!("engine.num_* expects 2 args (n, arg), got {}", args.len());
        }
        let n = self.lower_expr(module, &args[0])?;
        let n_f64 = self.coerce(n, Repr::Float64)?;
        let a = self.lower_expr(module, &args[1])?;
        let a_i64 = self.coerce(a, Repr::Int64)?;
        let res = self.call_runtime(module, symbol, &[n_f64, a_i64])?;
        let h = res.expect("engine.num_* returns a string handle");
        let w = emit_marshal::emit_box_real_string(module, self.builder, h);
        Ok(Val::tagged_kind(w, JsKind::Str))
    }

    /// Lower an `engine.str_*(s, ...args)` string-method call: marshal the string
    /// receiver `s` (a boxed primitive-string word) to its real GC handle, marshal
    /// each declared arg (a string → real handle, a number → i64), call the
    /// wrapping `__RTS_FN_GL_STRING_*` extern, and rebox the result per its return
    /// kind (string handle → `TAG_STR` PolyValue; number → `Int64`; bool → `Bool`).
    /// The first `.ts` arg is always `this` (the primitive string); the rest are the
    /// method's own params (already defaulted by the `.ts` prologue).
    fn lower_engine_str(
        &mut self,
        module: &mut dyn Module,
        spec: EngineStr,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        let want = 1 + spec.args.len();
        if args.len() != want {
            return unsupported!(
                "{} expects {} args (s, ...), got {}",
                spec.symbol,
                want,
                args.len()
            );
        }
        let mut call_args = Vec::with_capacity(want);
        // ---- receiver string `s` → real GC handle ----
        let s = self.lower_expr(module, &args[0])?;
        let s_word = self.box_value(s);
        let s_handle = emit_marshal::emit_table_load(module, self.builder, s_word);
        call_args.push(s_handle);
        // ---- explicit args per the descriptor ----
        for (a, &kind) in args[1..].iter().zip(spec.args) {
            let v = self.lower_expr(module, a)?;
            match kind {
                StrArg::Str => {
                    let w = self.box_value(v);
                    let h = emit_marshal::emit_table_load(module, self.builder, w);
                    call_args.push(h);
                }
                StrArg::Num => {
                    // A number index/count → i64. `numeric_to_i64` accepts a proven
                    // Int32/Int64/Float64 (a `number` param / default like the
                    // `slice` "to end" sentinel arrives as Float64) and truncates
                    // toward zero; a Tagged number is decoded via `coerce`.
                    let i = match v.repr {
                        Repr::Int32 | Repr::Int64 | Repr::Float64 => self.numeric_to_i64(v)?,
                        _ => self.coerce(v, Repr::Int64)?,
                    };
                    call_args.push(i);
                }
            }
        }
        let res = self.call_runtime(module, spec.symbol, &call_args)?;
        Ok(match spec.ret {
            StrRet::Str => {
                let h = res.expect("engine.str_* string member returns a handle");
                let w = emit_marshal::emit_box_real_string(module, self.builder, h);
                Val::tagged_kind(w, JsKind::Str)
            }
            StrRet::Num => {
                Val::new(res.expect("engine.str_* number member returns a value"), Repr::Int64)
            }
            StrRet::Bool => {
                Val::new(res.expect("engine.str_* bool member returns a value"), Repr::Bool)
            }
        })
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

/// Resolve an `engine.num_*` numeric-format member name to its real symbol. Each
/// takes `(n: number, arg: number) => string` and wraps a `__RTS_FN_GL_NUMBER_*`
/// formatter (the irreducible-format bridge for the `.ts` `class Number`).
fn engine_num_member(method: &str) -> Option<&'static str> {
    Some(match method {
        "num_to_string_radix" => "__RTS_FN_NS_ENGINE_NUM_TO_STRING_RADIX",
        "num_to_fixed" => "__RTS_FN_NS_ENGINE_NUM_TO_FIXED",
        "num_to_precision" => "__RTS_FN_NS_ENGINE_NUM_TO_PRECISION",
        "num_to_exponential" => "__RTS_FN_NS_ENGINE_NUM_TO_EXPONENTIAL",
        _ => return None,
    })
}

/// How an `engine.str_*` arg (after the receiver `s`) is marshaled.
#[derive(Clone, Copy)]
enum StrArg {
    /// A string arg → boxed to a string word, table-loaded to a real GC handle.
    Str,
    /// A number arg → coerced to i64 (index/count/length).
    Num,
}

/// How an `engine.str_*` result is reboxed.
#[derive(Clone, Copy)]
enum StrRet {
    /// A GC string handle → `TAG_STR` PolyValue.
    Str,
    /// A native i64 number (index/code unit).
    Num,
    /// A boolean (extern "C" i64 0/1).
    Bool,
}

/// The marshaling descriptor of an `engine.str_*` member: the real
/// `__RTS_FN_GL_STRING_*` symbol it wraps + how its (post-receiver) args and its
/// result marshal. The receiver `s` is always the first arg (a string handle).
#[derive(Clone, Copy)]
struct EngineStr {
    symbol: &'static str,
    args: &'static [StrArg],
    ret: StrRet,
}

/// Resolve an `engine.str_*` string-method member name to its marshaling
/// descriptor. Each wraps a `__RTS_FN_GL_STRING_*` Rust impl (the irreducible
/// Unicode-aware logic — one source of truth) the `.ts` `class String` bodies call.
fn engine_str_member(method: &str) -> Option<EngineStr> {
    use StrArg::{Num, Str};
    let row = |symbol, args: &'static [StrArg], ret| EngineStr { symbol, args, ret };
    Some(match method {
        "str_to_upper" => row("__RTS_FN_NS_ENGINE_STR_TO_UPPER", &[], StrRet::Str),
        "str_to_lower" => row("__RTS_FN_NS_ENGINE_STR_TO_LOWER", &[], StrRet::Str),
        "str_trim" => row("__RTS_FN_NS_ENGINE_STR_TRIM", &[], StrRet::Str),
        "str_trim_start" => row("__RTS_FN_NS_ENGINE_STR_TRIM_START", &[], StrRet::Str),
        "str_trim_end" => row("__RTS_FN_NS_ENGINE_STR_TRIM_END", &[], StrRet::Str),
        "str_char_at" => row("__RTS_FN_NS_ENGINE_STR_CHAR_AT", &[Num], StrRet::Str),
        "str_char_code_at" => row("__RTS_FN_NS_ENGINE_STR_CHAR_CODE_AT", &[Num], StrRet::Num),
        "str_at" => row("__RTS_FN_NS_ENGINE_STR_AT", &[Num], StrRet::Str),
        "str_repeat" => row("__RTS_FN_NS_ENGINE_STR_REPEAT", &[Num], StrRet::Str),
        "str_slice" => row("__RTS_FN_NS_ENGINE_STR_SLICE", &[Num, Num], StrRet::Str),
        "str_substring" => row("__RTS_FN_NS_ENGINE_STR_SUBSTRING", &[Num, Num], StrRet::Str),
        "str_index_of" => row("__RTS_FN_NS_ENGINE_STR_INDEX_OF", &[Str], StrRet::Num),
        "str_last_index_of" => row("__RTS_FN_NS_ENGINE_STR_LAST_INDEX_OF", &[Str], StrRet::Num),
        "str_includes" => row("__RTS_FN_NS_ENGINE_STR_INCLUDES", &[Str], StrRet::Bool),
        "str_starts_with" => row("__RTS_FN_NS_ENGINE_STR_STARTS_WITH", &[Str], StrRet::Bool),
        "str_ends_with" => row("__RTS_FN_NS_ENGINE_STR_ENDS_WITH", &[Str], StrRet::Bool),
        "str_pad_start" => row("__RTS_FN_NS_ENGINE_STR_PAD_START", &[Num, Str], StrRet::Str),
        "str_pad_end" => row("__RTS_FN_NS_ENGINE_STR_PAD_END", &[Num, Str], StrRet::Str),
        "str_concat" => row("__RTS_FN_NS_ENGINE_STR_CONCAT", &[Str], StrRet::Str),
        "str_replace" => row("__RTS_FN_NS_ENGINE_STR_REPLACE", &[Str, Str], StrRet::Str),
        "str_replace_all" => row("__RTS_FN_NS_ENGINE_STR_REPLACE_ALL", &[Str, Str], StrRet::Str),
        _ => return None,
    })
}
