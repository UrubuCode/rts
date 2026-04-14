use super::*;

pub(super) fn alloc(next_vreg: &mut u32) -> VReg {
    let v = VReg(*next_vreg);
    *next_vreg += 1;
    v
}

pub(super) fn lower_stmt_with_pool(
    stmt: &Stmt,
    original_text: &str,
    instructions: &mut Vec<MirInstruction>,
    next_vreg: &mut u32,
    constant_pool: &mut ConstantPool,
) {
    match stmt {
        Stmt::Decl(Decl::Var(var_decl)) => {
            let mutable = var_decl.kind != VarDeclKind::Const;
            for decl in &var_decl.decls {
                let name = match &decl.name {
                    Pat::Ident(ident) => ident.id.sym.to_string(),
                    _ => {
                        let vreg = alloc(next_vreg);
                        instructions
                            .push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
                        continue;
                    }
                };
                if let Some(init) = &decl.init {
                    let vreg = lower_expr_with_pool(
                        init,
                        original_text,
                        instructions,
                        next_vreg,
                        constant_pool,
                    );
                    instructions.push(MirInstruction::Bind(name, vreg, mutable));
                } else {
                    let vreg = constant_pool.get_or_create_undef(next_vreg);
                    instructions.push(MirInstruction::Bind(name, vreg, mutable));
                }
            }
        }
        Stmt::Return(ret_stmt) => {
            if let Some(arg) = &ret_stmt.arg {
                let vreg = lower_expr_with_pool(
                    arg,
                    original_text,
                    instructions,
                    next_vreg,
                    constant_pool,
                );
                instructions.push(MirInstruction::Return(Some(vreg)));
            } else {
                instructions.push(MirInstruction::Return(None));
            }
        }
        Stmt::Expr(expr_stmt) => {
            let _vreg = lower_expr_with_pool(
                &expr_stmt.expr,
                original_text,
                instructions,
                next_vreg,
                constant_pool,
            );
        }
        Stmt::Block(block_stmt) => {
            for inner_stmt in &block_stmt.stmts {
                lower_stmt_with_pool(
                    inner_stmt,
                    original_text,
                    instructions,
                    next_vreg,
                    constant_pool,
                );
            }
        }
        Stmt::If(if_stmt) => {
            // For now, use the original version without pool for control flow
            lower_if_stmt(if_stmt, original_text, instructions, next_vreg);
        }
        Stmt::While(while_stmt) => {
            lower_while_stmt(while_stmt, original_text, instructions, next_vreg);
        }
        Stmt::DoWhile(do_while_stmt) => {
            lower_do_while_stmt(do_while_stmt, original_text, instructions, next_vreg);
        }
        Stmt::For(for_stmt) => {
            lower_for_stmt(for_stmt, original_text, instructions, next_vreg);
        }
        Stmt::Switch(switch_stmt) => {
            lower_switch_stmt(switch_stmt, original_text, instructions, next_vreg);
        }
        Stmt::Break(_) => {
            instructions.push(MirInstruction::Break);
        }
        Stmt::Continue(_) => {
            instructions.push(MirInstruction::Continue);
        }
        _ => {
            let vreg = alloc(next_vreg);
            instructions.push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
        }
    }
}

pub(super) fn lower_stmt(
    stmt: &Stmt,
    original_text: &str,
    instructions: &mut Vec<MirInstruction>,
    next_vreg: &mut u32,
) {
    match stmt {
        Stmt::Decl(Decl::Var(var_decl)) => {
            let mutable = var_decl.kind != VarDeclKind::Const;
            for decl in &var_decl.decls {
                let name = match &decl.name {
                    Pat::Ident(ident) => ident.id.sym.to_string(),
                    _ => {
                        let vreg = alloc(next_vreg);
                        instructions
                            .push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
                        continue;
                    }
                };
                if let Some(init) = &decl.init {
                    let vreg = lower_expr(init, original_text, instructions, next_vreg);
                    instructions.push(MirInstruction::Bind(name, vreg, mutable));
                } else {
                    let vreg = alloc(next_vreg);
                    instructions.push(MirInstruction::ConstUndef(vreg));
                    instructions.push(MirInstruction::Bind(name, vreg, mutable));
                }
            }
        }
        Stmt::Return(ret_stmt) => {
            if let Some(arg) = &ret_stmt.arg {
                let vreg = lower_expr(arg, original_text, instructions, next_vreg);
                instructions.push(MirInstruction::Return(Some(vreg)));
            } else {
                instructions.push(MirInstruction::Return(None));
            }
        }
        Stmt::Expr(expr_stmt) => {
            let _vreg = lower_expr(&expr_stmt.expr, original_text, instructions, next_vreg);
        }
        Stmt::Block(block_stmt) => {
            for inner_stmt in &block_stmt.stmts {
                lower_stmt(inner_stmt, original_text, instructions, next_vreg);
            }
        }
        Stmt::If(if_stmt) => {
            lower_if_stmt(if_stmt, original_text, instructions, next_vreg);
        }
        Stmt::While(while_stmt) => {
            lower_while_stmt(while_stmt, original_text, instructions, next_vreg);
        }
        Stmt::DoWhile(do_while_stmt) => {
            lower_do_while_stmt(do_while_stmt, original_text, instructions, next_vreg);
        }
        Stmt::For(for_stmt) => {
            lower_for_stmt(for_stmt, original_text, instructions, next_vreg);
        }
        Stmt::Switch(switch_stmt) => {
            lower_switch_stmt(switch_stmt, original_text, instructions, next_vreg);
        }
        Stmt::Break(_) => {
            instructions.push(MirInstruction::Break);
        }
        Stmt::Continue(_) => {
            instructions.push(MirInstruction::Continue);
        }
        _ => {
            let vreg = alloc(next_vreg);
            instructions.push(MirInstruction::RuntimeEval(vreg, original_text.to_string()));
        }
    }
}
