//! Coercoes globais: `Number(x)`, `String(x)`, `Boolean(x)`, `isNaN(x)`,
//! `isFinite(x)`. Cada uma reescreve o callsite em IR Cranelift direto
//! quando o tipo do argumento permite.

use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{InstBuilder, types as cl};
use swc_ecma_ast::CallExpr;

use crate::codegen::lower::ctx::{FnCtx, TypedVal};
use crate::codegen::lower::expressions::operators::to_f64;

pub(super) fn lower_coerce_is_nan(ctx: &mut FnCtx, call: &CallExpr) -> Result<TypedVal> {
    use cranelift_codegen::ir::condcodes::FloatCC;
    use crate::codegen::lower::ctx::{TypedVal, ValTy};
    let arg = call.args.first().ok_or_else(|| anyhow!("isNaN requires 1 arg"))?;
    if arg.spread.is_some() {
        return Err(anyhow!("isNaN: spread arg not supported"));
    }
    let tv = super::lower_expr(ctx, &arg.expr)?;
    let f = to_f64(ctx, tv);
    // FloatCC::Unordered nao eh suportado em alguns backends Cranelift
    // (notavelmente aarch64 — panic \"not implemented\" em
    // lower_fp_condcode). Usa NotEqual(f, f) que eh equivalente:
    // - NaN != NaN -> true (NaN nunca eh ordered-equal a si mesmo,
    //   porem NotEqual em Cranelift eh ordered, e NaN comparison
    //   ordered retorna false)
    // Solucao robusta: !(f == f) — NaN ordered-equal a si mesmo eh
    // false, !false = true.
    let eq = ctx.builder.ins().fcmp(FloatCC::Equal, f, f);
    // bxor com 1 inverte (Bool eh i8 0/1).
    let one = ctx.builder.ins().iconst(cranelift_codegen::ir::types::I8, 1);
    let result = ctx.builder.ins().bxor(eq, one);
    Ok(TypedVal::new(result, ValTy::Bool))
}

pub(super) fn lower_coerce_is_finite(ctx: &mut FnCtx, call: &CallExpr) -> Result<TypedVal> {
    use cranelift_codegen::ir::condcodes::FloatCC;
    use crate::codegen::lower::ctx::{TypedVal, ValTy};
    let arg = call.args.first().ok_or_else(|| anyhow!("isFinite requires 1 arg"))?;
    if arg.spread.is_some() {
        return Err(anyhow!("isFinite: spread arg not supported"));
    }
    let tv = super::lower_expr(ctx, &arg.expr)?;
    let f = to_f64(ctx, tv);
    let abs_f = ctx.builder.ins().fabs(f);
    let inf = ctx.builder.ins().f64const(f64::INFINITY);
    let result = ctx.builder.ins().fcmp(FloatCC::LessThan, abs_f, inf);
    Ok(TypedVal::new(result, ValTy::Bool))
}

pub(super) fn lower_coerce_to_number(ctx: &mut FnCtx, call: &CallExpr) -> Result<Option<TypedVal>> {
    use crate::codegen::lower::ctx::{TypedVal, ValTy};
    if let Some(arg) = call.args.first() {
        if arg.spread.is_some() {
            return Ok(None);
        }
        let tv = super::lower_expr(ctx, &arg.expr)?;
        if matches!(tv.ty, ValTy::Handle) {
            // Delega para __RTS_FN_GL_NUMBER_FROM_STR(handle) -> f64
            let from_str = ctx.get_extern("__RTS_FN_GL_NUMBER_FROM_STR", &[cl::I64], Some(cl::F64))?;
            let inst = ctx.builder.ins().call(from_str, &[tv.val]);
            let v = ctx.builder.inst_results(inst)[0];
            return Ok(Some(TypedVal::new(v, ValTy::F64)));
        }
        let f = to_f64(ctx, tv);
        return Ok(Some(TypedVal::new(f, ValTy::F64)));
    }
    let v = ctx.builder.ins().f64const(0.0);
    Ok(Some(TypedVal::new(v, ValTy::F64)))
}

pub(super) fn lower_coerce_to_string(ctx: &mut FnCtx, call: &CallExpr) -> Result<Option<TypedVal>> {
    if let Some(arg) = call.args.first() {
        if arg.spread.is_some() {
            return Ok(None);
        }
        let tv = super::lower_expr(ctx, &arg.expr)?;
        let h = ctx.coerce_to_handle(tv)?;
        return Ok(Some(h));
    }
    let h = ctx.emit_str_handle(b"")?;
    Ok(Some(h))
}

pub(super) fn lower_coerce_to_boolean(ctx: &mut FnCtx, call: &CallExpr) -> Result<Option<TypedVal>> {
    use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
    use crate::codegen::lower::ctx::{TypedVal, ValTy};
    if let Some(arg) = call.args.first() {
        if arg.spread.is_some() {
            return Ok(None);
        }
        let tv = super::lower_expr(ctx, &arg.expr)?;
        // (#550) String coerce: empty string e' falsy. Handle aponta para
        // Entry::String — usar gc.string_len(handle) > 0.
        if matches!(tv.ty, ValTy::Handle) {
            // Caminho generico: chama gc.string_len(handle) e compara > 0.
            // Para handles nao-string o len volta 0, o que coincide com
            // Boolean({}) === true em JS — mas em RTS objetos nao-string-like
            // sao raros como input direto de Boolean(); aceitamos para empty
            // string virar false (caso comum).
            let str_len_fn = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
            let inst_l = ctx.builder.ins().call(str_len_fn, &[tv.val]);
            let len = ctx.builder.inst_results(inst_l)[0];
            let zero = ctx.builder.ins().iconst(cl::I64, 0);
            let result = ctx.builder.ins().icmp(IntCC::NotEqual, len, zero);
            return Ok(Some(TypedVal::new(result, ValTy::Bool)));
        }
        if matches!(tv.ty, ValTy::F64) {
            // (#208 fix): NaN deve ser falsy. fcmp NotEqual retorna false
            // pra NaN (NaN != X e' Unordered, nao true). Use OrderedNotEqual
            // que e' false quando NaN — exatamente o comportamento desejado.
            // Em Cranelift: OrderedNotEqual = "ordered AND not equal".
            let zero = ctx.builder.ins().f64const(0.0);
            let ne_zero = ctx.builder.ins().fcmp(FloatCC::OrderedNotEqual, tv.val, zero);
            return Ok(Some(TypedVal::new(ne_zero, ValTy::Bool)));
        }
        let v = ctx.coerce_to_i64(tv).val;
        let zero = ctx.builder.ins().iconst(cl::I64, 0);
        let result = ctx.builder.ins().icmp(IntCC::NotEqual, v, zero);
        return Ok(Some(TypedVal::new(result, ValTy::Bool)));
    }
    let v = ctx.builder.ins().iconst(cl::I64, 0);
    Ok(Some(TypedVal::new(v, ValTy::Bool)))
}
