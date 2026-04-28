use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{InstBuilder, condcodes::IntCC, types as cl};
use swc_ecma_ast::{Expr, Lit};

use super::super::ctx::{FnCtx, ValTy};
use super::super::expressions::lower_expr;
use super::{lower_block, lower_stmt};

pub(super) fn lower_if_stmt(ctx: &mut FnCtx, if_stmt: &swc_ecma_ast::IfStmt) -> Result<bool> {
    // Branchless pattern: \`if (cond) { var = var <op> imm; }\` (sem else)
    // vira \`var = select(cond, new_val, var)\`. Elimina branch
    // imprevisivel em hot loops como Monte Carlo \`if (... <= 1.0)\`
    // onde branch predictor falha ~50% do tempo. Sem branch = sem
    // pipeline stall.
    if if_stmt.alt.is_none() {
        if let Some(()) = try_lower_if_to_select(ctx, if_stmt)? {
            return Ok(false);
        }
    } else {
        // Pattern \`if (cond) { return A; } else { return B; }\` vira
        // \`return select(cond, A, B);\`. Elimina branch + dois returns.
        if let Some(terminated) = try_lower_if_else_return_to_select(ctx, if_stmt)? {
            return Ok(terminated);
        }
    }

    let cond = lower_expr(ctx, &if_stmt.test)?;
    let is_true = ctx.to_branch_cond(cond);

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

/// Tenta lower \`if (cond) { return A; } else { return B; }\` como
/// \`return select(cond, A, B);\`. A e B devem ser pure (literals,
/// idents, binops, unarys puros). Elimina branch — return unico.
fn try_lower_if_else_return_to_select(
    ctx: &mut FnCtx,
    if_stmt: &swc_ecma_ast::IfStmt,
) -> Result<Option<bool>> {
    use swc_ecma_ast::Stmt;
    let Some(alt) = if_stmt.alt.as_ref() else { return Ok(None) };
    // Body de cada lado deve ser exatamente \`return <expr>\`.
    fn extract_return(stmt: &Stmt) -> Option<&swc_ecma_ast::Expr> {
        match stmt {
            Stmt::Return(r) => r.arg.as_deref(),
            Stmt::Block(b) if b.stmts.len() == 1 => {
                if let Stmt::Return(r) = &b.stmts[0] {
                    r.arg.as_deref()
                } else {
                    None
                }
            }
            _ => None,
        }
    }
    let Some(cons_expr) = extract_return(&if_stmt.cons) else { return Ok(None) };
    let Some(alt_expr) = extract_return(alt) else { return Ok(None) };
    if !is_pure_for_select(cons_expr) || !is_pure_for_select(alt_expr) {
        return Ok(None);
    }

    // Lower no fluxo atual sem branch.
    let cond = lower_expr(ctx, &if_stmt.test)?;
    let cond_val = ctx.to_branch_cond(cond);

    let cons_tv = lower_expr(ctx, cons_expr)?;
    let alt_tv = lower_expr(ctx, alt_expr)?;

    // Coerce ambos pro mesmo tipo (igual ao caminho de return normal).
    let ret_ty = ctx.return_ty.unwrap_or(crate::codegen::lower::ctx::ValTy::I64);
    let cons_v = match ret_ty {
        crate::codegen::lower::ctx::ValTy::I32 => ctx.coerce_to_i32(cons_tv).val,
        crate::codegen::lower::ctx::ValTy::F64 => ctx.coerce_to_f64(cons_tv).val,
        _ => ctx.coerce_to_i64(cons_tv).val,
    };
    let alt_v = match ret_ty {
        crate::codegen::lower::ctx::ValTy::I32 => ctx.coerce_to_i32(alt_tv).val,
        crate::codegen::lower::ctx::ValTy::F64 => ctx.coerce_to_f64(alt_tv).val,
        _ => ctx.coerce_to_i64(alt_tv).val,
    };

    let result = ctx.builder.ins().select(cond_val, cons_v, alt_v);
    ctx.builder.ins().return_(&[result]);
    // Sinaliza que o block esta terminated (caller nao precisa emitir
    // jump pro merge nem fallthrough).
    Ok(Some(true))
}

/// Tenta lower \`if (cond) { var = var <op> rhs; }\` (sem else) como
/// uma assinatura \`var = select(cond, new_val, var)\`. Retorna Some(())
/// quando o pattern bate e foi emitido; None quando o caller deve cair
/// no caminho normal de branch.
///
/// Restricoes:
/// - body e' exatamente 1 statement (ou Block com 1 stmt) — sem else;
/// - statement e' \`var = expr\` simples ou \`var <op>= expr\`;
/// - target e' Ident local (Variable I64/I32/F64). Globais nao
///   suportados (semantica de escrita atrasada via select complica).
fn try_lower_if_to_select(
    ctx: &mut FnCtx,
    if_stmt: &swc_ecma_ast::IfStmt,
) -> Result<Option<()>> {
    use swc_ecma_ast::{AssignOp, AssignTarget, Expr as E, SimpleAssignTarget, Stmt};

    // Extrai o single statement do body.
    let body_stmt: &Stmt = match &*if_stmt.cons {
        Stmt::Block(b) if b.stmts.len() == 1 => &b.stmts[0],
        s @ (Stmt::Expr(_)) => s,
        _ => return Ok(None),
    };
    let assign_expr = if let Stmt::Expr(e) = body_stmt {
        if let E::Assign(a) = e.expr.as_ref() {
            a
        } else {
            return Ok(None);
        }
    } else {
        return Ok(None);
    };

    // Target precisa ser Ident simples.
    let target_name = match &assign_expr.left {
        AssignTarget::Simple(SimpleAssignTarget::Ident(id)) => id.id.sym.as_str().to_string(),
        _ => return Ok(None),
    };

    // Var deve ser local (Variable Cranelift) — escrita via def_var.
    // Para globais o select nao pode ser direto (precisaria store
    // condicional).
    let local = match ctx.read_local_info(&target_name) {
        Some(l) => l,
        None => return Ok(None),
    };
    if local.is_const {
        return Ok(None);
    }

    // RHS limites: pra simplificar, aceita qualquer expr — o select
    // espera new_val/old_val do mesmo tipo, e a expressao deve nao
    // ter side effects observaveis quando descartada... mas em pratica
    // Cranelift mantem ambos lados execucao. Pra ser conservador,
    // limitamos a operacoes puras (literal, var, var <op> literal).
    if !is_pure_for_select(&assign_expr.right) {
        return Ok(None);
    }

    // Lower cond no fluxo atual (sem branch).
    let cond = lower_expr(ctx, &if_stmt.test)?;
    let cond_val = ctx.to_branch_cond(cond);

    // Compound assigns (\`+=\`, \`-=\`, etc) sao tratados como
    // \`var = var <op> rhs\`. Resultado da expressao + write de var
    // — old e' lido antes (pra select), op aplicado, select decide.
    let new_tv = if matches!(assign_expr.op, AssignOp::Assign) {
        lower_expr(ctx, &assign_expr.right)?
    } else {
        let bin_op = match assign_expr.op {
            AssignOp::AddAssign => swc_ecma_ast::BinaryOp::Add,
            AssignOp::SubAssign => swc_ecma_ast::BinaryOp::Sub,
            AssignOp::MulAssign => swc_ecma_ast::BinaryOp::Mul,
            AssignOp::DivAssign => swc_ecma_ast::BinaryOp::Div,
            AssignOp::ModAssign => swc_ecma_ast::BinaryOp::Mod,
            AssignOp::BitAndAssign => swc_ecma_ast::BinaryOp::BitAnd,
            AssignOp::BitOrAssign => swc_ecma_ast::BinaryOp::BitOr,
            AssignOp::BitXorAssign => swc_ecma_ast::BinaryOp::BitXor,
            AssignOp::LShiftAssign => swc_ecma_ast::BinaryOp::LShift,
            AssignOp::RShiftAssign => swc_ecma_ast::BinaryOp::RShift,
            AssignOp::ZeroFillRShiftAssign => swc_ecma_ast::BinaryOp::ZeroFillRShift,
            _ => return Ok(None),
        };
        // Sintetiza \`var <bin_op> rhs\`.
        let synthetic_bin = swc_ecma_ast::BinExpr {
            span: assign_expr.span,
            op: bin_op,
            left: Box::new(swc_ecma_ast::Expr::Ident(swc_ecma_ast::Ident {
                span: assign_expr.span,
                ctxt: Default::default(),
                sym: target_name.as_str().into(),
                optional: false,
            })),
            right: assign_expr.right.clone(),
        };
        lower_expr(ctx, &swc_ecma_ast::Expr::Bin(synthetic_bin))?
    };

    let new_val = match local.ty {
        crate::codegen::lower::ctx::ValTy::I32 => ctx.coerce_to_i32(new_tv).val,
        crate::codegen::lower::ctx::ValTy::F64 => ctx.coerce_to_f64(new_tv).val,
        _ => ctx.coerce_to_i64(new_tv).val,
    };
    let old_tv = ctx.read_local(&target_name).expect("var existe");
    let old_val = old_tv.val;
    let result = ctx.builder.ins().select(cond_val, new_val, old_val);
    ctx.write_local(&target_name, result)?;
    Ok(Some(()))
}

/// Conservador: define quais expressoes sao seguras pra serem lower
/// no fluxo principal (sem branch). Side effects (calls, ++) nao
/// devem rodar incondicionalmente quando o source so' espera execucao
/// no caminho true.
fn is_pure_for_select(e: &swc_ecma_ast::Expr) -> bool {
    use swc_ecma_ast::Expr;
    match e {
        Expr::Lit(_) | Expr::Ident(_) | Expr::This(_) => true,
        Expr::Bin(b) => is_pure_for_select(&b.left) && is_pure_for_select(&b.right),
        Expr::Unary(u) => {
            use swc_ecma_ast::UnaryOp;
            matches!(
                u.op,
                UnaryOp::Minus | UnaryOp::Plus | UnaryOp::Tilde | UnaryOp::Bang
            ) && is_pure_for_select(&u.arg)
        }
        Expr::Paren(p) => is_pure_for_select(&p.expr),
        _ => false,
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

/// Heuristica conservadora: percorre o try block e, se cada `throw`
/// encontrado for `throw new C(...)` com a mesma `C`, retorna `C`.
/// Multiplos throws de classes diferentes ou throws de expressoes nao-new
/// → None (catch param permanece sem classe estatica). Nao desce em
/// nested try/catch (o inner pega seus proprios throws). Nao desce em
/// fns aninhadas (closures/arrows criam contexto novo).
fn infer_throw_class(stmts: &[swc_ecma_ast::Stmt]) -> Option<String> {
    let mut found: Option<String> = None;
    for stmt in stmts {
        if let Some(name) = walk_for_throw_class(stmt) {
            match &found {
                None => found = Some(name),
                Some(prev) if prev == &name => {}
                Some(_) => return None, // mistura de classes
            }
        }
    }
    found
}

fn walk_for_throw_class(stmt: &swc_ecma_ast::Stmt) -> Option<String> {
    use swc_ecma_ast::Stmt;
    match stmt {
        Stmt::Throw(t) => extract_new_class_name(&t.arg),
        Stmt::Block(b) => {
            let mut acc: Option<String> = None;
            for s in &b.stmts {
                if let Some(c) = walk_for_throw_class(s) {
                    match &acc {
                        None => acc = Some(c),
                        Some(prev) if prev == &c => {}
                        Some(_) => return None,
                    }
                }
            }
            acc
        }
        Stmt::If(i) => {
            let a = walk_for_throw_class(&i.cons);
            let b = i.alt.as_ref().and_then(|alt| walk_for_throw_class(alt));
            match (a, b) {
                (Some(x), Some(y)) if x == y => Some(x),
                (Some(x), None) => Some(x),
                (None, Some(y)) => Some(y),
                _ => None,
            }
        }
        Stmt::For(f) => walk_for_throw_class(&f.body),
        Stmt::While(w) => walk_for_throw_class(&w.body),
        Stmt::DoWhile(d) => walk_for_throw_class(&d.body),
        Stmt::ForIn(f) => walk_for_throw_class(&f.body),
        Stmt::ForOf(f) => walk_for_throw_class(&f.body),
        Stmt::Try(_) => None, // nested try captura seus proprios throws
        _ => None,
    }
}

fn extract_new_class_name(expr: &Expr) -> Option<String> {
    if let Expr::New(n) = expr {
        if let Expr::Ident(id) = n.callee.as_ref() {
            return Some(id.sym.to_string());
        }
    }
    None
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
                // Anotacao `catch (e: ClassName)` propaga classe estatica
                // para que `e.field` use field_type_in_hierarchy + dispatch
                // virtual em vez de cair em map_get<i64>. Sem isso, leituras
                // de string/number do payload retornam handle/i64 cru. (#214)
                let mut class_for_catch: Option<String> = None;
                if let Some(ann) = id.type_ann.as_ref() {
                    if let Some(class_name) =
                        super::decls::class_name_from_annotation(&ann.type_ann)
                    {
                        if ctx.classes.contains_key(&class_name) {
                            class_for_catch = Some(class_name);
                        }
                    }
                }
                // Sem anotacao: heuristica simples — se todos os throw no
                // try block sao `throw new SameClass(...)` e SameClass eh
                // conhecida, usa essa classe. Multiplos throws de classes
                // diferentes (ou throw de expressao nao-new) nao infere.
                // Cobre o caso comum `try { throw new TypeError(...); } catch(e) { e.message }`.
                if class_for_catch.is_none() {
                    let inferred = infer_throw_class(&t.block.stmts);
                    if let Some(cls) = inferred {
                        if ctx.classes.contains_key(&cls) {
                            class_for_catch = Some(cls);
                        }
                    }
                }
                if let Some(cls) = class_for_catch {
                    ctx.local_class_ty.insert(name.to_string(), cls);
                }
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
