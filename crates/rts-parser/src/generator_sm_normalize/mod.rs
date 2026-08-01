//! `yield` em posição de SUBEXPRESSÃO → statements que a state-machine já sabe.
//!
//! ## O problema
//!
//! A máquina modela `yield` em exatamente duas posições: statement isolado
//! (`yield E;`) e valor de uma ligação simples (`const a = yield E;` /
//! `a = yield E;`). Qualquer outra posição fazia `lower_stmt` devolver `None`,
//! o build caía no eager-buffer — que só sabe expressar `yield` como
//! `__gen_buf.push` — e o `yield` de valor sobrevivia até o lowering:
//!
//! ```text
//! function* g(t, n) { return t && (yield n); }
//! // → "expression raw/unrecognized: Yield(...)"
//! ```
//!
//! Essa é a saída padrão do Babel para `return await`-like patterns, e era a
//! causa MEDIDA do cluster restante numa carga real (7 ocorrências na página,
//! presentes em todos os bundles grandes). Seis formas caíam aqui: `return
//! yield x`, `a && (yield b)`, `a || (yield b)`, `a ? (yield b) : c`,
//! `f(yield x)` e `1 + (yield x)`.
//!
//! ## A abordagem: normalizar ANTES, não fatiar expressão DEPOIS
//!
//! Em vez de ensinar a máquina a suspender no meio da avaliação de uma
//! expressão (que exigiria fatiar expressões em estados, muito mais superfície
//! e muito mais formas de errar em silêncio), este módulo reescreve o corpo
//! ANTES do `SmBuilder` ver qualquer coisa, e sua saída usa SOMENTE as duas
//! formas que a máquina já modela. A máquina não ganha um caso novo sequer, e
//! nenhum gate foi afrouxado.
//!
//! ## Curto-circuito é o ponto onde isto se erra
//!
//! Em `t && (yield n)`, se `t` for falsy o `yield` NÃO executa e o generator
//! NÃO suspende. Um lowering que avaliasse o yield antes do teste mudaria o
//! comportamento observável. Por isso os operadores curto-circuitantes viram
//! ramos de verdade, com o yield DENTRO do ramo:
//!
//! ```text
//! A && (yield B)    →   let t = A;  if (t)  { t = yield B; }             → t
//! A || (yield B)    →   let t = A;  if (!t) { t = yield B; }             → t
//! A ?? (yield B)    →   let t = A;  if (t === null || t === undefined) { t = yield B; } → t
//! A ? (yield B) : C →   let t;      if (A) { t = yield B; } else { t = C; }              → t
//! ```
//!
//! ## Ordem de avaliação é o outro
//!
//! Içar só o `yield` estaria errado: em `f() + (yield a)`, deixar `f()` na
//! expressão residual o faria rodar DEPOIS da suspensão. Então, quando algo à
//! direita contém `yield`, o que vem antes vai para temporários NA ORDEM:
//!
//! ```text
//! f() + (yield a)   →   let t1 = f();  let t2 = yield a;   … t1 + t2
//! ```
//!
//! Um `this` de método também não sobrevive a um temporário: `obj.m(yield x)`
//! não pode virar `let t = obj.m; t(…)`. O RECEPTOR é que vai para o
//! temporário (`let r = obj; r.m(…)`), preservando a ligação.
//!
//! ## O que recusa (e por que recusar é seguro)
//!
//! Uma subárvore SEM `yield` não é tocada — nada de temporário, nada de
//! reescrita — então toda forma que já funcionava segue byte-a-byte igual. E
//! uma posição que este módulo não sabe içar devolve `None`, o que aborta o
//! build e mantém o comportamento de HOJE (eager-buffer), nunca um palpite:
//!
//! * `yield` no teste de um laço (`while (yield x)`) ou num cabeçalho de `for`
//!   — içar para fora do laço mudaria quantas vezes ele avalia;
//! * `yield` no discriminante de um `switch` ou num `case` — mesma razão;
//! * `yield*` (delegate) em posição de valor — a máquina não modela o valor de
//!   retorno de uma delegação aqui;
//! * `yield` dentro de um alvo de atribuição, de um `?.`, ou de um `await`.

mod expr;

use swc_ecma_ast::{AssignTarget, Decl, Expr, Pat, SimpleAssignTarget, Stmt};

use crate::generator_sm::{call_stmt, expr_has_yield, let_decl, sp, undef_expr};

use expr::norm_expr;

/// Reescreve `stmts` para que todo `yield` esteja numa posição que a
/// state-machine modela. `None` quando alguma posição não é içável — o caller
/// aborta o build e cai no eager-buffer, como antes.
pub(crate) fn normalize_body(stmts: &[Stmt]) -> Option<Vec<Stmt>> {
    let mut ctr = 0usize;
    let mut out = Vec::with_capacity(stmts.len());
    for s in stmts {
        norm_stmt(s, &mut out, &mut ctr)?;
    }
    Some(out)
}

/// Nome do próximo temporário. O prefixo é sintético e não colide com nome de
/// usuário; cada um vira um slot de frame comum (a máquina interna qualquer
/// `let`), então sobrevive à suspensão sem tratamento especial.
pub(super) fn temp(ctr: &mut usize) -> String {
    let n = format!("__yx_{ctr}");
    *ctr += 1;
    n
}

// ── statements ───────────────────────────────────────────────────────────────

fn norm_stmt(s: &Stmt, out: &mut Vec<Stmt>, ctr: &mut usize) -> Option<()> {
    match s {
        // `yield E;` / `a = yield E;` já são as formas da máquina — só o ARGUMENTO
        // do yield pode precisar de normalização.
        Stmt::Expr(es) => {
            if let Expr::Yield(y) = es.expr.as_ref() {
                let arg = match y.arg.as_deref() {
                    Some(a) if expr_has_yield(a) => Some(Box::new(norm_expr(a, out, ctr)?)),
                    other => other.map(|a| Box::new(a.clone())),
                };
                let mut y2 = y.clone();
                y2.arg = arg;
                out.push(call_stmt(Expr::Yield(y2)));
                return Some(());
            }
            if let Expr::Assign(a) = es.expr.as_ref() {
                if let (AssignTarget::Simple(SimpleAssignTarget::Ident(_)), Expr::Yield(y)) =
                    (&a.left, a.right.as_ref())
                {
                    if !y.delegate && !y.arg.as_deref().is_some_and(expr_has_yield) {
                        out.push(s.clone());
                        return Some(());
                    }
                }
            }
            if !expr_has_yield(&es.expr) {
                out.push(s.clone());
                return Some(());
            }
            let e = norm_expr(&es.expr, out, ctr)?;
            out.push(call_stmt(e));
            Some(())
        }

        Stmt::Decl(Decl::Var(vd)) => {
            for d in &vd.decls {
                let Pat::Ident(id) = &d.name else {
                    // Destructuring: a máquina já recusa; deixa passar intacto
                    // para ela recusar (ou aceitar) exatamente como hoje.
                    if d.init.as_deref().is_some_and(expr_has_yield) {
                        return None;
                    }
                    out.push(Stmt::Decl(Decl::Var(vd.clone())));
                    return Some(());
                };
                let name = id.id.sym.to_string();
                match d.init.as_deref() {
                    // `const a = yield E;` — a forma da máquina.
                    Some(Expr::Yield(y))
                        if !y.delegate && !y.arg.as_deref().is_some_and(expr_has_yield) =>
                    {
                        out.push(let_decl(&name, Expr::Yield(y.clone())));
                    }
                    Some(init) if expr_has_yield(init) => {
                        let e = norm_expr(init, out, ctr)?;
                        out.push(let_decl(&name, e));
                    }
                    Some(init) => out.push(let_decl(&name, init.clone())),
                    None => out.push(let_decl(&name, undef_expr())),
                }
            }
            Some(())
        }

        Stmt::Return(r) => {
            match r.arg.as_deref() {
                Some(a) if expr_has_yield(a) => {
                    let e = norm_expr(a, out, ctr)?;
                    out.push(Stmt::Return(swc_ecma_ast::ReturnStmt {
                        span: sp(),
                        arg: Some(Box::new(e)),
                    }));
                }
                _ => out.push(s.clone()),
            }
            Some(())
        }

        Stmt::Throw(t) => {
            if expr_has_yield(&t.arg) {
                let e = norm_expr(&t.arg, out, ctr)?;
                out.push(Stmt::Throw(swc_ecma_ast::ThrowStmt {
                    span: sp(),
                    arg: Box::new(e),
                }));
            } else {
                out.push(s.clone());
            }
            Some(())
        }

        // O TESTE é avaliado uma vez antes de ramificar, então içá-lo para fora
        // do `if` preserva a semântica exatamente (ao contrário do teste de um
        // laço, que é reavaliado — ver `Stmt::While` abaixo).
        Stmt::If(i) => {
            let test = if expr_has_yield(&i.test) {
                norm_expr(&i.test, out, ctr)?
            } else {
                (*i.test).clone()
            };
            let cons = norm_block(&i.cons, ctr)?;
            let alt = match i.alt.as_deref() {
                Some(a) => Some(Box::new(norm_block(a, ctr)?)),
                None => None,
            };
            out.push(Stmt::If(swc_ecma_ast::IfStmt {
                span: sp(),
                test: Box::new(test),
                cons: Box::new(cons),
                alt,
            }));
            Some(())
        }

        Stmt::Block(b) => {
            out.push(Stmt::Block(swc_ecma_ast::BlockStmt {
                span: sp(),
                ctxt: Default::default(),
                stmts: normalize_body(&b.stmts)?,
            }));
            Some(())
        }

        // Laços: o teste/cabeçalho é REAVALIADO a cada volta, então içá-lo para
        // fora rodaria uma vez só — recusa em vez de mudar o número de
        // avaliações. O CORPO é normalizado normalmente.
        Stmt::While(w) => {
            if expr_has_yield(&w.test) {
                return None;
            }
            out.push(Stmt::While(swc_ecma_ast::WhileStmt {
                span: sp(),
                test: w.test.clone(),
                body: Box::new(norm_block(&w.body, ctr)?),
            }));
            Some(())
        }
        Stmt::DoWhile(d) => {
            if expr_has_yield(&d.test) {
                return None;
            }
            out.push(Stmt::DoWhile(swc_ecma_ast::DoWhileStmt {
                span: sp(),
                test: d.test.clone(),
                body: Box::new(norm_block(&d.body, ctr)?),
            }));
            Some(())
        }
        Stmt::For(f) => {
            let head_yield = match &f.init {
                Some(swc_ecma_ast::VarDeclOrExpr::VarDecl(vd)) => vd
                    .decls
                    .iter()
                    .any(|d| d.init.as_deref().is_some_and(expr_has_yield)),
                Some(swc_ecma_ast::VarDeclOrExpr::Expr(e)) => expr_has_yield(e),
                None => false,
            } || f.test.as_deref().is_some_and(expr_has_yield)
                || f.update.as_deref().is_some_and(expr_has_yield);
            if head_yield {
                return None;
            }
            let mut f2 = f.clone();
            f2.body = Box::new(norm_block(&f.body, ctr)?);
            out.push(Stmt::For(f2));
            Some(())
        }
        Stmt::ForOf(f) => {
            if expr_has_yield(&f.right) {
                return None;
            }
            let mut f2 = f.clone();
            f2.body = Box::new(norm_block(&f.body, ctr)?);
            out.push(Stmt::ForOf(f2));
            Some(())
        }
        Stmt::ForIn(f) => {
            if expr_has_yield(&f.right) {
                return None;
            }
            let mut f2 = f.clone();
            f2.body = Box::new(norm_block(&f.body, ctr)?);
            out.push(Stmt::ForIn(f2));
            Some(())
        }

        Stmt::Switch(sw) => {
            if expr_has_yield(&sw.discriminant)
                || sw
                    .cases
                    .iter()
                    .any(|c| c.test.as_deref().is_some_and(expr_has_yield))
            {
                return None;
            }
            let mut sw2 = sw.clone();
            for (i, c) in sw.cases.iter().enumerate() {
                sw2.cases[i].cons = normalize_body(&c.cons)?;
            }
            out.push(Stmt::Switch(sw2));
            Some(())
        }

        Stmt::Try(t) => {
            let mut t2 = t.clone();
            t2.block.stmts = normalize_body(&t.block.stmts)?;
            if let Some(h) = t.handler.as_ref() {
                let mut h2 = h.clone();
                h2.body.stmts = normalize_body(&h.body.stmts)?;
                t2.handler = Some(h2);
            }
            if let Some(f) = t.finalizer.as_ref() {
                let mut f2 = f.clone();
                f2.stmts = normalize_body(&f.stmts)?;
                t2.finalizer = Some(f2);
            }
            out.push(Stmt::Try(t2));
            Some(())
        }

        Stmt::Labeled(l) => {
            let mut inner = Vec::new();
            norm_stmt(&l.body, &mut inner, ctr)?;
            // Um rótulo governa UM statement; se a normalização tivesse partido
            // o corpo em vários, o rótulo passaria a cobrir só o primeiro.
            if inner.len() != 1 {
                return None;
            }
            out.push(Stmt::Labeled(swc_ecma_ast::LabeledStmt {
                span: sp(),
                label: l.label.clone(),
                body: Box::new(inner.pop()?),
            }));
            Some(())
        }

        // Sem yield → intocado. Com yield numa posição não coberta acima →
        // recusa (o caller cai no eager-buffer, comportamento de hoje).
        other => {
            if stmt_mentions_yield(other) {
                return None;
            }
            out.push(other.clone());
            Some(())
        }
    }
}

/// Normaliza o corpo de um `if`/laço, sempre devolvendo um BLOCO (a
/// normalização pode transformar um statement único em vários).
fn norm_block(s: &Stmt, ctr: &mut usize) -> Option<Stmt> {
    let mut inner = Vec::new();
    norm_stmt(s, &mut inner, ctr)?;
    Some(Stmt::Block(swc_ecma_ast::BlockStmt {
        span: sp(),
        ctxt: Default::default(),
        stmts: inner,
    }))
}

/// `yield` em qualquer lugar deste statement (sem descer em fn/arrow aninhada,
/// que tem o próprio corpo). Usado só pelo braço de recusa.
fn stmt_mentions_yield(s: &Stmt) -> bool {
    use swc_ecma_visit::{Visit, VisitWith};
    #[derive(Default)]
    struct V(bool);
    impl Visit for V {
        fn visit_yield_expr(&mut self, _: &swc_ecma_ast::YieldExpr) {
            self.0 = true;
        }
        fn visit_function(&mut self, _: &swc_ecma_ast::Function) {}
        fn visit_arrow_expr(&mut self, _: &swc_ecma_ast::ArrowExpr) {}
    }
    let mut v = V::default();
    s.visit_with(&mut v);
    v.0
}
