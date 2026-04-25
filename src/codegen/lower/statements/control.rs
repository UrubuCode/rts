use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{InstBuilder, condcodes::IntCC, types as cl};
use swc_ecma_ast::{Expr, Lit};

use super::super::ctx::{FnCtx, ValTy};
use super::super::expressions::lower_expr;
use super::{lower_block, lower_stmt};

pub(super) fn lower_if_stmt(ctx: &mut FnCtx, if_stmt: &swc_ecma_ast::IfStmt) -> Result<bool> {
    let cond = lower_expr(ctx, &if_stmt.test)?;
    let cond_i64 = ctx.coerce_to_i64(cond);
    let zero = ctx.builder.ins().iconst(cl::I64, 0);
    let is_true = ctx.builder.ins().icmp(IntCC::NotEqual, cond_i64.val, zero);

    let then_block = ctx.builder.create_block();
    let merge_block = ctx.builder.create_block();

    if if_stmt.alt.is_some() {
        let else_block = ctx.builder.create_block();
        ctx.builder
            .ins()
            .brif(is_true, then_block, &[], else_block, &[]);

        ctx.builder.switch_to_block(then_block);
        ctx.builder.seal_block(then_block);
        let then_exits = lower_stmt(ctx, &if_stmt.cons)?;
        if !then_exits {
            ctx.builder.ins().jump(merge_block, &[]);
        }

        ctx.builder.switch_to_block(else_block);
        ctx.builder.seal_block(else_block);
        let else_exits = lower_stmt(ctx, if_stmt.alt.as_ref().unwrap())?;
        if !else_exits {
            ctx.builder.ins().jump(merge_block, &[]);
        }

        ctx.builder.switch_to_block(merge_block);
        ctx.builder.seal_block(merge_block);
        Ok(then_exits && else_exits)
    } else {
        ctx.builder
            .ins()
            .brif(is_true, then_block, &[], merge_block, &[]);

        ctx.builder.switch_to_block(then_block);
        ctx.builder.seal_block(then_block);
        let then_exits = lower_stmt(ctx, &if_stmt.cons)?;
        if !then_exits && !ctx.builder.is_unreachable() {
            ctx.builder.ins().jump(merge_block, &[]);
        }

        ctx.builder.switch_to_block(merge_block);
        ctx.builder.seal_block(merge_block);
        Ok(false)
    }
}

pub(super) fn lower_switch_stmt(ctx: &mut FnCtx, sw: &swc_ecma_ast::SwitchStmt) -> Result<bool> {
    let discriminant = lower_expr(ctx, &sw.discriminant)?;
    let disc_i64 = ctx.coerce_to_i64(discriminant);
    let exit = ctx.builder.create_block();

    let case_blocks: Vec<cranelift_codegen::ir::Block> = sw
        .cases
        .iter()
        .map(|_| ctx.builder.create_block())
        .collect();

    let default_idx = sw.cases.iter().position(|case| case.test.is_none());
    let non_default_indices: Vec<usize> = sw
        .cases
        .iter()
        .enumerate()
        .filter_map(|(idx, case)| if case.test.is_some() { Some(idx) } else { None })
        .collect();

    let integer_tests: Option<Vec<u128>> = non_default_indices
        .iter()
        .map(|case_idx| {
            let test_expr = sw.cases[*case_idx].test.as_ref()?;
            extract_integer_literal(test_expr)
        })
        .collect();

    if non_default_indices.is_empty() {
        if let Some(di) = default_idx {
            ctx.builder.ins().jump(case_blocks[di], &[]);
        } else {
            ctx.builder.ins().jump(exit, &[]);
        }
    } else if let Some(values) = integer_tests {
        let mut table = cranelift_frontend::Switch::new();
        for (pos, case_idx) in non_default_indices.iter().enumerate() {
            table.set_entry(values[pos], case_blocks[*case_idx]);
        }
        let fallback = default_idx.map(|di| case_blocks[di]).unwrap_or(exit);
        table.emit(ctx.builder, disc_i64.val, fallback);
    } else {
        for (pos, case_idx) in non_default_indices.iter().enumerate() {
            let test_expr = sw.cases[*case_idx]
                .test
                .as_ref()
                .expect("non-default case must have test expression");
            let test_val = lower_expr(ctx, test_expr)?;
            let test_i64 = ctx.coerce_to_i64(test_val);
            let eq = ctx
                .builder
                .ins()
                .icmp(IntCC::Equal, disc_i64.val, test_i64.val);

            let false_block = if pos + 1 < non_default_indices.len() {
                ctx.builder.create_block()
            } else {
                default_idx.map(|di| case_blocks[di]).unwrap_or(exit)
            };

            ctx.builder
                .ins()
                .brif(eq, case_blocks[*case_idx], &[], false_block, &[]);

            if pos + 1 < non_default_indices.len() {
                ctx.builder.switch_to_block(false_block);
                ctx.builder.seal_block(false_block);
            }
        }
    }

    ctx.loop_stack.push((exit, exit, ctx.pending_label.take()));
    for (i, case) in sw.cases.iter().enumerate() {
        ctx.builder.switch_to_block(case_blocks[i]);
        ctx.builder.seal_block(case_blocks[i]);
        let mut case_exits = false;
        for s in &case.cons {
            let exits = lower_stmt(ctx, s)?;
            if exits {
                case_exits = true;
                break;
            }
        }
        if !case_exits && !ctx.builder.is_unreachable() {
            let next = if i + 1 < case_blocks.len() {
                case_blocks[i + 1]
            } else {
                exit
            };
            ctx.builder.ins().jump(next, &[]);
        }
    }
    ctx.loop_stack.pop();

    ctx.builder.switch_to_block(exit);
    ctx.builder.seal_block(exit);
    Ok(false)
}

pub(super) fn lower_return_stmt(
    ctx: &mut FnCtx,
    ret_stmt: &swc_ecma_ast::ReturnStmt,
) -> Result<bool> {
    if let Some(arg) = &ret_stmt.arg {
        let is_direct_tail_call = is_direct_call_expr(arg);
        let prev = ctx.in_tail_position;
        ctx.in_tail_position = is_direct_tail_call;
        let tv = lower_expr(ctx, arg)?;
        ctx.in_tail_position = prev;

        let coerced = match ctx.return_ty {
            Some(ValTy::I32) => ctx.coerce_to_i32(tv),
            Some(ValTy::F64) => ctx.coerce_to_f64(tv),
            Some(ValTy::Handle) => ctx.coerce_to_handle(tv)?,
            _ => ctx.coerce_to_i64(tv),
        };
        ctx.builder.ins().return_(&[coerced.val]);
    } else {
        ctx.builder.ins().return_(&[]);
    }
    Ok(true)
}

pub(super) fn lower_break_stmt(ctx: &mut FnCtx, b: &swc_ecma_ast::BreakStmt) -> Result<bool> {
    let target = if let Some(lbl) = &b.label {
        let name = lbl.sym.as_str();
        ctx.break_block_for_label(name)
            .ok_or_else(|| anyhow!("break: label `{name}` nao encontrado em loops envolventes"))?
    } else {
        ctx.break_block()
            .ok_or_else(|| anyhow!("break outside of loop or switch"))?
    };
    ctx.builder.ins().jump(target, &[]);
    Ok(true)
}

pub(super) fn lower_continue_stmt(ctx: &mut FnCtx, c: &swc_ecma_ast::ContinueStmt) -> Result<bool> {
    let target = if let Some(lbl) = &c.label {
        let name = lbl.sym.as_str();
        ctx.continue_block_for_label(name).ok_or_else(|| {
            anyhow!("continue: label `{name}` nao encontrado em loops envolventes")
        })?
    } else {
        ctx.continue_block()
            .ok_or_else(|| anyhow!("continue outside of loop"))?
    };
    ctx.builder.ins().jump(target, &[]);
    Ok(true)
}

pub(super) fn lower_labeled_stmt(ctx: &mut FnCtx, lbl: &swc_ecma_ast::LabeledStmt) -> Result<bool> {
    let name = lbl.label.sym.as_str().to_string();
    let prev = ctx.pending_label.take();
    ctx.pending_label = Some(name);
    let terminated = lower_stmt(ctx, &lbl.body)?;
    ctx.pending_label = prev;
    Ok(terminated)
}

pub(super) fn lower_throw_stmt(
    ctx: &mut FnCtx,
    throw_stmt: &swc_ecma_ast::ThrowStmt,
) -> Result<bool> {
    let tv = lower_expr(ctx, &throw_stmt.arg)?;
    let handle = ctx.coerce_to_handle(tv)?;
    let set_fref = ctx.get_extern("__RTS_FN_RT_ERROR_SET", &[cl::I64], None)?;
    ctx.builder.ins().call(set_fref, &[handle.val]);
    Ok(false)
}

pub(super) fn lower_try_stmt(ctx: &mut FnCtx, t: &swc_ecma_ast::TryStmt) -> Result<bool> {
    let has_catch = t.handler.is_some();
    let has_finally = t.finalizer.is_some();

    let clear_fref = ctx.get_extern("__RTS_FN_RT_ERROR_CLEAR", &[], None)?;
    ctx.builder.ins().call(clear_fref, &[]);
    lower_block(ctx, &t.block)?;

    let catch_block = if has_catch {
        Some(ctx.builder.create_block())
    } else {
        None
    };
    let finally_block = if has_finally {
        Some(ctx.builder.create_block())
    } else {
        None
    };
    let after_block = ctx.builder.create_block();

    if !ctx.builder.is_unreachable() {
        let get_fref = ctx.get_extern("__RTS_FN_RT_ERROR_GET", &[], Some(cl::I64))?;
        let inst = ctx.builder.ins().call(get_fref, &[]);
        let err_handle = ctx.builder.inst_results(inst)[0];
        let zero = ctx.builder.ins().iconst(cl::I64, 0);
        let is_err = ctx.builder.ins().icmp(IntCC::NotEqual, err_handle, zero);
        let ok_target = finally_block.unwrap_or(after_block);
        let err_target = catch_block.unwrap_or(ok_target);
        ctx.builder
            .ins()
            .brif(is_err, err_target, &[], ok_target, &[]);
    }

    if let Some(cb) = catch_block {
        ctx.builder.switch_to_block(cb);
        ctx.builder.seal_block(cb);

        let handler = t.handler.as_ref().unwrap();
        if let Some(param) = &handler.param {
            if let swc_ecma_ast::Pat::Ident(id) = param {
                let name = id.id.sym.as_str();
                let get_fref = ctx.get_extern("__RTS_FN_RT_ERROR_GET", &[], Some(cl::I64))?;
                let inst = ctx.builder.ins().call(get_fref, &[]);
                let err_handle = ctx.builder.inst_results(inst)[0];
                ctx.declare_local(name, ValTy::Handle, err_handle);
            }
        }
        let clear_fref = ctx.get_extern("__RTS_FN_RT_ERROR_CLEAR", &[], None)?;
        ctx.builder.ins().call(clear_fref, &[]);

        lower_block(ctx, &handler.body)?;
        if !ctx.builder.is_unreachable() {
            let next = finally_block.unwrap_or(after_block);
            ctx.builder.ins().jump(next, &[]);
        }
    }

    if let Some(fb) = finally_block {
        ctx.builder.switch_to_block(fb);
        ctx.builder.seal_block(fb);
        let finalizer = t.finalizer.as_ref().unwrap();
        lower_block(ctx, finalizer)?;
        if !ctx.builder.is_unreachable() {
            ctx.builder.ins().jump(after_block, &[]);
        }
    }

    ctx.builder.switch_to_block(after_block);
    ctx.builder.seal_block(after_block);
    Ok(false)
}

fn is_direct_call_expr(expr: &swc_ecma_ast::Expr) -> bool {
    match expr {
        swc_ecma_ast::Expr::Call(_) => true,
        swc_ecma_ast::Expr::Paren(p) => is_direct_call_expr(&p.expr),
        _ => false,
    }
}

fn extract_integer_literal(expr: &Expr) -> Option<u128> {
    match expr {
        Expr::Lit(Lit::Num(n)) => {
            let v = n.value;
            if v.fract() != 0.0 || !v.is_finite() {
                return None;
            }
            if let Some(raw) = n.raw.as_ref() {
                let bytes = raw.as_bytes();
                if bytes.iter().any(|&b| b == b'.' || b == b'e' || b == b'E') {
                    return None;
                }
            }
            Some(v as i64 as u128)
        }
        Expr::Unary(u) if matches!(u.op, swc_ecma_ast::UnaryOp::Minus) => {
            let inner = extract_integer_literal(&u.arg)?;
            Some((inner as i64).wrapping_neg() as u128)
        }
        Expr::Unary(u) if matches!(u.op, swc_ecma_ast::UnaryOp::Plus) => {
            extract_integer_literal(&u.arg)
        }
        Expr::Paren(p) => extract_integer_literal(&p.expr),
        _ => None,
    }
}
