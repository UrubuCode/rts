//! Statement lowering to Cranelift IR.

mod control;
mod decls;
mod loops;

use anyhow::{Result, anyhow};
use swc_ecma_ast::{BlockStmt, Decl, Stmt};

use super::ctx::FnCtx;
use super::expressions::lower_expr;

pub fn lower_stmt(ctx: &mut FnCtx, stmt: &Stmt) -> Result<bool> {
    match stmt {
        Stmt::Decl(Decl::Var(var_decl)) => decls::lower_var_decl(ctx, var_decl),
        Stmt::Expr(expr_stmt) => {
            lower_expr(ctx, &expr_stmt.expr)?;
            Ok(false)
        }
        Stmt::Block(block) => lower_block(ctx, block),
        Stmt::If(if_stmt) => control::lower_if_stmt(ctx, if_stmt),
        Stmt::While(wh) => loops::lower_while_stmt(ctx, wh),
        Stmt::DoWhile(dw) => loops::lower_do_while_stmt(ctx, dw),
        Stmt::For(for_stmt) => loops::lower_for_stmt(ctx, for_stmt),
        Stmt::ForOf(for_of) => loops::lower_for_of(ctx, for_of),
        Stmt::ForIn(for_in) => loops::lower_for_in(ctx, for_in),
        Stmt::Switch(sw) => control::lower_switch_stmt(ctx, sw),
        Stmt::Return(ret_stmt) => control::lower_return_stmt(ctx, ret_stmt),
        Stmt::Break(b) => control::lower_break_stmt(ctx, b),
        Stmt::Continue(c) => control::lower_continue_stmt(ctx, c),
        Stmt::Empty(_) => Ok(false),
        Stmt::Labeled(lbl) => control::lower_labeled_stmt(ctx, lbl),
        Stmt::Throw(throw_stmt) => control::lower_throw_stmt(ctx, throw_stmt),
        Stmt::Try(try_stmt) => control::lower_try_stmt(ctx, try_stmt),
        other => Err(anyhow!("unsupported statement: {}", stmt_kind_name(other))),
    }
}

pub fn lower_block(ctx: &mut FnCtx, block: &BlockStmt) -> Result<bool> {
    ctx.push_scope();
    let mut exited = false;
    let mut err = None;
    let mut iter = block.stmts.iter();
    while let Some(s) = iter.next() {
        match lower_stmt(ctx, s) {
            Ok(true) => {
                exited = true;
                // #205 — warn sobre o primeiro stmt nao-trivial apos
                // um terminal (return/throw/break/continue). Empty/Decl
                // stmts puros (var hoisting) nao contam — a idiomatica
                // de declarar var no fim do escopo apos return early
                // ainda eh comum, e o codigo morto real eh statement
                // executavel.
                if let Some(next) = iter.next() {
                    if !is_trivially_empty(next) {
                        ctx.warnings.push(format!(
                            "warning: unreachable code after `{}`",
                            terminal_kind(s)
                        ));
                    }
                }
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

fn is_trivially_empty(stmt: &Stmt) -> bool {
    matches!(stmt, Stmt::Empty(_))
}

fn terminal_kind(stmt: &Stmt) -> &'static str {
    match stmt {
        Stmt::Return(_) => "return",
        Stmt::Throw(_) => "throw",
        Stmt::Break(_) => "break",
        Stmt::Continue(_) => "continue",
        _ => "terminal statement",
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
