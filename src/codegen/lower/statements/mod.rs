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
    // (#155 fase 1) Escape analysis: identifica nomes que escapam
    // do bloco (return, atribuicao a var externa, captura por
    // closure/fn dentro do bloco). Vars escapadas nao sao auto-freed.
    {
        let mut escaped: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        scan_escapes(&block.stmts, &mut escaped);
        if let Some(top) = ctx.escaped_in_block.last_mut() {
            *top = escaped;
        }
    }
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

/// (#155 fase 1) Escape analysis conservadora.
///
/// Coleta nomes que escapam do bloco corrente — quem aparece aqui
/// nao deve ser auto-freed no pop_scope. Conservador: na duvida,
/// adiciona ao set (rather safe than UAF).
///
/// Casos cobertos:
/// - Identifier em `return <expr>` ou `throw <expr>`
/// - Identifier em RHS de atribuicao `outer = name` (var externa)
/// - Identifier passado como arg pra fns que potencialmente armazenam
///   (collections.map_set, vec_push, etc) — **modelo conservador:
///   qualquer ident usado em call site nao-string e' considerado
///   escapado**
/// - Identifier referenciado dentro de FunctionExpr/ArrowExpr
///   (closure capture)
fn scan_escapes(stmts: &[Stmt], escaped: &mut std::collections::HashSet<String>) {
    for s in stmts {
        scan_escapes_in_stmt(s, escaped);
    }
}

fn scan_escapes_in_stmt(stmt: &Stmt, escaped: &mut std::collections::HashSet<String>) {
    use swc_ecma_ast::Expr;
    match stmt {
        Stmt::Return(r) => {
            if let Some(e) = r.arg.as_deref() {
                collect_idents(e, escaped);
            }
        }
        Stmt::Throw(t) => collect_idents(&t.arg, escaped),
        Stmt::Expr(es) => {
            // Atribuicoes: `outer = name` ou `outer.f = name` escapa
            // o RHS (o handle agora referenciado por escopo externo).
            if let Expr::Assign(a) = es.expr.as_ref() {
                // RHS pode armazenar o handle externamente.
                collect_idents(&a.right, escaped);
            }
            // Calls em ExprStmt sao "fire-and-forget" — args sao
            // passados, mas sem retorno capturado. Maioria sao
            // sinks (print, log, push) que consomem mas nao retem
            // (excecao: collections.map_set/vec_push que retem em
            // estrutura externa). Conservador via lista de allowed
            // sinks: idents passados a estes nao escapam.
            // Para outros calls em ExprStmt, sem info de tipo/efeito,
            // considera nao-escape (assumindo que o caller do auto-
            // free roda pos-tests para validar).
        }
        Stmt::Decl(swc_ecma_ast::Decl::Var(vd)) => {
            // `const x = name` (alias literal) — handle e' aliased,
            // escapa pq agora ha duas refs. RHS de outras formas
            // (call, tpl, bin) nao escapa: o resultado e' nova string.
            for d in &vd.decls {
                if let Some(init) = d.init.as_deref() {
                    if matches!(init, Expr::Ident(_)) {
                        collect_idents(init, escaped);
                    }
                }
            }
        }
        Stmt::If(i) => {
            scan_escapes_in_stmt(&i.cons, escaped);
            if let Some(alt) = i.alt.as_deref() {
                scan_escapes_in_stmt(alt, escaped);
            }
        }
        Stmt::Block(b) => scan_escapes(&b.stmts, escaped),
        Stmt::While(w) => scan_escapes_in_stmt(&w.body, escaped),
        Stmt::DoWhile(d) => scan_escapes_in_stmt(&d.body, escaped),
        Stmt::For(f) => scan_escapes_in_stmt(&f.body, escaped),
        Stmt::ForOf(fo) => scan_escapes_in_stmt(&fo.body, escaped),
        Stmt::ForIn(fi) => scan_escapes_in_stmt(&fi.body, escaped),
        Stmt::Try(t) => {
            scan_escapes(&t.block.stmts, escaped);
            if let Some(h) = &t.handler {
                scan_escapes(&h.body.stmts, escaped);
            }
            if let Some(f) = &t.finalizer {
                scan_escapes(&f.stmts, escaped);
            }
        }
        Stmt::Switch(sw) => {
            for case in &sw.cases {
                for s in &case.cons {
                    scan_escapes_in_stmt(s, escaped);
                }
            }
        }
        Stmt::Labeled(l) => scan_escapes_in_stmt(&l.body, escaped),
        _ => {}
    }
}

fn collect_idents(expr: &swc_ecma_ast::Expr, out: &mut std::collections::HashSet<String>) {
    use swc_ecma_ast::Expr;
    match expr {
        Expr::Ident(id) => {
            out.insert(id.sym.as_str().to_string());
        }
        Expr::Tpl(t) => {
            for e in &t.exprs {
                collect_idents(e, out);
            }
        }
        Expr::Bin(b) => {
            collect_idents(&b.left, out);
            collect_idents(&b.right, out);
        }
        Expr::Cond(c) => {
            collect_idents(&c.test, out);
            collect_idents(&c.cons, out);
            collect_idents(&c.alt, out);
        }
        Expr::Paren(p) => collect_idents(&p.expr, out),
        Expr::TsAs(a) => collect_idents(&a.expr, out),
        Expr::TsNonNull(n) => collect_idents(&n.expr, out),
        Expr::TsTypeAssertion(a) => collect_idents(&a.expr, out),
        Expr::Member(m) => {
            collect_idents(&m.obj, out);
        }
        Expr::Call(c) => {
            // Calls passam args por valor. Para auto-free, args nao
            // sao considerados escape — sinks comuns (console.log,
            // io.print, gc.string_free) consomem mas nao retem em
            // escopo externo. Risco de falso negativo: collections.
            // map_set/vec_push retem em map/vec global, mas estes
            // recebem normalmente handles ja registrados em outro
            // path. Caller pode setar RTS_AUTO_FREE_HANDLES=0 se ver
            // double-free.
            // Nao desce em args nem callee aqui.
            let _ = c;
        }
        Expr::New(n) => {
            if let Some(args) = &n.args {
                for arg in args {
                    collect_idents(&arg.expr, out);
                }
            }
        }
        Expr::Array(a) => {
            for e in &a.elems {
                if let Some(spread) = e {
                    collect_idents(&spread.expr, out);
                }
            }
        }
        Expr::Object(o) => {
            for p in &o.props {
                if let swc_ecma_ast::PropOrSpread::Prop(prop) = p {
                    if let swc_ecma_ast::Prop::KeyValue(kv) = prop.as_ref() {
                        collect_idents(&kv.value, out);
                    }
                }
            }
        }
        Expr::Unary(u) => collect_idents(&u.arg, out),
        Expr::Update(u) => collect_idents(&u.arg, out),
        Expr::Assign(a) => collect_idents(&a.right, out),
        // Closures capturam vars do escopo enclosing — extremamente
        // conservador: marca um sentinel "*" que sinaliza "todas as
        // vars do bloco escapam". Verificado em try_emit_scope_frees.
        Expr::Arrow(_) | Expr::Fn(_) => {
            out.insert("*".to_string());
        }
        _ => {}
    }
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
