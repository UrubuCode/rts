//! Pass que desugara o protocolo de iterator custom (`[Symbol.iterator]`).
//!
//! Cobre o padrao classico:
//!
//! ```ts
//! class Bag {
//!   data = [3,4,5];
//!   [Symbol.iterator]() {
//!     let i = 0; const d = this.data;
//!     return { next() { return i < d.length
//!       ? { value: d[i++], done: false }
//!       : { value: undefined, done: true }; } };
//!   }
//! }
//! for (const n of new Bag() as any) out.push(n);
//! ```
//!
//! Transformacoes (sem closures reais — reusa `this`-based object methods):
//!
//! 1. Renomeia o metodo de classe `[Symbol.iterator]` (parseado com nome
//!    textual `"Symbol.iterator"`) para `__rts_sym_iterator`, contornando o
//!    dispatch por chave computada.
//! 2. No corpo desse metodo, o object literal retornado que tem `next` tem
//!    suas capturas (vars livres do `next` que pertencem ao escopo do metodo)
//!    promovidas a CAMPOS do objeto; refs a essas vars dentro do `next` viram
//!    `this.<cap>`. Mutacao de campo (`i++`) persiste entre chamadas de
//!    `next()` via o slot `this` do object method (PR #451).
//! 3. `for (const N of EXPR)` cujo `EXPR` resolve a uma instancia de classe
//!    com `[Symbol.iterator]` vira:
//!    ```ts
//!    { const __it = EXPR.__rts_sym_iterator();
//!      while (true) { const __r = __it.next(); if (__r.done) break;
//!        const N = __r.value; BODY } }
//!    ```

use std::collections::{HashSet, BTreeSet};

use swc_ecma_ast::{Expr, Stmt};

use crate::parser::ast::{ClassMember, Item, Program, Statement};

const SYM_ITER_SRC: &str = "Symbol.iterator";
const SYM_ITER_DST: &str = "__rts_sym_iterator";

/// Mapeia um nome de membro de classe `[Symbol.<wk>]` (parseado como texto
/// `"Symbol.<wk>"`) para o nome interno `__rts_wk_<wk>` que o motor usa no lugar
/// de citar "Symbol.*". Drena os well-known symbols do motor (a classe Symbol em
/// si segue no Registry). `iterator` tem o seu próprio canal (`__rts_sym_iterator`).
fn well_known_internal_name(name: &str) -> Option<String> {
    let wk = name.strip_prefix("Symbol.")?;
    match wk {
        "toPrimitive" => Some("__rts_wk_toPrimitive".to_string()),
        "hasInstance" => Some("__rts_wk_hasInstance".to_string()),
        _ => None,
    }
}

thread_local! {
    /// (#216/#271) Classes com `[Symbol.iterator]` (renomeado p/
    /// `__rts_sym_iterator`). Consultado pelo codegen de `Array.from`/spread.
    static ITER_CLASSES: std::cell::RefCell<HashSet<String>> =
        std::cell::RefCell::new(HashSet::new());
}

/// (#216/#271) True se `class_name` tem um `[Symbol.iterator]` (iteravel custom).
pub(crate) fn is_iter_class(class_name: &str) -> bool {
    ITER_CLASSES.with(|c| c.borrow().contains(class_name))
}

/// (#273) `for await/of (const n of obj)` onde `obj` eh um object literal com
/// metodo `[Symbol.asyncIterator]`/`[Symbol.iterator]` (apos object_methods, um
/// KeyValue de chave computada): reescreve o iteravel para
/// `obj[Symbol.<key>]()` (o metodo devolve um Vec via __gen_buf), tornando o
/// for-of plano sobre o Vec. `is_await` vira false (await de non-Promise = no-op).
pub(crate) fn desugar_object_symbol_iterators(program: &mut Program) {
    use std::collections::HashMap;
    let mut iter_objs: HashMap<String, String> = HashMap::new();
    for item in program.items.iter_mut() {
        walk_stmts_in_item(item, &mut |s| collect_obj_iter_decl(s, &mut iter_objs));
    }
    if iter_objs.is_empty() {
        return;
    }
    for item in program.items.iter_mut() {
        walk_stmts_in_item(item, &mut |s| {
            if let Stmt::ForOf(f) = s {
                // Peel `asyncIter as any` / `(x)` / `x!` no iteravel.
                fn peel(e: &Expr) -> &Expr {
                    match e {
                        Expr::TsAs(a) => peel(&a.expr),
                        Expr::TsConstAssertion(a) => peel(&a.expr),
                        Expr::TsNonNull(a) => peel(&a.expr),
                        Expr::Paren(p) => peel(&p.expr),
                        _ => e,
                    }
                }
                if let Expr::Ident(id) = peel(f.right.as_ref()) {
                    if let Some(key) = iter_objs.get(id.sym.as_str()) {
                        f.right = Box::new(build_symbol_iter_call(id.sym.as_ref(), key));
                        f.is_await = false;
                    }
                }
            }
        });
    }
}

fn collect_obj_iter_decl(stmt: &Stmt, out: &mut std::collections::HashMap<String, String>) {
    if let Stmt::Decl(swc_ecma_ast::Decl::Var(vd)) = stmt {
        for d in &vd.decls {
            if let (swc_ecma_ast::Pat::Ident(id), Some(init)) = (&d.name, d.init.as_deref()) {
                if let Some(key) = object_symbol_iterator_key(init) {
                    out.insert(id.id.sym.to_string(), key);
                }
            }
        }
    }
}

/// Devolve "asyncIterator"/"iterator" se o object literal tem um metodo/prop com
/// chave computada `Symbol.asyncIterator`/`Symbol.iterator`.
fn object_symbol_iterator_key(e: &Expr) -> Option<String> {
    let Expr::Object(obj) = e else { return None };
    for p in &obj.props {
        if let swc_ecma_ast::PropOrSpread::Prop(prop) = p {
            let key = match prop.as_ref() {
                swc_ecma_ast::Prop::KeyValue(kv) => Some(&kv.key),
                swc_ecma_ast::Prop::Method(m) => Some(&m.key),
                _ => None,
            };
            if let Some(swc_ecma_ast::PropName::Computed(c)) = key {
                if let Expr::Member(m) = c.expr.as_ref() {
                    if let (Expr::Ident(o), swc_ecma_ast::MemberProp::Ident(pr)) =
                        (m.obj.as_ref(), &m.prop)
                    {
                        if o.sym.as_str() == "Symbol" {
                            match pr.sym.as_str() {
                                "asyncIterator" => return Some("asyncIterator".into()),
                                "iterator" => return Some("iterator".into()),
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Constroi `obj[Symbol.<key>]()`.
fn build_symbol_iter_call(obj: &str, key: &str) -> Expr {
    let sp = swc_common::DUMMY_SP;
    let symbol_member = Expr::Member(swc_ecma_ast::MemberExpr {
        span: sp,
        obj: Box::new(Expr::Ident(swc_ecma_ast::Ident::new("Symbol".into(), sp, Default::default()))),
        prop: swc_ecma_ast::MemberProp::Ident(swc_ecma_ast::IdentName::new(key.into(), sp)),
    });
    let computed = Expr::Member(swc_ecma_ast::MemberExpr {
        span: sp,
        obj: Box::new(Expr::Ident(swc_ecma_ast::Ident::new(obj.into(), sp, Default::default()))),
        prop: swc_ecma_ast::MemberProp::Computed(swc_ecma_ast::ComputedPropName {
            span: sp,
            expr: Box::new(symbol_member),
        }),
    });
    Expr::Call(swc_ecma_ast::CallExpr {
        span: sp,
        ctxt: Default::default(),
        callee: swc_ecma_ast::Callee::Expr(Box::new(computed)),
        args: Vec::new(),
        type_args: None,
    })
}

/// Walk recursivo sobre todos os statements de um Item (top-level stmt + corpos
/// de fn/metodo), aplicando `f`. Usado pelos passes de iterator de objeto.
fn walk_stmts_in_item(item: &mut Item, f: &mut impl FnMut(&mut Stmt)) {
    match item {
        Item::Statement(Statement::Raw(raw)) => {
            if let Some(stmt) = raw.stmt.as_mut() {
                walk_stmt_rec(stmt, f);
            }
        }
        Item::Function(fd) => {
            for s in fd.body.iter_mut() {
                let Statement::Raw(raw) = s;
                if let Some(stmt) = raw.stmt.as_mut() {
                    walk_stmt_rec(stmt, f);
                }
            }
        }
        _ => {}
    }
}

fn walk_stmt_rec(stmt: &mut Stmt, f: &mut impl FnMut(&mut Stmt)) {
    f(stmt);
    match stmt {
        Stmt::Block(b) => b.stmts.iter_mut().for_each(|s| walk_stmt_rec(s, f)),
        Stmt::If(i) => {
            walk_stmt_rec(&mut i.cons, f);
            if let Some(a) = i.alt.as_mut() {
                walk_stmt_rec(a, f);
            }
        }
        Stmt::While(w) => walk_stmt_rec(&mut w.body, f),
        Stmt::DoWhile(w) => walk_stmt_rec(&mut w.body, f),
        Stmt::For(fo) => walk_stmt_rec(&mut fo.body, f),
        Stmt::ForIn(fo) => walk_stmt_rec(&mut fo.body, f),
        Stmt::ForOf(fo) => walk_stmt_rec(&mut fo.body, f),
        Stmt::Try(t) => {
            t.block.stmts.iter_mut().for_each(|s| walk_stmt_rec(s, f));
            if let Some(h) = t.handler.as_mut() {
                h.body.stmts.iter_mut().for_each(|s| walk_stmt_rec(s, f));
            }
            if let Some(fin) = t.finalizer.as_mut() {
                fin.stmts.iter_mut().for_each(|s| walk_stmt_rec(s, f));
            }
        }
        Stmt::Labeled(l) => walk_stmt_rec(&mut l.body, f),
        Stmt::Switch(s) => {
            for c in s.cases.iter_mut() {
                c.cons.iter_mut().for_each(|st| walk_stmt_rec(st, f));
            }
        }
        _ => {}
    }
}

pub(crate) fn desugar_custom_iterators(program: &mut Program) {
    // 1) Coleta classes que tem [Symbol.iterator] + renomeia o metodo.
    let mut iter_classes: HashSet<String> = HashSet::new();
    for item in program.items.iter_mut() {
        if let Item::Class(class) = item {
            let mut has = false;
            for member in class.members.iter_mut() {
                if let ClassMember::Method(m) = member {
                    if m.name == SYM_ITER_SRC {
                        m.name = SYM_ITER_DST.to_string();
                        has = true;
                        // Promove capturas do object iterator a `this`-fields.
                        let method_locals = collect_method_locals(m);
                        for s in m.body.iter_mut() {
                            let Statement::Raw(raw) = s;
                            if let Some(stmt) = raw.stmt.as_mut() {
                                promote_iter_object(stmt, &method_locals);
                            }
                        }
                    } else if let Some(internal) = well_known_internal_name(&m.name) {
                        // Drena well-known symbol do MOTOR: renomeia o membro de
                        // classe p/ um nome interno `__rts_wk_*` ANTES da síntese,
                        // então `meta.methods`/`static_methods` carregam o nome
                        // interno e o engine não precisa citar "Symbol.*".
                        m.name = internal;
                    }
                }
            }
            if has {
                iter_classes.insert(class.name.clone());
            }
        }
    }

    // (#216/#271) Registra as classes iteraveis pra que `Array.from(c)` /
    // spread `[...c]` detectem e drenem via `c.__rts_sym_iterator()`.
    ITER_CLASSES.with(|c| *c.borrow_mut() = iter_classes.clone());

    if iter_classes.is_empty() {
        return;
    }

    // 2) Reescreve for-of sobre instancias dessas classes.
    for item in program.items.iter_mut() {
        match item {
            Item::Function(f) => rewrite_stmts(&mut f.body, &iter_classes),
            Item::Statement(Statement::Raw(raw)) => {
                if let Some(stmt) = raw.stmt.as_mut() {
                    rewrite_for_of_in_stmt(stmt, &iter_classes);
                }
            }
            Item::Class(class) => {
                for member in class.members.iter_mut() {
                    let body = match member {
                        ClassMember::Method(m) => &mut m.body,
                        ClassMember::Constructor(c) => &mut c.body,
                        _ => continue,
                    };
                    rewrite_stmts(body, &iter_classes);
                }
            }
            _ => {}
        }
    }
}

fn rewrite_stmts(body: &mut [Statement], iter_classes: &HashSet<String>) {
    for s in body.iter_mut() {
        let Statement::Raw(raw) = s;
        if let Some(stmt) = raw.stmt.as_mut() {
            rewrite_for_of_in_stmt(stmt, iter_classes);
        }
    }
}

// ---------------------------------------------------------------------------
// (1) Coleta de locals do metodo iterator (params + var decls top-level).
// ---------------------------------------------------------------------------

fn collect_method_locals(m: &crate::parser::ast::MethodDecl) -> HashSet<String> {
    let mut locals: HashSet<String> = HashSet::new();
    for p in &m.parameters {
        locals.insert(p.name.clone());
    }
    for s in &m.body {
        let Statement::Raw(raw) = s;
        if let Some(stmt) = raw.stmt.as_ref() {
            collect_decls_in_stmt(stmt, &mut locals);
        }
    }
    locals
}

fn collect_decls_in_stmt(stmt: &Stmt, locals: &mut HashSet<String>) {
    match stmt {
        Stmt::Decl(swc_ecma_ast::Decl::Var(v)) => {
            for d in &v.decls {
                collect_decls_in_pat(&d.name, locals);
            }
        }
        Stmt::Block(b) => {
            for s in &b.stmts {
                collect_decls_in_stmt(s, locals);
            }
        }
        Stmt::If(i) => {
            collect_decls_in_stmt(&i.cons, locals);
            if let Some(alt) = &i.alt {
                collect_decls_in_stmt(alt, locals);
            }
        }
        Stmt::For(f) => {
            if let Some(swc_ecma_ast::VarDeclOrExpr::VarDecl(v)) = &f.init {
                for d in &v.decls {
                    collect_decls_in_pat(&d.name, locals);
                }
            }
            collect_decls_in_stmt(&f.body, locals);
        }
        _ => {}
    }
}

fn collect_decls_in_pat(pat: &swc_ecma_ast::Pat, locals: &mut HashSet<String>) {
    if let swc_ecma_ast::Pat::Ident(id) = pat {
        locals.insert(id.id.sym.to_string());
    }
}

// ---------------------------------------------------------------------------
// (2) Promove capturas de `next` a campos do object iterator.
// ---------------------------------------------------------------------------

fn promote_iter_object(stmt: &mut Stmt, method_locals: &HashSet<String>) {
    match stmt {
        Stmt::Return(r) => {
            if let Some(arg) = r.arg.as_mut() {
                promote_in_expr(arg, method_locals);
            }
        }
        Stmt::Decl(swc_ecma_ast::Decl::Var(v)) => {
            for d in v.decls.iter_mut() {
                if let Some(init) = d.init.as_mut() {
                    promote_in_expr(init, method_locals);
                }
            }
        }
        Stmt::Expr(e) => promote_in_expr(&mut e.expr, method_locals),
        Stmt::If(i) => {
            promote_iter_object(&mut i.cons, method_locals);
            if let Some(alt) = i.alt.as_mut() {
                promote_iter_object(alt, method_locals);
            }
        }
        Stmt::Block(b) => {
            for s in b.stmts.iter_mut() {
                promote_iter_object(s, method_locals);
            }
        }
        _ => {}
    }
}

fn promote_in_expr(expr: &mut Expr, method_locals: &HashSet<String>) {
    // Desce em wrappers comuns.
    match expr {
        Expr::Paren(p) => return promote_in_expr(&mut p.expr, method_locals),
        Expr::TsAs(a) => return promote_in_expr(&mut a.expr, method_locals),
        Expr::TsNonNull(n) => return promote_in_expr(&mut n.expr, method_locals),
        _ => {}
    }
    let Expr::Object(obj) = expr else { return };

    // Acha o metodo `next` (shorthand `next() {}` ou `next: function(){}`).
    let has_next = obj.props.iter().any(|p| {
        if let swc_ecma_ast::PropOrSpread::Prop(pp) = p {
            prop_key_name(pp).as_deref() == Some("next")
        } else {
            false
        }
    });
    if !has_next {
        return;
    }

    // Nomes ja' presentes como props (nao sao capturas — sao campos do iter).
    let mut own_props: HashSet<String> = HashSet::new();
    for p in &obj.props {
        if let swc_ecma_ast::PropOrSpread::Prop(pp) = p {
            if let Some(n) = prop_key_name(pp) {
                own_props.insert(n);
            }
        }
    }

    // Coleta capturas do corpo de `next` (e de quaisquer metodos do iter).
    let mut captured: BTreeSet<String> = BTreeSet::new();
    for p in obj.props.iter() {
        if let swc_ecma_ast::PropOrSpread::Prop(pp) = p {
            collect_captures_in_prop(pp, method_locals, &own_props, &mut captured);
        }
    }

    if captured.is_empty() {
        return;
    }

    // Reescreve refs `cap` -> `this.cap` dentro dos metodos do iter.
    for p in obj.props.iter_mut() {
        if let swc_ecma_ast::PropOrSpread::Prop(pp) = p {
            rewrite_captures_in_prop(pp, &captured);
        }
    }

    // Prepend campos `{ cap: cap }` (seed com o valor capturado atual).
    let mut new_fields: Vec<swc_ecma_ast::PropOrSpread> = Vec::with_capacity(captured.len());
    for cap in &captured {
        new_fields.push(swc_ecma_ast::PropOrSpread::Prop(Box::new(
            swc_ecma_ast::Prop::KeyValue(swc_ecma_ast::KeyValueProp {
                key: swc_ecma_ast::PropName::Ident(swc_ecma_ast::IdentName::new(
                    cap.as_str().into(),
                    Default::default(),
                )),
                value: Box::new(ident_expr(cap)),
            }),
        )));
    }
    new_fields.append(&mut obj.props);
    obj.props = new_fields;
}

fn prop_key_name(p: &swc_ecma_ast::Prop) -> Option<String> {
    use swc_ecma_ast::Prop;
    let key = match p {
        Prop::KeyValue(kv) => &kv.key,
        Prop::Method(m) => &m.key,
        Prop::Getter(g) => &g.key,
        Prop::Setter(s) => &s.key,
        Prop::Shorthand(id) => return Some(id.sym.to_string()),
        _ => return None,
    };
    match key {
        swc_ecma_ast::PropName::Ident(i) => Some(i.sym.to_string()),
        swc_ecma_ast::PropName::Str(s) => Some(s.value.to_string_lossy().to_string()),
        _ => None,
    }
}

fn collect_captures_in_prop(
    p: &swc_ecma_ast::Prop,
    method_locals: &HashSet<String>,
    own_props: &HashSet<String>,
    captured: &mut BTreeSet<String>,
) {
    use swc_ecma_ast::Prop;
    let (params, body): (Vec<String>, Vec<Stmt>) = match p {
        Prop::Method(m) => (
            fn_param_names(&m.function.params),
            m.function.body.as_ref().map(|b| b.stmts.clone()).unwrap_or_default(),
        ),
        Prop::KeyValue(kv) => match kv.value.as_ref() {
            Expr::Fn(f) => (
                fn_param_names(&f.function.params),
                f.function.body.as_ref().map(|b| b.stmts.clone()).unwrap_or_default(),
            ),
            Expr::Arrow(a) => (
                arrow_param_names(&a.params),
                arrow_body_stmts(a),
            ),
            _ => return,
        },
        _ => return,
    };
    // shadowed = params + decls locais da fn + props do proprio iter.
    let mut shadowed: HashSet<String> = own_props.clone();
    shadowed.extend(params);
    for s in &body {
        collect_decls_in_stmt(s, &mut shadowed);
    }
    for s in &body {
        collect_idents_in_stmt(s, method_locals, &shadowed, captured);
    }
}

fn fn_param_names(params: &[swc_ecma_ast::Param]) -> Vec<String> {
    params
        .iter()
        .filter_map(|p| {
            if let swc_ecma_ast::Pat::Ident(id) = &p.pat {
                Some(id.id.sym.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn arrow_param_names(params: &[swc_ecma_ast::Pat]) -> Vec<String> {
    params
        .iter()
        .filter_map(|p| {
            if let swc_ecma_ast::Pat::Ident(id) = p {
                Some(id.id.sym.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn arrow_body_stmts(a: &swc_ecma_ast::ArrowExpr) -> Vec<Stmt> {
    match a.body.as_ref() {
        swc_ecma_ast::BlockStmtOrExpr::BlockStmt(b) => b.stmts.clone(),
        swc_ecma_ast::BlockStmtOrExpr::Expr(e) => vec![Stmt::Return(swc_ecma_ast::ReturnStmt {
            span: Default::default(),
            arg: Some(e.clone()),
        })],
    }
}

fn collect_idents_in_stmt(
    stmt: &Stmt,
    enclosing: &HashSet<String>,
    shadowed: &HashSet<String>,
    captured: &mut BTreeSet<String>,
) {
    match stmt {
        Stmt::Expr(e) => collect_idents_in_expr(&e.expr, enclosing, shadowed, captured),
        Stmt::Return(r) => {
            if let Some(a) = r.arg.as_deref() {
                collect_idents_in_expr(a, enclosing, shadowed, captured);
            }
        }
        Stmt::If(i) => {
            collect_idents_in_expr(&i.test, enclosing, shadowed, captured);
            collect_idents_in_stmt(&i.cons, enclosing, shadowed, captured);
            if let Some(alt) = i.alt.as_deref() {
                collect_idents_in_stmt(alt, enclosing, shadowed, captured);
            }
        }
        Stmt::Block(b) => {
            for s in &b.stmts {
                collect_idents_in_stmt(s, enclosing, shadowed, captured);
            }
        }
        Stmt::While(w) => {
            collect_idents_in_expr(&w.test, enclosing, shadowed, captured);
            collect_idents_in_stmt(&w.body, enclosing, shadowed, captured);
        }
        Stmt::DoWhile(w) => {
            collect_idents_in_expr(&w.test, enclosing, shadowed, captured);
            collect_idents_in_stmt(&w.body, enclosing, shadowed, captured);
        }
        Stmt::For(f) => {
            if let Some(swc_ecma_ast::VarDeclOrExpr::VarDecl(vd)) = f.init.as_ref() {
                for d in &vd.decls {
                    if let Some(e) = d.init.as_deref() {
                        collect_idents_in_expr(e, enclosing, shadowed, captured);
                    }
                }
            }
            if let Some(t) = f.test.as_deref() {
                collect_idents_in_expr(t, enclosing, shadowed, captured);
            }
            if let Some(u) = f.update.as_deref() {
                collect_idents_in_expr(u, enclosing, shadowed, captured);
            }
            collect_idents_in_stmt(&f.body, enclosing, shadowed, captured);
        }
        Stmt::Decl(swc_ecma_ast::Decl::Var(v)) => {
            for d in &v.decls {
                if let Some(e) = d.init.as_deref() {
                    collect_idents_in_expr(e, enclosing, shadowed, captured);
                }
            }
        }
        _ => {}
    }
}

fn collect_idents_in_expr(
    expr: &Expr,
    enclosing: &HashSet<String>,
    shadowed: &HashSet<String>,
    captured: &mut BTreeSet<String>,
) {
    match expr {
        Expr::Ident(id) => {
            let name = id.sym.as_str();
            if enclosing.contains(name) && !shadowed.contains(name) {
                captured.insert(name.to_string());
            }
        }
        Expr::Member(m) => {
            collect_idents_in_expr(&m.obj, enclosing, shadowed, captured);
            if let swc_ecma_ast::MemberProp::Computed(c) = &m.prop {
                collect_idents_in_expr(&c.expr, enclosing, shadowed, captured);
            }
        }
        Expr::Bin(b) => {
            collect_idents_in_expr(&b.left, enclosing, shadowed, captured);
            collect_idents_in_expr(&b.right, enclosing, shadowed, captured);
        }
        Expr::Assign(a) => {
            if let swc_ecma_ast::AssignTarget::Simple(
                swc_ecma_ast::SimpleAssignTarget::Member(m),
            ) = &a.left
            {
                collect_idents_in_expr(&m.obj, enclosing, shadowed, captured);
            }
            collect_idents_in_expr(&a.right, enclosing, shadowed, captured);
        }
        Expr::Unary(u) => collect_idents_in_expr(&u.arg, enclosing, shadowed, captured),
        Expr::Update(u) => collect_idents_in_expr(&u.arg, enclosing, shadowed, captured),
        Expr::Cond(c) => {
            collect_idents_in_expr(&c.test, enclosing, shadowed, captured);
            collect_idents_in_expr(&c.cons, enclosing, shadowed, captured);
            collect_idents_in_expr(&c.alt, enclosing, shadowed, captured);
        }
        Expr::Call(c) => {
            if let swc_ecma_ast::Callee::Expr(callee) = &c.callee {
                collect_idents_in_expr(callee, enclosing, shadowed, captured);
            }
            for arg in &c.args {
                collect_idents_in_expr(&arg.expr, enclosing, shadowed, captured);
            }
        }
        Expr::New(n) => {
            collect_idents_in_expr(&n.callee, enclosing, shadowed, captured);
            if let Some(args) = &n.args {
                for arg in args {
                    collect_idents_in_expr(&arg.expr, enclosing, shadowed, captured);
                }
            }
        }
        Expr::Paren(p) => collect_idents_in_expr(&p.expr, enclosing, shadowed, captured),
        Expr::TsAs(a) => collect_idents_in_expr(&a.expr, enclosing, shadowed, captured),
        Expr::TsNonNull(n) => collect_idents_in_expr(&n.expr, enclosing, shadowed, captured),
        Expr::Array(arr) => {
            for elem in arr.elems.iter().flatten() {
                collect_idents_in_expr(&elem.expr, enclosing, shadowed, captured);
            }
        }
        Expr::Object(obj) => {
            for p in &obj.props {
                if let swc_ecma_ast::PropOrSpread::Prop(pp) = p {
                    if let swc_ecma_ast::Prop::KeyValue(kv) = pp.as_ref() {
                        collect_idents_in_expr(&kv.value, enclosing, shadowed, captured);
                    }
                }
            }
        }
        Expr::Tpl(t) => {
            for e in &t.exprs {
                collect_idents_in_expr(e, enclosing, shadowed, captured);
            }
        }
        _ => {}
    }
}

/// Reescreve refs `cap` -> `this.cap` dentro do corpo de cada metodo do iter.
fn rewrite_captures_in_prop(p: &mut swc_ecma_ast::Prop, captured: &BTreeSet<String>) {
    use swc_ecma_ast::Prop;
    match p {
        Prop::Method(m) => {
            if let Some(body) = m.function.body.as_mut() {
                for s in body.stmts.iter_mut() {
                    rewrite_captures_in_stmt(s, captured);
                }
            }
        }
        Prop::KeyValue(kv) => match kv.value.as_mut() {
            Expr::Fn(f) => {
                if let Some(body) = f.function.body.as_mut() {
                    for s in body.stmts.iter_mut() {
                        rewrite_captures_in_stmt(s, captured);
                    }
                }
            }
            Expr::Arrow(a) => match a.body.as_mut() {
                swc_ecma_ast::BlockStmtOrExpr::BlockStmt(b) => {
                    for s in b.stmts.iter_mut() {
                        rewrite_captures_in_stmt(s, captured);
                    }
                }
                swc_ecma_ast::BlockStmtOrExpr::Expr(e) => rewrite_captures_in_expr(e, captured),
            },
            _ => {}
        },
        _ => {}
    }
}

fn rewrite_captures_in_stmt(stmt: &mut Stmt, captured: &BTreeSet<String>) {
    match stmt {
        Stmt::Expr(e) => rewrite_captures_in_expr(&mut e.expr, captured),
        Stmt::Return(r) => {
            if let Some(a) = r.arg.as_mut() {
                rewrite_captures_in_expr(a, captured);
            }
        }
        Stmt::If(i) => {
            rewrite_captures_in_expr(&mut i.test, captured);
            rewrite_captures_in_stmt(&mut i.cons, captured);
            if let Some(alt) = i.alt.as_mut() {
                rewrite_captures_in_stmt(alt, captured);
            }
        }
        Stmt::Block(b) => {
            for s in b.stmts.iter_mut() {
                rewrite_captures_in_stmt(s, captured);
            }
        }
        Stmt::While(w) => {
            rewrite_captures_in_expr(&mut w.test, captured);
            rewrite_captures_in_stmt(&mut w.body, captured);
        }
        Stmt::DoWhile(w) => {
            rewrite_captures_in_expr(&mut w.test, captured);
            rewrite_captures_in_stmt(&mut w.body, captured);
        }
        Stmt::For(f) => {
            if let Some(swc_ecma_ast::VarDeclOrExpr::VarDecl(vd)) = f.init.as_mut() {
                for d in vd.decls.iter_mut() {
                    if let Some(e) = d.init.as_mut() {
                        rewrite_captures_in_expr(e, captured);
                    }
                }
            }
            if let Some(t) = f.test.as_mut() {
                rewrite_captures_in_expr(t, captured);
            }
            if let Some(u) = f.update.as_mut() {
                rewrite_captures_in_expr(u, captured);
            }
            rewrite_captures_in_stmt(&mut f.body, captured);
        }
        Stmt::Decl(swc_ecma_ast::Decl::Var(v)) => {
            for d in v.decls.iter_mut() {
                if let Some(e) = d.init.as_mut() {
                    rewrite_captures_in_expr(e, captured);
                }
            }
        }
        _ => {}
    }
}

fn rewrite_captures_in_expr(expr: &mut Expr, captured: &BTreeSet<String>) {
    // Substitui `Ident(cap)` por `this.cap`. Aplica recursivamente, mas
    // primeiro trata o no atual.
    if let Expr::Ident(id) = expr {
        if captured.contains(id.sym.as_str()) {
            *expr = member_this(id.sym.as_str());
            return;
        }
    }
    match expr {
        Expr::Member(m) => {
            rewrite_captures_in_expr(&mut m.obj, captured);
            if let swc_ecma_ast::MemberProp::Computed(c) = &mut m.prop {
                rewrite_captures_in_expr(&mut c.expr, captured);
            }
        }
        Expr::Bin(b) => {
            rewrite_captures_in_expr(&mut b.left, captured);
            rewrite_captures_in_expr(&mut b.right, captured);
        }
        Expr::Assign(a) => {
            // Reescreve target Ident(cap) -> this.cap.
            if let swc_ecma_ast::AssignTarget::Simple(
                swc_ecma_ast::SimpleAssignTarget::Ident(id),
            ) = &a.left
            {
                if captured.contains(id.id.sym.as_str()) {
                    let name = id.id.sym.to_string();
                    a.left = swc_ecma_ast::AssignTarget::Simple(
                        swc_ecma_ast::SimpleAssignTarget::Member(member_this_target(&name)),
                    );
                }
            }
            if let swc_ecma_ast::AssignTarget::Simple(
                swc_ecma_ast::SimpleAssignTarget::Member(m),
            ) = &mut a.left
            {
                rewrite_captures_in_expr(&mut m.obj, captured);
            }
            rewrite_captures_in_expr(&mut a.right, captured);
        }
        Expr::Unary(u) => rewrite_captures_in_expr(&mut u.arg, captured),
        Expr::Update(u) => {
            // `i++` / `++i` com `i` capturado -> `this.i++`.
            if let Expr::Ident(id) = u.arg.as_ref() {
                if captured.contains(id.sym.as_str()) {
                    let name = id.sym.to_string();
                    *u.arg = member_this(&name);
                    return;
                }
            }
            rewrite_captures_in_expr(&mut u.arg, captured);
        }
        Expr::Cond(c) => {
            rewrite_captures_in_expr(&mut c.test, captured);
            rewrite_captures_in_expr(&mut c.cons, captured);
            rewrite_captures_in_expr(&mut c.alt, captured);
        }
        Expr::Call(c) => {
            if let swc_ecma_ast::Callee::Expr(callee) = &mut c.callee {
                rewrite_captures_in_expr(callee, captured);
            }
            for arg in c.args.iter_mut() {
                rewrite_captures_in_expr(&mut arg.expr, captured);
            }
        }
        Expr::New(n) => {
            rewrite_captures_in_expr(&mut n.callee, captured);
            if let Some(args) = n.args.as_mut() {
                for arg in args.iter_mut() {
                    rewrite_captures_in_expr(&mut arg.expr, captured);
                }
            }
        }
        Expr::Paren(p) => rewrite_captures_in_expr(&mut p.expr, captured),
        Expr::TsAs(a) => rewrite_captures_in_expr(&mut a.expr, captured),
        Expr::TsNonNull(n) => rewrite_captures_in_expr(&mut n.expr, captured),
        Expr::Array(arr) => {
            for elem in arr.elems.iter_mut().flatten() {
                rewrite_captures_in_expr(&mut elem.expr, captured);
            }
        }
        Expr::Object(obj) => {
            for p in obj.props.iter_mut() {
                if let swc_ecma_ast::PropOrSpread::Prop(pp) = p {
                    if let swc_ecma_ast::Prop::KeyValue(kv) = pp.as_mut() {
                        rewrite_captures_in_expr(&mut kv.value, captured);
                    }
                }
            }
        }
        Expr::Tpl(t) => {
            for e in t.exprs.iter_mut() {
                rewrite_captures_in_expr(e, captured);
            }
        }
        _ => {}
    }
}

fn ident_expr(name: &str) -> Expr {
    Expr::Ident(swc_ecma_ast::Ident {
        span: Default::default(),
        ctxt: Default::default(),
        sym: name.into(),
        optional: false,
    })
}

fn member_this(name: &str) -> Expr {
    Expr::Member(swc_ecma_ast::MemberExpr {
        span: Default::default(),
        obj: Box::new(Expr::This(swc_ecma_ast::ThisExpr {
            span: Default::default(),
        })),
        prop: swc_ecma_ast::MemberProp::Ident(swc_ecma_ast::IdentName::new(
            name.into(),
            Default::default(),
        )),
    })
}

fn member_this_target(name: &str) -> swc_ecma_ast::MemberExpr {
    swc_ecma_ast::MemberExpr {
        span: Default::default(),
        obj: Box::new(Expr::This(swc_ecma_ast::ThisExpr {
            span: Default::default(),
        })),
        prop: swc_ecma_ast::MemberProp::Ident(swc_ecma_ast::IdentName::new(
            name.into(),
            Default::default(),
        )),
    }
}

// ---------------------------------------------------------------------------
// (3) Reescrita do for-of sobre instancia com iterator custom.
// ---------------------------------------------------------------------------

fn rewrite_for_of_in_stmt(stmt: &mut Stmt, iter_classes: &HashSet<String>) {
    match stmt {
        Stmt::ForOf(_) => {
            if let Some(new_stmt) = try_rewrite_for_of(stmt, iter_classes) {
                *stmt = new_stmt;
                // Reescreve recursivamente o bloco gerado (BODY pode ter
                // for-of aninhado).
                rewrite_for_of_in_stmt(stmt, iter_classes);
            } else if let Stmt::ForOf(f) = stmt {
                rewrite_for_of_in_stmt(&mut f.body, iter_classes);
            }
        }
        Stmt::Block(b) => {
            for s in b.stmts.iter_mut() {
                rewrite_for_of_in_stmt(s, iter_classes);
            }
        }
        Stmt::If(i) => {
            rewrite_for_of_in_stmt(&mut i.cons, iter_classes);
            if let Some(alt) = i.alt.as_mut() {
                rewrite_for_of_in_stmt(alt, iter_classes);
            }
        }
        Stmt::While(w) => rewrite_for_of_in_stmt(&mut w.body, iter_classes),
        Stmt::DoWhile(w) => rewrite_for_of_in_stmt(&mut w.body, iter_classes),
        Stmt::For(f) => rewrite_for_of_in_stmt(&mut f.body, iter_classes),
        Stmt::ForIn(f) => rewrite_for_of_in_stmt(&mut f.body, iter_classes),
        Stmt::Try(t) => {
            for s in t.block.stmts.iter_mut() {
                rewrite_for_of_in_stmt(s, iter_classes);
            }
            if let Some(h) = t.handler.as_mut() {
                for s in h.body.stmts.iter_mut() {
                    rewrite_for_of_in_stmt(s, iter_classes);
                }
            }
            if let Some(fin) = t.finalizer.as_mut() {
                for s in fin.stmts.iter_mut() {
                    rewrite_for_of_in_stmt(s, iter_classes);
                }
            }
        }
        Stmt::Switch(s) => {
            for c in s.cases.iter_mut() {
                for st in c.cons.iter_mut() {
                    rewrite_for_of_in_stmt(st, iter_classes);
                }
            }
        }
        _ => {}
    }
}

/// Se o for-of itera uma instancia de classe iterator-custom, retorna o
/// bloco-protocolo equivalente. Caso contrario, None.
fn try_rewrite_for_of(stmt: &Stmt, iter_classes: &HashSet<String>) -> Option<Stmt> {
    let Stmt::ForOf(f) = stmt else { return None };
    if f.is_await {
        return None;
    }
    // Resolve a classe do `right`.
    if !expr_is_iter_class_instance(&f.right, iter_classes) {
        return None;
    }
    // Bind: aceita `for (const N of ...)` com N ident simples.
    let bind_name = match &f.left {
        swc_ecma_ast::ForHead::VarDecl(vd) => {
            if vd.decls.len() != 1 {
                return None;
            }
            match &vd.decls[0].name {
                swc_ecma_ast::Pat::Ident(id) => id.id.sym.to_string(),
                _ => return None,
            }
        }
        swc_ecma_ast::ForHead::Pat(p) => match p.as_ref() {
            swc_ecma_ast::Pat::Ident(id) => id.id.sym.to_string(),
            _ => return None,
        },
        _ => return None,
    };

    let uniq = format!("{:p}", &f.span);
    let it_name = format!("__rts_it_{}", uniq);
    let res_name = format!("__rts_itr_{}", uniq);

    // const __it = (EXPR).__rts_sym_iterator();
    let it_init = call_method(unwrap_expr(&f.right), SYM_ITER_DST, vec![]);
    let decl_it = const_decl(&it_name, it_init);

    // while (true) { const __r = __it.next(); if (__r.done) break;
    //   const N = __r.value; BODY }
    let r_init = call_method(ident_expr(&it_name), "next", vec![]);
    let decl_r = const_decl(&res_name, r_init);
    let if_done = Stmt::If(swc_ecma_ast::IfStmt {
        span: Default::default(),
        test: Box::new(member(ident_expr(&res_name), "done")),
        cons: Box::new(Stmt::Break(swc_ecma_ast::BreakStmt {
            span: Default::default(),
            label: None,
        })),
        alt: None,
    });
    let decl_bind = const_decl(&bind_name, member(ident_expr(&res_name), "value"));

    let mut while_body: Vec<Stmt> = vec![decl_r, if_done, decl_bind];
    // Corpo original.
    match f.body.as_ref() {
        Stmt::Block(b) => while_body.extend(b.stmts.iter().cloned()),
        other => while_body.push(other.clone()),
    }
    let while_stmt = Stmt::While(swc_ecma_ast::WhileStmt {
        span: Default::default(),
        test: Box::new(Expr::Lit(swc_ecma_ast::Lit::Bool(swc_ecma_ast::Bool {
            span: Default::default(),
            value: true,
        }))),
        body: Box::new(Stmt::Block(swc_ecma_ast::BlockStmt {
            span: Default::default(),
            ctxt: Default::default(),
            stmts: while_body,
        })),
    });

    Some(Stmt::Block(swc_ecma_ast::BlockStmt {
        span: Default::default(),
        ctxt: Default::default(),
        stmts: vec![decl_it, while_stmt],
    }))
}

fn unwrap_expr(e: &Expr) -> Expr {
    match e {
        Expr::Paren(p) => unwrap_expr(&p.expr),
        Expr::TsAs(a) => unwrap_expr(&a.expr),
        Expr::TsNonNull(n) => unwrap_expr(&n.expr),
        Expr::TsConstAssertion(a) => unwrap_expr(&a.expr),
        other => other.clone(),
    }
}

fn expr_is_iter_class_instance(e: &Expr, iter_classes: &HashSet<String>) -> bool {
    match e {
        Expr::Paren(p) => expr_is_iter_class_instance(&p.expr, iter_classes),
        Expr::TsAs(a) => expr_is_iter_class_instance(&a.expr, iter_classes),
        Expr::TsNonNull(n) => expr_is_iter_class_instance(&n.expr, iter_classes),
        Expr::TsConstAssertion(a) => expr_is_iter_class_instance(&a.expr, iter_classes),
        Expr::New(n) => {
            if let Expr::Ident(id) = n.callee.as_ref() {
                iter_classes.contains(id.sym.as_str())
            } else {
                false
            }
        }
        _ => false,
    }
}

fn call_method(obj: Expr, method: &str, args: Vec<Expr>) -> Expr {
    Expr::Call(swc_ecma_ast::CallExpr {
        span: Default::default(),
        ctxt: Default::default(),
        callee: swc_ecma_ast::Callee::Expr(Box::new(member(obj, method))),
        args: args
            .into_iter()
            .map(|e| swc_ecma_ast::ExprOrSpread {
                spread: None,
                expr: Box::new(e),
            })
            .collect(),
        type_args: None,
    })
}

fn member(obj: Expr, prop: &str) -> Expr {
    Expr::Member(swc_ecma_ast::MemberExpr {
        span: Default::default(),
        obj: Box::new(obj),
        prop: swc_ecma_ast::MemberProp::Ident(swc_ecma_ast::IdentName::new(
            prop.into(),
            Default::default(),
        )),
    })
}

fn const_decl(name: &str, init: Expr) -> Stmt {
    Stmt::Decl(swc_ecma_ast::Decl::Var(Box::new(swc_ecma_ast::VarDecl {
        span: Default::default(),
        ctxt: Default::default(),
        kind: swc_ecma_ast::VarDeclKind::Const,
        declare: false,
        decls: vec![swc_ecma_ast::VarDeclarator {
            span: Default::default(),
            name: swc_ecma_ast::Pat::Ident(swc_ecma_ast::BindingIdent {
                id: swc_ecma_ast::Ident {
                    span: Default::default(),
                    ctxt: Default::default(),
                    sym: name.into(),
                    optional: false,
                },
                type_ann: None,
            }),
            init: Some(Box::new(init)),
            definite: false,
        }],
    })))
}
