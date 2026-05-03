//! Object lowering helpers.
//!
//! Implementação dos métodos estáticos do Object para Cranelift IR.

use anyhow::{Result, anyhow};
use cranelift_codegen::ir::types as cl;
use swc_ecma_ast::CallExpr;

use crate::abi::lookup;
use crate::codegen::lower::ctx::{FnCtx, TypedVal, ValTy};
use crate::codegen::lower::expressions::lower_expr;

/// Lower chamadas de métodos estáticos do Object (Object.keys, Object.create, etc.)
pub fn lower_object_static_call(
    ctx: &mut FnCtx,
    method: &str,
    call: &CallExpr,
) -> Result<Option<TypedVal>> {
    match method {
        "keys" | "values" | "hasOwn" => {
            let target = match method {
                "keys" => "collections.map_keys",
                "values" => "collections.map_values",
                "hasOwn" => "collections.map_has",
                _ => "",
            };
            if !target.is_empty() && lookup(target).is_some() {
                // Usar lower_ns_call exigiria importação, então inlineamos a lógica básica
                // Para simplificar, retornamos None e deixamos o caller lidar com o fallback
                return Ok(None);
            }
            Ok(None)
        }
        "create" => {
            if call.args.len() != 1 {
                return Ok(None);
            }
            let arg_tv = lower_expr(ctx, &call.args[0].expr)?;
            let proto_h = ctx.coerce_to_i64(arg_tv).val;
            let create_fn = ctx.get_extern(
                "__RTS_FN_GL_OBJECT_CREATE",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(create_fn, &[proto_h]);
            let v = ctx.builder.inst_results(inst)[0];
            return Ok(Some(TypedVal::new(v, ValTy::Handle)));
        }
        "entries" => {
            if call.args.len() != 1 {
                return Ok(None);
            }
            let arg_tv = lower_expr(ctx, &call.args[0].expr)?;
            let h = ctx.coerce_to_i64(arg_tv).val;
            let f = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_ENTRIES",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(f, &[h]);
            let v = ctx.builder.inst_results(inst)[0];
            return Ok(Some(TypedVal::new(v, ValTy::Handle)));
        }
        "freeze" => {
            if call.args.len() != 1 {
                return Ok(None);
            }
            let arg_tv = lower_expr(ctx, &call.args[0].expr)?;
            let h = ctx.coerce_to_i64(arg_tv).val;
            let f = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_FREEZE",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(f, &[h]);
            let v = ctx.builder.inst_results(inst)[0];
            return Ok(Some(TypedVal::new(v, ValTy::Handle)));
        }
        "fromEntries" => {
            if call.args.len() != 1 {
                return Ok(None);
            }
            let arg_tv = lower_expr(ctx, &call.args[0].expr)?;
            let h = ctx.coerce_to_i64(arg_tv).val;
            let f = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_FROM_ENTRIES",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(f, &[h]);
            let v = ctx.builder.inst_results(inst)[0];
            return Ok(Some(TypedVal::new(v, ValTy::Handle)));
        }
        "assign" => {
            if call.args.len() < 2 {
                return Ok(None);
            }
            let target_tv = lower_expr(ctx, &call.args[0].expr)?;
            let mut acc = ctx.coerce_to_i64(target_tv).val;
            let f = ctx.get_extern(
                "__RTS_FN_NS_COLLECTIONS_MAP_ASSIGN",
                &[cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            for arg in &call.args[1..] {
                if arg.spread.is_some() {
                    return Err(anyhow!("spread not supported in Object.assign"));
                }
                let s_tv = lower_expr(ctx, &arg.expr)?;
                let s = ctx.coerce_to_i64(s_tv).val;
                let inst = ctx.builder.ins().call(f, &[acc, s]);
                acc = ctx.builder.inst_results(inst)[0];
            }
            return Ok(Some(TypedVal::new(acc, ValTy::Handle)));
        }
        _ => Ok(None),
    }
}
