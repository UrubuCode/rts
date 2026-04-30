use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{InstBuilder, types as cl};
use swc_ecma_ast::{Expr, Lit, Tpl, UnaryOp};

use super::lower_expr;
use crate::codegen::lower::ctx::{FnCtx, TypedVal, ValTy};

pub(super) fn lower_lit(ctx: &mut FnCtx, lit: &Lit) -> Result<TypedVal> {
    match lit {
        Lit::Num(n) => {
            let v = n.value;
            // Se o source escreveu \`1.0\` ou \`1e3\` (com . ou expoente),
            // mantemos F64 mesmo que matematicamente seja inteiro. Isso
            // evita que codigo \`x <= 1.0\` em loop quente faca
            // iconst.i32 + fcvt_from_sint.f64 toda iter — basta
            // f64const direto. Cranelift egraph nao hoist esse fcvt
            // mesmo quando trivialmente loop-invariant.
            let wrote_as_float = n
                .raw
                .as_ref()
                .map(|r| {
                    let s = r.as_bytes();
                    s.iter().any(|&b| b == b'.' || b == b'e' || b == b'E')
                })
                .unwrap_or(false);
            if !wrote_as_float
                && v.fract() == 0.0
                && v >= i32::MIN as f64
                && v <= i32::MAX as f64
            {
                Ok(TypedVal::new(
                    ctx.builder.ins().iconst(cl::I32, v as i64),
                    ValTy::I32,
                ))
            } else if !wrote_as_float && v.fract() == 0.0 && v.is_finite() {
                Ok(TypedVal::new(
                    ctx.builder.ins().iconst(cl::I64, v as i64),
                    ValTy::I64,
                ))
            } else {
                Ok(TypedVal::new(ctx.builder.ins().f64const(v), ValTy::F64))
            }
        }
        Lit::Bool(b) => Ok(TypedVal::new(
            ctx.builder.ins().iconst(cl::I64, i64::from(b.value)),
            ValTy::Bool,
        )),
        Lit::Null(_) => Ok(TypedVal::new(
            ctx.builder.ins().iconst(cl::I64, 0),
            ValTy::Handle,
        )),
        Lit::Str(s) => {
            let tv = ctx.emit_str_handle(s.value.as_bytes())?;
            Ok(TypedVal::new(tv.val, ValTy::Handle))
        }
        Lit::Regex(r) => {
            // /pattern/flags  →  regex.compile(pattern, flags)
            let pat_bytes = r.exp.as_bytes();
            let flag_bytes = r.flags.as_bytes();
            let (pp, pl) = ctx.emit_str_literal(pat_bytes)?;
            let (fp, fl) = ctx.emit_str_literal(flag_bytes)?;
            let compile = ctx.get_extern(
                "__RTS_FN_NS_REGEX_COMPILE",
                &[cl::I64, cl::I64, cl::I64, cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(compile, &[pp, pl, fp, fl]);
            Ok(TypedVal::new(
                ctx.builder.inst_results(inst)[0],
                ValTy::Handle,
            ))
        }
        other => Err(anyhow!("unsupported literal: {other:?}")),
    }
}

pub(super) fn lower_unary(ctx: &mut FnCtx, u: &swc_ecma_ast::UnaryExpr) -> Result<TypedVal> {
    if matches!(u.op, UnaryOp::TypeOf) {
        return lower_typeof(ctx, &u.arg);
    }
    if matches!(u.op, UnaryOp::Void) {
        let _ = lower_expr(ctx, &u.arg)?;
        return Ok(TypedVal::new(
            ctx.builder.ins().iconst(cl::I64, 0),
            ValTy::I64,
        ));
    }

    let operand = lower_expr(ctx, &u.arg)?;
    match u.op {
        UnaryOp::Minus => match operand.ty {
            ValTy::F64 => Ok(TypedVal::new(
                ctx.builder.ins().fneg(operand.val),
                ValTy::F64,
            )),
            ValTy::I32 => Ok(TypedVal::new(
                ctx.builder.ins().ineg(operand.val),
                ValTy::I32,
            )),
            _ => {
                let operand_i64 = ctx.coerce_to_i64(operand).val;
                Ok(TypedVal::new(
                    ctx.builder.ins().ineg(operand_i64),
                    ValTy::I64,
                ))
            }
        },
        UnaryOp::Plus => Ok(operand),
        UnaryOp::Bang => {
            let value = ctx.coerce_to_i64(operand).val;
            let zero = ctx.builder.ins().iconst(cl::I64, 0);
            let is_zero =
                ctx.builder
                    .ins()
                    .icmp(cranelift_codegen::ir::condcodes::IntCC::Equal, value, zero);
            Ok(TypedVal::new(
                ctx.builder.ins().uextend(cl::I64, is_zero),
                ValTy::Bool,
            ))
        }
        UnaryOp::Tilde => {
            let operand_i64 = ctx.coerce_to_i64(operand).val;
            Ok(TypedVal::new(
                ctx.builder.ins().bnot(operand_i64),
                ValTy::I64,
            ))
        }
        UnaryOp::Delete => Ok(TypedVal::new(
            ctx.builder.ins().iconst(cl::I64, 1),
            ValTy::Bool,
        )),
        UnaryOp::Void | UnaryOp::TypeOf => unreachable!(),
    }
}

fn lower_typeof(ctx: &mut FnCtx, operand: &Expr) -> Result<TypedVal> {
    // Resolucao AST-level cobre os casos JS antes de tentar lowering:
    // - undeclaredVar -> "undefined" (sem ReferenceError)
    // - null literal  -> "object" (quirk JS)
    // - object/array literal -> "object"
    // - fn/arrow expr ou ident de user fn -> "function"
    // - Symbol(...) call -> "symbol"
    if let Expr::Ident(id) = operand {
        let name = id.sym.as_str();
        if ctx.user_fns.contains_key(name) {
            return ctx.emit_str_handle(b"function");
        }
        let is_js_global = matches!(name, "NaN" | "Infinity" | "undefined");
        if !is_js_global && ctx.read_local(name).is_none() {
            return ctx.emit_str_handle(b"undefined");
        }
        if name == "undefined" {
            return ctx.emit_str_handle(b"undefined");
        }
    }
    if let Expr::Lit(Lit::Null(_)) = operand {
        return ctx.emit_str_handle(b"object");
    }
    if matches!(operand, Expr::Object(_) | Expr::Array(_)) {
        return ctx.emit_str_handle(b"object");
    }
    if matches!(operand, Expr::Fn(_) | Expr::Arrow(_)) {
        return ctx.emit_str_handle(b"function");
    }
    if let Expr::Call(c) = operand {
        if let swc_ecma_ast::Callee::Expr(callee) = &c.callee {
            if let Expr::Ident(id) = callee.as_ref() {
                if id.sym.as_ref() == "Symbol" {
                    return ctx.emit_str_handle(b"symbol");
                }
            }
        }
    }
    let tv = lower_expr(ctx, operand)?;
    let ty_str = match tv.ty {
        ValTy::Bool => "boolean",
        ValTy::Handle => "string",
        ValTy::F64 | ValTy::I32 | ValTy::I64 | ValTy::U64 => "number",
    };
    ctx.emit_str_handle(ty_str.as_bytes())
}

pub(super) fn lower_tpl(ctx: &mut FnCtx, tpl: &Tpl) -> Result<TypedVal> {
    fn cook(q: &swc_ecma_ast::TplElement) -> Vec<u8> {
        q.cooked
            .as_ref()
            .map(|v| v.to_string_lossy().into_owned().into_bytes())
            .unwrap_or_default()
    }

    let first = tpl
        .quasis
        .first()
        .ok_or_else(|| anyhow!("template literal sem quasi inicial"))?;
    let mut acc = ctx.emit_str_handle(&cook(first))?;

    for (expr, quasi) in tpl.exprs.iter().zip(tpl.quasis.iter().skip(1)) {
        let val = lower_expr(ctx, expr)?;
        let concat = ctx.get_extern(
            "__RTS_FN_NS_GC_STRING_CONCAT",
            &[cl::I64, cl::I64],
            Some(cl::I64),
        )?;
        let rhs = ctx.coerce_to_handle(val)?.val;
        let inst = ctx.builder.ins().call(concat, &[acc.val, rhs]);
        let r = ctx.builder.inst_results(inst)[0];
        ctx.register_temp_handle(r);
        acc = TypedVal::new(r, ValTy::Handle);

        let bytes = cook(quasi);
        if !bytes.is_empty() {
            let qh = ctx.emit_str_handle(&bytes)?;
            let inst = ctx.builder.ins().call(concat, &[acc.val, qh.val]);
            let r = ctx.builder.inst_results(inst)[0];
            ctx.register_temp_handle(r);
            acc = TypedVal::new(r, ValTy::Handle);
        }
    }

    Ok(acc)
}
