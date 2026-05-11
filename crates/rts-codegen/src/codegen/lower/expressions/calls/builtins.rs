//! Builtins de tipo: string/number/array/console/map+set.
//!
//! Cada `lower_*_builtin` reescreve uma chamada `recv.method(...)` em
//! IR Cranelift direto quando o codegen pode resolver pelo tipo do
//! receiver. Quando nao consegue, retorna `None` e o caller cai em
//! caminhos genericos (lower_var_member_call etc).

use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{InstBuilder, types as cl};
use swc_ecma_ast::{CallExpr, Expr};

use crate::codegen::lower::ctx::{FnCtx, TypedVal, ValTy};
use super::super::lower_expr;
use super::emit_user_fn_addr;

pub(super) fn lower_string_builtin(
    ctx: &mut FnCtx,
    method: &str,
    recv_h: cranelift_codegen::ir::Value,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    // Helper: lower arg[idx] -> handle i64.
    fn arg_handle(ctx: &mut FnCtx, call: &CallExpr, idx: usize) -> Result<cranelift_codegen::ir::Value> {
        let arg = call.args.get(idx).ok_or_else(|| anyhow!("missing arg #{idx}"))?;
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in string builtin"));
        }
        let tv = lower_expr(ctx, &arg.expr)?;
        Ok(ctx.coerce_to_handle(tv)?.val)
    }

    fn arg_i64(ctx: &mut FnCtx, call: &CallExpr, idx: usize) -> Result<cranelift_codegen::ir::Value> {
        let arg = call.args.get(idx).ok_or_else(|| anyhow!("missing arg #{idx}"))?;
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in string builtin"));
        }
        let tv = lower_expr(ctx, &arg.expr)?;
        Ok(ctx.coerce_to_i64(tv).val)
    }

    // Macro para chamadas simples: symbol(recv [, args...]) -> ret_ty
    macro_rules! call_h {
        ($sym:expr, $params:expr, $ret:expr, $args:expr) => {{
            let f = ctx.get_extern($sym, $params, $ret)?;
            let i = ctx.builder.ins().call(f, $args);
            ctx.builder.inst_results(i)[0]
        }};
    }

    match method {
        // ── length as method call (parens) ── kept for compat
        "length" => {
            let v = call_h!("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64), &[recv_h]);
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        // ── search ──────────────────────────────────────────────────────
        "indexOf" => {
            let needle = arg_handle(ctx, call, 0)?;
            let v = call_h!("__RTS_FN_GL_STRING_INDEX_OF", &[cl::I64, cl::I64], Some(cl::I64), &[recv_h, needle]);
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "lastIndexOf" => {
            let needle = arg_handle(ctx, call, 0)?;
            let v = call_h!("__RTS_FN_GL_STRING_LAST_INDEX_OF", &[cl::I64, cl::I64], Some(cl::I64), &[recv_h, needle]);
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "includes" | "contains" => {
            let needle = arg_handle(ctx, call, 0)?;
            let v = call_h!("__RTS_FN_GL_STRING_INCLUDES", &[cl::I64, cl::I64], Some(cl::I64), &[recv_h, needle]);
            Ok(Some(TypedVal::new(v, ValTy::Bool)))
        }
        "startsWith" | "starts_with" => {
            let prefix = arg_handle(ctx, call, 0)?;
            // (#208) startsWith(prefix, position?).
            if call.args.len() >= 2 {
                let pos = arg_i64(ctx, call, 1)?;
                let v = call_h!(
                    "__RTS_FN_GL_STRING_STARTS_WITH_AT",
                    &[cl::I64, cl::I64, cl::I64],
                    Some(cl::I64),
                    &[recv_h, prefix, pos]
                );
                return Ok(Some(TypedVal::new(v, ValTy::Bool)));
            }
            let v = call_h!("__RTS_FN_GL_STRING_STARTS_WITH", &[cl::I64, cl::I64], Some(cl::I64), &[recv_h, prefix]);
            Ok(Some(TypedVal::new(v, ValTy::Bool)))
        }
        "endsWith" | "ends_with" => {
            let suffix = arg_handle(ctx, call, 0)?;
            // (#208) endsWith(suffix, endPosition?).
            if call.args.len() >= 2 {
                let end_pos = arg_i64(ctx, call, 1)?;
                let v = call_h!(
                    "__RTS_FN_GL_STRING_ENDS_WITH_AT",
                    &[cl::I64, cl::I64, cl::I64],
                    Some(cl::I64),
                    &[recv_h, suffix, end_pos]
                );
                return Ok(Some(TypedVal::new(v, ValTy::Bool)));
            }
            let v = call_h!("__RTS_FN_GL_STRING_ENDS_WITH", &[cl::I64, cl::I64], Some(cl::I64), &[recv_h, suffix]);
            Ok(Some(TypedVal::new(v, ValTy::Bool)))
        }
        // ── indexing ─────────────────────────────────────────────────────
        "charAt" => {
            let idx = arg_i64(ctx, call, 0)?;
            let v = call_h!("__RTS_FN_GL_STRING_CHAR_AT", &[cl::I64, cl::I64], Some(cl::I64), &[recv_h, idx]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "charCodeAt" => {
            let idx = arg_i64(ctx, call, 0)?;
            let v = call_h!("__RTS_FN_GL_STRING_CHAR_CODE_AT", &[cl::I64, cl::I64], Some(cl::I64), &[recv_h, idx]);
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "codePointAt" => {
            let idx = arg_i64(ctx, call, 0)?;
            let v = call_h!("__RTS_FN_GL_STRING_CODE_POINT_AT", &[cl::I64, cl::I64], Some(cl::I64), &[recv_h, idx]);
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "at" => {
            let idx = arg_i64(ctx, call, 0)?;
            let v = call_h!("__RTS_FN_GL_STRING_AT", &[cl::I64, cl::I64], Some(cl::I64), &[recv_h, idx]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        // ── slicing ───────────────────────────────────────────────────────
        "slice" => {
            let start = arg_i64(ctx, call, 0)?;
            let end = if call.args.len() > 1 {
                arg_i64(ctx, call, 1)?
            } else {
                ctx.builder.ins().iconst(cl::I64, i64::MAX)
            };
            let v = call_h!("__RTS_FN_GL_STRING_SLICE", &[cl::I64, cl::I64, cl::I64], Some(cl::I64), &[recv_h, start, end]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "substring" => {
            let start = arg_i64(ctx, call, 0)?;
            let end = if call.args.len() > 1 {
                arg_i64(ctx, call, 1)?
            } else {
                ctx.builder.ins().iconst(cl::I64, i64::MAX)
            };
            let v = call_h!("__RTS_FN_GL_STRING_SUBSTRING", &[cl::I64, cl::I64, cl::I64], Some(cl::I64), &[recv_h, start, end]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "substr" => {
            let start = arg_i64(ctx, call, 0)?;
            let len = if call.args.len() > 1 {
                arg_i64(ctx, call, 1)?
            } else {
                ctx.builder.ins().iconst(cl::I64, i64::MAX)
            };
            let v = call_h!("__RTS_FN_GL_STRING_SUBSTR", &[cl::I64, cl::I64, cl::I64], Some(cl::I64), &[recv_h, start, len]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        // ── transform ─────────────────────────────────────────────────────
        "toLowerCase" | "toLocaleLowerCase" => {
            let v = call_h!("__RTS_FN_GL_STRING_TO_LOWER_CASE", &[cl::I64], Some(cl::I64), &[recv_h]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "toUpperCase" | "toLocaleUpperCase" => {
            let v = call_h!("__RTS_FN_GL_STRING_TO_UPPER_CASE", &[cl::I64], Some(cl::I64), &[recv_h]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "trim" => {
            let v = call_h!("__RTS_FN_GL_STRING_TRIM", &[cl::I64], Some(cl::I64), &[recv_h]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "trimStart" | "trimLeft" | "trim_start" => {
            let v = call_h!("__RTS_FN_GL_STRING_TRIM_START", &[cl::I64], Some(cl::I64), &[recv_h]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "trimEnd" | "trimRight" | "trim_end" => {
            let v = call_h!("__RTS_FN_GL_STRING_TRIM_END", &[cl::I64], Some(cl::I64), &[recv_h]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "repeat" => {
            let n = arg_i64(ctx, call, 0)?;
            let v = call_h!("__RTS_FN_GL_STRING_REPEAT", &[cl::I64, cl::I64], Some(cl::I64), &[recv_h, n]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "replace" => {
            use swc_ecma_ast::{Expr, Lit};
            let is_regex = call
                .args
                .first()
                .map(|a| matches!(a.expr.as_ref(), Expr::Lit(Lit::Regex(_))))
                .unwrap_or(false);
            if is_regex {
                let pattern = arg_handle(ctx, call, 0)?;
                let to = arg_handle(ctx, call, 1)?;
                let p1 = call_h!("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64), &[recv_h]);
                let l1 = call_h!("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64), &[recv_h]);
                let v = call_h!(
                    "__RTS_FN_NS_STRING_REPLACE_REGEX",
                    &[cl::I64, cl::I64, cl::I64, cl::I64],
                    Some(cl::I64),
                    &[p1, l1, pattern, to]
                );
                return Ok(Some(TypedVal::new(v, ValTy::Handle)));
            }
            let from = arg_handle(ctx, call, 0)?;
            let to   = arg_handle(ctx, call, 1)?;
            let v = call_h!("__RTS_FN_GL_STRING_REPLACE", &[cl::I64, cl::I64, cl::I64], Some(cl::I64), &[recv_h, from, to]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "replaceAll" => {
            use swc_ecma_ast::{Expr, Lit};
            let is_regex = call
                .args
                .first()
                .map(|a| matches!(a.expr.as_ref(), Expr::Lit(Lit::Regex(_))))
                .unwrap_or(false);
            if is_regex {
                let pattern = arg_handle(ctx, call, 0)?;
                let to = arg_handle(ctx, call, 1)?;
                let p1 = call_h!("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64), &[recv_h]);
                let l1 = call_h!("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64), &[recv_h]);
                let v = call_h!(
                    "__RTS_FN_NS_STRING_REPLACE_REGEX",
                    &[cl::I64, cl::I64, cl::I64, cl::I64],
                    Some(cl::I64),
                    &[p1, l1, pattern, to]
                );
                return Ok(Some(TypedVal::new(v, ValTy::Handle)));
            }
            let from = arg_handle(ctx, call, 0)?;
            let to   = arg_handle(ctx, call, 1)?;
            let v = call_h!("__RTS_FN_GL_STRING_REPLACE_ALL", &[cl::I64, cl::I64, cl::I64], Some(cl::I64), &[recv_h, from, to]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "concat" => {
            let other = arg_handle(ctx, call, 0)?;
            let v = call_h!("__RTS_FN_GL_STRING_CONCAT", &[cl::I64, cl::I64], Some(cl::I64), &[recv_h, other]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "padStart" => {
            let target = arg_i64(ctx, call, 0)?;
            let pad = if call.args.len() > 1 {
                arg_handle(ctx, call, 1)?
            } else {
                ctx.emit_str_handle(b" ")?.val
            };
            let v = call_h!("__RTS_FN_GL_STRING_PAD_START", &[cl::I64, cl::I64, cl::I64], Some(cl::I64), &[recv_h, target, pad]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "padEnd" => {
            let target = arg_i64(ctx, call, 0)?;
            let pad = if call.args.len() > 1 {
                arg_handle(ctx, call, 1)?
            } else {
                ctx.emit_str_handle(b" ")?.val
            };
            let v = call_h!("__RTS_FN_GL_STRING_PAD_END", &[cl::I64, cl::I64, cl::I64], Some(cl::I64), &[recv_h, target, pad]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "split" => {
            let sep = arg_handle(ctx, call, 0)?;
            // (#208) split(sep, limit?) — limit truncamento opcional.
            if call.args.len() >= 2 {
                let limit = arg_i64(ctx, call, 1)?;
                let v = call_h!(
                    "__RTS_FN_GL_STRING_SPLIT_LIMIT",
                    &[cl::I64, cl::I64, cl::I64],
                    Some(cl::I64),
                    &[recv_h, sep, limit]
                );
                return Ok(Some(TypedVal::new(v, ValTy::Handle)));
            }
            let v = call_h!("__RTS_FN_GL_STRING_SPLIT", &[cl::I64, cl::I64], Some(cl::I64), &[recv_h, sep]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        // (#208) `s.match(pattern)` — primeiro match, retorna string handle ou 0.
        // Detecta se pattern eh regex literal (Expr::Regex) e usa
        // STRING_MATCH_REGEX que aceita Entry::Regex handle direto.
        "match" => {
            use swc_ecma_ast::{Expr, Lit};
            let is_regex_literal = call
                .args
                .first()
                .map(|a| matches!(a.expr.as_ref(), Expr::Lit(Lit::Regex(_))))
                .unwrap_or(false);
            let p1 = call_h!("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64), &[recv_h]);
            let l1 = call_h!("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64), &[recv_h]);
            let pattern = arg_handle(ctx, call, 0)?;
            if is_regex_literal {
                let v = call_h!(
                    "__RTS_FN_NS_STRING_MATCH_REGEX",
                    &[cl::I64, cl::I64, cl::I64],
                    Some(cl::I64),
                    &[p1, l1, pattern]
                );
                return Ok(Some(TypedVal::new(v, ValTy::Handle)));
            }
            let p2 = call_h!("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64), &[pattern]);
            let l2 = call_h!("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64), &[pattern]);
            let v = call_h!(
                "__RTS_FN_NS_STRING_MATCH",
                &[cl::I64, cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
                &[p1, l1, p2, l2]
            );
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        // (#208) `s.search(pattern)` — index do primeiro match, ou -1.
        "search" => {
            use swc_ecma_ast::{Expr, Lit};
            let is_regex = call
                .args
                .first()
                .map(|a| matches!(a.expr.as_ref(), Expr::Lit(Lit::Regex(_))))
                .unwrap_or(false);
            let p1 = call_h!("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64), &[recv_h]);
            let l1 = call_h!("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64), &[recv_h]);
            let pattern = arg_handle(ctx, call, 0)?;
            if is_regex {
                let v = call_h!(
                    "__RTS_FN_NS_STRING_SEARCH_REGEX",
                    &[cl::I64, cl::I64, cl::I64],
                    Some(cl::I64),
                    &[p1, l1, pattern]
                );
                return Ok(Some(TypedVal::new(v, ValTy::I64)));
            }
            let p2 = call_h!("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64), &[pattern]);
            let l2 = call_h!("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64), &[pattern]);
            let v = call_h!(
                "__RTS_FN_NS_STRING_SEARCH",
                &[cl::I64, cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
                &[p1, l1, p2, l2]
            );
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        // (#208) `s.matchAll(pattern)` — Vec de string handles, um por match.
        "matchAll" => {
            let pattern = arg_handle(ctx, call, 0)?;
            let p1 = call_h!("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64), &[recv_h]);
            let l1 = call_h!("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64), &[recv_h]);
            let p2 = call_h!("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64), &[pattern]);
            let l2 = call_h!("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64), &[pattern]);
            let v = call_h!(
                "__RTS_FN_NS_STRING_MATCH_ALL",
                &[cl::I64, cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
                &[p1, l1, p2, l2]
            );
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "localeCompare" => {
            let other = arg_handle(ctx, call, 0)?;
            let v = call_h!("__RTS_FN_GL_STRING_LOCALE_COMPARE", &[cl::I64, cl::I64], Some(cl::I64), &[recv_h, other]);
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "toString" | "valueOf" | "toWellFormed" | "normalize" => {
            let v = call_h!("__RTS_FN_GL_STRING_TO_STRING", &[cl::I64], Some(cl::I64), &[recv_h]);
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "isWellFormed" => {
            let v = call_h!("__RTS_FN_GL_STRING_IS_WELL_FORMED", &[cl::I64], Some(cl::I64), &[recv_h]);
            Ok(Some(TypedVal::new(v, ValTy::Bool)))
        }
        _ => Ok(None),
    }
}

/// Number instance methods em receiver F64/I64 (n.toFixed(), n.toString(), etc.).
/// Retorna `Some` quando reconheceu o metodo — semântica idêntica ao lower_string_builtin.
pub(super) fn lower_number_builtin(
    ctx: &mut FnCtx,
    method: &str,
    recv_f: cranelift_codegen::ir::Value,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    // arg[idx] como i64 (digits, radix)
    fn arg_i64_opt(
        ctx: &mut FnCtx,
        call: &CallExpr,
        idx: usize,
        default: i64,
    ) -> Result<cranelift_codegen::ir::Value> {
        if let Some(arg) = call.args.get(idx) {
            if arg.spread.is_some() {
                return Ok(ctx.builder.ins().iconst(cl::I64, default));
            }
            let tv = lower_expr(ctx, &arg.expr)?;
            Ok(ctx.coerce_to_i64(tv).val)
        } else {
            Ok(ctx.builder.ins().iconst(cl::I64, default))
        }
    }

    macro_rules! call_num {
        ($sym:expr, $params:expr, $ret:expr, $args:expr) => {{
            let f = ctx.get_extern($sym, $params, $ret)?;
            let i = ctx.builder.ins().call(f, $args);
            ctx.builder.inst_results(i)[0]
        }};
    }

    match method {
        "toFixed" => {
            let digits = arg_i64_opt(ctx, call, 0, 0)?;
            let v = call_num!(
                "__RTS_FN_GL_NUMBER_TO_FIXED",
                &[cl::F64, cl::I64],
                Some(cl::I64),
                &[recv_f, digits]
            );
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "toPrecision" => {
            let digits = arg_i64_opt(ctx, call, 0, 0)?;
            let v = call_num!(
                "__RTS_FN_GL_NUMBER_TO_PRECISION",
                &[cl::F64, cl::I64],
                Some(cl::I64),
                &[recv_f, digits]
            );
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "toExponential" => {
            let digits = arg_i64_opt(ctx, call, 0, 6)?;
            let v = call_num!(
                "__RTS_FN_GL_NUMBER_TO_EXPONENTIAL",
                &[cl::F64, cl::I64],
                Some(cl::I64),
                &[recv_f, digits]
            );
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "toString" => {
            // toString(radix?) — sem radix ou radix=10 usa string_from_f64.
            // Com radix usa __RTS_FN_GL_NUMBER_TO_STRING_RADIX.
            let radix = arg_i64_opt(ctx, call, 0, 10)?;
            let v = call_num!(
                "__RTS_FN_GL_NUMBER_TO_STRING_RADIX",
                &[cl::F64, cl::I64],
                Some(cl::I64),
                &[recv_f, radix]
            );
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "valueOf" => Ok(Some(TypedVal::new(recv_f, ValTy::F64))),
        "toLocaleString" => {
            // Stub: mesma saída que toString() sem localização.
            let v = call_num!(
                "__RTS_FN_NS_GC_STRING_FROM_F64",
                &[cl::F64],
                Some(cl::I64),
                &[recv_f]
            );
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        _ => Ok(None),
    }
}

/// Map/Set methods (#222) em receiver Handle. v0 mapeia direto pra
/// collections.map_* (mesmo backing store). Set usa Map<key, 1> com
/// key sempre string — limitacao aceita de v0.
///
/// Reconhecidos: set/get/has/delete/clear/add/size. Para `m.size`
/// (sem parens) ainda nao tem caminho — usuario chama `m.size()` em v0.
pub(super) fn lower_map_set_builtin(
    ctx: &mut FnCtx,
    method: &str,
    recv_h: cranelift_codegen::ir::Value,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    fn arg_strptr(
        ctx: &mut FnCtx,
        call: &CallExpr,
        idx: usize,
    ) -> Result<(cranelift_codegen::ir::Value, cranelift_codegen::ir::Value)> {
        let arg = call
            .args
            .get(idx)
            .ok_or_else(|| anyhow!("missing arg #{idx}"))?;
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported"));
        }
        let tv = lower_expr(ctx, &arg.expr)?;
        // Coerce qualquer valor a string handle (string_from_i64 / passthrough).
        let h = ctx.coerce_to_handle(tv)?.val;
        let ptr_fref =
            ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
        let len_fref =
            ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
        let pi = ctx.builder.ins().call(ptr_fref, &[h]);
        let p = ctx.builder.inst_results(pi)[0];
        let li = ctx.builder.ins().call(len_fref, &[h]);
        let l = ctx.builder.inst_results(li)[0];
        Ok((p, l))
    }

    // Arity check: a heuristica so' deve disparar quando o nº de args
    // bate com a assinatura JS de Map/Set. Sem isso, qualquer obj literal
    // com chave `add`/`set`/`get`/`has`/`delete` que carregue uma fn de
    // user com aridade diferente cai aqui e retorna lixo (#311). Caller
    // recebe None e tenta o path generico de map_get + call_indirect.
    let arity = call.args.len();
    let expected_arity = match method {
        "set" => 2, // Map.set(key, value)
        "add" | "delete" | "has" | "get" => 1,
        _ => usize::MAX, // outros metodos nao tem checagem aqui
    };
    if expected_arity != usize::MAX && arity != expected_arity {
        return Ok(None);
    }
    match method {
        "set" => {
            // Map.set(key, value) — value pode ser handle ou number.
            let (kp, kl) = arg_strptr(ctx, call, 0)?;
            let val_arg = call
                .args
                .get(1)
                .ok_or_else(|| anyhow!("Map.set requires value"))?;
            let val_tv = lower_expr(ctx, &val_arg.expr)?;
            let val_i64 = ctx.coerce_to_i64(val_tv).val;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_SET",
                &[cl::I64, cl::I64, cl::I64, cl::I64],
                None,
            )?;
            ctx.builder.ins().call(fref, &[recv_h, kp, kl, val_i64]);
            // Map.set retorna o proprio map (chainable em JS).
            Ok(Some(TypedVal::new(recv_h, ValTy::Handle)))
        }
        "add" => {
            // Set.add(value) → map_set(h, value, 1).
            let (kp, kl) = arg_strptr(ctx, call, 0)?;
            let one = ctx.builder.ins().iconst(cl::I64, 1);
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_SET",
                &[cl::I64, cl::I64, cl::I64, cl::I64],
                None,
            )?;
            ctx.builder.ins().call(fref, &[recv_h, kp, kl, one]);
            Ok(Some(TypedVal::new(recv_h, ValTy::Handle)))
        }
        "get" => {
            let (kp, kl) = arg_strptr(ctx, call, 0)?;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_GET",
                &[cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[recv_h, kp, kl]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "has" => {
            let (kp, kl) = arg_strptr(ctx, call, 0)?;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_HAS",
                &[cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[recv_h, kp, kl]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Bool)))
        }
        "delete" => {
            let (kp, kl) = arg_strptr(ctx, call, 0)?;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_DELETE",
                &[cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[recv_h, kp, kl]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Bool)))
        }
        "clear" => {
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_CLEAR",
                &[cl::I64],
                None,
            )?;
            ctx.builder.ins().call(fref, &[recv_h]);
            Ok(Some(TypedVal::new(
                ctx.builder.ins().iconst(cl::I64, 0),
                ValTy::I64,
            )))
        }
        "size" => {
            // Em JS `m.size` eh property; v0 aceita `m.size()` como method
            // call ate ter property access em handles.
            if !call.args.is_empty() {
                return Ok(None);
            }
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_LEN",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[recv_h]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        // (#222) Map iteration methods. Reusam fns de Object.keys/values/entries.
        "entries" if call.args.is_empty() => {
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_ENTRIES",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[recv_h]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "keys" if call.args.is_empty() => {
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_KEYS",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[recv_h]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "values" if call.args.is_empty() => {
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_VALUES",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[recv_h]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        _ => Ok(None),
    }
}

/// Console object (#221) — mapeia console.log/info/debug → io.print
/// e console.error/warn → io.eprint. Args sao concatenados como string
/// separados por espaco (semantica JS). Implementado em codegen direto
/// pra que `console.X(...)` funcione sem import explicito.
pub(super) fn lower_console_call(
    ctx: &mut FnCtx,
    qualified: &str,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    let Some(method) = qualified.strip_prefix("console.") else {
        return Ok(None);
    };

    let target_symbol: &str = match method {
        "log" | "info" | "debug" => "__RTS_FN_NS_IO_PRINT",
        "error" | "warn" => "__RTS_FN_NS_IO_EPRINT",
        _ => return Ok(None),
    };

    // Concatena todos os args como string. JS: separador eh " " entre args.
    // Caso 0 args: imprime linha vazia (io.print/eprint ja adicionam \n).
    let space = ctx.emit_str_handle(b" ")?.val;
    let mut acc: Option<cranelift_codegen::ir::Value> = None;
    let concat = ctx.get_extern(
        "__RTS_FN_NS_GC_STRING_CONCAT",
        &[cl::I64, cl::I64],
        Some(cl::I64),
    )?;

    // (#573) Para Handle ambiguo (retorno de var member call, WeakMap.get,
    // ?? heterogeneo, etc.), usa TPL_COERCE_AUTO que detecta string vs
    // numero em runtime. Literals string/template ja sao Handle conhecido,
    // skip da coercao auto via heuristica simples: arg e' Lit::Str ou Tpl.
    let auto_coerce = ctx.get_extern(
        "__RTS_FN_RT_TPL_COERCE_AUTO",
        &[cl::I64],
        Some(cl::I64),
    )?;
    for arg in &call.args {
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in console.* args"));
        }
        let is_known_str = matches!(
            arg.expr.as_ref(),
            Expr::Lit(swc_ecma_ast::Lit::Str(_)) | Expr::Tpl(_)
        );
        let tv = lower_expr(ctx, &arg.expr)?;
        // (#573) U64 tambem pode ser handle ambiguo (ex: JSON.parse retorno
        // de '42' eh i64 raw com tipo U64). Auto-coerce decide em runtime.
        let needs_auto = matches!(tv.ty, ValTy::Handle | ValTy::U64) && !is_known_str;
        let h = ctx.coerce_to_handle(tv)?.val;
        let h = if needs_auto {
            let inst = ctx.builder.ins().call(auto_coerce, &[h]);
            ctx.builder.inst_results(inst)[0]
        } else {
            h
        };
        acc = Some(match acc {
            None => h,
            Some(prev) => {
                let with_space = ctx.builder.ins().call(concat, &[prev, space]);
                let prev_sp = ctx.builder.inst_results(with_space)[0];
                let combined = ctx.builder.ins().call(concat, &[prev_sp, h]);
                ctx.builder.inst_results(combined)[0]
            }
        });
    }

    let msg_handle = match acc {
        Some(v) => v,
        None => ctx.emit_str_handle(b"")?.val,
    };

    // Extrai (ptr, len) do handle e chama io.print/eprint (assinatura StrPtr).
    let ptr_fref =
        ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
    let len_fref =
        ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
    let pi = ctx.builder.ins().call(ptr_fref, &[msg_handle]);
    let p = ctx.builder.inst_results(pi)[0];
    let li = ctx.builder.ins().call(len_fref, &[msg_handle]);
    let l = ctx.builder.inst_results(li)[0];

    let print_fref = ctx.get_extern(target_symbol, &[cl::I64, cl::I64], None)?;
    ctx.builder.ins().call(print_fref, &[p, l]);

    Ok(Some(TypedVal::new(
        ctx.builder.ins().iconst(cl::I64, 0),
        ValTy::I64,
    )))
}

/// Builtins universais para arrays/maps via handle. Retorna `Some` se
/// a chamada foi tratada como builtin.
pub(super) fn lower_array_builtin(
    ctx: &mut FnCtx,
    method: &str,
    obj_h: cranelift_codegen::ir::Value,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    match method {
        "push" => {
            if call.args.len() != 1 {
                return Ok(None);
            }
            let arg = &call.args[0];
            if arg.spread.is_some() {
                return Ok(None);
            }
            let push_fn = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
                &[cl::I64, cl::I64],
                None,
            )?;
            let tv = lower_expr(ctx, &arg.expr)?;
            let v = ctx.coerce_to_i64(tv).val;
            ctx.builder.ins().call(push_fn, &[obj_h, v]);
            // JS: push retorna novo length.
            let len_fn = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_LEN",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(len_fn, &[obj_h]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "pop" => {
            // JS: retorna o ultimo elemento (ou undefined). v0 retorna 0 quando vazio.
            let pop_fn = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_POP",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(pop_fn, &[obj_h]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "length" | "size" => {
            // length/size sao property em JS, mas v0 aceita como method call
            // (`arr.length()`) ate ter property access em handles.
            if !call.args.is_empty() {
                return Ok(None);
            }
            let len_fn = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_LEN",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(len_fn, &[obj_h]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "at" => {
            use cranelift_codegen::ir::condcodes::IntCC;
            let idx_arg = call.args.first().ok_or_else(|| anyhow!("at requires index"))?;
            let tv = lower_expr(ctx, &idx_arg.expr)?;
            let idx = ctx.coerce_to_i64(tv).val;
            // Negative indexing: idx = len + idx quando idx < 0.
            let len_fn = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_LEN",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let len_inst = ctx.builder.ins().call(len_fn, &[obj_h]);
            let len = ctx.builder.inst_results(len_inst)[0];
            let zero = ctx.builder.ins().iconst(cl::I64, 0);
            let is_neg = ctx.builder.ins().icmp(IntCC::SignedLessThan, idx, zero);
            let adjusted = ctx.builder.ins().iadd(len, idx);
            let final_idx = ctx.builder.ins().select(is_neg, adjusted, idx);
            let get_fn = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_GET",
                &[cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(get_fn, &[obj_h, final_idx]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "join" => {
            // arr.join(sep): converte sep para string handle, chama runtime.
            let sep_h = if let Some(arg) = call.args.first() {
                if arg.spread.is_some() {
                    return Ok(None);
                }
                let tv = lower_expr(ctx, &arg.expr)?;
                ctx.coerce_to_handle(tv)?.val
            } else {
                // Default JS: separador "," sem argumento.
                ctx.emit_str_handle(b",")?.val
            };
            let join_fn = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_JOIN",
                &[cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(join_fn, &[obj_h, sep_h]);
            let v = ctx.builder.inst_results(inst)[0];
            return Ok(Some(TypedVal::new(v, ValTy::Handle)));
        }
        "clear" => {
            let fref =
                ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_CLEAR", &[cl::I64], None)?;
            ctx.builder.ins().call(fref, &[obj_h]);
            Ok(Some(TypedVal::new(
                ctx.builder.ins().iconst(cl::I64, 0),
                ValTy::I64,
            )))
        }
        // (#208 / #476) Array methods sem callback — args concretos só.
        "indexOf" | "lastIndexOf" | "includes" => {
            if call.args.is_empty() || call.args.iter().any(|a| a.spread.is_some()) {
                return Ok(None);
            }
            let needle_tv = lower_expr(ctx, &call.args[0].expr)?;
            let needle = ctx.coerce_to_i64(needle_tv).val;
            // (#208) indexOf/lastIndexOf/includes(needle, fromIndex) — 2-arg.
            if matches!(method, "indexOf" | "lastIndexOf" | "includes")
                && call.args.len() == 2
            {
                let from_tv = lower_expr(ctx, &call.args[1].expr)?;
                let from = ctx.coerce_to_i64(from_tv).val;
                let sym = match method {
                    "indexOf" => "__RTS_FN_NS_COLLECTIONS_VEC_INDEX_OF_FROM",
                    "lastIndexOf" => "__RTS_FN_NS_COLLECTIONS_VEC_LAST_INDEX_OF_FROM",
                    _ => "__RTS_FN_NS_COLLECTIONS_VEC_INCLUDES_FROM",
                };
                let fref = ctx.get_extern(
                    sym,
                    &[cl::I64, cl::I64, cl::I64],
                    Some(cl::I64),
                )?;
                let inst = ctx.builder.ins().call(fref, &[obj_h, needle, from]);
                let v = ctx.builder.inst_results(inst)[0];
                let ty = if method == "includes" { ValTy::Bool } else { ValTy::I64 };
                return Ok(Some(TypedVal::new(v, ty)));
            }
            if call.args.len() != 1 {
                return Ok(None);
            }
            let sym = match method {
                "indexOf" => "__RTS_FN_NS_COLLECTIONS_VEC_INDEX_OF",
                "lastIndexOf" => "__RTS_FN_NS_COLLECTIONS_VEC_LAST_INDEX_OF",
                _ => "__RTS_FN_NS_COLLECTIONS_VEC_INCLUDES",
            };
            let fref = ctx.get_extern(sym, &[cl::I64, cl::I64], Some(cl::I64))?;
            let inst = ctx.builder.ins().call(fref, &[obj_h, needle]);
            let v = ctx.builder.inst_results(inst)[0];
            let ty = if method == "includes" { ValTy::Bool } else { ValTy::I64 };
            Ok(Some(TypedVal::new(v, ty)))
        }
        "reverse" | "flat" => {
            // flat(depth) com 1 arg: usa VEC_FLAT_DEPTH.
            if method == "flat" && call.args.len() == 1 && call.args[0].spread.is_none() {
                let d_tv = lower_expr(ctx, &call.args[0].expr)?;
                let depth = ctx.coerce_to_i64(d_tv).val;
                let fref = ctx.get_extern(
                    "__RTS_FN_NS_COLLECTIONS_VEC_FLAT_DEPTH",
                    &[cl::I64, cl::I64],
                    Some(cl::I64),
                )?;
                let inst = ctx.builder.ins().call(fref, &[obj_h, depth]);
                let v = ctx.builder.inst_results(inst)[0];
                return Ok(Some(TypedVal::new(v, ValTy::Handle)));
            }
            if !call.args.is_empty() && method == "reverse" {
                return Ok(None);
            }
            let sym = match method {
                "reverse" => "__RTS_FN_NS_COLLECTIONS_VEC_REVERSE",
                _ => "__RTS_FN_NS_COLLECTIONS_VEC_FLAT",
            };
            let fref = ctx.get_extern(sym, &[cl::I64], Some(cl::I64))?;
            let inst = ctx.builder.ins().call(fref, &[obj_h]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "shift" => {
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_SHIFT",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[obj_h]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "unshift" => {
            if call.args.len() != 1 || call.args[0].spread.is_some() {
                return Ok(None);
            }
            let arg_tv = lower_expr(ctx, &call.args[0].expr)?;
            let arg = ctx.coerce_to_i64(arg_tv).val;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_UNSHIFT",
                &[cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[obj_h, arg]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "slice" => {
            // arr.slice(start?, end?). end omitido = i64::MIN sentinel.
            let start_tv = if let Some(arg) = call.args.first() {
                if arg.spread.is_some() { return Ok(None); }
                lower_expr(ctx, &arg.expr)?
            } else {
                TypedVal::new(ctx.builder.ins().iconst(cl::I64, 0), ValTy::I64)
            };
            let start = ctx.coerce_to_i64(start_tv).val;
            let end = if let Some(arg) = call.args.get(1) {
                if arg.spread.is_some() { return Ok(None); }
                let tv = lower_expr(ctx, &arg.expr)?;
                ctx.coerce_to_i64(tv).val
            } else {
                ctx.builder.ins().iconst(cl::I64, i64::MIN)
            };
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_SLICE",
                &[cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[obj_h, start, end]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "concat" => {
            if call.args.len() != 1 || call.args[0].spread.is_some() {
                return Ok(None);
            }
            let other_tv = lower_expr(ctx, &call.args[0].expr)?;
            let other = ctx.coerce_to_i64(other_tv).val;
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_CONCAT",
                &[cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[obj_h, other]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "fill" => {
            if call.args.is_empty() || call.args.iter().any(|a| a.spread.is_some()) {
                return Ok(None);
            }
            let val_tv = lower_expr(ctx, &call.args[0].expr)?;
            let value = ctx.coerce_to_i64(val_tv).val;
            let start = if let Some(arg) = call.args.get(1) {
                let tv = lower_expr(ctx, &arg.expr)?;
                ctx.coerce_to_i64(tv).val
            } else {
                ctx.builder.ins().iconst(cl::I64, 0)
            };
            let end = if let Some(arg) = call.args.get(2) {
                let tv = lower_expr(ctx, &arg.expr)?;
                ctx.coerce_to_i64(tv).val
            } else {
                ctx.builder.ins().iconst(cl::I64, i64::MIN)
            };
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_FILL",
                &[cl::I64, cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[obj_h, value, start, end]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "splice" => {
            // splice(start, deleteCount, ...items)
            if call.args.is_empty() || call.args.iter().any(|a| a.spread.is_some()) {
                return Ok(None);
            }
            let start_tv = lower_expr(ctx, &call.args[0].expr)?;
            let start = ctx.coerce_to_i64(start_tv).val;
            let count = if let Some(arg) = call.args.get(1) {
                let tv = lower_expr(ctx, &arg.expr)?;
                ctx.coerce_to_i64(tv).val
            } else {
                ctx.builder.ins().iconst(cl::I64, i64::MAX)
            };
            if call.args.len() <= 2 {
                let fref = ctx.get_extern(
                    "__RTS_FN_NS_COLLECTIONS_VEC_SPLICE_REMOVE",
                    &[cl::I64, cl::I64, cl::I64],
                    Some(cl::I64),
                )?;
                let inst = ctx.builder.ins().call(fref, &[obj_h, start, count]);
                let v = ctx.builder.inst_results(inst)[0];
                return Ok(Some(TypedVal::new(v, ValTy::Handle)));
            }
            // splice com ...items: aloca vec novo e usa VEC_SPLICE_INSERT.
            let new_vec = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_NEW",
                &[],
                Some(cl::I64),
            )?;
            let push = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
                &[cl::I64, cl::I64],
                None,
            )?;
            let new_inst = ctx.builder.ins().call(new_vec, &[]);
            let items_h = ctx.builder.inst_results(new_inst)[0];
            for arg in &call.args[2..] {
                let tv = lower_expr(ctx, &arg.expr)?;
                let v = ctx.coerce_to_i64(tv).val;
                ctx.builder.ins().call(push, &[items_h, v]);
            }
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_SPLICE_INSERT",
                &[cl::I64, cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[obj_h, start, count, items_h]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        // (#208) `arr.sort()` sem comparator. `arr.sort(fn)` com comparator
        // precisa do lifter de arrow (PR separada — usuario passa Ident
        // de user fn por enquanto, que `address_taken_fns` captura).
        "sort" => {
            let fn_ptr = if let Some(arg) = call.args.first() {
                if arg.spread.is_some() { return Ok(None); }
                if let Expr::Ident(id) = arg.expr.as_ref() {
                    let fn_name = id.sym.as_str().to_string();
                    if ctx.user_fns.contains_key(&fn_name) && ctx.var_ty(&fn_name).is_none() {
                        let tv = emit_user_fn_addr(ctx, &fn_name)?;
                        ctx.coerce_to_i64(tv).val
                    } else {
                        ctx.builder.ins().iconst(cl::I64, 0)
                    }
                } else {
                    return Ok(None);
                }
            } else {
                ctx.builder.ins().iconst(cl::I64, 0)
            };
            let f = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_SORT",
                &[cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(f, &[obj_h, fn_ptr]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        // (#208) `arr.copyWithin(target, start?, end?)` — args concretos.
        "copyWithin" => {
            if call.args.is_empty() || call.args.iter().any(|a| a.spread.is_some()) {
                return Ok(None);
            }
            let target_tv = lower_expr(ctx, &call.args[0].expr)?;
            let target = ctx.coerce_to_i64(target_tv).val;
            let start = if let Some(arg) = call.args.get(1) {
                let tv = lower_expr(ctx, &arg.expr)?;
                ctx.coerce_to_i64(tv).val
            } else {
                ctx.builder.ins().iconst(cl::I64, 0)
            };
            let end = if let Some(arg) = call.args.get(2) {
                let tv = lower_expr(ctx, &arg.expr)?;
                ctx.coerce_to_i64(tv).val
            } else {
                ctx.builder.ins().iconst(cl::I64, i64::MIN)
            };
            let f = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_COPY_WITHIN",
                &[cl::I64, cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(f, &[obj_h, target, start, end]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        // (#208) `arr.findLast/findLastIndex/reduceRight/flatMap` com user fn ident.
        // Lifting de arrow inline pra esses fica pra outra PR — segue padrao
        // do lift_inline_arrows_in_array_methods em func.rs.
        "findLast" | "findLastIndex" => {
            if call.args.len() != 1 || call.args[0].spread.is_some() {
                return Ok(None);
            }
            let fn_ptr = match call.args[0].expr.as_ref() {
                Expr::Ident(id) => {
                    let fn_name = id.sym.as_str().to_string();
                    if ctx.user_fns.contains_key(&fn_name) && ctx.var_ty(&fn_name).is_none() {
                        let tv = emit_user_fn_addr(ctx, &fn_name)?;
                        ctx.coerce_to_i64(tv).val
                    } else {
                        return Ok(None);
                    }
                }
                _ => return Ok(None),
            };
            let sym = if method == "findLast" {
                "__RTS_FN_NS_COLLECTIONS_VEC_FIND_LAST"
            } else {
                "__RTS_FN_NS_COLLECTIONS_VEC_FIND_LAST_INDEX"
            };
            let f = ctx.get_extern(sym, &[cl::I64, cl::I64], Some(cl::I64))?;
            let inst = ctx.builder.ins().call(f, &[obj_h, fn_ptr]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        "reduceRight" => {
            if call.args.len() != 2 || call.args.iter().any(|a| a.spread.is_some()) {
                return Ok(None);
            }
            let fn_ptr = match call.args[0].expr.as_ref() {
                Expr::Ident(id) => {
                    let fn_name = id.sym.as_str().to_string();
                    if ctx.user_fns.contains_key(&fn_name) && ctx.var_ty(&fn_name).is_none() {
                        let tv = emit_user_fn_addr(ctx, &fn_name)?;
                        ctx.coerce_to_i64(tv).val
                    } else {
                        return Ok(None);
                    }
                }
                _ => return Ok(None),
            };
            let init_tv = lower_expr(ctx, &call.args[1].expr)?;
            let init = ctx.coerce_to_i64(init_tv).val;
            let f = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_REDUCE_RIGHT",
                &[cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(f, &[obj_h, init, fn_ptr]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::I64)))
        }
        // (#208 ES2023) Immutable variants.
        "toReversed" if call.args.is_empty() => {
            let f = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_TO_REVERSED",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(f, &[obj_h]);
            let v = ctx.builder.inst_results(inst)[0];
            return Ok(Some(TypedVal::new(v, ValTy::Handle)));
        }
        "toSorted" => {
            let fn_ptr = if let Some(arg) = call.args.first() {
                if arg.spread.is_some() { return Ok(None); }
                if let Expr::Ident(id) = arg.expr.as_ref() {
                    let fn_name = id.sym.as_str().to_string();
                    if ctx.user_fns.contains_key(&fn_name) && ctx.var_ty(&fn_name).is_none() {
                        let tv = emit_user_fn_addr(ctx, &fn_name)?;
                        ctx.coerce_to_i64(tv).val
                    } else {
                        ctx.builder.ins().iconst(cl::I64, 0)
                    }
                } else {
                    return Ok(None);
                }
            } else {
                ctx.builder.ins().iconst(cl::I64, 0)
            };
            let f = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_TO_SORTED",
                &[cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(f, &[obj_h, fn_ptr]);
            let v = ctx.builder.inst_results(inst)[0];
            return Ok(Some(TypedVal::new(v, ValTy::Handle)));
        }
        "toSpliced" if call.args.len() >= 2
            && call.args.iter().all(|a| a.spread.is_none()) =>
        {
            let start_tv = lower_expr(ctx, &call.args[0].expr)?;
            let start = ctx.coerce_to_i64(start_tv).val;
            let count_tv = lower_expr(ctx, &call.args[1].expr)?;
            let count = ctx.coerce_to_i64(count_tv).val;
            if call.args.len() == 2 {
                let f = ctx.get_extern(
                    "__RTS_FN_NS_COLLECTIONS_VEC_TO_SPLICED",
                    &[cl::I64, cl::I64, cl::I64],
                    Some(cl::I64),
                )?;
                let inst = ctx.builder.ins().call(f, &[obj_h, start, count]);
                let v = ctx.builder.inst_results(inst)[0];
                return Ok(Some(TypedVal::new(v, ValTy::Handle)));
            }
            // toSpliced com inserts.
            let new_vec = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_NEW",
                &[],
                Some(cl::I64),
            )?;
            let push = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
                &[cl::I64, cl::I64],
                None,
            )?;
            let new_inst = ctx.builder.ins().call(new_vec, &[]);
            let items_h = ctx.builder.inst_results(new_inst)[0];
            for arg in &call.args[2..] {
                let tv = lower_expr(ctx, &arg.expr)?;
                let v = ctx.coerce_to_i64(tv).val;
                ctx.builder.ins().call(push, &[items_h, v]);
            }
            let f = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_TO_SPLICED_INSERT",
                &[cl::I64, cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(f, &[obj_h, start, count, items_h]);
            let v = ctx.builder.inst_results(inst)[0];
            return Ok(Some(TypedVal::new(v, ValTy::Handle)));
        }
        "with" if call.args.len() == 2
            && call.args.iter().all(|a| a.spread.is_none()) =>
        {
            let idx_tv = lower_expr(ctx, &call.args[0].expr)?;
            let idx = ctx.coerce_to_i64(idx_tv).val;
            let val_tv = lower_expr(ctx, &call.args[1].expr)?;
            let val = ctx.coerce_to_i64(val_tv).val;
            let f = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_WITH",
                &[cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(f, &[obj_h, idx, val]);
            let v = ctx.builder.inst_results(inst)[0];
            return Ok(Some(TypedVal::new(v, ValTy::Handle)));
        }
        // (#208) Iterators eager: values()/keys()/entries().
        "values" if call.args.is_empty() => {
            let f = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_VALUES",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(f, &[obj_h]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "keys" if call.args.is_empty() => {
            let f = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_KEYS",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(f, &[obj_h]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "entries" if call.args.is_empty() => {
            let f = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_ENTRIES",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(f, &[obj_h]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        "flatMap" => {
            if call.args.len() != 1 || call.args[0].spread.is_some() {
                return Ok(None);
            }
            let fn_ptr = match call.args[0].expr.as_ref() {
                Expr::Ident(id) => {
                    let fn_name = id.sym.as_str().to_string();
                    if ctx.user_fns.contains_key(&fn_name) && ctx.var_ty(&fn_name).is_none() {
                        let tv = emit_user_fn_addr(ctx, &fn_name)?;
                        ctx.coerce_to_i64(tv).val
                    } else {
                        return Ok(None);
                    }
                }
                _ => return Ok(None),
            };
            let f = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_FLAT_MAP",
                &[cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(f, &[obj_h, fn_ptr]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        _ => Ok(None),
    }
}

/// Builtins de RegExp.prototype em receiver Handle (Entry::Regex).
/// Cobre `re.test(str)`, `re.exec(str)`. Sem isso, `r.test("...")`
/// cai em MAP_GET_CHAIN (regex nao eh Map), trapz dispara → SIGILL.
pub(super) fn lower_regexp_builtin(
    ctx: &mut FnCtx,
    method: &str,
    recv_h: cranelift_codegen::ir::Value,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    fn arg_strptr(
        ctx: &mut FnCtx,
        call: &CallExpr,
        idx: usize,
    ) -> Result<(cranelift_codegen::ir::Value, cranelift_codegen::ir::Value)> {
        let arg = call.args.get(idx).ok_or_else(|| anyhow!("missing arg #{idx}"))?;
        if arg.spread.is_some() {
            return Err(anyhow!("spread not supported in regexp builtin"));
        }
        let tv = lower_expr(ctx, &arg.expr)?;
        let h = ctx.coerce_to_handle(tv)?.val;
        let ptr_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
        let len_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
        let pi = ctx.builder.ins().call(ptr_fn, &[h]);
        let p = ctx.builder.inst_results(pi)[0];
        let li = ctx.builder.ins().call(len_fn, &[h]);
        let l = ctx.builder.inst_results(li)[0];
        Ok((p, l))
    }

    match method {
        "test" => {
            let (p, l) = arg_strptr(ctx, call, 0)?;
            let f = ctx.get_extern(
                "__RTS_FN_GL_REGEXP_TEST",
                &[cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(f, &[recv_h, p, l]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Bool)))
        }
        "exec" => {
            let (p, l) = arg_strptr(ctx, call, 0)?;
            let f = ctx.get_extern(
                "__RTS_FN_GL_REGEXP_EXEC",
                &[cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(f, &[recv_h, p, l]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        _ => Ok(None),
    }
}


/// Math object (#760) — trata Math.hypot variádico.
/// JS spec: Math.hypot(...values) calcula sqrt(sum(values[i]²)).
/// Casos especiais: hypot() = 0, hypot(x) = abs(x).
pub(super) fn lower_math_builtin(
    ctx: &mut FnCtx,
    qualified: &str,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    let Some(method) = qualified.strip_prefix("Math.") else {
        return Ok(None);
    };

    match method {
        "hypot" => {
            // Implementação variádica de Math.hypot
            if call.args.is_empty() {
                // hypot() = 0
                let zero = ctx.builder.ins().f64const(0.0);
                return Ok(Some(TypedVal::new(zero, ValTy::F64)));
            }

            if call.args.len() == 1 {
                // hypot(x) = abs(x)
                if call.args[0].spread.is_some() {
                    return Err(anyhow!("spread not supported in Math.hypot"));
                }
                let tv = lower_expr(ctx, &call.args[0].expr)?;
                let x = ctx.coerce_to_f64(tv).val;
                let abs_val = ctx.builder.ins().fabs(x);
                return Ok(Some(TypedVal::new(abs_val, ValTy::F64)));
            }

            // hypot(x1, x2, ..., xn) = sqrt(x1² + x2² + ... + xn²)
            // Implementação numericamente estável: encontra o máximo absoluto
            // e normaliza para evitar overflow/underflow.
            let mut values = Vec::new();
            for arg in &call.args {
                if arg.spread.is_some() {
                    return Err(anyhow!("spread not supported in Math.hypot"));
                }
                let tv = lower_expr(ctx, &arg.expr)?;
                let v = ctx.coerce_to_f64(tv).val;
                values.push(v);
            }

            // Encontra o máximo absoluto
            let mut max_abs = ctx.builder.ins().fabs(values[0]);
            for &v in &values[1..] {
                let abs_v = ctx.builder.ins().fabs(v);
                max_abs = ctx.builder.ins().fmax(max_abs, abs_v);
            }

            // Se max_abs é 0, retorna 0 (evita divisão por zero)
            let zero = ctx.builder.ins().f64const(0.0);
            let is_zero = ctx.builder.ins().fcmp(
                cranelift_codegen::ir::condcodes::FloatCC::Equal,
                max_abs,
                zero,
            );

            // Normaliza e soma os quadrados
            let mut sum_sq = zero;
            for &v in &values {
                let normalized = ctx.builder.ins().fdiv(v, max_abs);
                let sq = ctx.builder.ins().fmul(normalized, normalized);
                sum_sq = ctx.builder.ins().fadd(sum_sq, sq);
            }

            // result = max_abs * sqrt(sum_sq)
            let sqrt_sum = ctx.builder.ins().sqrt(sum_sq);
            let result = ctx.builder.ins().fmul(max_abs, sqrt_sum);

            // Se max_abs era zero, retorna zero; senão retorna result
            let final_result = ctx.builder.ins().select(is_zero, zero, result);

            Ok(Some(TypedVal::new(final_result, ValTy::F64)))
        }
        _ => Ok(None),
    }
}
