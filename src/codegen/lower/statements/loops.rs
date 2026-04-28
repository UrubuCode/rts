use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{InstBuilder, condcodes::IntCC, types as cl};
use swc_ecma_ast::{Decl, ForHead, Pat, Stmt, VarDeclOrExpr};

use super::super::ctx::{FnCtx, ValTy};
use super::super::expressions::lower_expr;
use super::decls::ts_type_to_val_ty;
use super::lower_stmt;

pub(super) fn lower_while_stmt(ctx: &mut FnCtx, wh: &swc_ecma_ast::WhileStmt) -> Result<bool> {
    let header = ctx.builder.create_block();
    let body = ctx.builder.create_block();
    let exit = ctx.builder.create_block();

    ctx.builder.ins().jump(header, &[]);
    ctx.builder.switch_to_block(header);

    let cond = lower_expr(ctx, &wh.test)?;
    let is_true = ctx.to_branch_cond(cond);
    ctx.builder.ins().brif(is_true, body, &[], exit, &[]);

    ctx.builder.switch_to_block(body);
    ctx.loop_stack
        .push((exit, header, ctx.pending_label.take()));
    lower_stmt(ctx, &wh.body)?;
    ctx.loop_stack.pop();
    if !ctx.builder.is_unreachable() {
        ctx.builder.ins().jump(header, &[]);
    }
    ctx.builder.seal_block(body);
    ctx.builder.seal_block(header);

    ctx.builder.switch_to_block(exit);
    ctx.builder.seal_block(exit);
    Ok(false)
}

pub(super) fn lower_do_while_stmt(ctx: &mut FnCtx, dw: &swc_ecma_ast::DoWhileStmt) -> Result<bool> {
    let body = ctx.builder.create_block();
    let cond_block = ctx.builder.create_block();
    let exit = ctx.builder.create_block();

    ctx.builder.ins().jump(body, &[]);
    ctx.builder.switch_to_block(body);

    ctx.loop_stack
        .push((exit, cond_block, ctx.pending_label.take()));
    lower_stmt(ctx, &dw.body)?;
    ctx.loop_stack.pop();
    if !ctx.builder.is_unreachable() {
        ctx.builder.ins().jump(cond_block, &[]);
    }

    ctx.builder.switch_to_block(cond_block);
    let cond = lower_expr(ctx, &dw.test)?;
    let is_true = ctx.to_branch_cond(cond);
    ctx.builder.ins().brif(is_true, body, &[], exit, &[]);
    ctx.builder.seal_block(body);
    ctx.builder.seal_block(cond_block);

    ctx.builder.switch_to_block(exit);
    ctx.builder.seal_block(exit);
    Ok(false)
}

pub(super) fn lower_for_stmt(ctx: &mut FnCtx, for_stmt: &swc_ecma_ast::ForStmt) -> Result<bool> {
    if let Some(init) = &for_stmt.init {
        match init {
            VarDeclOrExpr::VarDecl(vd) => {
                lower_stmt(ctx, &Stmt::Decl(Decl::Var(vd.clone())))?;
            }
            VarDeclOrExpr::Expr(e) => {
                lower_expr(ctx, e)?;
            }
        }
    }

    let header = ctx.builder.create_block();
    let body = ctx.builder.create_block();
    let update_block = ctx.builder.create_block();
    let exit = ctx.builder.create_block();

    ctx.builder.ins().jump(header, &[]);
    ctx.builder.switch_to_block(header);

    if let Some(test) = &for_stmt.test {
        let cond = lower_expr(ctx, test)?;
        let is_true = ctx.to_branch_cond(cond);
        ctx.builder.ins().brif(is_true, body, &[], exit, &[]);
    } else {
        ctx.builder.ins().jump(body, &[]);
    }

    ctx.builder.switch_to_block(body);
    ctx.loop_stack
        .push((exit, update_block, ctx.pending_label.take()));
    lower_stmt(ctx, &for_stmt.body)?;
    ctx.loop_stack.pop();
    if !ctx.builder.is_unreachable() {
        ctx.builder.ins().jump(update_block, &[]);
    }
    ctx.builder.seal_block(body);

    ctx.builder.switch_to_block(update_block);
    if let Some(update) = &for_stmt.update {
        lower_expr(ctx, update)?;
    }
    if !ctx.builder.is_unreachable() {
        ctx.builder.ins().jump(header, &[]);
    }
    ctx.builder.seal_block(update_block);
    ctx.builder.seal_block(header);

    ctx.builder.switch_to_block(exit);
    ctx.builder.seal_block(exit);
    Ok(false)
}

pub(super) fn lower_for_of(ctx: &mut FnCtx, for_of: &swc_ecma_ast::ForOfStmt) -> Result<bool> {
    if for_of.is_await {
        return Err(anyhow!("for-await-of nao suportado"));
    }

    let (bind_name, bind_ty) = match &for_of.left {
        ForHead::VarDecl(vd) => {
            if vd.decls.len() != 1 {
                return Err(anyhow!("for-of bind deve declarar uma variavel"));
            }
            match &vd.decls[0].name {
                Pat::Ident(id) => {
                    let ty = id
                        .type_ann
                        .as_ref()
                        .and_then(|t| ts_type_to_val_ty(&t.type_ann))
                        .unwrap_or(ValTy::I64);
                    (id.sym.as_str().to_string(), ty)
                }
                _ => return Err(anyhow!("for-of bind deve ser ident simples")),
            }
        }
        ForHead::Pat(p) => match p.as_ref() {
            Pat::Ident(id) => (id.sym.as_str().to_string(), ValTy::I64),
            _ => return Err(anyhow!("for-of bind deve ser ident simples")),
        },
        ForHead::UsingDecl(_) => return Err(anyhow!("`using` em for-of nao suportado")),
    };

    let iter_tv = lower_expr(ctx, &for_of.right)?;
    let handle = ctx.coerce_to_i64(iter_tv).val;
    let len_fref = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_VEC_LEN", &[cl::I64], Some(cl::I64))?;
    let inst = ctx.builder.ins().call(len_fref, &[handle]);
    let len = ctx.builder.inst_results(inst)[0];

    let get_fref = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_VEC_GET",
        &[cl::I64, cl::I64],
        Some(cl::I64),
    )?;
    let zero = ctx.builder.ins().iconst(cl::I64, 0);
    if matches!(&for_of.left, ForHead::VarDecl(_)) {
        ctx.declare_local(&bind_name, bind_ty, zero);
    }

    if let swc_ecma_ast::Expr::Ident(id) = for_of.right.as_ref() {
        let arr_name = id.sym.as_str();
        if let Some(elem_cls) = ctx.local_array_class_ty.get(arr_name).cloned() {
            ctx.local_class_ty.insert(bind_name.clone(), elem_cls);
        }
    }

    let counter_name = format!("__rts_for_of_i_{:p}", &for_of.span);
    ctx.declare_local(&counter_name, ValTy::I64, zero);

    let header = ctx.builder.create_block();
    let body = ctx.builder.create_block();
    let update_block = ctx.builder.create_block();
    let exit = ctx.builder.create_block();

    ctx.builder.ins().jump(header, &[]);
    ctx.builder.switch_to_block(header);

    let i_now = ctx
        .read_local(&counter_name)
        .ok_or_else(|| anyhow!("for-of counter sumiu"))?;
    let is_in_range = ctx
        .builder
        .ins()
        .icmp(IntCC::SignedLessThan, i_now.val, len);
    ctx.builder.ins().brif(is_in_range, body, &[], exit, &[]);

    ctx.builder.switch_to_block(body);
    let i_now = ctx
        .read_local(&counter_name)
        .ok_or_else(|| anyhow!("for-of counter sumiu"))?;
    let inst = ctx.builder.ins().call(get_fref, &[handle, i_now.val]);
    let elem = ctx.builder.inst_results(inst)[0];
    ctx.write_local(&bind_name, elem)?;

    ctx.loop_stack
        .push((exit, update_block, ctx.pending_label.take()));
    lower_stmt(ctx, &for_of.body)?;
    ctx.loop_stack.pop();
    if !ctx.builder.is_unreachable() {
        ctx.builder.ins().jump(update_block, &[]);
    }
    ctx.builder.seal_block(body);

    ctx.builder.switch_to_block(update_block);
    let i_now = ctx
        .read_local(&counter_name)
        .ok_or_else(|| anyhow!("for-of counter sumiu"))?;
    let one = ctx.builder.ins().iconst(cl::I64, 1);
    let i_next = ctx.builder.ins().iadd(i_now.val, one);
    ctx.write_local(&counter_name, i_next)?;
    ctx.builder.ins().jump(header, &[]);
    ctx.builder.seal_block(update_block);
    ctx.builder.seal_block(header);

    ctx.builder.switch_to_block(exit);
    ctx.builder.seal_block(exit);
    Ok(false)
}

pub(super) fn lower_for_in(ctx: &mut FnCtx, for_in: &swc_ecma_ast::ForInStmt) -> Result<bool> {
    let bind_name = match &for_in.left {
        ForHead::VarDecl(vd) => {
            if vd.decls.len() != 1 {
                return Err(anyhow!("for-in bind deve declarar uma variavel"));
            }
            match &vd.decls[0].name {
                Pat::Ident(id) => id.sym.as_str().to_string(),
                _ => return Err(anyhow!("for-in bind deve ser ident simples")),
            }
        }
        ForHead::Pat(p) => match p.as_ref() {
            Pat::Ident(id) => id.sym.as_str().to_string(),
            _ => return Err(anyhow!("for-in bind deve ser ident simples")),
        },
        ForHead::UsingDecl(_) => return Err(anyhow!("`using` em for-in nao suportado")),
    };

    let iter_tv = lower_expr(ctx, &for_in.right)?;
    let handle = ctx.coerce_to_i64(iter_tv).val;
    let len_fref = ctx.get_extern("__RTS_FN_NS_COLLECTIONS_MAP_LEN", &[cl::I64], Some(cl::I64))?;
    let inst = ctx.builder.ins().call(len_fref, &[handle]);
    let len = ctx.builder.inst_results(inst)[0];

    let key_at_fref = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_MAP_KEY_AT",
        &[cl::I64, cl::I64],
        Some(cl::I64),
    )?;
    let zero = ctx.builder.ins().iconst(cl::I64, 0);
    if matches!(&for_in.left, ForHead::VarDecl(_)) {
        ctx.declare_local(&bind_name, ValTy::Handle, zero);
    }

    let counter_name = format!("__rts_for_in_i_{:p}", &for_in.span);
    ctx.declare_local(&counter_name, ValTy::I64, zero);

    let header = ctx.builder.create_block();
    let body = ctx.builder.create_block();
    let update_block = ctx.builder.create_block();
    let exit = ctx.builder.create_block();

    ctx.builder.ins().jump(header, &[]);
    ctx.builder.switch_to_block(header);

    let i_now = ctx
        .read_local(&counter_name)
        .ok_or_else(|| anyhow!("for-in counter sumiu"))?;
    let is_in_range = ctx
        .builder
        .ins()
        .icmp(IntCC::SignedLessThan, i_now.val, len);
    ctx.builder.ins().brif(is_in_range, body, &[], exit, &[]);

    ctx.builder.switch_to_block(body);
    let i_now = ctx
        .read_local(&counter_name)
        .ok_or_else(|| anyhow!("for-in counter sumiu"))?;
    let inst = ctx.builder.ins().call(key_at_fref, &[handle, i_now.val]);
    let key_handle = ctx.builder.inst_results(inst)[0];
    ctx.write_local(&bind_name, key_handle)?;

    ctx.loop_stack
        .push((exit, update_block, ctx.pending_label.take()));
    lower_stmt(ctx, &for_in.body)?;
    ctx.loop_stack.pop();
    if !ctx.builder.is_unreachable() {
        ctx.builder.ins().jump(update_block, &[]);
    }
    ctx.builder.seal_block(body);

    ctx.builder.switch_to_block(update_block);
    let i_now = ctx
        .read_local(&counter_name)
        .ok_or_else(|| anyhow!("for-in counter sumiu"))?;
    let one = ctx.builder.ins().iconst(cl::I64, 1);
    let i_next = ctx.builder.ins().iadd(i_now.val, one);
    ctx.write_local(&counter_name, i_next)?;
    ctx.builder.ins().jump(header, &[]);
    ctx.builder.seal_block(update_block);
    ctx.builder.seal_block(header);

    ctx.builder.switch_to_block(exit);
    ctx.builder.seal_block(exit);
    Ok(false)
}
