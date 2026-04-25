//! Statement lowering to Cranelift IR.
//!
//! `lower_stmt` handles: variable declarations, expression statements,
//! if/else, while, do-while, for, switch/case, return, break, continue.

use anyhow::{Result, anyhow};
use cranelift_codegen::ir::{InstBuilder, condcodes::IntCC, types as cl};
use swc_ecma_ast::{BlockStmt, Decl, Pat, Stmt, VarDeclKind, VarDeclOrExpr};

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

                // Trackeia tipo estatico de classe quando o bind tem
                // anotacao `: ClassName` cuja classe esta registrada.
                // Usado para dispatch de `obj.method(...)`.
                if let Pat::Ident(id) = &decl.name {
                    if let Some(ann) = id.type_ann.as_ref() {
                        if let Some(cn) = class_name_from_annotation(&ann.type_ann) {
                            if ctx.classes.contains_key(&cn) {
                                ctx.local_class_ty.insert(name.clone(), cn);
                            }
                        }
                        // Anotacao `C[]` → array de instancias de C
                        if let swc_ecma_ast::TsType::TsArrayType(arr) = ann.type_ann.as_ref() {
                            if let Some(cn) = class_name_from_annotation(&arr.elem_type) {
                                if ctx.classes.contains_key(&cn) {
                                    ctx.local_array_class_ty.insert(name.clone(), cn);
                                }
                            }
                        }
                    }
                }
                // Heuristica: quando o init e `new C(...)`, a var herda
                // a classe sem precisar de anotacao explicita.
                if !ctx.local_class_ty.contains_key(&name) {
                    if let Some(init) = decl.init.as_ref() {
                        if let swc_ecma_ast::Expr::New(ne) = init.as_ref() {
                            if let swc_ecma_ast::Expr::Ident(cid) = ne.callee.as_ref() {
                                let cn = cid.sym.as_str().to_string();
                                if ctx.classes.contains_key(&cn) {
                                    ctx.local_class_ty.insert(name.clone(), cn);
                                }
                            }
                        }
                        // Quando init e chamada `f(...)` cujo return_type
                        // e classe registrada, herda essa classe.
                        if let swc_ecma_ast::Expr::Call(call) = init.as_ref() {
                            if let swc_ecma_ast::Callee::Expr(cb) = &call.callee {
                                if let swc_ecma_ast::Expr::Ident(fid) = cb.as_ref() {
                                    if let Some(cn) = ctx.fn_class_returns.get(fid.sym.as_str()) {
                                        ctx.local_class_ty.insert(name.clone(), cn.clone());
                                    }
                                }
                            }
                        }
                    }
                }

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
                    let is_const = matches!(var_decl.kind, VarDeclKind::Const);
                    let function_scope = matches!(var_decl.kind, VarDeclKind::Var);
                    ctx.declare_local_kind(&name, ty, init_coerced, is_const, function_scope);
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

        // ── For...of ──────────────────────────────────────────────────────
        // MVP: trata o iteravel como vec handle (arrays sao Vec<i64> via
        // collections.vec_*). Desugara em loop indexado:
        //   const __h = <iter>;
        //   const __len = vec_len(__h);
        //   for (let __i = 0; __i < __len; __i++) {
        //     <bind> = vec_get(__h, __i);
        //     <body>
        //   }
        Stmt::ForOf(for_of) => return lower_for_of(ctx, for_of),

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

            // Fast path: every non-default case tests against an integer
            // literal. Emit a Cranelift `Switch` table — the backend picks
            // between `br_table` (dense) and binary search (sparse).
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
                // Mark the expression as being in tail position so
                // lower_user_call can emit `return_call` when the callee
                // is a user function. The flag is cleared after lowering
                // whether or not the call was tail-eligible.
                let prev = ctx.in_tail_position;
                ctx.in_tail_position = true;
                let tv = lower_expr(ctx, arg)?;
                ctx.in_tail_position = prev;

                let coerced = match ctx.return_ty {
                    Some(ValTy::I32) => ctx.coerce_to_i32(tv),
                    Some(ValTy::F64) => ctx.coerce_to_f64(tv),
                    Some(ValTy::Handle) => ctx.coerce_to_handle(tv)?,
                    // Default and I64/Bool lanes share i64 storage.
                    _ => ctx.coerce_to_i64(tv),
                };
                // Emit the return terminator. If `lower_expr` tail-called,
                // the current block is an unreachable placeholder — the
                // `return_` here still acts as a valid terminator for it.
                ctx.builder.ins().return_(&[coerced.val]);
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

        // ── Throw ─────────────────────────────────────────────────────────
        // Fase 1 de #62: seta o slot global thread-local com o handle
        // da string de erro e **segue o fluxo normal**. O caller (seja
        // try/catch ou a funcao raiz) eh responsavel por checar o slot
        // apos call sites sensiveis. Semantica imperfeita mas pragmatica
        // — codigo apos `throw` no mesmo bloco ainda executa. Fase 2
        // integra unwind real via Cranelift invoke.
        Stmt::Throw(throw_stmt) => {
            let tv = lower_expr(ctx, &throw_stmt.arg)?;
            let handle = ctx.coerce_to_handle(tv)?;
            let set_fref = ctx.get_extern("__RTS_FN_RT_ERROR_SET", &[cl::I64], None)?;
            ctx.builder.ins().call(set_fref, &[handle.val]);
            // Retorna false — fluxo continua normalmente. A diferenca
            // de comportamento fica com o try/catch que enxerga o
            // slot apos o body.
            Ok(false)
        }

        // ── Try / catch / finally ─────────────────────────────────────────
        // Fase 1: zera slot antes do body, roda body; apos o body,
        // checa slot != 0 → pula para catch com binding = handle
        // corrente; zera slot no inicio do catch. Finally roda
        // em qualquer caminho (normal e apos catch).
        Stmt::Try(try_stmt) => lower_try(ctx, try_stmt),

        other => Err(anyhow!("unsupported statement: {}", stmt_kind_name(other))),
    }
}

pub fn lower_block(ctx: &mut FnCtx, block: &BlockStmt) -> Result<bool> {
    ctx.push_scope();
    let mut exited = false;
    let mut err = None;
    for s in &block.stmts {
        match lower_stmt(ctx, s) {
            Ok(true) => {
                exited = true;
                break;
            }
            Ok(false) => {}
            Err(e) => {
                err = Some(e);
                break;
            }
        }
    }
    ctx.pop_scope();
    if let Some(e) = err {
        return Err(e);
    }
    Ok(exited)
}

// ── for...of ──────────────────────────────────────────────────────────────

/// Lowers `for (let x of arr) { body }` no MVP de #60.
///
/// Trata o iteravel como handle de `collections.vec_*`: usa `vec_len`
/// para o limite e `vec_get` por iteracao. Sem suporte a iteradores
/// arbitrarios (Symbol.iterator), strings ou Map.
fn lower_for_of(ctx: &mut FnCtx, for_of: &swc_ecma_ast::ForOfStmt) -> Result<bool> {
    use swc_ecma_ast::ForHead;

    if for_of.is_await {
        return Err(anyhow!("for-await-of nao suportado"));
    }

    // Bind do elemento: aceita `for (let|const|var x of ...)` ou
    // `for (x of ...)` quando x ja e local. Le anotacao de tipo
    // (`for (const x: string of arr)`) para decidir se o resultado de
    // vec_get sera tratado como Handle (string) ou I64 (number/handle
    // de objeto crus).
    let (bind_name, bind_ty): (String, ValTy) = match &for_of.left {
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

    // Avalia o iteravel uma vez: handle u64.
    let iter_tv = lower_expr(ctx, &for_of.right)?;
    let handle = ctx.coerce_to_i64(iter_tv).val;

    // Comprimento via vec_len.
    let len_fref = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_VEC_LEN",
        &[cl::I64],
        Some(cl::I64),
    )?;
    let inst = ctx.builder.ins().call(len_fref, &[handle]);
    let len = ctx.builder.inst_results(inst)[0];

    // Helpers para read/write do elemento atual via vec_get.
    let get_fref = ctx.get_extern(
        "__RTS_FN_NS_COLLECTIONS_VEC_GET",
        &[cl::I64, cl::I64],
        Some(cl::I64),
    )?;

    // Declara `bind` no tipo escolhido. Para ForHead::Pat, o ident
    // precisa ja estar declarado no escopo atual (sem decl no MVP).
    let zero = ctx.builder.ins().iconst(cl::I64, 0);
    if matches!(&for_of.left, ForHead::VarDecl(_)) {
        ctx.declare_local(&bind_name, bind_ty, zero);
    }

    // Inferencia de classe: se o iter e local com array tipado
    // `: C[]`, popula local_class_ty para o bind, habilitando
    // dispatch de metodo no body do for-of.
    if let swc_ecma_ast::Expr::Ident(id) = for_of.right.as_ref() {
        let arr_name = id.sym.as_str();
        if let Some(elem_cls) = ctx.local_array_class_ty.get(arr_name).cloned() {
            ctx.local_class_ty.insert(bind_name.clone(), elem_cls);
        }
    }

    // Counter local em i64 (handle/index sao i64). Nome unico via
    // ponteiro do iter span para evitar colisoes em loops aninhados.
    let counter_name: String = format!("__rts_for_of_i_{:p}", &for_of.span);
    ctx.declare_local(&counter_name, ValTy::I64, zero);

    let header = ctx.builder.create_block();
    let body = ctx.builder.create_block();
    let update_block = ctx.builder.create_block();
    let exit = ctx.builder.create_block();

    ctx.builder.ins().jump(header, &[]);
    ctx.builder.switch_to_block(header);

    // i < len ?
    let i_now = ctx
        .read_local(&counter_name)
        .ok_or_else(|| anyhow!("for-of counter sumiu"))?;
    let is_in_range = ctx.builder.ins().icmp(IntCC::SignedLessThan, i_now.val, len);
    ctx.builder.ins().brif(is_in_range, body, &[], exit, &[]);

    ctx.builder.switch_to_block(body);
    // bind = vec_get(handle, i)
    let i_now = ctx
        .read_local(&counter_name)
        .ok_or_else(|| anyhow!("for-of counter sumiu"))?;
    let inst = ctx.builder.ins().call(get_fref, &[handle, i_now.val]);
    let elem = ctx.builder.inst_results(inst)[0];
    ctx.write_local(&bind_name, elem)?;

    ctx.loop_stack.push((exit, update_block));
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

/// Extrai `ClassName` de uma anotacao `: ClassName`. Retorna None
/// quando a anotacao nao e um simple ident.
fn class_name_from_annotation(ty: &swc_ecma_ast::TsType) -> Option<String> {
    use swc_ecma_ast::TsType;
    if let TsType::TsTypeRef(r) = ty {
        if let swc_ecma_ast::TsEntityName::Ident(id) = &r.type_name {
            return Some(id.sym.as_str().to_string());
        }
    }
    None
}

/// Lowers `try { ... } catch (e) { ... } finally { ... }`.
///
/// Estrategia simples (fase 1 de #62): zera o slot de erro antes do
/// body, roda o body, checa o slot depois. Sem unwind real —
/// captura so funciona para `throw` cujo caminho de execucao
/// retorna ao fim do body `try` com o slot setado (ex: throw
/// direto no try, ou throw dentro de funcao chamada do try que
/// retornou).
fn lower_try(ctx: &mut FnCtx, t: &swc_ecma_ast::TryStmt) -> Result<bool> {
    use cranelift_codegen::ir::condcodes::IntCC;

    let has_catch = t.handler.is_some();
    let has_finally = t.finalizer.is_some();

    // Clear do slot antes do body.
    let clear_fref = ctx.get_extern("__RTS_FN_RT_ERROR_CLEAR", &[], None)?;
    ctx.builder.ins().call(clear_fref, &[]);

    // Compila body. Depois dele, verifica slot.
    lower_block(ctx, &t.block)?;

    // Blocos de orchestration.
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

    // Se o body ja terminou (return/throw), nao emite check — mas
    // se o body caiu pro fluxo normal, emite check do slot.
    if !ctx.builder.is_unreachable() {
        let get_fref =
            ctx.get_extern("__RTS_FN_RT_ERROR_GET", &[], Some(cl::I64))?;
        let inst = ctx.builder.ins().call(get_fref, &[]);
        let err_handle = ctx.builder.inst_results(inst)[0];
        let zero = ctx.builder.ins().iconst(cl::I64, 0);
        let is_err = ctx
            .builder
            .ins()
            .icmp(IntCC::NotEqual, err_handle, zero);

        // Branch: se erro, catch (se existir); senao finally/after.
        let ok_target = finally_block.unwrap_or(after_block);
        let err_target = catch_block.unwrap_or(ok_target);
        ctx.builder
            .ins()
            .brif(is_err, err_target, &[], ok_target, &[]);
    }

    // Emite catch block.
    if let Some(cb) = catch_block {
        ctx.builder.switch_to_block(cb);
        ctx.builder.seal_block(cb);

        // Binding: le slot, atribui a variavel (se houver param), limpa.
        let handler = t.handler.as_ref().unwrap();
        if let Some(param) = &handler.param {
            if let swc_ecma_ast::Pat::Ident(id) = param {
                let name = id.id.sym.as_str();
                let get_fref =
                    ctx.get_extern("__RTS_FN_RT_ERROR_GET", &[], Some(cl::I64))?;
                let inst = ctx.builder.ins().call(get_fref, &[]);
                let err_handle = ctx.builder.inst_results(inst)[0];
                ctx.declare_local(name, ValTy::Handle, err_handle);
            }
        }
        let clear_fref =
            ctx.get_extern("__RTS_FN_RT_ERROR_CLEAR", &[], None)?;
        ctx.builder.ins().call(clear_fref, &[]);

        lower_block(ctx, &handler.body)?;
        if !ctx.builder.is_unreachable() {
            let next = finally_block.unwrap_or(after_block);
            ctx.builder.ins().jump(next, &[]);
        }
    }

    // Emite finally block.
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

/// Extracts an integer case label for jump-table switch lowering.
/// Accepts numeric literals with no decimal point, optionally with `+` or
/// unary minus. Returns None for any other shape (string, computed, float).
fn extract_integer_literal(expr: &swc_ecma_ast::Expr) -> Option<u128> {
    use swc_ecma_ast::{Expr, Lit, UnaryOp};
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
        Expr::Unary(u) if matches!(u.op, UnaryOp::Minus) => {
            let inner = extract_integer_literal(&u.arg)?;
            // Negate within i64 range via wrapping.
            let as_i64 = inner as i64;
            Some(as_i64.wrapping_neg() as u128)
        }
        Expr::Unary(u) if matches!(u.op, UnaryOp::Plus) => extract_integer_literal(&u.arg),
        Expr::Paren(p) => extract_integer_literal(&p.expr),
        _ => None,
    }
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
