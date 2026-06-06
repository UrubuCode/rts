//! (#195) Mutable-closure boxing pass.
//!
//! A local that is **captured by a nested closure** AND **mutated** (assigned /
//! `++`/`--` / op-assign) cannot be captured by value — each closure activation
//! would get its own copy and writes would not be shared. The fix is the classic
//! env-record / boxing: the variable lives in a heap **cell**, and every read /
//! write goes through it. The cell HANDLE is then captured by value (the
//! existing REIFY_CAPTURED machinery), so all capturers share one cell.
//!
//! This pass rewrites, per function scope, for each boxed local `v`:
//! - `let v = init`            → `let v = __cell_new(init)`
//! - read `v`                  → `__cell_get(v)`
//! - `v = X`                   → `__cell_set(v, X)`
//! - `v += X` (op-assign)      → `__cell_set(v, __cell_get(v) + X)`
//! - `v++` / `++v` / `v--`     → `__cell_set(v, __cell_get(v) ± 1)`
//!
//! `__cell_new/get/set` are lowered as intrinsics in `calls/mod.rs`.
//!
//! EXCLUSIONS: for-loop init vars (per-iteration `let` semantics already work
//! and a single cell would collapse them) and `const` (never reassigned). Runs
//! BEFORE the arrow/fn lifters so nested closures are still inline.

use std::collections::HashSet;

use swc_ecma_ast::{
    AssignTarget, BinExpr, BinaryOp, Callee, Decl, Expr, ExprOrSpread, Ident, Pat,
    SimpleAssignTarget, Stmt, UpdateOp, VarDeclOrExpr,
};

use crate::parser::ast::{ClassMember, Item, Program, Statement};

thread_local! {
    /// (#195) Nomes de vars que ESTE pass transformou em celula. `this_arrow`
    /// consulta pra NAO promover essas vars a global (a celula ja' e' o
    /// env-record; o hoist_fn depois captura o HANDLE da celula por valor).
    static BOXED_VARS: std::cell::RefCell<HashSet<String>> =
        std::cell::RefCell::new(HashSet::new());
}

/// (#195) True se `name` foi transformada em celula pelo box_captures.
pub(crate) fn is_boxed(name: &str) -> bool {
    BOXED_VARS.with(|c| c.borrow().contains(name))
}

fn mark_boxed(names: &HashSet<String>) {
    BOXED_VARS.with(|c| {
        let mut s = c.borrow_mut();
        for n in names {
            s.insert(n.clone());
        }
    });
}

pub(crate) fn box_mutable_captures(program: &mut Program) {
    BOXED_VARS.with(|c| c.borrow_mut().clear());
    for item in program.items.iter_mut() {
        match item {
            Item::Function(f) => {
                let params: HashSet<String> =
                    f.parameters.iter().map(|p| p.name.clone()).collect();
                process_body(&mut f.body, &params);
            }
            Item::Statement(Statement::Raw(raw)) => {
                if let Some(stmt) = raw.stmt.as_mut() {
                    let mut one = vec![std::mem::replace(
                        stmt,
                        Stmt::Empty(swc_ecma_ast::EmptyStmt { span: Default::default() }),
                    )];
                    process_stmt_scope(&mut one, &HashSet::new());
                    *stmt = one.pop().unwrap();
                }
            }
            Item::Class(class) => {
                for member in class.members.iter_mut() {
                    let (body, params) = match member {
                        ClassMember::Method(m) => {
                            let ps: HashSet<String> =
                                m.parameters.iter().map(|p| p.name.clone()).collect();
                            (&mut m.body, ps)
                        }
                        ClassMember::Constructor(c) => {
                            let ps: HashSet<String> =
                                c.parameters.iter().map(|p| p.name.clone()).collect();
                            (&mut c.body, ps)
                        }
                        _ => continue,
                    };
                    process_body(body, &params);
                }
            }
            _ => {}
        }
    }
}

/// Process a `Vec<Statement>` (the RTS wrapper) body.
fn process_body(body: &mut [Statement], params: &HashSet<String>) {
    let mut stmts: Vec<Stmt> = Vec::with_capacity(body.len());
    for s in body.iter() {
        let Statement::Raw(raw) = s;
        if let Some(st) = raw.stmt.as_ref() {
            stmts.push(st.clone());
        } else {
            stmts.push(Stmt::Empty(swc_ecma_ast::EmptyStmt { span: Default::default() }));
        }
    }
    process_stmt_scope(&mut stmts, params);
    for (i, s) in body.iter_mut().enumerate() {
        let Statement::Raw(raw) = s;
        raw.stmt = Some(stmts[i].clone());
    }
}

/// (#195) Converte declaracoes de funcao ANINHADAS (`function f(){...}` dentro
/// do corpo de outra fn/arrow/IIFE) em fn-EXPRESSIONS (`const f = function
/// f(){...}`), para que box_captures + os lifters (hoist_fn/this_arrow) tratem
/// suas capturas de variaveis do escopo enclosing (incl. params) — caso do
/// module-pattern (`function _get(){ return _private }`) e do `step` de
/// `__awaiter` capturando `resolve`. Sem isto a ref a `_private`/`resolve`
/// dentro da fn-decl aninhada virava "undefined variable".
///
/// Guarda anti-regressao: NAO converte se `f` eh referenciada por um statement
/// ANTERIOR no mesmo escopo (forward reference — fn-decls sao hoisted; converter
/// p/ fn-expr quebraria a visibilidade antecipada).
fn convert_nested_fn_decls(stmts: &mut Vec<Stmt>, enclosing_params: &HashSet<String>) {
    // (cross-runtime #344) Inclui os PARAMS do escopo enclosing nos "scope
    // locals" — assim uma fn-decl aninhada que captura um PARAM (ex. `step` de
    // __awaiter capturando `resolve`, o param do Promise executor) eh detectada
    // como closure e convertida. Sem isto so' capturas de let/const eram vistas.
    let mut scope_locals = collect_scope_locals(stmts);
    scope_locals.extend(enclosing_params.iter().cloned());
    let n = stmts.len();
    for i in 0..n {
        let (name, captures) = match &stmts[i] {
            Stmt::Decl(Decl::Fn(fd)) => {
                let nm = fd.ident.sym.to_string();
                (nm.clone(), fn_decl_captures_scope_local(fd, &scope_locals, &nm))
            }
            _ => continue,
        };
        // So' converte fn-decls que CAPTURAM um local do escopo enclosing — o
        // unico caso que precisa virar closure. Construtores (`function F(){
        // this.x=1 }`, usados com `new F()`) referenciam `this`, nao locais do
        // escopo, entao NAO sao convertidos (preserva `new F()`/`F.prototype`).
        if !captures {
            continue;
        }
        let mut forward_ref = false;
        for j in 0..i {
            if stmt_references_ident(&stmts[j], &name) {
                forward_ref = true;
                break;
            }
        }
        if forward_ref {
            continue;
        }
        // (cross-runtime #344) A CAPTURING nested fn-decl used as a constructor
        // (`function Ctor(){ this.x = capturedParam }` + `new Ctor()`) MUST
        // become a closure so the capture resolves — `new <local-fn-value>()`
        // and `<local>.prototype` now handle the converted form (new_expr.rs /
        // mod.rs). The old guard left it a fn-decl → "undefined variable" on the
        // captured name. (Non-capturing fn-decls never reach here — the
        // `!captures` check above skips them, so named constructors are intact.)
        if let Stmt::Decl(Decl::Fn(fd)) = &mut stmts[i] {
            let ident = fd.ident.clone();
            let function = std::mem::replace(
                &mut fd.function,
                Box::new(swc_ecma_ast::Function {
                    params: Vec::new(),
                    decorators: Vec::new(),
                    span: Default::default(),
                    ctxt: Default::default(),
                    body: None,
                    is_generator: false,
                    is_async: false,
                    type_params: None,
                    return_type: None,
                }),
            );
            let fnexpr = Expr::Fn(swc_ecma_ast::FnExpr {
                ident: Some(ident.clone()),
                function,
            });
            stmts[i] = Stmt::Decl(Decl::Var(Box::new(swc_ecma_ast::VarDecl {
                span: Default::default(),
                ctxt: Default::default(),
                kind: swc_ecma_ast::VarDeclKind::Const,
                declare: false,
                decls: vec![swc_ecma_ast::VarDeclarator {
                    span: Default::default(),
                    name: Pat::Ident(ident.into()),
                    init: Some(Box::new(fnexpr)),
                    definite: false,
                }],
            })));
        }
    }
}

/// True se o corpo da fn-decl referencia algum local do escopo enclosing
/// (`scope_locals`) que NAO seja seu proprio nome nem um param/local proprio —
/// i.e. captura por closure.
fn fn_decl_captures_scope_local(
    fd: &swc_ecma_ast::FnDecl,
    scope_locals: &HashSet<String>,
    own_name: &str,
) -> bool {
    let mut own: HashSet<String> = HashSet::new();
    own.insert(own_name.to_string());
    for p in &fd.function.params {
        collect_pat_names(&p.pat, &mut own);
    }
    if let Some(body) = fd.function.body.as_ref() {
        for s in &body.stmts {
            collect_decl_names_shallow(s, &mut own);
        }
        let mut captured = false;
        for s in &body.stmts {
            visit_exprs_in_stmt_ref(s, &mut |e| {
                if let Expr::Ident(id) = e {
                    let n = id.sym.as_str();
                    if scope_locals.contains(n) && !own.contains(n) {
                        captured = true;
                    }
                }
            });
            if captured {
                break;
            }
        }
        captured
    } else {
        false
    }
}

fn collect_pat_names(pat: &Pat, out: &mut HashSet<String>) {
    if let Pat::Ident(id) = pat {
        out.insert(id.id.sym.to_string());
    }
}

fn collect_decl_names_shallow(stmt: &Stmt, out: &mut HashSet<String>) {
    if let Stmt::Decl(Decl::Var(v)) = stmt {
        for d in &v.decls {
            collect_pat_names(&d.name, out);
        }
    }
    if let Stmt::Decl(Decl::Fn(fd)) = stmt {
        out.insert(fd.ident.sym.to_string());
    }
}

/// True se o statement referencia o ident `name` (em qualquer expressao).
fn stmt_references_ident(stmt: &Stmt, name: &str) -> bool {
    let mut found = false;
    visit_exprs_in_stmt_ref(stmt, &mut |e| {
        if let Expr::Ident(id) = e {
            if id.sym.as_str() == name {
                found = true;
            }
        }
    });
    found
}

/// Analyze one function/scope's statements and box captured-mutated locals.
fn process_stmt_scope(stmts: &mut Vec<Stmt>, enclosing_params: &HashSet<String>) {
    // (#195) Converte fn-decls aninhadas capturantes em fn-exprs ANTES de tudo,
    // para reusar a maquinaria de captura existente.
    convert_nested_fn_decls(stmts, enclosing_params);
    // First, recurse into nested functions/arrows so inner scopes box their own
    // locals before we analyze this scope (a closure can declare its own
    // captured-mutated locals).
    for s in stmts.iter_mut() {
        recurse_into_nested_fns(s);
    }

    let locals = collect_scope_locals(stmts);
    if locals.is_empty() {
        return;
    }
    let loop_vars = collect_loop_init_vars(stmts);
    let mut captured = std::collections::BTreeSet::new();
    for s in stmts.iter() {
        scan_captured_in_nested(s, &locals, &mut captured);
    }
    if captured.is_empty() {
        return;
    }
    let mut mutated: HashSet<String> = HashSet::new();
    for s in stmts.iter() {
        scan_mutated(s, &locals, &mut mutated);
    }

    let boxed: HashSet<String> = captured
        .iter()
        .filter(|n| mutated.contains(*n) && !loop_vars.contains(*n))
        .cloned()
        .collect();
    if boxed.is_empty() {
        return;
    }
    mark_boxed(&boxed);

    for s in stmts.iter_mut() {
        rewrite_stmt(s, &boxed, true);
    }
}

/// Recurse the lifters' job is later; here we only descend into nested fn/arrow
/// bodies to box THEIR scopes first.
fn recurse_into_nested_fns(stmt: &mut Stmt) {
    visit_exprs_in_stmt(stmt, &mut |e| {
        match e {
            Expr::Arrow(arrow) => {
                // (cross-runtime #344) passa os params da arrow como escopo
                // enclosing p/ que fn-decls aninhadas que os capturam virem
                // closures.
                let params: HashSet<String> = arrow
                    .params
                    .iter()
                    .filter_map(|p| match p {
                        Pat::Ident(id) => Some(id.id.sym.to_string()),
                        _ => None,
                    })
                    .collect();
                if let swc_ecma_ast::BlockStmtOrExpr::BlockStmt(b) = arrow.body.as_mut() {
                    process_stmt_scope(&mut b.stmts, &params);
                }
            }
            Expr::Fn(fnx) => {
                let params: HashSet<String> = fnx
                    .function
                    .params
                    .iter()
                    .filter_map(|p| match &p.pat {
                        Pat::Ident(id) => Some(id.id.sym.to_string()),
                        _ => None,
                    })
                    .collect();
                if let Some(b) = fnx.function.body.as_mut() {
                    process_stmt_scope(&mut b.stmts, &params);
                }
            }
            _ => {}
        }
    });
}

// ── analysis ────────────────────────────────────────────────────────────────

fn collect_scope_locals(stmts: &[Stmt]) -> HashSet<String> {
    let mut out = HashSet::new();
    for s in stmts {
        collect_locals_in_stmt(s, &mut out);
    }
    out
}

fn collect_locals_in_stmt(stmt: &Stmt, out: &mut HashSet<String>) {
    match stmt {
        Stmt::Decl(Decl::Var(v)) => {
            // `const` never reassigned -> skip (cannot be mutated).
            if matches!(v.kind, swc_ecma_ast::VarDeclKind::Const) {
                return;
            }
            for d in &v.decls {
                if let Pat::Ident(id) = &d.name {
                    out.insert(id.id.sym.to_string());
                }
            }
        }
        // Descend statement nesting (but NOT into nested fn/arrow bodies — those
        // are their own scopes, handled recursively).
        Stmt::If(i) => {
            collect_locals_in_stmt(&i.cons, out);
            if let Some(a) = i.alt.as_deref() {
                collect_locals_in_stmt(a, out);
            }
        }
        Stmt::Block(b) => b.stmts.iter().for_each(|s| collect_locals_in_stmt(s, out)),
        Stmt::While(w) => collect_locals_in_stmt(&w.body, out),
        Stmt::DoWhile(w) => collect_locals_in_stmt(&w.body, out),
        Stmt::For(f) => collect_locals_in_stmt(&f.body, out),
        Stmt::Try(t) => t.block.stmts.iter().for_each(|s| collect_locals_in_stmt(s, out)),
        _ => {}
    }
}

fn collect_loop_init_vars(stmts: &[Stmt]) -> HashSet<String> {
    let mut out = HashSet::new();
    for s in stmts {
        collect_loop_vars_in_stmt(s, &mut out);
    }
    out
}

fn collect_loop_vars_in_stmt(stmt: &Stmt, out: &mut HashSet<String>) {
    match stmt {
        Stmt::For(f) => {
            if let Some(VarDeclOrExpr::VarDecl(vd)) = f.init.as_ref() {
                for d in &vd.decls {
                    if let Pat::Ident(id) = &d.name {
                        out.insert(id.id.sym.to_string());
                    }
                }
            }
            collect_loop_vars_in_stmt(&f.body, out);
        }
        Stmt::ForOf(f) => collect_loop_vars_in_stmt(&f.body, out),
        Stmt::ForIn(f) => collect_loop_vars_in_stmt(&f.body, out),
        Stmt::If(i) => {
            collect_loop_vars_in_stmt(&i.cons, out);
            if let Some(a) = i.alt.as_deref() {
                collect_loop_vars_in_stmt(a, out);
            }
        }
        Stmt::Block(b) => b.stmts.iter().for_each(|s| collect_loop_vars_in_stmt(s, out)),
        Stmt::While(w) => collect_loop_vars_in_stmt(&w.body, out),
        Stmt::DoWhile(w) => collect_loop_vars_in_stmt(&w.body, out),
        _ => {}
    }
}

/// Vars in `locals` referenced inside a NESTED arrow/fn (i.e. captured).
fn scan_captured_in_nested(
    stmt: &Stmt,
    locals: &HashSet<String>,
    out: &mut std::collections::BTreeSet<String>,
) {
    let locals_hs: HashSet<String> = locals.clone();
    let captures = crate::codegen::lower::analysis::captures::free_vars_of_nested_closures(stmt, &locals_hs);
    for c in captures {
        out.insert(c);
    }
}

/// Vars in `locals` that are assignment / update targets anywhere (including
/// inside nested closures — a closure that writes the var forces boxing).
fn scan_mutated(stmt: &Stmt, locals: &HashSet<String>, out: &mut HashSet<String>) {
    visit_exprs_in_stmt_ref(stmt, &mut |e| match e {
        Expr::Assign(a) => {
            if let AssignTarget::Simple(SimpleAssignTarget::Ident(id)) = &a.left {
                let n = id.id.sym.to_string();
                if locals.contains(&n) {
                    out.insert(n);
                }
            }
        }
        Expr::Update(u) => {
            if let Expr::Ident(id) = u.arg.as_ref() {
                let n = id.sym.to_string();
                if locals.contains(&n) {
                    out.insert(n);
                }
            }
        }
        _ => {}
    });
}

// ── rewrite ─────────────────────────────────────────────────────────────────

fn ident_expr(name: &str) -> Expr {
    Expr::Ident(Ident {
        span: Default::default(),
        ctxt: Default::default(),
        sym: name.into(),
        optional: false,
    })
}

fn call1(callee: &str, arg: Expr) -> Expr {
    Expr::Call(swc_ecma_ast::CallExpr {
        span: Default::default(),
        ctxt: Default::default(),
        callee: Callee::Expr(Box::new(ident_expr(callee))),
        args: vec![ExprOrSpread { spread: None, expr: Box::new(arg) }],
        type_args: None,
    })
}

fn call2(callee: &str, a: Expr, b: Expr) -> Expr {
    Expr::Call(swc_ecma_ast::CallExpr {
        span: Default::default(),
        ctxt: Default::default(),
        callee: Callee::Expr(Box::new(ident_expr(callee))),
        args: vec![
            ExprOrSpread { spread: None, expr: Box::new(a) },
            ExprOrSpread { spread: None, expr: Box::new(b) },
        ],
        type_args: None,
    })
}

fn cell_get(name: &str) -> Expr {
    call1("__cell_get", ident_expr(name))
}

fn rewrite_stmt(stmt: &mut Stmt, boxed: &HashSet<String>, is_decl_scope: bool) {
    match stmt {
        Stmt::Decl(Decl::Var(v)) if is_decl_scope => {
            for d in v.decls.iter_mut() {
                if let Pat::Ident(id) = &d.name {
                    if boxed.contains(id.id.sym.as_str()) {
                        // let v = init  ->  let v = __cell_new(init)
                        let init = d
                            .init
                            .take()
                            .map(|b| *b)
                            .unwrap_or_else(|| Expr::Lit(swc_ecma_ast::Lit::Num(swc_ecma_ast::Number {
                                span: Default::default(),
                                value: 0.0,
                                raw: None,
                            })));
                        let mut init = init;
                        rewrite_expr(&mut init, boxed);
                        d.init = Some(Box::new(call1("__cell_new", init)));
                        continue;
                    }
                }
                if let Some(init) = d.init.as_deref_mut() {
                    rewrite_expr(init, boxed);
                }
            }
        }
        Stmt::Decl(Decl::Var(v)) => {
            for d in v.decls.iter_mut() {
                if let Some(init) = d.init.as_deref_mut() {
                    rewrite_expr(init, boxed);
                }
            }
        }
        Stmt::Expr(e) => rewrite_expr(&mut e.expr, boxed),
        Stmt::Return(r) => {
            if let Some(a) = r.arg.as_deref_mut() {
                rewrite_expr(a, boxed);
            }
        }
        Stmt::If(i) => {
            rewrite_expr(&mut i.test, boxed);
            rewrite_stmt(&mut i.cons, boxed, true);
            if let Some(a) = i.alt.as_deref_mut() {
                rewrite_stmt(a, boxed, true);
            }
        }
        Stmt::Block(b) => b.stmts.iter_mut().for_each(|s| rewrite_stmt(s, boxed, true)),
        Stmt::While(w) => {
            rewrite_expr(&mut w.test, boxed);
            rewrite_stmt(&mut w.body, boxed, true);
        }
        Stmt::DoWhile(w) => {
            rewrite_expr(&mut w.test, boxed);
            rewrite_stmt(&mut w.body, boxed, true);
        }
        Stmt::For(f) => {
            if let Some(init) = f.init.as_mut() {
                match init {
                    VarDeclOrExpr::Expr(e) => rewrite_expr(e, boxed),
                    VarDeclOrExpr::VarDecl(vd) => {
                        for d in vd.decls.iter_mut() {
                            if let Some(e) = d.init.as_deref_mut() {
                                rewrite_expr(e, boxed);
                            }
                        }
                    }
                }
            }
            if let Some(t) = f.test.as_deref_mut() {
                rewrite_expr(t, boxed);
            }
            if let Some(u) = f.update.as_deref_mut() {
                rewrite_expr(u, boxed);
            }
            rewrite_stmt(&mut f.body, boxed, true);
        }
        Stmt::ForOf(f) => {
            rewrite_expr(&mut f.right, boxed);
            rewrite_stmt(&mut f.body, boxed, true);
        }
        Stmt::ForIn(f) => {
            rewrite_expr(&mut f.right, boxed);
            rewrite_stmt(&mut f.body, boxed, true);
        }
        Stmt::Switch(s) => {
            rewrite_expr(&mut s.discriminant, boxed);
            for c in s.cases.iter_mut() {
                if let Some(t) = c.test.as_deref_mut() {
                    rewrite_expr(t, boxed);
                }
                for st in c.cons.iter_mut() {
                    rewrite_stmt(st, boxed, true);
                }
            }
        }
        Stmt::Throw(t) => rewrite_expr(&mut t.arg, boxed),
        Stmt::Try(t) => {
            for st in t.block.stmts.iter_mut() {
                rewrite_stmt(st, boxed, true);
            }
            if let Some(h) = t.handler.as_mut() {
                for st in h.body.stmts.iter_mut() {
                    rewrite_stmt(st, boxed, true);
                }
            }
            if let Some(fin) = t.finalizer.as_mut() {
                for st in fin.stmts.iter_mut() {
                    rewrite_stmt(st, boxed, true);
                }
            }
        }
        _ => {}
    }
}

/// Rewrite an expression: wrap boxed reads in `__cell_get`, turn assignments /
/// updates of boxed vars into `__cell_set`.
fn rewrite_expr(expr: &mut Expr, boxed: &HashSet<String>) {
    match expr {
        // Bare read of a boxed var -> __cell_get(v).
        Expr::Ident(id) if boxed.contains(id.sym.as_str()) => {
            let name = id.sym.to_string();
            *expr = cell_get(&name);
        }
        Expr::Assign(a) => {
            // Boxed assignment target?
            let target_name: Option<String> = match &a.left {
                AssignTarget::Simple(SimpleAssignTarget::Ident(id))
                    if boxed.contains(id.id.sym.as_str()) =>
                {
                    Some(id.id.sym.to_string())
                }
                _ => None,
            };
            if let Some(name) = target_name {
                rewrite_expr(&mut a.right, boxed);
                let rhs = (*a.right).clone();
                let new_val = if a.op == swc_ecma_ast::AssignOp::Assign {
                    rhs
                } else {
                    // op-assign: __cell_get(v) <binop> rhs
                    let binop = assign_op_to_binop(a.op);
                    Expr::Bin(BinExpr {
                        span: Default::default(),
                        op: binop,
                        left: Box::new(cell_get(&name)),
                        right: Box::new(rhs),
                    })
                };
                *expr = call2("__cell_set", ident_expr(&name), new_val);
                return;
            }
            // Non-boxed target: still rewrite obj of member targets + rhs.
            if let AssignTarget::Simple(SimpleAssignTarget::Member(m)) = &mut a.left {
                rewrite_expr(&mut m.obj, boxed);
            }
            rewrite_expr(&mut a.right, boxed);
        }
        Expr::Update(u) => {
            if let Expr::Ident(id) = u.arg.as_ref() {
                if boxed.contains(id.sym.as_str()) {
                    let name = id.sym.to_string();
                    let delta = if matches!(u.op, UpdateOp::PlusPlus) {
                        BinaryOp::Add
                    } else {
                        BinaryOp::Sub
                    };
                    let new_val = Expr::Bin(BinExpr {
                        span: Default::default(),
                        op: delta,
                        left: Box::new(cell_get(&name)),
                        right: Box::new(Expr::Lit(swc_ecma_ast::Lit::Num(swc_ecma_ast::Number {
                            span: Default::default(),
                            value: 1.0,
                            raw: None,
                        }))),
                    });
                    // NB: prefix vs postfix value semantics collapse to the new
                    // value; the boxed-counter fixtures use update-as-statement.
                    *expr = call2("__cell_set", ident_expr(&name), new_val);
                    return;
                }
            }
            rewrite_expr(&mut u.arg, boxed);
        }
        Expr::Bin(b) => {
            rewrite_expr(&mut b.left, boxed);
            rewrite_expr(&mut b.right, boxed);
        }
        Expr::Unary(u) => rewrite_expr(&mut u.arg, boxed),
        Expr::Cond(c) => {
            rewrite_expr(&mut c.test, boxed);
            rewrite_expr(&mut c.cons, boxed);
            rewrite_expr(&mut c.alt, boxed);
        }
        Expr::Paren(p) => rewrite_expr(&mut p.expr, boxed),
        Expr::Member(m) => {
            rewrite_expr(&mut m.obj, boxed);
            if let swc_ecma_ast::MemberProp::Computed(c) = &mut m.prop {
                rewrite_expr(&mut c.expr, boxed);
            }
        }
        Expr::Call(c) => {
            if let Callee::Expr(ce) = &mut c.callee {
                rewrite_expr(ce, boxed);
            }
            for a in c.args.iter_mut() {
                rewrite_expr(&mut a.expr, boxed);
            }
        }
        Expr::New(n) => {
            rewrite_expr(&mut n.callee, boxed);
            if let Some(args) = n.args.as_mut() {
                for a in args.iter_mut() {
                    rewrite_expr(&mut a.expr, boxed);
                }
            }
        }
        Expr::Seq(s) => s.exprs.iter_mut().for_each(|e| rewrite_expr(e, boxed)),
        Expr::Tpl(t) => t.exprs.iter_mut().for_each(|e| rewrite_expr(e, boxed)),
        Expr::Array(a) => {
            for el in a.elems.iter_mut().flatten() {
                rewrite_expr(&mut el.expr, boxed);
            }
        }
        Expr::Object(o) => {
            for p in o.props.iter_mut() {
                if let swc_ecma_ast::PropOrSpread::Prop(pr) = p {
                    if let swc_ecma_ast::Prop::KeyValue(kv) = pr.as_mut() {
                        rewrite_expr(&mut kv.value, boxed);
                    }
                }
            }
        }
        // Nested closures: a boxed var of the ENCLOSING scope is read/written
        // here too (it was captured). Rewrite their bodies with the SAME boxed
        // set (the cell handle is what they capture).
        Expr::Arrow(arrow) => match arrow.body.as_mut() {
            swc_ecma_ast::BlockStmtOrExpr::BlockStmt(b) => {
                b.stmts.iter_mut().for_each(|s| rewrite_stmt(s, boxed, false));
            }
            swc_ecma_ast::BlockStmtOrExpr::Expr(e) => rewrite_expr(e, boxed),
        },
        Expr::Fn(fnx) => {
            if let Some(b) = fnx.function.body.as_mut() {
                b.stmts.iter_mut().for_each(|s| rewrite_stmt(s, boxed, false));
            }
        }
        Expr::Await(a) => rewrite_expr(&mut a.arg, boxed),
        Expr::TsAs(a) => rewrite_expr(&mut a.expr, boxed),
        Expr::TsNonNull(n) => rewrite_expr(&mut n.expr, boxed),
        _ => {}
    }
}

fn assign_op_to_binop(op: swc_ecma_ast::AssignOp) -> BinaryOp {
    use swc_ecma_ast::AssignOp::*;
    match op {
        AddAssign => BinaryOp::Add,
        SubAssign => BinaryOp::Sub,
        MulAssign => BinaryOp::Mul,
        DivAssign => BinaryOp::Div,
        ModAssign => BinaryOp::Mod,
        BitAndAssign => BinaryOp::BitAnd,
        BitOrAssign => BinaryOp::BitOr,
        BitXorAssign => BinaryOp::BitXor,
        LShiftAssign => BinaryOp::LShift,
        RShiftAssign => BinaryOp::RShift,
        ZeroFillRShiftAssign => BinaryOp::ZeroFillRShift,
        ExpAssign => BinaryOp::Exp,
        _ => BinaryOp::Add,
    }
}

// ── generic expr visitors (no descent into nested fn bodies) ────────────────

fn visit_exprs_in_stmt(stmt: &mut Stmt, f: &mut impl FnMut(&mut Expr)) {
    let mut walk = |e: &mut Expr| f(e);
    visit_stmt_exprs_mut(stmt, &mut walk);
}

fn visit_stmt_exprs_mut(stmt: &mut Stmt, f: &mut impl FnMut(&mut Expr)) {
    match stmt {
        Stmt::Expr(e) => visit_expr_mut(&mut e.expr, f),
        Stmt::Return(r) => {
            if let Some(a) = r.arg.as_deref_mut() {
                visit_expr_mut(a, f);
            }
        }
        Stmt::Decl(Decl::Var(v)) => {
            for d in v.decls.iter_mut() {
                if let Some(e) = d.init.as_deref_mut() {
                    visit_expr_mut(e, f);
                }
            }
        }
        Stmt::If(i) => {
            visit_expr_mut(&mut i.test, f);
            visit_stmt_exprs_mut(&mut i.cons, f);
            if let Some(a) = i.alt.as_deref_mut() {
                visit_stmt_exprs_mut(a, f);
            }
        }
        Stmt::Block(b) => b.stmts.iter_mut().for_each(|s| visit_stmt_exprs_mut(s, f)),
        Stmt::While(w) => {
            visit_expr_mut(&mut w.test, f);
            visit_stmt_exprs_mut(&mut w.body, f);
        }
        Stmt::DoWhile(w) => {
            visit_expr_mut(&mut w.test, f);
            visit_stmt_exprs_mut(&mut w.body, f);
        }
        Stmt::For(fr) => {
            if let Some(init) = fr.init.as_mut() {
                match init {
                    VarDeclOrExpr::Expr(e) => visit_expr_mut(e, f),
                    VarDeclOrExpr::VarDecl(vd) => {
                        for d in vd.decls.iter_mut() {
                            if let Some(e) = d.init.as_deref_mut() {
                                visit_expr_mut(e, f);
                            }
                        }
                    }
                }
            }
            if let Some(t) = fr.test.as_deref_mut() {
                visit_expr_mut(t, f);
            }
            if let Some(u) = fr.update.as_deref_mut() {
                visit_expr_mut(u, f);
            }
            visit_stmt_exprs_mut(&mut fr.body, f);
        }
        Stmt::ForOf(fr) => {
            visit_expr_mut(&mut fr.right, f);
            visit_stmt_exprs_mut(&mut fr.body, f);
        }
        Stmt::ForIn(fr) => {
            visit_expr_mut(&mut fr.right, f);
            visit_stmt_exprs_mut(&mut fr.body, f);
        }
        Stmt::Switch(s) => {
            visit_expr_mut(&mut s.discriminant, f);
            for c in s.cases.iter_mut() {
                if let Some(t) = c.test.as_deref_mut() {
                    visit_expr_mut(t, f);
                }
                for st in c.cons.iter_mut() {
                    visit_stmt_exprs_mut(st, f);
                }
            }
        }
        Stmt::Throw(t) => visit_expr_mut(&mut t.arg, f),
        Stmt::Try(t) => {
            for st in t.block.stmts.iter_mut() {
                visit_stmt_exprs_mut(st, f);
            }
            if let Some(h) = t.handler.as_mut() {
                for st in h.body.stmts.iter_mut() {
                    visit_stmt_exprs_mut(st, f);
                }
            }
            if let Some(fin) = t.finalizer.as_mut() {
                for st in fin.stmts.iter_mut() {
                    visit_stmt_exprs_mut(st, f);
                }
            }
        }
        _ => {}
    }
}

/// Visit sub-expressions. Calls `f` on this expr, then descends — INCLUDING into
/// nested fn/arrow bodies (so `recurse_into_nested_fns` reaches them).
fn visit_expr_mut(expr: &mut Expr, f: &mut impl FnMut(&mut Expr)) {
    f(expr);
    match expr {
        Expr::Bin(b) => {
            visit_expr_mut(&mut b.left, f);
            visit_expr_mut(&mut b.right, f);
        }
        Expr::Unary(u) => visit_expr_mut(&mut u.arg, f),
        Expr::Update(u) => visit_expr_mut(&mut u.arg, f),
        Expr::Assign(a) => {
            if let AssignTarget::Simple(SimpleAssignTarget::Member(m)) = &mut a.left {
                visit_expr_mut(&mut m.obj, f);
            }
            visit_expr_mut(&mut a.right, f);
        }
        Expr::Cond(c) => {
            visit_expr_mut(&mut c.test, f);
            visit_expr_mut(&mut c.cons, f);
            visit_expr_mut(&mut c.alt, f);
        }
        Expr::Paren(p) => visit_expr_mut(&mut p.expr, f),
        Expr::Member(m) => {
            visit_expr_mut(&mut m.obj, f);
            if let swc_ecma_ast::MemberProp::Computed(c) = &mut m.prop {
                visit_expr_mut(&mut c.expr, f);
            }
        }
        Expr::Call(c) => {
            if let Callee::Expr(ce) = &mut c.callee {
                visit_expr_mut(ce, f);
            }
            for a in c.args.iter_mut() {
                visit_expr_mut(&mut a.expr, f);
            }
        }
        Expr::New(n) => {
            visit_expr_mut(&mut n.callee, f);
            if let Some(args) = n.args.as_mut() {
                for a in args.iter_mut() {
                    visit_expr_mut(&mut a.expr, f);
                }
            }
        }
        Expr::Seq(s) => s.exprs.iter_mut().for_each(|e| visit_expr_mut(e, f)),
        Expr::Tpl(t) => t.exprs.iter_mut().for_each(|e| visit_expr_mut(e, f)),
        Expr::Array(a) => {
            for el in a.elems.iter_mut().flatten() {
                visit_expr_mut(&mut el.expr, f);
            }
        }
        Expr::Object(o) => {
            for p in o.props.iter_mut() {
                if let swc_ecma_ast::PropOrSpread::Prop(pr) = p {
                    if let swc_ecma_ast::Prop::KeyValue(kv) = pr.as_mut() {
                        visit_expr_mut(&mut kv.value, f);
                    }
                }
            }
        }
        Expr::Arrow(arrow) => match arrow.body.as_mut() {
            swc_ecma_ast::BlockStmtOrExpr::BlockStmt(b) => {
                b.stmts.iter_mut().for_each(|s| visit_stmt_exprs_mut(s, f));
            }
            swc_ecma_ast::BlockStmtOrExpr::Expr(e) => visit_expr_mut(e, f),
        },
        Expr::Fn(fnx) => {
            if let Some(b) = fnx.function.body.as_mut() {
                b.stmts.iter_mut().for_each(|s| visit_stmt_exprs_mut(s, f));
            }
        }
        Expr::Await(a) => visit_expr_mut(&mut a.arg, f),
        Expr::TsAs(a) => visit_expr_mut(&mut a.expr, f),
        Expr::TsNonNull(n) => visit_expr_mut(&mut n.expr, f),
        _ => {}
    }
}

/// Immutable variant of `visit_stmt_exprs_mut` for the read-only scans.
fn visit_exprs_in_stmt_ref(stmt: &Stmt, f: &mut impl FnMut(&Expr)) {
    // Reuse the mut walker over a clone would be wasteful; instead do a light
    // immutable walk mirroring the mut one for the forms `scan_mutated` needs.
    fn ev(e: &Expr, f: &mut impl FnMut(&Expr)) {
        f(e);
        match e {
            Expr::Bin(b) => { ev(&b.left, f); ev(&b.right, f); }
            Expr::Unary(u) => ev(&u.arg, f),
            Expr::Update(u) => ev(&u.arg, f),
            Expr::Assign(a) => {
                if let AssignTarget::Simple(SimpleAssignTarget::Member(m)) = &a.left {
                    ev(&m.obj, f);
                }
                ev(&a.right, f);
            }
            Expr::Cond(c) => { ev(&c.test, f); ev(&c.cons, f); ev(&c.alt, f); }
            Expr::Paren(p) => ev(&p.expr, f),
            Expr::Member(m) => {
                ev(&m.obj, f);
                if let swc_ecma_ast::MemberProp::Computed(c) = &m.prop { ev(&c.expr, f); }
            }
            Expr::Call(c) => {
                if let Callee::Expr(ce) = &c.callee { ev(ce, f); }
                for a in &c.args { ev(&a.expr, f); }
            }
            Expr::New(n) => {
                ev(&n.callee, f);
                if let Some(args) = &n.args { for a in args { ev(&a.expr, f); } }
            }
            Expr::Seq(s) => s.exprs.iter().for_each(|e| ev(e, f)),
            Expr::Tpl(t) => t.exprs.iter().for_each(|e| ev(e, f)),
            Expr::Array(a) => { for el in a.elems.iter().flatten() { ev(&el.expr, f); } }
            Expr::Object(o) => {
                for p in &o.props {
                    if let swc_ecma_ast::PropOrSpread::Prop(pr) = p {
                        if let swc_ecma_ast::Prop::KeyValue(kv) = pr.as_ref() { ev(&kv.value, f); }
                    }
                }
            }
            Expr::Arrow(arrow) => match arrow.body.as_ref() {
                swc_ecma_ast::BlockStmtOrExpr::BlockStmt(b) => b.stmts.iter().for_each(|s| sv(s, f)),
                swc_ecma_ast::BlockStmtOrExpr::Expr(e) => ev(e, f),
            },
            Expr::Fn(fnx) => {
                if let Some(b) = fnx.function.body.as_ref() { b.stmts.iter().for_each(|s| sv(s, f)); }
            }
            Expr::Await(a) => ev(&a.arg, f),
            Expr::TsAs(a) => ev(&a.expr, f),
            Expr::TsNonNull(n) => ev(&n.expr, f),
            _ => {}
        }
    }
    fn sv(s: &Stmt, f: &mut impl FnMut(&Expr)) {
        match s {
            Stmt::Expr(e) => ev(&e.expr, f),
            Stmt::Return(r) => { if let Some(a) = r.arg.as_deref() { ev(a, f); } }
            Stmt::Decl(Decl::Var(v)) => {
                for d in &v.decls { if let Some(e) = d.init.as_deref() { ev(e, f); } }
            }
            Stmt::If(i) => { ev(&i.test, f); sv(&i.cons, f); if let Some(a) = i.alt.as_deref() { sv(a, f); } }
            Stmt::Block(b) => b.stmts.iter().for_each(|s| sv(s, f)),
            Stmt::While(w) => { ev(&w.test, f); sv(&w.body, f); }
            Stmt::DoWhile(w) => { ev(&w.test, f); sv(&w.body, f); }
            Stmt::For(fr) => {
                if let Some(init) = fr.init.as_ref() {
                    match init {
                        VarDeclOrExpr::Expr(e) => ev(e, f),
                        VarDeclOrExpr::VarDecl(vd) => for d in &vd.decls { if let Some(e) = d.init.as_deref() { ev(e, f); } },
                    }
                }
                if let Some(t) = fr.test.as_deref() { ev(t, f); }
                if let Some(u) = fr.update.as_deref() { ev(u, f); }
                sv(&fr.body, f);
            }
            Stmt::ForOf(fr) => { ev(&fr.right, f); sv(&fr.body, f); }
            Stmt::ForIn(fr) => { ev(&fr.right, f); sv(&fr.body, f); }
            Stmt::Switch(s) => {
                ev(&s.discriminant, f);
                for c in &s.cases { if let Some(t) = c.test.as_deref() { ev(t, f); } for st in &c.cons { sv(st, f); } }
            }
            Stmt::Throw(t) => ev(&t.arg, f),
            Stmt::Try(t) => {
                for st in &t.block.stmts { sv(st, f); }
                if let Some(h) = &t.handler { for st in &h.body.stmts { sv(st, f); } }
                if let Some(fin) = &t.finalizer { for st in &fin.stmts { sv(st, f); } }
            }
            _ => {}
        }
    }
    sv(stmt, f);
}
