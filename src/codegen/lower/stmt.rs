//! Statement lowering to Cranelift IR.
//!
//! `lower_stmt` handles: variable declarations, expression statements,
//! if/else, while, do-while, for, switch/case, return, break, continue.

use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{InstBuilder, condcodes::IntCC, types as cl};
use swc_ecma_ast::{BlockStmt, Decl, Pat, Stmt, VarDeclOrExpr};

use super::ctx::{FnCtx, TypedVal, ValTy};
use super::expr::lower_expr;

/// Lowers a single SWC statement into the current builder.
///
/// Returns `true` if the statement unconditionally transfers control
/// (return / break / continue), so the caller can skip unreachable code.
pub fn lower_stmt(ctx: &mut FnCtx, stmt: &Stmt) -> Result<bool> {
    match stmt {
        // ── Variable declaration ──────────────────────────────────────────
        Stmt::Decl(Decl::Var(var_decl)) => {
            for decl in &var_decl.decls {
                let name = match &decl.name {
                    Pat::Ident(id) => id.sym.as_str().to_string(),
                    Pat::Array(_) | Pat::Object(_) => {
                        return Err(anyhow!("destructuring not supported"));
                    }
                    other => return Err(anyhow!("unsupported binding pattern: {other:?}")),
                };

                // Determine type from annotation or initialiser
                let ann_ty = match &decl.name {
                    Pat::Ident(id) => id
                        .type_ann
                        .as_ref()
                        .and_then(|t| ts_type_to_val_ty(&t.type_ann)),
                    _ => None,
                };

                let (init_val, inferred_ty) = if let Some(init) = &decl.init {
                    let tv = lower_expr(ctx, init)?;
                    (tv.val, tv.ty)
                } else {
                    let ty = ann_ty.unwrap_or(ValTy::I64);
                    let zero = zero_for_ty(ctx, ty);
                    (zero, ty)
                };

                let ty = if ctx.module_scope && ctx.has_global(&name) {
                    ctx.var_ty(&name).unwrap_or(ann_ty.unwrap_or(inferred_ty))
                } else {
                    ann_ty.unwrap_or(inferred_ty)
                };
                // Coerce init to declared type
                let init_coerced = match ty {
                    ValTy::I32 => {
                        let tv = TypedVal::new(init_val, inferred_ty);
                        ctx.coerce_to_i32(tv).val
                    }
                    ValTy::I64 => {
                        let tv = TypedVal::new(init_val, inferred_ty);
                        ctx.coerce_to_i64(tv).val
                    }
                    _ => init_val,
                };

                if ctx.module_scope && ctx.has_global(&name) {
                    // Top-level declarations initialize module globals.
                    ctx.write_local(&name, init_coerced)?;
                } else {
                    ctx.declare_local(&name, ty, init_coerced);
                }
            }
            Ok(false)
        }

        // ── Expression statement ──────────────────────────────────────────
        Stmt::Expr(expr_stmt) => {
            lower_expr(ctx, &expr_stmt.expr)?;
            Ok(false)
        }

        // ── Block ─────────────────────────────────────────────────────────
        Stmt::Block(block) => lower_block(ctx, block),

        // ── If / else ─────────────────────────────────────────────────────
        Stmt::If(if_stmt) => {
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

        // ── While ─────────────────────────────────────────────────────────
        Stmt::While(wh) => {
            let header = ctx.builder.create_block();
            let body = ctx.builder.create_block();
            let exit = ctx.builder.create_block();

            ctx.builder.ins().jump(header, &[]);
            ctx.builder.switch_to_block(header);

            let cond = lower_expr(ctx, &wh.test)?;
            let cond_i64 = ctx.coerce_to_i64(cond);
            let zero = ctx.builder.ins().iconst(cl::I64, 0);
            let is_true = ctx.builder.ins().icmp(IntCC::NotEqual, cond_i64.val, zero);
            ctx.builder.ins().brif(is_true, body, &[], exit, &[]);

            ctx.builder.switch_to_block(body);
            ctx.loop_stack.push((exit, header));
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

        // ── Do-while ──────────────────────────────────────────────────────
        Stmt::DoWhile(dw) => {
            let body = ctx.builder.create_block();
            let cond_block = ctx.builder.create_block();
            let exit = ctx.builder.create_block();

            ctx.builder.ins().jump(body, &[]);
            ctx.builder.switch_to_block(body);

            ctx.loop_stack.push((exit, cond_block));
            lower_stmt(ctx, &dw.body)?;
            ctx.loop_stack.pop();
            if !ctx.builder.is_unreachable() {
                ctx.builder.ins().jump(cond_block, &[]);
            }

            ctx.builder.switch_to_block(cond_block);
            let cond = lower_expr(ctx, &dw.test)?;
            let cond_i64 = ctx.coerce_to_i64(cond);
            let zero = ctx.builder.ins().iconst(cl::I64, 0);
            let is_true = ctx.builder.ins().icmp(IntCC::NotEqual, cond_i64.val, zero);
            ctx.builder.ins().brif(is_true, body, &[], exit, &[]);
            ctx.builder.seal_block(body);
            ctx.builder.seal_block(cond_block);

            ctx.builder.switch_to_block(exit);
            ctx.builder.seal_block(exit);
            Ok(false)
        }

        // ── For ───────────────────────────────────────────────────────────
        Stmt::For(for_stmt) => {
            // Init
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

            // Condition
            if let Some(test) = &for_stmt.test {
                let cond = lower_expr(ctx, test)?;
                let cond_i64 = ctx.coerce_to_i64(cond);
                let zero = ctx.builder.ins().iconst(cl::I64, 0);
                let is_true = ctx.builder.ins().icmp(IntCC::NotEqual, cond_i64.val, zero);
                ctx.builder.ins().brif(is_true, body, &[], exit, &[]);
            } else {
                ctx.builder.ins().jump(body, &[]);
            }

            ctx.builder.switch_to_block(body);
            ctx.loop_stack.push((exit, update_block));
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

        // ── Switch ────────────────────────────────────────────────────────
        Stmt::Switch(sw) => {
            let discriminant = lower_expr(ctx, &sw.discriminant)?;
            let disc_i64 = ctx.coerce_to_i64(discriminant);
            let exit = ctx.builder.create_block();

            // One block per case in source order (fallthrough semantics).
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

            // Build comparison chain. False branch continues comparisons;
            // final false goes to default (if present) or exits switch.
            if non_default_indices.is_empty() {
                if let Some(di) = default_idx {
                    ctx.builder.ins().jump(case_blocks[di], &[]);
                } else {
                    ctx.builder.ins().jump(exit, &[]);
                }
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

            // Emit case bodies.
            ctx.loop_stack.push((exit, exit)); // break target
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
                // Fallthrough to next case if not terminated.
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

        // ── Return ────────────────────────────────────────────────────────
        Stmt::Return(ret_stmt) => {
            if let Some(arg) = &ret_stmt.arg {
                let tv = lower_expr(ctx, arg)?;
                let as_i64 = ctx.coerce_to_i64(tv);
                ctx.builder.ins().return_(&[as_i64.val]);
            } else {
                ctx.builder.ins().return_(&[]);
            }
            Ok(true)
        }

        // ── Break ─────────────────────────────────────────────────────────
        Stmt::Break(_) => {
            if let Some(brk) = ctx.break_block() {
                ctx.builder.ins().jump(brk, &[]);
                Ok(true)
            } else {
                Err(anyhow!("break outside of loop or switch"))
            }
        }

        // ── Continue ──────────────────────────────────────────────────────
        Stmt::Continue(_) => {
            if let Some(cont) = ctx.continue_block() {
                ctx.builder.ins().jump(cont, &[]);
                Ok(true)
            } else {
                Err(anyhow!("continue outside of loop"))
            }
        }

        Stmt::Empty(_) => Ok(false),

        other => Err(anyhow!("unsupported statement: {}", stmt_kind_name(other))),
    }
}

pub fn lower_block(ctx: &mut FnCtx, block: &BlockStmt) -> Result<bool> {
    for s in &block.stmts {
        let exits = lower_stmt(ctx, s)?;
        if exits {
            return Ok(true);
        }
    }
    Ok(false)
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn ts_type_to_val_ty(ty: &swc_ecma_ast::TsType) -> Option<ValTy> {
    use swc_ecma_ast::{TsKeywordTypeKind, TsType};
    if let TsType::TsKeywordType(kw) = ty {
        return Some(match kw.kind {
            TsKeywordTypeKind::TsNumberKeyword => ValTy::I32,
            TsKeywordTypeKind::TsBooleanKeyword => ValTy::Bool,
            TsKeywordTypeKind::TsStringKeyword => ValTy::Handle,
            TsKeywordTypeKind::TsVoidKeyword => ValTy::I64,
            _ => return None,
        });
    }
    if let TsType::TsTypeRef(r) = ty {
        let name = match &r.type_name {
            swc_ecma_ast::TsEntityName::Ident(id) => id.sym.as_str(),
            _ => return None,
        };
        return Some(ValTy::from_annotation(name));
    }
    None
}

fn zero_for_ty(ctx: &mut FnCtx, ty: ValTy) -> cranelift_codegen::ir::Value {
    match ty {
        ValTy::I32 => ctx.builder.ins().iconst(cl::I32, 0),
        ValTy::F64 => ctx.builder.ins().f64const(0.0),
        _ => ctx.builder.ins().iconst(cl::I64, 0),
    }
}

fn stmt_kind_name(stmt: &Stmt) -> &'static str {
    match stmt {
        Stmt::Block(_) => "block",
        Stmt::Empty(_) => "empty",
        Stmt::Debugger(_) => "debugger",
        Stmt::With(_) => "with",
        Stmt::Return(_) => "return",
        Stmt::Labeled(_) => "labeled",
        Stmt::Break(_) => "break",
        Stmt::Continue(_) => "continue",
        Stmt::If(_) => "if",
        Stmt::Switch(_) => "switch",
        Stmt::Throw(_) => "throw",
        Stmt::Try(_) => "try",
        Stmt::While(_) => "while",
        Stmt::DoWhile(_) => "do-while",
        Stmt::For(_) => "for",
        Stmt::ForIn(_) => "for-in",
        Stmt::ForOf(_) => "for-of",
        Stmt::Decl(_) => "decl",
        Stmt::Expr(_) => "expr",
    }
}
