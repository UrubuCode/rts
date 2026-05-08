use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{InstBuilder, condcodes::IntCC, types as cl};
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
            // Cache key: (bits, is_float, block) — per-block para evitar
            // usar Values de blocos não-dominadores (mesmo bug do str_handle_cache).
            let cur_block = ctx.builder.current_block().unwrap_or_else(|| {
                cranelift_codegen::ir::Block::with_number(0).unwrap()
            });
            let cache_key = (v.to_bits(), wrote_as_float as u8, cur_block);
            if let Some(&cached) = ctx.num_val_cache.get(&cache_key) {
                let ty = if !wrote_as_float && v.fract() == 0.0 && v >= i32::MIN as f64 && v <= i32::MAX as f64 {
                    ValTy::I32
                } else if !wrote_as_float && v.fract() == 0.0 && v.is_finite() {
                    ValTy::I64
                } else {
                    ValTy::F64
                };
                return Ok(TypedVal::new(cached, ty));
            }
            let tv = if !wrote_as_float
                && v.fract() == 0.0
                && v >= i32::MIN as f64
                && v <= i32::MAX as f64
            {
                TypedVal::new(ctx.builder.ins().iconst(cl::I32, v as i64), ValTy::I32)
            } else if !wrote_as_float && v.fract() == 0.0 && v.is_finite() {
                TypedVal::new(ctx.builder.ins().iconst(cl::I64, v as i64), ValTy::I64)
            } else {
                TypedVal::new(ctx.builder.ins().f64const(v), ValTy::F64)
            };
            ctx.num_val_cache.insert(cache_key, tv.val);
            Ok(tv)
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
            use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
            // (#550) Handle: !handle === true quando handle == 0 OU handle
            // aponta para Entry::String vazia. Caso contrario falsy.
            // Esquema:
            //   handle == 0      -> falsy   -> !x = true
            //   string_len > 0   -> truthy  -> !x = false
            //   string_len == 0  -> falsy   -> !x = true
            //   string_len < 0 (nao-string handle) -> truthy -> !x = false
            if matches!(operand.ty, ValTy::Handle) {
                let str_len = ctx.get_extern("__RTS_FN_NS_GC_STRING_LEN", &[cl::I64], Some(cl::I64))?;
                let zero = ctx.builder.ins().iconst(cl::I64, 0);
                let is_zero_h = ctx.builder.ins().icmp(IntCC::Equal, operand.val, zero);
                let inst = ctx.builder.ins().call(str_len, &[operand.val]);
                let len = ctx.builder.inst_results(inst)[0];
                let is_empty_str = ctx.builder.ins().icmp(IntCC::Equal, len, zero);
                let falsy = ctx.builder.ins().bor(is_zero_h, is_empty_str);
                return Ok(TypedVal::new(
                    ctx.builder.ins().uextend(cl::I64, falsy),
                    ValTy::Bool,
                ));
            }
            // F64: NaN tambem e' falsy. !x = (x == 0 OR x eh NaN).
            // Usa fcmp Equal(x, 0) | Equal(x, x)==false. Como Equal eh
            // ordered (false p/ NaN), nao usamos NotEqual nem
            // OrderedNotEqual (panic em aarch64 Cranelift 0.131).
            if matches!(operand.ty, ValTy::F64) {
                let zero = ctx.builder.ins().f64const(0.0);
                // is_zero = x == 0
                let is_zero = ctx.builder.ins().fcmp(FloatCC::Equal, operand.val, zero);
                // is_self_eq = x == x (false sse NaN)
                let is_self_eq = ctx.builder.ins().fcmp(FloatCC::Equal, operand.val, operand.val);
                let is_nan_i = {
                    let i = ctx.builder.ins().uextend(cl::I64, is_self_eq);
                    let one = ctx.builder.ins().iconst(cl::I64, 1);
                    ctx.builder.ins().bxor(i, one)
                };
                let is_zero_i = ctx.builder.ins().uextend(cl::I64, is_zero);
                let falsy = ctx.builder.ins().bor(is_zero_i, is_nan_i);
                return Ok(TypedVal::new(falsy, ValTy::Bool));
            }
            let value = ctx.coerce_to_i64(operand).val;
            let zero = ctx.builder.ins().iconst(cl::I64, 0);
            let is_zero =
                ctx.builder
                    .ins()
                    .icmp(IntCC::Equal, value, zero);
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
        // Classes (user e globais) sao "function" em JS (typeof Class === 'function').
        if ctx.classes.contains_key(name)
            || crate::abi::global_class_lookup(name).is_some()
        {
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
    // Handle pode ser string OU symbol/function/object/etc. Em vez de
    // assumir "string" estaticamente, despacha pra runtime helper que
    // inspeciona Entry e retorna o tipo JS correto.
    if matches!(tv.ty, ValTy::Handle) {
        use cranelift_codegen::ir::InstBuilder;
        let typeof_fn = ctx.get_extern(
            "__RTS_FN_RT_TYPEOF_HANDLE",
            &[cl::I64],
            Some(cl::I64),
        )?;
        let h = ctx.coerce_to_i64(tv).val;
        let inst = ctx.builder.ins().call(typeof_fn, &[h]);
        let r = ctx.builder.inst_results(inst)[0];
        return Ok(TypedVal::new(r, ValTy::Handle));
    }
    let ty_str = match tv.ty {
        ValTy::Bool => "boolean",
        ValTy::Handle => "string", // unreachable but exhaustive
        ValTy::F64 | ValTy::I32 | ValTy::I64 | ValTy::U64
        | ValTy::I8 | ValTy::I16 | ValTy::U8 | ValTy::U16 => "number",
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
        // (#432) Se o val veio de optional chain, decide em runtime:
        // val == 0 → handle de "undefined"; senao coerce normal.
        let is_opt_chain = ctx.optional_chain_values.contains(&val.val);
        // (#proto-method) Se veio de var_member_call, use coerce auto que
        // detecta string handle em runtime.
        let is_var_member_call = ctx.var_member_call_values.contains(&val.val);
        let val_ty = val.ty;
        let rhs = if is_opt_chain {
            let undef_h = ctx.emit_str_handle(b"undefined")?.val;
            let val_i64 = ctx.coerce_to_i64(val).val;
            let normal_h = ctx.coerce_to_handle(val)?.val;
            let zero = ctx.builder.ins().iconst(cl::I64, 0);
            let is_null = ctx.builder.ins().icmp(IntCC::Equal, val_i64, zero);
            ctx.builder.ins().select(is_null, undef_h, normal_h)
        } else if is_var_member_call || matches!(val_ty, ValTy::Handle) {
            // (#573) Handle ambiguo (string/numero embutido) usa COERCE_AUTO.
            let val_i64 = ctx.coerce_to_i64(val).val;
            let coerce_fn = ctx.get_extern(
                "__RTS_FN_RT_TPL_COERCE_AUTO",
                &[cl::I64],
                Some(cl::I64),
            )?;
            let inst = ctx.builder.ins().call(coerce_fn, &[val_i64]);
            ctx.builder.inst_results(inst)[0]
        } else {
            ctx.coerce_to_handle(val)?.val
        };
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
