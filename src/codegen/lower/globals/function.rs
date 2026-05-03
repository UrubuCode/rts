//! Function lowering helpers.
//!
//! Implementação dos métodos do Function global para Cranelift IR.
//! Inclui call, apply, bind, toString e reificação de funções.

use anyhow::{Result, anyhow};
use cranelift_codegen::ir::types as cl;
use swc_ecma_ast::{CallExpr, Expr};

use crate::codegen::lower::ctx::{FnCtx, TypedVal, ValTy};
use crate::codegen::lower::expressions::lower_expr;

/// Function global (#359): emite reify + chamada do metodo (call/apply/bind/toString)
/// pra um ident de user fn. Retorna `Ok(None)` se algo nao se encaixa (caller
/// segue pro fallback). Args sao empacotados em Vec handle pra call/apply/bind.
pub fn lower_function_method_call(
    ctx: &mut FnCtx,
    fn_name: &str,
    method: &str,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    // 1. emit_user_fn_addr garante callconv C (address_taken_fns).
    let fn_ptr = emit_user_fn_addr(ctx, fn_name)?.val;

    // 2. Reify: __RTS_FN_GL_FUNCTION_REIFY(fn_ptr, arity, name_ptr, name_len, is_arrow).
    let arity = ctx
        .user_fns
        .get(fn_name)
        .map(|f| f.params.len() as i64)
        .unwrap_or(0);
    let name_tv = ctx.emit_str_handle(fn_name.as_bytes())?;
    let name_h = ctx.coerce_to_i64(name_tv).val;
    // emit_str_handle retorna handle GC — pra REIFY precisamos de (ptr, len).
    // Usamos gc.string_ptr/string_len no handle.
    let str_ptr_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_PTR", &[cl::I64], Some(cl::I64))?;
    let str_len_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
    let inst_p = ctx.builder.ins().call(str_ptr_fn, &[name_h]);
    let n_ptr = ctx.builder.inst_results(inst_p)[0];
    let inst_l = ctx.builder.ins().call(str_len_fn, &[name_h]);
    let n_len = ctx.builder.inst_results(inst_l)[0];

    let arity_v = ctx.builder.ins().iconst(cl::I64, arity);
    let is_arrow_v = ctx.builder.ins().iconst(cl::I32, 0);
    let has_this_v = ctx.builder.ins().iconst(
        cl::I32,
        i64::from(fn_name_has_this_param(fn_name)),
    );
    let reify_fn = ctx.get_extern(
        "__RTS_FN_GL_FUNCTION_REIFY",
        &[cl::I64, cl::I64, cl::I64, cl::I64, cl::I32, cl::I32],
        Some(cl::I64),
    )?;
    let inst_r = ctx
        .builder
        .ins()
        .call(reify_fn, &[fn_ptr, arity_v, n_ptr, n_len, is_arrow_v, has_this_v]);
    let fn_handle = ctx.builder.inst_results(inst_r)[0];

    // 3. Despacha por metodo.
    match method {
        "toString" => {
            let to_str_fn = ctx.get_extern(
                "__RTS_FN_GL_FUNCTION_TO_STRING",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(to_str_fn, &[fn_handle]);
            let r = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(r, ValTy::Handle)))
        }
        "call" | "apply" | "bind" => {
            // Args: primeiro arg eh thisArg, demais sao argumentos da fn (call/bind)
            // ou argsArray (apply). Empacotamos demais em Vec.
            let this_arg = if let Some(arg) = call.args.first() {
                let tv = lower_expr(ctx, &arg.expr)?;
                ctx.coerce_to_i64(tv).val
            } else {
                ctx.builder.ins().iconst(cl::I64, 0)
            };

            let args_vec_h = if method == "apply" {
                // apply(this, [a, b, c]) — segundo arg ja' eh array.
                if let Some(arg) = call.args.get(1) {
                    let tv = lower_expr(ctx, &arg.expr)?;
                    ctx.coerce_to_i64(tv).val
                } else {
                    ctx.builder.ins().iconst(cl::I64, 0)
                }
            } else {
                // call/bind: empacota call.args[1..] em Vec.
                let vec_new_fn = ctx.get_extern(
                    "__RTS_FN_NS_COLLECTIONS_VEC_NEW",
                    &[],
                    Some(cl::I64),
                )?;
                let push_fn = ctx.get_extern(
                    "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
                    &[cl::I64, cl::I64],
                    None,
                )?;
                let inst_v = ctx.builder.ins().call(vec_new_fn, &[]);
                let vec_h = ctx.builder.inst_results(inst_v)[0];
                for arg in call.args.iter().skip(1) {
                    let tv = lower_expr(ctx, &arg.expr)?;
                    let v = ctx.coerce_to_i64(tv).val;
                    ctx.builder.ins().call(push_fn, &[vec_h, v]);
                }
                vec_h
            };

            let symbol = match method {
                "call" => "__RTS_FN_GL_FUNCTION_CALL",
                "apply" => "__RTS_FN_GL_FUNCTION_APPLY",
                "bind" => "__RTS_FN_GL_FUNCTION_BIND",
                _ => unreachable!(),
            };
            let ret_ty = if method == "bind" { cl::I64 } else { cl::I64 };
            let target_fn = ctx.get_extern(
                symbol,
                &[cl::I64, cl::I64, cl::I64],
                Some(ret_ty),
            )?;
            let inst = ctx
                .builder
                .ins()
                .call(target_fn, &[fn_handle, this_arg, args_vec_h]);
            let r = ctx.builder.inst_results(inst)[0];
            let ty = if method == "bind" { ValTy::Handle } else { ValTy::I64 };
            Ok(Some(TypedVal::new(r, ty)))
        }
        _ => Ok(None),
    }
}

/// Function global (#359): chama metodo (call/apply/bind/toString) em handle
/// de Function (variavel que contem funcao reificada ou new Function).
pub fn lower_function_handle_method(
    ctx: &mut FnCtx,
    obj: &Expr,
    method: &str,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    let obj_tv = lower_expr(ctx, obj)?;
    let fn_handle = ctx.coerce_to_i64(obj_tv).val;

    match method {
        "toString" => {
            let to_str_fn = ctx.get_extern(
                "__RTS_FN_GL_FUNCTION_TO_STRING",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(to_str_fn, &[fn_handle]);
            let r = ctx.builder.inst_results(inst)[0];
            Ok(Some(TypedVal::new(r, ValTy::Handle)))
        }
        "call" | "apply" | "bind" => {
            let this_arg = if let Some(arg) = call.args.first() {
                let tv = lower_expr(ctx, &arg.expr)?;
                ctx.coerce_to_i64(tv).val
            } else {
                ctx.builder.ins().iconst(cl::I64, 0)
            };
            let args_vec_h = if method == "apply" {
                if let Some(arg) = call.args.get(1) {
                    let tv = lower_expr(ctx, &arg.expr)?;
                    ctx.coerce_to_i64(tv).val
                } else {
                    ctx.builder.ins().iconst(cl::I64, 0)
                }
            } else {
                let vec_new_fn = ctx.get_extern(
                    "__RTS_FN_NS_COLLECTIONS_VEC_NEW",
                    &[],
                    Some(cl::I64),
                )?;
                let push_fn = ctx.get_extern(
                    "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
                    &[cl::I64, cl::I64],
                    None,
                )?;
                let inst_v = ctx.builder.ins().call(vec_new_fn, &[]);
                let vec_h = ctx.builder.inst_results(inst_v)[0];
                for arg in call.args.iter().skip(1) {
                    let tv = lower_expr(ctx, &arg.expr)?;
                    let v = ctx.coerce_to_i64(tv).val;
                    ctx.builder.ins().call(push_fn, &[vec_h, v]);
                }
                vec_h
            };
            let symbol = match method {
                "call" => "__RTS_FN_GL_FUNCTION_CALL",
                "apply" => "__RTS_FN_GL_FUNCTION_APPLY",
                "bind" => "__RTS_FN_GL_FUNCTION_BIND",
                _ => unreachable!(),
            };
            let target_fn = ctx.get_extern(
                symbol,
                &[cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx
                .builder
                .ins()
                .call(target_fn, &[fn_handle, this_arg, args_vec_h]);
            let r = ctx.builder.inst_results(inst)[0];
            let ty = if method == "bind" { ValTy::Handle } else { ValTy::I64 };
            Ok(Some(TypedVal::new(r, ty)))
        }
        _ => Ok(None),
    }
}

fn fn_name_has_this_param(_fn_name: &str) -> bool {
    // Heuristica simples: methods de classe tem 'this'.
    // Pode ser expandido conforme necessario.
    false
}

pub fn emit_user_fn_addr(ctx: &mut FnCtx, name: &str) -> Result<TypedVal> {
    // User fns cujo endereço é tomado são declaradas com C callconv
    // (ver `address_taken_fns` em compile_program / #206) — seguro para
    // `thread.spawn` e FFI.
    let mangled: String = format!("__user_{name}");
    let func_id = *ctx
        .extern_cache
        .get(mangled.as_str())
        .ok_or_else(|| anyhow!("user function `{name}` has no cached id"))?;
    let fref = ctx.fref_for_id(func_id);
    let ptr_ty = ctx.module.isa().pointer_type();
    let addr = ctx.builder.ins().func_addr(ptr_ty, fref);
    Ok(TypedVal::new(addr, ValTy::I64))
}
