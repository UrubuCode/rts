//! State-machine lazy para generators (`function*`) — issue #477.
//!
//! Substitui o eager-buffer (que materializa TODOS os yields num Vec e trava
//! em `while(true)`) por suspensao/retomada real. So' eh aplicado a generators
//! ELEGIVEIS (control-flow simples coberto pelo flattener abaixo); os demais
//! continuam no caminho eager (`generator_desugar.rs`) — fallback que garante
//! zero-regressao.
//!
//! Transformacao (regenerator-style): o corpo vira uma fn de estado
//! `__gen_state_<name>(__g)` que, a cada chamada, le `__RTS_GEN_SM_STATE(__g)`,
//! executa o bloco daquele estado ate o proximo `yield`, salva o proximo estado
//! e suspende devolvendo o valor (`__RTS_GEN_SM_YIELD`). Os locais sao
//! persistidos no frame do generator (`__RTS_GEN_SM_FSET`/`FGET`) ao redor de
//! cada suspensao. `g(params)` apenas aloca o `Entry::GenState`
//! (`__RTS_GEN_SM_NEW`), escreve os params no frame e devolve o handle — NAO
//! roda o corpo (lazy de verdade).
//!
//! Slice 1 (#477): cobre sequencias lineares de `yield`, `return X`,
//! atribuicoes/decls simples, UM `while(true)` ou `while(test)`/`for(...)`
//! envolvendo yields. `if`/`try`/`yield*`/nesting profundo => inelegivel
//! (fallback eager).

use swc_ecma_ast::{
    AssignExpr, AssignOp, AssignTarget, BlockStmt, CallExpr, Callee, Decl, Expr, ExprOrSpread,
    ExprStmt, FnDecl, Function, Ident, Lit, Number, Param, Pat, SimpleAssignTarget, Stmt, VarDecl,
    VarDeclKind, VarDeclarator,
};
use swc_common::Span;

thread_local! {
    /// Span real do corpo da generator fn em compilacao. Usado em TODOS os nodes
    /// sinteticos para que `span_snippet` (gate do `raw_statement`) devolva texto
    /// nao-vazio — caso contrario o codegen DESCARTA o statement (era o bug que
    /// deixava o corpo da state-machine vazio).
    static SM_SPAN: std::cell::Cell<Span> = const { std::cell::Cell::new(swc_common::DUMMY_SP) };
}

fn sp() -> Span {
    SM_SPAN.with(|c| c.get())
}

/// Tenta construir a state-machine. Em sucesso devolve `(constructor, state_fn)`
/// como dois `FnDecl` swc top-level. Em qualquer construct nao suportado,
/// devolve `None` (caller cai no eager-buffer).
pub fn try_build(name: &str, function: &Function) -> Option<(FnDecl, FnDecl)> {
    let body = function.body.as_ref()?;
    // Usa o span real do corpo para todos os nodes sinteticos (gate de
    // span_snippet no raw_statement).
    SM_SPAN.with(|c| c.set(body.span));

    // Coleta nomes de parametros (apenas idents simples; destructuring =>
    // inelegivel nesta fatia).
    let mut params: Vec<String> = Vec::new();
    for p in &function.params {
        match &p.pat {
            Pat::Ident(id) => params.push(id.id.sym.to_string()),
            _ => return None,
        }
    }

    // (#477 slice 1) So' aplica state-machine a generators que REALMENTE
    // precisam de lazy: corpo contem um loop (`while`/`for`) com `yield` dentro.
    // Generators lineares finitos (`yield 1; yield 2;`) continuam no eager-buffer
    // — caminho provado que ja' serve for-of/spread/.next sem regressao.
    if !body_needs_lazy(&body.stmts) {
        return None;
    }

    let mut builder = SmBuilder::new();
    // Pre-registra params como locais (slots 0..n).
    for p in &params {
        builder.intern_local(p);
    }

    // Flatten do corpo num grafo de estados. O estado inicial eh 0.
    let entry = builder.new_state();
    let exit = builder.lower_seq(&body.stmts, entry)?;
    // Estado de saida normal: done(undefined).
    builder.set_done(exit, None);

    let state_fn_name = format!("__gen_state_{}", name);
    let nslots = builder.locals.len() as f64;

    // ── Constructor: g(params) { aloca GenState, escreve params, retorna h } ──
    let ctor = build_ctor(name, &params, &state_fn_name, nslots, &builder);
    // ── State fn: __gen_state_<name>(__g) { switch sobre estado } ─────────────
    let state_fn = build_state_fn(&state_fn_name, &builder)?;

    Some((ctor, state_fn))
}

/// (#207 async-SM) Tenta construir a state-machine de uma `async function`
/// elegivel (corpo linear com `await` em statement/decl/assign; sem await
/// aninhado em sub-expressao, sem loops/if com await nesta fatia). Em sucesso
/// devolve `(constructor, state_fn)`; em qualquer construct inelegivel devolve
/// `None` (caller cai no caminho thread-blocking de `expand_async_functions`).
pub fn try_build_async(name: &str, function: &Function) -> Option<(FnDecl, FnDecl)> {
    let body = function.body.as_ref()?;
    SM_SPAN.with(|c| c.set(body.span));

    let mut params: Vec<String> = Vec::new();
    for p in &function.params {
        match &p.pat {
            Pat::Ident(id) => params.push(id.id.sym.to_string()),
            _ => return None, // destructuring em params => inelegivel
        }
    }

    // So' aplica a SM async se o corpo REALMENTE tem await (senao deixa o
    // caminho atual cuidar — uma async fn sem await e' so' uma fn que devolve
    // promise resolvida).
    if !body.stmts.iter().any(stmt_has_await) {
        return None;
    }

    let mut builder = SmBuilder::new();
    builder.is_async = true;
    for p in &params {
        builder.intern_local(p);
    }

    let entry = builder.new_state();
    let exit = builder.lower_seq(&body.stmts, entry)?;
    builder.set_done(exit, None);

    let state_fn_name = format!("__async_state_{}", name);
    let nslots = builder.locals.len() as f64;

    let ctor = build_async_ctor(name, &params, &state_fn_name, nslots);
    let state_fn = build_state_fn(&state_fn_name, &builder)?;

    Some((ctor, state_fn))
}

const G: &str = "__g";

fn ident(s: &str) -> Ident {
    Ident::new(s.into(), sp(), Default::default())
}
fn ident_expr(s: &str) -> Expr {
    Expr::Ident(ident(s))
}
fn num_expr(n: f64) -> Expr {
    Expr::Lit(Lit::Num(Number { span: sp(), value: n, raw: None }))
}
fn undef_expr() -> Expr {
    ident_expr("undefined")
}

fn call(callee: &str, args: Vec<Expr>) -> Expr {
    Expr::Call(CallExpr {
        span: sp(),
        ctxt: Default::default(),
        callee: Callee::Expr(Box::new(ident_expr(callee))),
        args: args
            .into_iter()
            .map(|e| ExprOrSpread { spread: None, expr: Box::new(e) })
            .collect(),
        type_args: None,
    })
}
fn call_stmt(e: Expr) -> Stmt {
    Stmt::Expr(ExprStmt { span: sp(), expr: Box::new(e) })
}

/// `let <name> = <init>;`
fn let_decl(name: &str, init: Expr) -> Stmt {
    Stmt::Decl(Decl::Var(Box::new(VarDecl {
        span: sp(),
        ctxt: Default::default(),
        kind: VarDeclKind::Let,
        declare: false,
        decls: vec![VarDeclarator {
            span: sp(),
            name: Pat::Ident(ident(name).into()),
            init: Some(Box::new(init)),
            definite: false,
        }],
    })))
}

/// `<name> = <value>;` (assign a ident existente).
fn assign_stmt(name: &str, value: Expr) -> Stmt {
    call_stmt(Expr::Assign(AssignExpr {
        span: sp(),
        op: AssignOp::Assign,
        left: AssignTarget::Simple(SimpleAssignTarget::Ident(ident(name).into())),
        right: Box::new(value),
    }))
}

/// Transicao terminal de um estado.
enum Trans {
    /// yield <value>; proximo estado = next.
    Yield { value: Expr, next: usize },
    /// goto <state> (sem suspensao).
    Goto(usize),
    /// return <ret> (done).
    Done(Option<Expr>),
    /// if (<test>) goto then else goto els.
    Cond { test: Expr, then_s: usize, els: usize },
    /// Fim do bloco `finally`: chama END_FINALLY (que honra return/throw
    /// pendente, marcando done + state=-2) e, se nada pendente, segue para
    /// `normal_next`. (#477 fatia 2)
    EndFinally { normal_next: usize },
    /// (#207 async-SM) `await <promise>`: salva locais, SETSTATE(next) e
    /// suspende devolvendo `ASYNC_SM_SUSPEND(__g, <promise>)`. O estado `next`
    /// comeca lendo o valor injetado via `ASYNC_SM_AWAITED(__g)`.
    Await { promise: Expr, next: usize },
    /// placeholder ate ser preenchido.
    Unset,
}

struct StateBlock {
    /// Statements (efeitos colaterais / decls / assigns) antes da transicao.
    stmts: Vec<Stmt>,
    trans: Trans,
}

struct SmBuilder {
    states: Vec<StateBlock>,
    /// Locais (params + lets) -> slot index, em ordem de criacao.
    locals: Vec<String>,
    /// (#207) True quando construindo a SM de uma `async function` (await=
    /// suspensao); muda os terminais (SUSPEND/RESOLVE em vez de YIELD/DONE).
    is_async: bool,
}

impl SmBuilder {
    fn new() -> Self {
        SmBuilder { states: Vec::new(), locals: Vec::new(), is_async: false }
    }

    fn intern_local(&mut self, name: &str) -> usize {
        if let Some(i) = self.locals.iter().position(|n| n == name) {
            return i;
        }
        self.locals.push(name.to_string());
        self.locals.len() - 1
    }

    fn new_state(&mut self) -> usize {
        self.states.push(StateBlock { stmts: Vec::new(), trans: Trans::Unset });
        self.states.len() - 1
    }

    fn push_stmt(&mut self, state: usize, s: Stmt) {
        self.states[state].stmts.push(s);
    }
    fn set_yield(&mut self, state: usize, value: Expr, next: usize) {
        self.states[state].trans = Trans::Yield { value, next };
    }
    fn set_goto(&mut self, state: usize, target: usize) {
        self.states[state].trans = Trans::Goto(target);
    }
    fn set_done(&mut self, state: usize, ret: Option<Expr>) {
        self.states[state].trans = Trans::Done(ret);
    }
    fn set_cond(&mut self, state: usize, test: Expr, then_s: usize, els: usize) {
        self.states[state].trans = Trans::Cond { test, then_s, els };
    }
    fn set_end_finally(&mut self, state: usize, normal_next: usize) {
        self.states[state].trans = Trans::EndFinally { normal_next };
    }
    fn set_await(&mut self, state: usize, promise: Expr, next: usize) {
        self.states[state].trans = Trans::Await { promise, next };
    }

    /// Lower uma sequencia de statements comecando em `cur`. Devolve o estado
    /// "depois" (que ainda nao tem transicao). `None` => construct inelegivel.
    fn lower_seq(&mut self, stmts: &[Stmt], mut cur: usize) -> Option<usize> {
        for s in stmts {
            cur = self.lower_stmt(s, cur)?;
        }
        Some(cur)
    }

    fn lower_stmt(&mut self, stmt: &Stmt, cur: usize) -> Option<usize> {
        // (#207 async-SM) Pontos de suspensao `await` em statement/decl/assign.
        // `await` aninhado em sub-expressao => inelegivel (fallback).
        if self.is_async {
            if let Some(next) = self.try_lower_await_stmt(stmt, cur)? {
                return Some(next);
            }
        }
        match stmt {
            // `yield expr;`
            Stmt::Expr(es) => {
                if let Expr::Yield(y) = es.expr.as_ref() {
                    if y.delegate {
                        return None; // yield* => inelegivel nesta fatia
                    }
                    let value = y.arg.as_deref().cloned().unwrap_or_else(undef_expr);
                    let next = self.new_state();
                    self.set_yield(cur, value, next);
                    return Some(next);
                }
                // assignment / call comum: registra qualquer ident-target como local
                if let Expr::Assign(a) = es.expr.as_ref() {
                    if expr_has_yield(&a.right) {
                        return None;
                    }
                    self.register_assign_target(&a.left);
                }
                if expr_has_yield(es.expr.as_ref()) {
                    return None;
                }
                self.push_stmt(cur, stmt.clone());
                Some(cur)
            }
            // `let/const x = init;`
            Stmt::Decl(Decl::Var(vd)) => {
                for d in &vd.decls {
                    let name = match &d.name {
                        Pat::Ident(id) => id.id.sym.to_string(),
                        _ => return None, // destructuring => inelegivel
                    };
                    if let Some(init) = d.init.as_deref() {
                        if expr_has_yield(init) {
                            return None;
                        }
                    }
                    self.intern_local(&name);
                    // Reescreve const->let (frame reload exige reatribuicao) e
                    // emite como assignment a um local ja' declarado no prologo.
                    let init = d.init.as_deref().cloned().unwrap_or_else(undef_expr);
                    self.push_stmt(cur, assign_stmt(&name, init));
                }
                Some(cur)
            }
            // `while (test) { body }`  (inclui while(true))
            Stmt::While(w) => {
                if expr_has_yield(&w.test) {
                    return None;
                }
                // header state
                let header = self.new_state();
                self.set_goto(cur, header);
                let body_entry = self.new_state();
                let after = self.new_state();
                // header: if (test) goto body else goto after
                if is_true_lit(&w.test) {
                    self.set_goto(header, body_entry);
                } else {
                    self.set_cond(header, w.test.as_ref().clone(), body_entry, after);
                }
                // body
                let body_stmts = block_stmts(&w.body)?;
                let body_exit = self.lower_seq(body_stmts, body_entry)?;
                // ao fim do corpo, volta ao header
                self.set_goto(body_exit, header);
                Some(after)
            }
            // `for (init; test; update) { body }`
            Stmt::For(f) => {
                // init
                let mut c = cur;
                if let Some(init) = &f.init {
                    match init {
                        swc_ecma_ast::VarDeclOrExpr::VarDecl(vd) => {
                            c = self.lower_stmt(&Stmt::Decl(Decl::Var(vd.clone())), c)?;
                        }
                        swc_ecma_ast::VarDeclOrExpr::Expr(e) => {
                            if expr_has_yield(e) {
                                return None;
                            }
                            self.push_stmt(c, call_stmt((**e).clone()));
                        }
                    }
                }
                let header = self.new_state();
                self.set_goto(c, header);
                let body_entry = self.new_state();
                let after = self.new_state();
                match &f.test {
                    Some(t) => {
                        if expr_has_yield(t) {
                            return None;
                        }
                        self.set_cond(header, (**t).clone(), body_entry, after);
                    }
                    None => self.set_goto(header, body_entry),
                }
                let body_stmts = block_stmts(&f.body)?;
                let body_exit = self.lower_seq(body_stmts, body_entry)?;
                // update no fim do corpo
                let upd_state = body_exit;
                if let Some(u) = &f.update {
                    if expr_has_yield(u) {
                        return None;
                    }
                    self.push_stmt(upd_state, call_stmt((**u).clone()));
                }
                self.set_goto(upd_state, header);
                Some(after)
            }
            // `return X;`
            Stmt::Return(r) => {
                if let Some(arg) = &r.arg {
                    if expr_has_yield(arg) {
                        return None;
                    }
                }
                self.set_done(cur, r.arg.as_deref().cloned());
                // codigo apos return eh inalcancavel; novo estado morto p/ seq.
                Some(self.new_state())
            }
            // `try { ... } finally { ... }` (#477 fatia 2)
            Stmt::Try(t) => {
                // catch com yield => fora de escopo desta fatia.
                if let Some(h) = &t.handler {
                    if h.body.stmts.iter().any(stmt_has_yield) {
                        return None;
                    }
                }
                let Some(finalizer) = &t.finalizer else {
                    // try sem finally: so' tem sentido aqui se o body tem yield.
                    // Achata o body como sequencia (sem semantica de catch real).
                    if t.handler.is_some() {
                        return None;
                    }
                    return self.lower_seq(&t.block.stmts, cur);
                };
                // Estado de entrada do finally (alvo de ENTER_TRY e da saida
                // normal do try body). Reservamos o indice antes de lower o body
                // para que ENTER_TRY aponte para ele.
                let finally_entry = self.new_state();
                // ENTER_TRY(__g, finally_entry) no estado atual, antes do body.
                self.push_stmt(
                    cur,
                    call_stmt(call(
                        "__RTS_GEN_SM_ENTER_TRY",
                        vec![ident_expr(G), num_expr(finally_entry as f64)],
                    )),
                );
                // try body: comeca num estado novo logo apos `cur`.
                let body_entry = self.new_state();
                self.set_goto(cur, body_entry);
                let body_exit = self.lower_seq(&t.block.stmts, body_entry)?;
                // saida normal do body -> finally_entry
                self.set_goto(body_exit, finally_entry);
                // finally body
                let fin_exit = self.lower_seq(&finalizer.stmts, finally_entry)?;
                // ao fim do finally: END_FINALLY; se nada pendente, segue p/ after.
                let after = self.new_state();
                self.set_end_finally(fin_exit, after);
                Some(after)
            }
            // bloco aninhado simples: achata
            Stmt::Block(b) => self.lower_seq(&b.stmts, cur),
            // `if` sem yield no corpo: mantemos verbatim (efeito colateral puro).
            Stmt::If(i) => {
                if stmt_has_yield(&i.cons) || i.alt.as_deref().map(stmt_has_yield).unwrap_or(false)
                    || expr_has_yield(&i.test)
                {
                    return None; // if com yield => inelegivel nesta fatia
                }
                self.push_stmt(cur, stmt.clone());
                Some(cur)
            }
            _ => None,
        }
    }

    /// (#207) Tenta lower de um statement com `await`. Contrato:
    /// - `Ok(Some(next))` => era um await elegivel, lowered (proximo estado `next`).
    /// - `Ok(None)` => NAO contem await; segue para o match generico.
    /// - `Err`(via `?` retornando None) => contem await em posicao inelegivel
    ///   (sub-expressao aninhada) => aborta o build (fallback thread-blocking).
    ///
    /// Elegiveis fatia 1: `await x` (statement), `const/let v = await x`,
    /// `v = await x`. O RHS do await NAO pode conter outro await aninhado.
    fn try_lower_await_stmt(&mut self, stmt: &Stmt, cur: usize) -> Option<Option<usize>> {
        match stmt {
            Stmt::Expr(es) => {
                match es.expr.as_ref() {
                    // `await x;` (valor descartado, mas AWAITED roda p/ propagar
                    // rejection ao error slot).
                    Expr::Await(aw) => {
                        if expr_has_await(&aw.arg) {
                            return None; // await aninhado no RHS => inelegivel
                        }
                        let next = self.new_state();
                        self.set_await(cur, (*aw.arg).clone(), next);
                        // estado `next`: chama AWAITED(__g) (descarta valor).
                        self.push_stmt(
                            next,
                            call_stmt(call("__RTS_ASYNC_SM_AWAITED", vec![ident_expr(G)])),
                        );
                        Some(Some(next))
                    }
                    // `v = await x;`
                    Expr::Assign(a) => {
                        if let Expr::Await(aw) = a.right.as_ref() {
                            if expr_has_await(&aw.arg) {
                                return None;
                            }
                            // alvo precisa ser ident simples.
                            let AssignTarget::Simple(SimpleAssignTarget::Ident(id)) = &a.left
                            else {
                                return None;
                            };
                            let name = id.id.sym.to_string();
                            self.intern_local(&name);
                            let next = self.new_state();
                            self.set_await(cur, (*aw.arg).clone(), next);
                            // estado `next`: v = AWAITED(__g)
                            self.push_stmt(
                                next,
                                assign_stmt(
                                    &name,
                                    call("__RTS_ASYNC_SM_AWAITED", vec![ident_expr(G)]),
                                ),
                            );
                            Some(Some(next))
                        } else if expr_has_await(es.expr.as_ref()) {
                            None // await aninhado em assign complexo => inelegivel
                        } else {
                            Some(None)
                        }
                    }
                    other => {
                        if expr_has_await(other) {
                            None // await aninhado em call/etc => inelegivel
                        } else {
                            Some(None)
                        }
                    }
                }
            }
            // `const/let v = await x;`
            Stmt::Decl(Decl::Var(vd)) => {
                // So' trata o caso de UM declarator com init = await direto.
                if vd.decls.len() == 1 {
                    let d = &vd.decls[0];
                    if let (Pat::Ident(id), Some(init)) = (&d.name, d.init.as_deref()) {
                        if let Expr::Await(aw) = init {
                            if expr_has_await(&aw.arg) {
                                return None;
                            }
                            let name = id.id.sym.to_string();
                            self.intern_local(&name);
                            let next = self.new_state();
                            self.set_await(cur, (*aw.arg).clone(), next);
                            self.push_stmt(
                                next,
                                assign_stmt(
                                    &name,
                                    call("__RTS_ASYNC_SM_AWAITED", vec![ident_expr(G)]),
                                ),
                            );
                            return Some(Some(next));
                        }
                    }
                }
                // contem await em init mas nao no formato simples => inelegivel
                if vd.decls.iter().any(|d| d.init.as_deref().map(expr_has_await).unwrap_or(false)) {
                    return None;
                }
                Some(None)
            }
            // `return await x;`
            Stmt::Return(r) => {
                if let Some(Expr::Await(aw)) = r.arg.as_deref() {
                    if expr_has_await(&aw.arg) {
                        return None;
                    }
                    let next = self.new_state();
                    self.set_await(cur, (*aw.arg).clone(), next);
                    // estado `next`: return AWAITED(__g)  (Done)
                    self.set_done(next, Some(call("__RTS_ASYNC_SM_AWAITED", vec![ident_expr(G)])));
                    return Some(Some(self.new_state()));
                }
                if r.arg.as_deref().map(expr_has_await).unwrap_or(false) {
                    return None;
                }
                Some(None)
            }
            // Qualquer outro statement que contenha await => inelegivel fatia 1.
            other if stmt_has_await(other) => None,
            _ => Some(None),
        }
    }

    fn register_assign_target(&mut self, t: &AssignTarget) {
        if let AssignTarget::Simple(SimpleAssignTarget::Ident(id)) = t {
            self.intern_local(&id.id.sym.to_string());
        }
    }
}

fn is_true_lit(e: &Expr) -> bool {
    matches!(e, Expr::Lit(Lit::Bool(b)) if b.value)
}

fn block_stmts(s: &Stmt) -> Option<&[Stmt]> {
    match s {
        Stmt::Block(b) => Some(&b.stmts),
        // corpo de loop como statement unico (ex: `while(x) yield a;`)
        other => Some(std::slice::from_ref(other_as_slice_hack(other))),
    }
}

// Helper: devolve referencia ao proprio stmt (corpos single-statement).
fn other_as_slice_hack(s: &Stmt) -> &Stmt {
    s
}

// ── Deteccao de yield em sub-arvores (para rejeitar casos nao modelados) ─────

fn expr_has_yield(e: &Expr) -> bool {
    match e {
        Expr::Yield(_) => true,
        Expr::Assign(a) => expr_has_yield(&a.right),
        Expr::Bin(b) => expr_has_yield(&b.left) || expr_has_yield(&b.right),
        Expr::Unary(u) => expr_has_yield(&u.arg),
        Expr::Paren(p) => expr_has_yield(&p.expr),
        Expr::Cond(c) => {
            expr_has_yield(&c.test) || expr_has_yield(&c.cons) || expr_has_yield(&c.alt)
        }
        Expr::Seq(s) => s.exprs.iter().any(|x| expr_has_yield(x)),
        Expr::Call(c) => c.args.iter().any(|a| expr_has_yield(&a.expr)),
        Expr::Array(a) => a
            .elems
            .iter()
            .any(|el| el.as_ref().map(|x| expr_has_yield(&x.expr)).unwrap_or(false)),
        _ => false,
    }
}

// ── Deteccao de await (#207 async-SM) ────────────────────────────────────────

fn expr_has_await(e: &Expr) -> bool {
    match e {
        Expr::Await(_) => true,
        Expr::Assign(a) => expr_has_await(&a.right),
        Expr::Bin(b) => expr_has_await(&b.left) || expr_has_await(&b.right),
        Expr::Unary(u) => expr_has_await(&u.arg),
        Expr::Paren(p) => expr_has_await(&p.expr),
        Expr::Cond(c) => {
            expr_has_await(&c.test) || expr_has_await(&c.cons) || expr_has_await(&c.alt)
        }
        Expr::Seq(s) => s.exprs.iter().any(|x| expr_has_await(x)),
        Expr::Call(c) => c.args.iter().any(|a| expr_has_await(&a.expr)),
        Expr::Member(m) => expr_has_await(&m.obj),
        Expr::Array(a) => a
            .elems
            .iter()
            .any(|el| el.as_ref().map(|x| expr_has_await(&x.expr)).unwrap_or(false)),
        _ => false,
    }
}

fn stmt_has_await(s: &Stmt) -> bool {
    match s {
        Stmt::Expr(e) => expr_has_await(&e.expr),
        Stmt::Block(b) => b.stmts.iter().any(stmt_has_await),
        Stmt::If(i) => {
            expr_has_await(&i.test)
                || stmt_has_await(&i.cons)
                || i.alt.as_deref().map(stmt_has_await).unwrap_or(false)
        }
        Stmt::While(w) => expr_has_await(&w.test) || stmt_has_await(&w.body),
        Stmt::DoWhile(d) => expr_has_await(&d.test) || stmt_has_await(&d.body),
        Stmt::For(f) => {
            f.test.as_deref().map(expr_has_await).unwrap_or(false)
                || f.update.as_deref().map(expr_has_await).unwrap_or(false)
                || stmt_has_await(&f.body)
        }
        Stmt::ForOf(fo) => stmt_has_await(&fo.body),
        Stmt::Decl(Decl::Var(vd)) => vd
            .decls
            .iter()
            .any(|d| d.init.as_deref().map(expr_has_await).unwrap_or(false)),
        Stmt::Return(r) => r.arg.as_deref().map(expr_has_await).unwrap_or(false),
        Stmt::Try(t) => {
            t.block.stmts.iter().any(stmt_has_await)
                || t.handler.as_ref().map(|h| h.body.stmts.iter().any(stmt_has_await)).unwrap_or(false)
                || t.finalizer.as_ref().map(|f| f.stmts.iter().any(stmt_has_await)).unwrap_or(false)
        }
        _ => false,
    }
}

/// True se o corpo precisa de suspensao lazy real: contem um loop com `yield`
/// (`while`/`for`/...) OU um `try`/`finally` envolvendo `yield` (#477 fatia 2).
/// Generators lineares finitos continuam no eager-buffer.
fn body_needs_lazy(stmts: &[Stmt]) -> bool {
    stmts.iter().any(stmt_needs_lazy)
}

fn stmt_needs_lazy(s: &Stmt) -> bool {
    match s {
        Stmt::While(w) => stmt_has_yield(&w.body),
        Stmt::DoWhile(d) => stmt_has_yield(&d.body),
        Stmt::For(f) => stmt_has_yield(&f.body),
        Stmt::ForOf(fo) => stmt_has_yield(&fo.body),
        Stmt::ForIn(fi) => stmt_has_yield(&fi.body),
        // try/finally com yield em qualquer parte exige state-machine.
        Stmt::Try(t) => {
            t.block.stmts.iter().any(stmt_has_yield)
                || t.handler.as_ref().map(|h| h.body.stmts.iter().any(stmt_has_yield)).unwrap_or(false)
                || t.finalizer.as_ref().map(|f| f.stmts.iter().any(stmt_has_yield)).unwrap_or(false)
        }
        Stmt::Block(b) => body_needs_lazy(&b.stmts),
        Stmt::If(i) => {
            stmt_needs_lazy(&i.cons)
                || i.alt.as_deref().map(stmt_needs_lazy).unwrap_or(false)
        }
        _ => false,
    }
}

fn stmt_has_yield(s: &Stmt) -> bool {
    match s {
        Stmt::Expr(e) => expr_has_yield(&e.expr),
        Stmt::Block(b) => b.stmts.iter().any(stmt_has_yield),
        Stmt::If(i) => {
            expr_has_yield(&i.test)
                || stmt_has_yield(&i.cons)
                || i.alt.as_deref().map(stmt_has_yield).unwrap_or(false)
        }
        Stmt::While(w) => expr_has_yield(&w.test) || stmt_has_yield(&w.body),
        Stmt::DoWhile(d) => expr_has_yield(&d.test) || stmt_has_yield(&d.body),
        Stmt::For(f) => {
            f.test.as_deref().map(expr_has_yield).unwrap_or(false)
                || f.update.as_deref().map(expr_has_yield).unwrap_or(false)
                || stmt_has_yield(&f.body)
        }
        Stmt::Decl(Decl::Var(vd)) => vd
            .decls
            .iter()
            .any(|d| d.init.as_deref().map(expr_has_yield).unwrap_or(false)),
        Stmt::Return(r) => r.arg.as_deref().map(expr_has_yield).unwrap_or(false),
        _ => false,
    }
}

// ── Construcao das duas fns swc ──────────────────────────────────────────────

fn param_ident(name: &str) -> Param {
    Param {
        span: sp(),
        decorators: Vec::new(),
        pat: Pat::Ident(ident(name).into()),
    }
}

fn build_ctor(
    name: &str,
    params: &[String],
    state_fn_name: &str,
    nslots: f64,
    _builder: &SmBuilder,
) -> FnDecl {
    let mut stmts: Vec<Stmt> = Vec::new();
    // const __g = __RTS_GEN_SM_NEW(<state_fn>, nslots);
    stmts.push(let_decl(
        G,
        call(
            "__RTS_GEN_SM_NEW",
            vec![ident_expr(state_fn_name), num_expr(nslots)],
        ),
    ));
    // escreve params nos slots 0..n
    for (i, p) in params.iter().enumerate() {
        stmts.push(call_stmt(call(
            "__RTS_GEN_SM_FSET",
            vec![ident_expr(G), num_expr(i as f64), ident_expr(p)],
        )));
    }
    // return __g;
    stmts.push(Stmt::Return(swc_ecma_ast::ReturnStmt {
        span: sp(),
        arg: Some(Box::new(ident_expr(G))),
    }));

    make_fn_decl(name, params, stmts, true)
}

/// (#207 async-SM) Ctor da async fn: aloca o GenState async, escreve params no
/// frame e devolve `__RTS_ASYNC_SM_START(__g)` — que roda o corpo ate o 1o
/// await e devolve o HANDLE da promise-resultado (preserva ABI `f(args) ->
/// Promise`).
fn build_async_ctor(
    name: &str,
    params: &[String],
    state_fn_name: &str,
    nslots: f64,
) -> FnDecl {
    let mut stmts: Vec<Stmt> = Vec::new();
    // const __g = __RTS_ASYNC_SM_NEW(<state_fn>, nslots);
    stmts.push(let_decl(
        G,
        call("__RTS_ASYNC_SM_NEW", vec![ident_expr(state_fn_name), num_expr(nslots)]),
    ));
    for (i, p) in params.iter().enumerate() {
        stmts.push(call_stmt(call(
            "__RTS_GEN_SM_FSET",
            vec![ident_expr(G), num_expr(i as f64), ident_expr(p)],
        )));
    }
    // return __RTS_ASYNC_SM_START(__g);
    stmts.push(Stmt::Return(swc_ecma_ast::ReturnStmt {
        span: sp(),
        arg: Some(Box::new(call("__RTS_ASYNC_SM_START", vec![ident_expr(G)]))),
    }));

    make_fn_decl(name, params, stmts, true)
}

fn build_state_fn(state_fn_name: &str, builder: &SmBuilder) -> Option<FnDecl> {
    let mut body: Vec<Stmt> = Vec::new();

    // Prologo: declara todos os locais (let) e carrega do frame.
    for (i, local) in builder.locals.iter().enumerate() {
        body.push(let_decl(
            local,
            call("__RTS_GEN_SM_FGET", vec![ident_expr(G), num_expr(i as f64)]),
        ));
    }

    // while (true) { const __st = STATE(__g); if(__st==0){...} else if ... }
    let mut chain: Option<Stmt> = None;
    // Construir a cadeia if/else-if de tras pra frente.
    for state_idx in (0..builder.states.len()).rev() {
        let st = &builder.states[state_idx];
        let mut blk: Vec<Stmt> = Vec::new();
        // statements do estado
        blk.extend(st.stmts.iter().cloned());
        // transicao
        match &st.trans {
            Trans::Yield { value, next } => {
                // salva todos os locais no frame
                blk.extend(store_all_locals(builder));
                // SETSTATE(__g, next)
                blk.push(call_stmt(call(
                    "__RTS_GEN_SM_SETSTATE",
                    vec![ident_expr(G), num_expr(*next as f64)],
                )));
                // return __RTS_GEN_SM_YIELD(__g, value)
                blk.push(Stmt::Return(swc_ecma_ast::ReturnStmt {
                    span: sp(),
                    arg: Some(Box::new(call(
                        "__RTS_GEN_SM_YIELD",
                        vec![ident_expr(G), value.clone()],
                    ))),
                }));
            }
            Trans::Goto(target) => {
                // salva locais e re-loop com novo estado (sem suspender)
                blk.extend(store_all_locals(builder));
                blk.push(call_stmt(call(
                    "__RTS_GEN_SM_SETSTATE",
                    vec![ident_expr(G), num_expr(*target as f64)],
                )));
                blk.push(Stmt::Continue(swc_ecma_ast::ContinueStmt {
                    span: sp(),
                    label: None,
                }));
            }
            Trans::Cond { test, then_s, els } => {
                blk.extend(store_all_locals(builder));
                // if (test) { SETSTATE then; continue } else { SETSTATE els; continue }
                let then_blk = goto_block(*then_s);
                let else_blk = goto_block(*els);
                blk.push(Stmt::If(swc_ecma_ast::IfStmt {
                    span: sp(),
                    test: Box::new(test.clone()),
                    cons: Box::new(then_blk),
                    alt: Some(Box::new(else_blk)),
                }));
            }
            Trans::EndFinally { normal_next } => {
                blk.extend(store_all_locals(builder));
                // const __ef = END_FINALLY(__g);
                blk.push(let_decl(
                    "__ef",
                    call("__RTS_GEN_SM_END_FINALLY", vec![ident_expr(G)]),
                ));
                // if (STATE(__g) === -2) { return __ef; }  -- completion honrada
                blk.push(Stmt::If(swc_ecma_ast::IfStmt {
                    span: sp(),
                    test: Box::new(Expr::Bin(swc_ecma_ast::BinExpr {
                        span: sp(),
                        op: swc_ecma_ast::BinaryOp::EqEqEq,
                        left: Box::new(call("__RTS_GEN_SM_STATE", vec![ident_expr(G)])),
                        right: Box::new(num_expr(-2.0)),
                    })),
                    cons: Box::new(Stmt::Block(BlockStmt {
                        span: sp(),
                        ctxt: Default::default(),
                        stmts: vec![Stmt::Return(swc_ecma_ast::ReturnStmt {
                            span: sp(),
                            arg: Some(Box::new(ident_expr("__ef"))),
                        })],
                    })),
                    alt: None,
                }));
                // senao: segue para o estado normal apos a try/finally.
                blk.push(call_stmt(call(
                    "__RTS_GEN_SM_SETSTATE",
                    vec![ident_expr(G), num_expr(*normal_next as f64)],
                )));
                blk.push(Stmt::Continue(swc_ecma_ast::ContinueStmt {
                    span: sp(),
                    label: None,
                }));
            }
            Trans::Done(ret) => {
                let ret_expr = ret.clone().unwrap_or_else(undef_expr);
                let term = if builder.is_async {
                    "__RTS_ASYNC_SM_RESOLVE"
                } else {
                    "__RTS_GEN_SM_DONE"
                };
                blk.push(Stmt::Return(swc_ecma_ast::ReturnStmt {
                    span: sp(),
                    arg: Some(Box::new(call(term, vec![ident_expr(G), ret_expr]))),
                }));
            }
            // (#207 async-SM) `await <promise>`: salva locais, SETSTATE(next),
            // suspende devolvendo SUSPEND(__g, <promise>). O drain retoma quando
            // a promise settla, injetando o valor no estado `next`.
            Trans::Await { promise, next } => {
                blk.extend(store_all_locals(builder));
                blk.push(call_stmt(call(
                    "__RTS_GEN_SM_SETSTATE",
                    vec![ident_expr(G), num_expr(*next as f64)],
                )));
                blk.push(Stmt::Return(swc_ecma_ast::ReturnStmt {
                    span: sp(),
                    arg: Some(Box::new(call(
                        "__RTS_ASYNC_SM_SUSPEND",
                        vec![ident_expr(G), promise.clone()],
                    ))),
                }));
            }
            Trans::Unset => return None,
        }

        let test = Expr::Bin(swc_ecma_ast::BinExpr {
            span: sp(),
            op: swc_ecma_ast::BinaryOp::EqEqEq,
            left: Box::new(ident_expr("__st")),
            right: Box::new(num_expr(state_idx as f64)),
        });
        let cons = Box::new(Stmt::Block(BlockStmt {
            span: sp(),
            ctxt: Default::default(),
            stmts: blk,
        }));
        chain = Some(Stmt::If(swc_ecma_ast::IfStmt {
            span: sp(),
            test: Box::new(test),
            cons,
            alt: chain.map(Box::new),
        }));
    }

    // else final: done(undefined) / resolve(undefined) p/ async.
    let final_term = if builder.is_async {
        "__RTS_ASYNC_SM_RESOLVE"
    } else {
        "__RTS_GEN_SM_DONE"
    };
    let final_else = Stmt::Return(swc_ecma_ast::ReturnStmt {
        span: sp(),
        arg: Some(Box::new(call(final_term, vec![ident_expr(G), undef_expr()]))),
    });
    let chain = match chain {
        Some(Stmt::If(mut i)) => {
            attach_final_else(&mut i, final_else);
            Stmt::If(i)
        }
        Some(other) => other,
        None => final_else,
    };

    let mut loop_body: Vec<Stmt> = Vec::new();
    // const __st = STATE(__g);
    loop_body.push(let_decl("__st", call("__RTS_GEN_SM_STATE", vec![ident_expr(G)])));
    loop_body.push(chain);

    body.push(Stmt::While(swc_ecma_ast::WhileStmt {
        span: sp(),
        test: Box::new(Expr::Lit(Lit::Bool(swc_ecma_ast::Bool {
            span: sp(),
            value: true,
        }))),
        body: Box::new(Stmt::Block(BlockStmt {
            span: sp(),
            ctxt: Default::default(),
            stmts: loop_body,
        })),
    }));

    Some(make_fn_decl(state_fn_name, &[G.to_string()], body, false))
}

/// Anexa o `else` final ao fim da cadeia if/else-if.
fn attach_final_else(iff: &mut swc_ecma_ast::IfStmt, final_else: Stmt) {
    match iff.alt.as_deref_mut() {
        Some(Stmt::If(inner)) => attach_final_else(inner, final_else),
        Some(_) => { /* ja' tem else (nao deveria ocorrer) */ }
        None => iff.alt = Some(Box::new(final_else)),
    }
}

/// Bloco `{ SETSTATE(__g, s); continue; }`.
fn goto_block(s: usize) -> Stmt {
    Stmt::Block(BlockStmt {
        span: sp(),
        ctxt: Default::default(),
        stmts: vec![
            call_stmt(call(
                "__RTS_GEN_SM_SETSTATE",
                vec![ident_expr(G), num_expr(s as f64)],
            )),
            Stmt::Continue(swc_ecma_ast::ContinueStmt { span: sp(), label: None }),
        ],
    })
}

/// Gera `FSET(__g, i, local_i)` para todos os locais (persiste antes de
/// suspender/transitar).
fn store_all_locals(builder: &SmBuilder) -> Vec<Stmt> {
    builder
        .locals
        .iter()
        .enumerate()
        .map(|(i, local)| {
            call_stmt(call(
                "__RTS_GEN_SM_FSET",
                vec![ident_expr(G), num_expr(i as f64), ident_expr(local)],
            ))
        })
        .collect()
}

fn make_fn_decl(name: &str, params: &[String], stmts: Vec<Stmt>, _is_ctor: bool) -> FnDecl {
    FnDecl {
        ident: ident(name),
        declare: false,
        function: Box::new(Function {
            params: params.iter().map(|p| param_ident(p)).collect(),
            decorators: Vec::new(),
            span: sp(),
            ctxt: Default::default(),
            body: Some(BlockStmt {
                span: sp(),
                ctxt: Default::default(),
                stmts,
            }),
            is_generator: false,
            is_async: false,
            type_params: None,
            return_type: None,
        }),
    }
}
