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

    // Coleta nomes de parametros. Idents simples + DEFAULTS (`x = 1`): o nome
    // entra como slot; o default fica preservado no `Param` do constructor para
    // que `expand_default_args` o aplique downstream (sem ele, `function*
    // g(start=1)` caia no eager-buffer e materializava infinito -> OOM).
    // Destructuring em params => inelegivel.
    let mut params: Vec<String> = Vec::new();
    let mut ctor_params: Vec<Param> = Vec::new();
    for p in &function.params {
        match &p.pat {
            Pat::Ident(id) => {
                params.push(id.id.sym.to_string());
                ctor_params.push(p.clone());
            }
            Pat::Assign(asn) => match asn.left.as_ref() {
                Pat::Ident(id) => {
                    params.push(id.id.sym.to_string());
                    ctor_params.push(p.clone());
                }
                _ => return None, // default sobre destructuring => inelegivel
            },
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
    let ctor = build_ctor(name, &params, &ctor_params, &state_fn_name, nslots, "__RTS_GEN_SM_NEW");
    // ── State fn: __gen_state_<name>(__g) { switch sobre estado } ─────────────
    let state_fn = build_state_fn(&state_fn_name, &builder)?;

    Some((ctor, state_fn))
}

/// (cross-runtime #392) `async function*` — async generator lazy. Combina o SM de
/// generator (yield) com o de async fn (await): `is_async=true` faz lower_stmt
/// tratar await como suspensao; o yield gera valores. Ctor usa `__RTS_AGEN_NEW`;
/// `.next()` devolve Promise<{value,done}>. SEM gate `body_needs_lazy` (async gens
/// precisam do SM mesmo finitos — eager nao faz .next()/await/throw lazy).
pub fn try_build_async_gen(name: &str, function: &Function) -> Option<(FnDecl, FnDecl)> {
    let body = function.body.as_ref()?;
    SM_SPAN.with(|c| c.set(body.span));

    let mut params: Vec<String> = Vec::new();
    let mut ctor_params: Vec<Param> = Vec::new();
    for p in &function.params {
        match &p.pat {
            Pat::Ident(id) => {
                params.push(id.id.sym.to_string());
                ctor_params.push(p.clone());
            }
            Pat::Assign(asn) => match asn.left.as_ref() {
                Pat::Ident(id) => {
                    params.push(id.id.sym.to_string());
                    ctor_params.push(p.clone());
                }
                _ => return None,
            },
            _ => return None,
        }
    }

    let mut builder = SmBuilder::new();
    builder.is_async = true;
    builder.is_async_gen = true;
    for p in &params {
        builder.intern_local(p);
    }

    let entry = builder.new_state();
    let exit = builder.lower_seq(&body.stmts, entry)?;
    builder.set_done(exit, None);

    let state_fn_name = format!("__agen_state_{}", name);
    let nslots = builder.locals.len() as f64;

    let ctor = build_ctor(name, &params, &ctor_params, &state_fn_name, nslots, "__RTS_AGEN_NEW");
    let state_fn = build_state_fn(&state_fn_name, &builder)?;

    Some((ctor, state_fn))
}

/// (#207 async-SM) Tenta construir a state-machine de uma `async function`
/// elegivel (corpo linear com `await` em statement/decl/assign; sem await
/// aninhado em sub-expressao, sem loops/if com await nesta fatia). Em sucesso
/// devolve `(constructor, state_fn)`; em qualquer construct inelegivel devolve
/// `None` (caller cai no caminho thread-blocking de `expand_async_functions`).
pub fn try_build_async(name: &str, function: &Function) -> Option<(FnDecl, FnDecl)> {
    // (motor novo) A state-machine async emite `__RTS_ASYNC_SM_SUSPEND`, que o motor
    // novo NAO instala (a SM e' acoplada ao modelo de valor antigo). Desabilitada
    // aqui: toda async fn cai no caminho `expand_async_functions` (promise.create/
    // wait, `ns::promise` registrado). Async paralelo/suspensao real fica p/ um
    // redesign limpo (latitude @drysius — desativar paralelo agora, refazer depois).
    return None;
    #[allow(unreachable_code)]
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

    // (cross-runtime #365) BAIL se o corpo tem um local capturado por uma
    // closure aninhada E mutado. A SM async re-invoca a state-fn a cada resume
    // com um frame NOVO; a captura por endereco do local (mecanismo do
    // foundation de closures) fica stale apos o suspend, e a mutacao feita pela
    // closure nao chega de volta ao frame retomado (bug: `attempts++` numa arrow
    // chamada apos um await rendia attempts=0). O caminho nao-SM (promise.wait
    // sincrono, sem re-invocacao de frame) preserva a captura corretamente.
    if body_has_captured_mutated_local(&body.stmts) {
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
    /// (cross-runtime #392) async generator (`async function*`): is_async tambem
    /// true, mas o terminal Done usa GEN_SM_DONE (gera {value,done:true}) e os
    /// loops com await internos sao permitidos (gate em try_lower_await_stmt).
    is_async_gen: bool,
    /// Contador de temporarios sinteticos unicos (yield* delegation).
    tmp_counter: usize,
}

impl SmBuilder {
    fn new() -> Self {
        SmBuilder { states: Vec::new(), locals: Vec::new(), is_async: false, is_async_gen: false, tmp_counter: 0 }
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
                        // `yield* SRC;` (posicao de statement). Delegacao lazy:
                        // normaliza SRC num iterador e re-yielda cada valor,
                        // suspendendo entre eles. O estado do iterador delegado
                        // vive runtime-side (keyed pelo handle), entao so' o
                        // handle precisa entrar no frame.
                        let src = y.arg.as_deref().cloned().unwrap_or_else(undef_expr);
                        let uid = self.tmp_counter;
                        self.tmp_counter += 1;
                        let dlg = format!("__dlg_{}", uid);
                        let dval = format!("__dlv_{}", uid);
                        self.intern_local(&dlg);
                        self.intern_local(&dval);
                        // cur: __dlg = DELEGATE_START(src)
                        self.push_stmt(
                            cur,
                            assign_stmt(&dlg, call("__RTS_GEN_DELEGATE_START", vec![src])),
                        );
                        let header = self.new_state();
                        self.set_goto(cur, header);
                        let body = self.new_state();
                        let after = self.new_state();
                        // header: __dlv = DELEGATE_NEXT(__dlg);
                        //         if (DELEGATE_DONE(__dlg)) goto after else goto body
                        self.push_stmt(
                            header,
                            assign_stmt(
                                &dval,
                                call("__RTS_GEN_DELEGATE_NEXT", vec![ident_expr(&dlg)]),
                            ),
                        );
                        self.set_cond(
                            header,
                            call("__RTS_GEN_DELEGATE_DONE", vec![ident_expr(&dlg)]),
                            after,
                            body,
                        );
                        // body: yield __dlv; volta ao header
                        self.set_yield(body, ident_expr(&dval), header);
                        return Some(after);
                    }
                    let value = y.arg.as_deref().cloned().unwrap_or_else(undef_expr);
                    let next = self.new_state();
                    self.set_yield(cur, value, next);
                    return Some(next);
                }
                // (#211 value-passing) `v = yield E;` (target ident simples,
                // yield nao-delegate): suspende e le SENT na retomada.
                if let Expr::Assign(a) = es.expr.as_ref() {
                    if let Expr::Yield(y) = a.right.as_ref() {
                        if !y.delegate {
                            if let AssignTarget::Simple(SimpleAssignTarget::Ident(id)) = &a.left {
                                let name = id.id.sym.to_string();
                                self.intern_local(&name);
                                let value =
                                    y.arg.as_deref().cloned().unwrap_or_else(undef_expr);
                                let next = self.new_state();
                                self.set_yield(cur, value, next);
                                self.push_stmt(
                                    next,
                                    assign_stmt(
                                        &name,
                                        call("__RTS_GEN_SM_SENT", vec![ident_expr(G)]),
                                    ),
                                );
                                return Some(next);
                            }
                        }
                    }
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
            // (#211 value-passing) `const/let v = yield E;` (1 declarator, yield
            // simples): suspende com E e, na retomada, le o valor passado em
            // `gen.next(v)` via SENT. `yield*` em init segue inelegivel (eager).
            Stmt::Decl(Decl::Var(vd))
                if vd.decls.len() == 1
                    && matches!(&vd.decls[0].name, Pat::Ident(_))
                    && matches!(
                        vd.decls[0].init.as_deref(),
                        Some(Expr::Yield(y)) if !y.delegate
                    ) =>
            {
                let Pat::Ident(id) = &vd.decls[0].name else { unreachable!() };
                let name = id.id.sym.to_string();
                let Some(Expr::Yield(y)) = vd.decls[0].init.as_deref() else { unreachable!() };
                self.intern_local(&name);
                let value = y.arg.as_deref().cloned().unwrap_or_else(undef_expr);
                let next = self.new_state();
                self.set_yield(cur, value, next);
                // estado `next`: name = SENT(__g)
                self.push_stmt(
                    next,
                    assign_stmt(&name, call("__RTS_GEN_SM_SENT", vec![ident_expr(G)])),
                );
                Some(next)
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
                // (#211 try/catch com yield) `try { ...yields... } catch (e) {
                // ...yields... }` SEM finally: `.throw(e)` suspenso na try salta
                // para o catch com `e` ligado. Combina try/catch + finally =>
                // fora desta fatia (bail).
                if let Some(h) = &t.handler {
                    let catch_has_yield = h.body.stmts.iter().any(stmt_has_yield);
                    if catch_has_yield && t.finalizer.is_none() {
                        // param do catch: ident simples (ou ausente).
                        let catch_param = match h.param.as_ref() {
                            Some(swc_ecma_ast::Pat::Ident(id)) => {
                                Some(id.id.sym.to_string())
                            }
                            Some(_) => return None, // destructuring catch => bail
                            None => None,
                        };
                        if let Some(name) = &catch_param {
                            self.intern_local(name);
                        }
                        // Reserva o estado de entrada do catch ANTES do body para
                        // que ENTER_TRY_CATCH aponte para ele.
                        let catch_entry = self.new_state();
                        let after = self.new_state();
                        // cur: ENTER_TRY_CATCH(__g, catch_entry); goto body.
                        self.push_stmt(
                            cur,
                            call_stmt(call(
                                "__RTS_GEN_SM_ENTER_TRY_CATCH",
                                vec![ident_expr(G), num_expr(catch_entry as f64)],
                            )),
                        );
                        let body_entry = self.new_state();
                        self.set_goto(cur, body_entry);
                        let body_exit = self.lower_seq(&t.block.stmts, body_entry)?;
                        // saida normal do body (sem excecao): limpa o catch e segue.
                        self.push_stmt(
                            body_exit,
                            call_stmt(call(
                                "__RTS_GEN_SM_EXIT_TRY_CATCH",
                                vec![ident_expr(G)],
                            )),
                        );
                        self.set_goto(body_exit, after);
                        // catch_entry: e = CAUGHT(__g); corpo do catch.
                        if let Some(name) = &catch_param {
                            self.push_stmt(
                                catch_entry,
                                assign_stmt(
                                    name,
                                    call("__RTS_GEN_SM_CAUGHT", vec![ident_expr(G)]),
                                ),
                            );
                        }
                        let catch_exit = self.lower_seq(&h.body.stmts, catch_entry)?;
                        self.set_goto(catch_exit, after);
                        return Some(after);
                    }
                    // catch com yield + finally, ou catch sem yield mas com
                    // finally tratado abaixo: catch-com-yield restante => bail.
                    if catch_has_yield {
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
                // Um `return` dentro do if (mesmo sem yield) NAO pode ir verbatim:
                // o `return;`/`return X;` cru nao casa com a assinatura i64 da fn
                // de estado (verifier error). Lower em estados ramificados:
                // Cond(test, then, else) e o `return` no then vira DONE.
                let has_return = stmt_has_return(&i.cons)
                    || i.alt.as_deref().map(stmt_has_return).unwrap_or(false);
                if has_return {
                    if expr_has_yield(&i.test) {
                        return None;
                    }
                    let then_entry = self.new_state();
                    let after = self.new_state();
                    let else_entry = if i.alt.is_some() {
                        self.new_state()
                    } else {
                        after
                    };
                    self.set_cond(cur, (*i.test).clone(), then_entry, else_entry);
                    let then_exit = self.lower_stmt(&i.cons, then_entry)?;
                    self.set_goto(then_exit, after);
                    if let Some(alt) = i.alt.as_deref() {
                        let else_exit = self.lower_stmt(alt, else_entry)?;
                        self.set_goto(else_exit, after);
                    }
                    return Some(after);
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
            // (cross-runtime #392) SO' para async generators: loops com await no
            // corpo sao lowerados pelo arm estrutural (For/While/DoWhile), que
            // processa o corpo state-by-state (cada await interno vira suspensao
            // via este mesmo try_lower_await_stmt). Gate em `is_async_gen` pra
            // NAO mudar o caminho de async functions puras (regrediu 365/303/64).
            Stmt::For(_) | Stmt::While(_) | Stmt::DoWhile(_) if self.is_async_gen => {
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

// ── (cross-runtime #365) captura-mutada por closure ──────────────────────────
// Detecta um `let` local do corpo que e' (a) referenciado dentro de uma closure
// aninhada (arrow/fn-expr) E (b) alvo de mutacao (++/--/assign) em qualquer
// lugar. Incompletude tende a "nao detectar" (mais conservador: usa a SM) — um
// falso-negativo nao regride (so' nao corrige); um falso-positivo (bail) cai no
// caminho nao-SM, coberto pela suite.
fn body_has_captured_mutated_local(stmts: &[Stmt]) -> bool {
    use std::collections::HashSet;
    let mut lets: HashSet<String> = HashSet::new();
    for s in stmts {
        collect_let_names(s, &mut lets);
    }
    if lets.is_empty() {
        return false;
    }
    let mut captured: HashSet<String> = HashSet::new();
    let mut mutated: HashSet<String> = HashSet::new();
    for s in stmts {
        scan_caps_muts_stmt(s, false, &lets, &mut captured, &mut mutated);
    }
    lets.iter().any(|n| captured.contains(n) && mutated.contains(n))
}

/// Nomes de `let`/`const` declarados neste escopo (desce em control-flow, NAO em
/// corpos de closure — esses sao escopos proprios).
fn collect_let_names(s: &Stmt, out: &mut std::collections::HashSet<String>) {
    match s {
        Stmt::Decl(Decl::Var(v)) => {
            for d in &v.decls {
                if let Pat::Ident(id) = &d.name {
                    out.insert(id.id.sym.to_string());
                }
            }
        }
        Stmt::If(i) => {
            collect_let_names(&i.cons, out);
            if let Some(a) = i.alt.as_deref() {
                collect_let_names(a, out);
            }
        }
        Stmt::Block(b) => b.stmts.iter().for_each(|s| collect_let_names(s, out)),
        Stmt::While(w) => collect_let_names(&w.body, out),
        Stmt::DoWhile(w) => collect_let_names(&w.body, out),
        Stmt::For(f) => {
            if let Some(swc_ecma_ast::VarDeclOrExpr::VarDecl(vd)) = f.init.as_ref() {
                for d in &vd.decls {
                    if let Pat::Ident(id) = &d.name {
                        out.insert(id.id.sym.to_string());
                    }
                }
            }
            collect_let_names(&f.body, out);
        }
        Stmt::ForOf(f) => collect_let_names(&f.body, out),
        Stmt::ForIn(f) => collect_let_names(&f.body, out),
        Stmt::Try(t) => {
            t.block.stmts.iter().for_each(|s| collect_let_names(s, out));
            if let Some(h) = &t.handler {
                h.body.stmts.iter().for_each(|s| collect_let_names(s, out));
            }
            if let Some(fin) = &t.finalizer {
                fin.stmts.iter().for_each(|s| collect_let_names(s, out));
            }
        }
        _ => {}
    }
}

fn scan_caps_muts_stmt(
    s: &Stmt,
    in_closure: bool,
    lets: &std::collections::HashSet<String>,
    cap: &mut std::collections::HashSet<String>,
    mutd: &mut std::collections::HashSet<String>,
) {
    match s {
        Stmt::Expr(e) => scan_caps_muts_expr(&e.expr, in_closure, lets, cap, mutd),
        Stmt::Return(r) => {
            if let Some(a) = r.arg.as_deref() {
                scan_caps_muts_expr(a, in_closure, lets, cap, mutd);
            }
        }
        Stmt::Decl(Decl::Var(v)) => {
            for d in &v.decls {
                if let Some(e) = d.init.as_deref() {
                    scan_caps_muts_expr(e, in_closure, lets, cap, mutd);
                }
            }
        }
        Stmt::If(i) => {
            scan_caps_muts_expr(&i.test, in_closure, lets, cap, mutd);
            scan_caps_muts_stmt(&i.cons, in_closure, lets, cap, mutd);
            if let Some(a) = i.alt.as_deref() {
                scan_caps_muts_stmt(a, in_closure, lets, cap, mutd);
            }
        }
        Stmt::Block(b) => b
            .stmts
            .iter()
            .for_each(|s| scan_caps_muts_stmt(s, in_closure, lets, cap, mutd)),
        Stmt::While(w) => {
            scan_caps_muts_expr(&w.test, in_closure, lets, cap, mutd);
            scan_caps_muts_stmt(&w.body, in_closure, lets, cap, mutd);
        }
        Stmt::DoWhile(w) => {
            scan_caps_muts_expr(&w.test, in_closure, lets, cap, mutd);
            scan_caps_muts_stmt(&w.body, in_closure, lets, cap, mutd);
        }
        Stmt::For(f) => {
            if let Some(swc_ecma_ast::VarDeclOrExpr::VarDecl(vd)) = f.init.as_ref() {
                for d in &vd.decls {
                    if let Some(e) = d.init.as_deref() {
                        scan_caps_muts_expr(e, in_closure, lets, cap, mutd);
                    }
                }
            } else if let Some(swc_ecma_ast::VarDeclOrExpr::Expr(e)) = f.init.as_ref() {
                scan_caps_muts_expr(e, in_closure, lets, cap, mutd);
            }
            if let Some(t) = f.test.as_deref() {
                scan_caps_muts_expr(t, in_closure, lets, cap, mutd);
            }
            if let Some(u) = f.update.as_deref() {
                scan_caps_muts_expr(u, in_closure, lets, cap, mutd);
            }
            scan_caps_muts_stmt(&f.body, in_closure, lets, cap, mutd);
        }
        Stmt::ForOf(f) => {
            scan_caps_muts_expr(&f.right, in_closure, lets, cap, mutd);
            scan_caps_muts_stmt(&f.body, in_closure, lets, cap, mutd);
        }
        Stmt::ForIn(f) => {
            scan_caps_muts_expr(&f.right, in_closure, lets, cap, mutd);
            scan_caps_muts_stmt(&f.body, in_closure, lets, cap, mutd);
        }
        Stmt::Try(t) => {
            t.block
                .stmts
                .iter()
                .for_each(|s| scan_caps_muts_stmt(s, in_closure, lets, cap, mutd));
            if let Some(h) = &t.handler {
                h.body
                    .stmts
                    .iter()
                    .for_each(|s| scan_caps_muts_stmt(s, in_closure, lets, cap, mutd));
            }
            if let Some(fin) = &t.finalizer {
                fin.stmts
                    .iter()
                    .for_each(|s| scan_caps_muts_stmt(s, in_closure, lets, cap, mutd));
            }
        }
        Stmt::Throw(t) => scan_caps_muts_expr(&t.arg, in_closure, lets, cap, mutd),
        Stmt::Switch(sw) => {
            scan_caps_muts_expr(&sw.discriminant, in_closure, lets, cap, mutd);
            for c in &sw.cases {
                if let Some(t) = &c.test {
                    scan_caps_muts_expr(t, in_closure, lets, cap, mutd);
                }
                c.cons
                    .iter()
                    .for_each(|s| scan_caps_muts_stmt(s, in_closure, lets, cap, mutd));
            }
        }
        _ => {}
    }
}

fn scan_caps_muts_expr(
    e: &Expr,
    in_closure: bool,
    lets: &std::collections::HashSet<String>,
    cap: &mut std::collections::HashSet<String>,
    mutd: &mut std::collections::HashSet<String>,
) {
    match e {
        Expr::Ident(id) => {
            if in_closure {
                let n = id.sym.to_string();
                if lets.contains(&n) {
                    cap.insert(n);
                }
            }
        }
        Expr::Update(u) => {
            if let Expr::Ident(id) = u.arg.as_ref() {
                let n = id.sym.to_string();
                if lets.contains(&n) {
                    mutd.insert(n);
                }
            }
            scan_caps_muts_expr(&u.arg, in_closure, lets, cap, mutd);
        }
        Expr::Assign(a) => {
            if let swc_ecma_ast::AssignTarget::Simple(
                swc_ecma_ast::SimpleAssignTarget::Ident(id),
            ) = &a.left
            {
                let n = id.id.sym.to_string();
                if lets.contains(&n) {
                    mutd.insert(n);
                }
            }
            if let swc_ecma_ast::AssignTarget::Simple(
                swc_ecma_ast::SimpleAssignTarget::Member(m),
            ) = &a.left
            {
                scan_caps_muts_expr(&m.obj, in_closure, lets, cap, mutd);
            }
            scan_caps_muts_expr(&a.right, in_closure, lets, cap, mutd);
        }
        Expr::Arrow(arrow) => match arrow.body.as_ref() {
            swc_ecma_ast::BlockStmtOrExpr::BlockStmt(b) => b
                .stmts
                .iter()
                .for_each(|s| scan_caps_muts_stmt(s, true, lets, cap, mutd)),
            swc_ecma_ast::BlockStmtOrExpr::Expr(ex) => {
                scan_caps_muts_expr(ex, true, lets, cap, mutd)
            }
        },
        Expr::Fn(fnx) => {
            if let Some(b) = &fnx.function.body {
                b.stmts
                    .iter()
                    .for_each(|s| scan_caps_muts_stmt(s, true, lets, cap, mutd));
            }
        }
        Expr::Call(c) => {
            if let Callee::Expr(ce) = &c.callee {
                scan_caps_muts_expr(ce, in_closure, lets, cap, mutd);
            }
            for a in &c.args {
                scan_caps_muts_expr(&a.expr, in_closure, lets, cap, mutd);
            }
        }
        Expr::New(n) => {
            scan_caps_muts_expr(&n.callee, in_closure, lets, cap, mutd);
            if let Some(args) = &n.args {
                for a in args {
                    scan_caps_muts_expr(&a.expr, in_closure, lets, cap, mutd);
                }
            }
        }
        Expr::Member(m) => {
            scan_caps_muts_expr(&m.obj, in_closure, lets, cap, mutd);
            if let swc_ecma_ast::MemberProp::Computed(c) = &m.prop {
                scan_caps_muts_expr(&c.expr, in_closure, lets, cap, mutd);
            }
        }
        Expr::Bin(b) => {
            scan_caps_muts_expr(&b.left, in_closure, lets, cap, mutd);
            scan_caps_muts_expr(&b.right, in_closure, lets, cap, mutd);
        }
        Expr::Unary(u) => scan_caps_muts_expr(&u.arg, in_closure, lets, cap, mutd),
        Expr::Paren(p) => scan_caps_muts_expr(&p.expr, in_closure, lets, cap, mutd),
        Expr::Await(a) => scan_caps_muts_expr(&a.arg, in_closure, lets, cap, mutd),
        Expr::Yield(y) => {
            if let Some(arg) = &y.arg {
                scan_caps_muts_expr(arg, in_closure, lets, cap, mutd);
            }
        }
        Expr::Cond(c) => {
            scan_caps_muts_expr(&c.test, in_closure, lets, cap, mutd);
            scan_caps_muts_expr(&c.cons, in_closure, lets, cap, mutd);
            scan_caps_muts_expr(&c.alt, in_closure, lets, cap, mutd);
        }
        Expr::Seq(s) => s
            .exprs
            .iter()
            .for_each(|x| scan_caps_muts_expr(x, in_closure, lets, cap, mutd)),
        Expr::Tpl(t) => t
            .exprs
            .iter()
            .for_each(|x| scan_caps_muts_expr(x, in_closure, lets, cap, mutd)),
        Expr::Array(a) => {
            for el in a.elems.iter().flatten() {
                scan_caps_muts_expr(&el.expr, in_closure, lets, cap, mutd);
            }
        }
        Expr::Object(o) => {
            for p in &o.props {
                if let swc_ecma_ast::PropOrSpread::Prop(prop) = p {
                    if let swc_ecma_ast::Prop::KeyValue(kv) = prop.as_ref() {
                        scan_caps_muts_expr(&kv.value, in_closure, lets, cap, mutd);
                    }
                }
            }
        }
        Expr::TsAs(a) => scan_caps_muts_expr(&a.expr, in_closure, lets, cap, mutd),
        Expr::TsNonNull(a) => scan_caps_muts_expr(&a.expr, in_closure, lets, cap, mutd),
        Expr::OptChain(o) => {
            if let swc_ecma_ast::OptChainBase::Member(m) = o.base.as_ref() {
                scan_caps_muts_expr(&m.obj, in_closure, lets, cap, mutd);
            } else if let swc_ecma_ast::OptChainBase::Call(c) = o.base.as_ref() {
                scan_caps_muts_expr(&c.callee, in_closure, lets, cap, mutd);
                for a in &c.args {
                    scan_caps_muts_expr(&a.expr, in_closure, lets, cap, mutd);
                }
            }
        }
        _ => {}
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

/// True se o stmt contem um `yield*` (delegate) em qualquer profundidade. A
/// delegacao pode apontar para uma fonte lazy/infinita, entao exige a SM (o
/// eager-buffer materializaria a fonte inteira).
fn stmt_has_yield_star(s: &Stmt) -> bool {
    fn expr_has_ys(e: &Expr) -> bool {
        match e {
            Expr::Yield(y) => y.delegate || y.arg.as_deref().map(expr_has_ys).unwrap_or(false),
            Expr::Assign(a) => expr_has_ys(&a.right),
            Expr::Bin(b) => expr_has_ys(&b.left) || expr_has_ys(&b.right),
            Expr::Unary(u) => expr_has_ys(&u.arg),
            Expr::Paren(p) => expr_has_ys(&p.expr),
            Expr::Cond(c) => expr_has_ys(&c.test) || expr_has_ys(&c.cons) || expr_has_ys(&c.alt),
            Expr::Seq(s) => s.exprs.iter().any(|x| expr_has_ys(x)),
            Expr::Call(c) => c.args.iter().any(|a| expr_has_ys(&a.expr)),
            _ => false,
        }
    }
    match s {
        Stmt::Expr(e) => expr_has_ys(&e.expr),
        Stmt::Block(b) => b.stmts.iter().any(stmt_has_yield_star),
        Stmt::If(i) => {
            stmt_has_yield_star(&i.cons)
                || i.alt.as_deref().map(stmt_has_yield_star).unwrap_or(false)
        }
        Stmt::While(w) => stmt_has_yield_star(&w.body),
        Stmt::DoWhile(d) => stmt_has_yield_star(&d.body),
        Stmt::For(f) => stmt_has_yield_star(&f.body),
        Stmt::ForOf(fo) => stmt_has_yield_star(&fo.body),
        Stmt::ForIn(fi) => stmt_has_yield_star(&fi.body),
        Stmt::Decl(Decl::Var(vd)) => vd
            .decls
            .iter()
            .any(|d| d.init.as_deref().map(expr_has_ys).unwrap_or(false)),
        _ => false,
    }
}

/// True se o stmt usa o VALOR de um `yield` (`const v = yield E` / `v = yield
/// E`) — value-passing exige suspensao real (o valor vem de um `next(v)`
/// posterior); o eager-buffer nao consegue (roda ate o fim, v=undefined).
fn stmt_has_value_yield(s: &Stmt) -> bool {
    match s {
        Stmt::Decl(Decl::Var(vd)) => vd.decls.iter().any(|d| {
            matches!(d.init.as_deref(), Some(Expr::Yield(y)) if !y.delegate)
        }),
        Stmt::Expr(e) => matches!(
            e.expr.as_ref(),
            Expr::Assign(a) if matches!(a.right.as_ref(), Expr::Yield(y) if !y.delegate)
        ),
        Stmt::Block(b) => b.stmts.iter().any(stmt_has_value_yield),
        Stmt::If(i) => {
            stmt_has_value_yield(&i.cons)
                || i.alt.as_deref().map(stmt_has_value_yield).unwrap_or(false)
        }
        Stmt::While(w) => stmt_has_value_yield(&w.body),
        Stmt::DoWhile(d) => stmt_has_value_yield(&d.body),
        Stmt::For(f) => stmt_has_value_yield(&f.body),
        Stmt::ForOf(fo) => stmt_has_value_yield(&fo.body),
        Stmt::Try(t) => {
            t.block.stmts.iter().any(stmt_has_value_yield)
                || t.handler.as_ref().map(|h| h.body.stmts.iter().any(stmt_has_value_yield)).unwrap_or(false)
                || t.finalizer.as_ref().map(|f| f.stmts.iter().any(stmt_has_value_yield)).unwrap_or(false)
        }
        _ => false,
    }
}

fn stmt_needs_lazy(s: &Stmt) -> bool {
    // `yield*` (delegate) sempre exige SM (fonte potencialmente lazy/infinita).
    if stmt_has_yield_star(s) {
        return true;
    }
    // value-passing `const v = yield E` exige SM (suspende, le SENT na retomada).
    if stmt_has_value_yield(s) {
        return true;
    }
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

/// True se o stmt contem um `return` em qualquer profundidade (sem descer em
/// fns/arrows aninhadas, que tem seu proprio return).
fn stmt_has_return(s: &Stmt) -> bool {
    match s {
        Stmt::Return(_) => true,
        Stmt::Block(b) => b.stmts.iter().any(stmt_has_return),
        Stmt::If(i) => {
            stmt_has_return(&i.cons) || i.alt.as_deref().map(stmt_has_return).unwrap_or(false)
        }
        Stmt::While(w) => stmt_has_return(&w.body),
        Stmt::DoWhile(d) => stmt_has_return(&d.body),
        Stmt::For(f) => stmt_has_return(&f.body),
        Stmt::ForOf(fo) => stmt_has_return(&fo.body),
        Stmt::ForIn(fi) => stmt_has_return(&fi.body),
        Stmt::Try(t) => {
            t.block.stmts.iter().any(stmt_has_return)
                || t.handler.as_ref().map(|h| h.body.stmts.iter().any(stmt_has_return)).unwrap_or(false)
                || t.finalizer.as_ref().map(|f| f.stmts.iter().any(stmt_has_return)).unwrap_or(false)
        }
        Stmt::Labeled(l) => stmt_has_return(&l.body),
        Stmt::Switch(sw) => sw.cases.iter().any(|c| c.cons.iter().any(stmt_has_return)),
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
    param_names: &[String],
    ctor_params: &[Param],
    state_fn_name: &str,
    nslots: f64,
    new_fn: &str,
) -> FnDecl {
    let mut stmts: Vec<Stmt> = Vec::new();
    // const __g = <new_fn>(<state_fn>, nslots);  (sync: __RTS_GEN_SM_NEW;
    // async gen: __RTS_AGEN_NEW)
    stmts.push(let_decl(
        G,
        call(new_fn, vec![ident_expr(state_fn_name), num_expr(nslots)]),
    ));
    // escreve params nos slots 0..n (pelo NOME — o default ja' foi aplicado
    // ao binding pelo expand_default_args sobre a assinatura do ctor).
    for (i, p) in param_names.iter().enumerate() {
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

    // Assinatura do ctor preserva os Param originais (com defaults).
    make_fn_decl_params(name, ctor_params.to_vec(), stmts)
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
                // Computa o VALOR do yield ANTES de persistir os locais: a expr
                // pode mutar um local (`yield n++`), e esse efeito tem de entrar
                // no frame salvo — senao a retomada recarrega o valor pre-mutacao
                // (bug: `while(true) yield n++` rendia 1,1,1 em vez de 1,2,3).
                blk.push(let_decl("__gen_yv", value.clone()));
                // salva todos os locais no frame (apos os efeitos do valor)
                blk.extend(store_all_locals(builder));
                // SETSTATE(__g, next)
                blk.push(call_stmt(call(
                    "__RTS_GEN_SM_SETSTATE",
                    vec![ident_expr(G), num_expr(*next as f64)],
                )));
                // return __RTS_GEN_SM_YIELD(__g, __gen_yv)
                blk.push(Stmt::Return(swc_ecma_ast::ReturnStmt {
                    span: sp(),
                    arg: Some(Box::new(call(
                        "__RTS_GEN_SM_YIELD",
                        vec![ident_expr(G), ident_expr("__gen_yv")],
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
                // async gen (is_async_gen): Done usa GEN_SM_DONE (.next resolve
                // {value:ret,done:true}); async fn pura: ASYNC_SM_RESOLVE.
                let term = if builder.is_async && !builder.is_async_gen {
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
    let final_term = if builder.is_async && !builder.is_async_gen {
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

/// Variante que recebe os `Param` swc originais (preserva defaults na
/// assinatura — usado pelo constructor do generator).
fn make_fn_decl_params(name: &str, params: Vec<Param>, stmts: Vec<Stmt>) -> FnDecl {
    FnDecl {
        ident: ident(name),
        declare: false,
        function: Box::new(Function {
            params,
            decorators: Vec::new(),
            span: sp(),
            ctxt: Default::default(),
            body: Some(BlockStmt { span: sp(), ctxt: Default::default(), stmts }),
            is_generator: false,
            is_async: false,
            type_params: None,
            return_type: None,
        }),
    }
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
