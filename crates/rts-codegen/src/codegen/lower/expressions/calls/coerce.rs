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
    // (#872/130) Global `isNaN(x)` coage para number antes de testar. String
    // handle vai por NUMBER_FROM_STR (retorna NaN quando nao parseavel — ex:
    // `isNaN("NaN")` → true).
    let f = if matches!(tv.ty, ValTy::Handle) {
        let from_str = ctx.get_extern(
            "__RTS_FN_GL_NUMBER_FROM_STR",
            &[cranelift_codegen::ir::types::I64],
            Some(cranelift_codegen::ir::types::F64),
        )?;
        let inst = ctx.builder.ins().call(from_str, &[tv.val]);
        ctx.builder.inst_results(inst)[0]
    } else {
        to_f64(ctx, tv)
    };
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
    // (#872/130) Global `isFinite(x)` coage para number antes de testar.
    let f = if matches!(tv.ty, ValTy::Handle) {
        let from_str = ctx.get_extern(
            "__RTS_FN_GL_NUMBER_FROM_STR",
            &[cranelift_codegen::ir::types::I64],
            Some(cranelift_codegen::ir::types::F64),
        )?;
        let inst = ctx.builder.ins().call(from_str, &[tv.val]);
        ctx.builder.inst_results(inst)[0]
    } else {
        to_f64(ctx, tv)
    };
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
        // JS spec: Number(null) → 0, Number(undefined) → NaN.
        if let swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Null(_)) = arg.expr.as_ref() {
            let v = ctx.builder.ins().f64const(0.0);
            return Ok(Some(TypedVal::new(v, ValTy::F64)));
        }
        if let swc_ecma_ast::Expr::Ident(id) = arg.expr.as_ref() {
            if id.sym.as_str() == "undefined" {
                let v = ctx.builder.ins().f64const(f64::NAN);
                return Ok(Some(TypedVal::new(v, ValTy::F64)));
            }
        }
        let tv = super::lower_expr(ctx, &arg.expr)?;
        // (cross-runtime #256/#262) I64 ambiguo (param de hoisted arrow,
        // var_member_call) pode ser handle de string — checa is_ambig_handle
        // ANTES do branch I64 puro para nao perder o caminho NUMBER_FROM_STR.
        let is_ambig_for_num = matches!(tv.ty, ValTy::I64 | ValTy::U64)
            && ctx.var_member_call_values.contains(&tv.val);
        // Sentinela undefined (i64::MIN+2) → NaN; sentinela null (i64::MIN+3) → 0.
        if matches!(tv.ty, ValTy::I64) && !is_ambig_for_num {
            let sentinel_undef = ctx.builder.ins().iconst(cl::I64, i64::MIN + 2);
            let sentinel_null  = ctx.builder.ins().iconst(cl::I64, i64::MIN + 3);
            let is_undef = ctx.builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::Equal, tv.val, sentinel_undef);
            let is_null  = ctx.builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::Equal, tv.val, sentinel_null);
            // is_undef → NaN; is_null → 0.0; else → fallthrough
            let is_special = ctx.builder.ins().bor(is_undef, is_null);
            // Emite runtime check via inline branch.
            // (cross-runtime #256/#262) brif eh terminator do bloco corrente;
            // o caminho "default" (nao-special) precisa de um bloco proprio,
            // nao pode emitir instructions inline no mesmo bloco apos brif.
            let special_block = ctx.builder.create_block();
            let default_block = ctx.builder.create_block();
            let undef_block   = ctx.builder.create_block();
            let null_block    = ctx.builder.create_block();
            let merge_block   = ctx.builder.create_block();
            ctx.builder.append_block_param(merge_block, cl::F64);
            ctx.builder.ins().brif(is_special, special_block, &[], default_block, &[]);
            // Default path: to_f64(tv) e jump para merge.
            ctx.builder.switch_to_block(default_block);
            ctx.builder.seal_block(default_block);
            {
                let fval = to_f64(ctx, tv);
                ctx.builder.ins().jump(merge_block, &[fval.into()]);
            }
            ctx.builder.switch_to_block(special_block);
            ctx.builder.seal_block(special_block);
            ctx.builder.ins().brif(is_undef, undef_block, &[], null_block, &[]);
            ctx.builder.switch_to_block(undef_block);
            ctx.builder.seal_block(undef_block);
            let nan = ctx.builder.ins().f64const(f64::NAN);
            ctx.builder.ins().jump(merge_block, &[nan.into()]);
            ctx.builder.switch_to_block(null_block);
            ctx.builder.seal_block(null_block);
            let zero = ctx.builder.ins().f64const(0.0);
            ctx.builder.ins().jump(merge_block, &[zero.into()]);
            ctx.builder.switch_to_block(merge_block);
            ctx.builder.seal_block(merge_block);
            let result = ctx.builder.block_params(merge_block)[0];
            return Ok(Some(TypedVal::new(result, ValTy::F64)));
        }
        let is_ambig_handle = matches!(tv.ty, ValTy::I64 | ValTy::U64)
            && ctx.var_member_call_values.contains(&tv.val);
        if matches!(tv.ty, ValTy::Handle) || is_ambig_handle {
            // Delega para __RTS_FN_GL_NUMBER_FROM_STR(handle) -> f64
            // Tambem aplica para I64 ambiguo (param de hoisted arrow que pode ser handle de string).
            let coerce = if is_ambig_handle {
                let coerce_fn = ctx.get_extern("__RTS_FN_RT_TPL_COERCE_AUTO", &[cl::I64], Some(cl::I64))?;
                let inst = ctx.builder.ins().call(coerce_fn, &[tv.val]);
                ctx.builder.inst_results(inst)[0]
            } else {
                tv.val
            };
            let from_str = ctx.get_extern("__RTS_FN_GL_NUMBER_FROM_STR", &[cl::I64], Some(cl::F64))?;
            let inst = ctx.builder.ins().call(from_str, &[coerce]);
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
        // (#643/JS spec) String(null) -> "null", String(undefined) -> "undefined".
        // Detecta literais direto antes de avaliar (evita lower_expr).
        if let swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Null(_)) = arg.expr.as_ref() {
            return Ok(Some(ctx.emit_str_handle(b"null")?));
        }
        if let swc_ecma_ast::Expr::Ident(id) = arg.expr.as_ref() {
            if id.sym.as_str() == "undefined" {
                return Ok(Some(ctx.emit_str_handle(b"undefined")?));
            }
        }
        let tv = super::lower_expr(ctx, &arg.expr)?;
        // Handle 0 (null em RTS) tambem stringifica como "null" via
        // TPL_COERCE_AUTO. Caso geral.
        let needs_auto = matches!(tv.ty, crate::codegen::lower::ctx::ValTy::Handle | crate::codegen::lower::ctx::ValTy::U64);
        if needs_auto {
            let coerce_fn = ctx.get_extern(
                "__RTS_FN_RT_TPL_COERCE_AUTO",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let val_i64 = ctx.coerce_to_i64(tv).val;
            let inst = ctx.builder.ins().call(coerce_fn, &[val_i64]);
            let v = ctx.builder.inst_results(inst)[0];
            return Ok(Some(crate::codegen::lower::ctx::TypedVal::new(v, crate::codegen::lower::ctx::ValTy::Handle)));
        }
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
        // (#879) `Boolean(undefined)` e `Boolean(null)` sao falsy. Em RTS,
        // `undefined` (Ident) lower vira handle de string "undefined" (len > 0)
        // e a coerção string_len > 0 daria true incorretamente. Detecta cedo.
        if let swc_ecma_ast::Expr::Ident(id) = arg.expr.as_ref() {
            if id.sym.as_str() == "undefined" && ctx.read_local("undefined").is_none() {
                let v = ctx.builder.ins().iconst(cranelift_codegen::ir::types::I8, 0);
                return Ok(Some(TypedVal::new(v, ValTy::Bool)));
            }
        }
        if let swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Null(_)) = arg.expr.as_ref() {
            let v = ctx.builder.ins().iconst(cranelift_codegen::ir::types::I8, 0);
            return Ok(Some(TypedVal::new(v, ValTy::Bool)));
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
            // (#208 fix): NaN deve ser falsy. Boolean(x) = !(x == 0 OR NaN).
            // Evita FloatCC::OrderedNotEqual / Unordered que dao panic
            // em Cranelift 0.131 aarch64. Usa Equal-based equivalents.
            use cranelift_codegen::ir::types as cl;
            let zero = ctx.builder.ins().f64const(0.0);
            let is_zero = ctx.builder.ins().fcmp(FloatCC::Equal, tv.val, zero);
            let is_self_eq = ctx.builder.ins().fcmp(FloatCC::Equal, tv.val, tv.val);
            let is_nan_i = {
                let i = ctx.builder.ins().uextend(cl::I64, is_self_eq);
                let one = ctx.builder.ins().iconst(cl::I64, 1);
                ctx.builder.ins().bxor(i, one)
            };
            let is_zero_i = ctx.builder.ins().uextend(cl::I64, is_zero);
            let falsy = ctx.builder.ins().bor(is_zero_i, is_nan_i);
            // truthy = !falsy
            let one = ctx.builder.ins().iconst(cl::I64, 1);
            let truthy = ctx.builder.ins().bxor(falsy, one);
            return Ok(Some(TypedVal::new(truthy, ValTy::Bool)));
        }
        let v = ctx.coerce_to_i64(tv).val;
        let zero = ctx.builder.ins().iconst(cl::I64, 0);
        let result = ctx.builder.ins().icmp(IntCC::NotEqual, v, zero);
        return Ok(Some(TypedVal::new(result, ValTy::Bool)));
    }
    let v = ctx.builder.ins().iconst(cl::I64, 0);
    Ok(Some(TypedVal::new(v, ValTy::Bool)))
}
