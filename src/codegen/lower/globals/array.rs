//! Array lowering helpers.
//!
//! Implementação dos métodos de array do JavaScript para Cranelift IR.

use anyhow::Result;
use cranelift_codegen::ir::types as cl;
use swc_ecma_ast::CallExpr;

use crate::codegen::lower::ctx::{FnCtx, TypedVal, ValTy};
use crate::codegen::lower::expressions::lower_expr;

/// Builtins universais para arrays/maps via handle. Retorna `Some` se
/// a chamada foi tratada como builtin.
pub fn lower_array_builtin(
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
            // Negative indexing seria mais complexo; v0 so aceita non-negative.
            let idx_arg = call.args.first().ok_or_else(|| anyhow::anyhow!("at requires index"))?;
            let tv = lower_expr(ctx, &idx_arg.expr)?;
            let idx = ctx.coerce_to_i64(tv).val;
            let get_fn = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_GET",
                &[cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(get_fn, &[obj_h, idx]);
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
            if call.args.len() != 1 || call.args[0].spread.is_some() {
                return Ok(None);
            }
            let needle_tv = lower_expr(ctx, &call.args[0].expr)?;
            let needle = ctx.coerce_to_i64(needle_tv).val;
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
            if !call.args.is_empty() {
                // flat(depth) e reverse() ambos sem args na versao v0.
                // depth diferente de 1 cai em outro PR.
                if method == "reverse" {
                    return Ok(None);
                }
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
            // splice(start, deleteCount). Versao com ...items vai em PR separada.
            if call.args.is_empty() || call.args.len() > 2
                || call.args.iter().any(|a| a.spread.is_some())
            {
                return Ok(None);
            }
            let start_tv = lower_expr(ctx, &call.args[0].expr)?;
            let start = ctx.coerce_to_i64(start_tv).val;
            let count = if let Some(arg) = call.args.get(1) {
                let tv = lower_expr(ctx, &arg.expr)?;
                ctx.coerce_to_i64(tv).val
            } else {
                // splice(start) sem count = remove tudo do start em diante.
                ctx.builder.ins().iconst(cl::I64, i64::MAX)
            };
            let fref = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_VEC_SPLICE_REMOVE",
                &[cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(fref, &[obj_h, start, count]);
            let v = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(v, ValTy::Handle)))
        }
        _ => Ok(None),
    }
}
