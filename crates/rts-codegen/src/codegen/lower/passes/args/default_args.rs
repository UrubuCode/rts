//! Pass que injeta defaults de parametros nos callsites.
//!
//! Pra cada user fn ou class method com pelo menos um param `: T = expr`,
//! preenche callsites que omitem aqueles args com clones do default.

use swc_ecma_ast::{Callee, Expr, Stmt};

use crate::parser::ast::{ClassMember, Item, Parameter, Program, Statement};

thread_local! {
    /// (cross-runtime closures) `fn_name -> [Option<f64 literal default>]` por
    /// param. Lido em `main_fn` para registrar os defaults no runtime, de modo
    /// que chamadas INDIRETAS (`fn(...args)` via handle) apliquem `acc = 1` em
    /// vez de preencher com 0. Só literais numéricos/bool (default complexo =
    /// None → sem default registrado, comportamento antigo de pad-0).
    static FN_DEFAULT_LITS: std::cell::RefCell<
        std::collections::HashMap<String, Vec<Option<f64>>>,
    > = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Consulta os defaults literais (numéricos) registrados para uma user fn.
pub(crate) fn fn_default_lits(name: &str) -> Option<Vec<Option<f64>>> {
    FN_DEFAULT_LITS.with(|c| c.borrow().get(name).cloned())
}

/// Registra os defaults literais de uma fn pelos seus `Parameter`s finais
/// (pós-hoist). Cobre fns liftadas (fn-expr inline `trampoline(function f(){}))`
/// que o pass `expand_default_args` (pré-hoist, só top-level) não alcança.
pub(crate) fn record_fn_default_lits(name: &str, params: &[Parameter]) {
    let lits: Vec<Option<f64>> = params
        .iter()
        .map(|p| p.default.as_deref().and_then(default_lit_value))
        .collect();
    if lits.iter().any(|v| v.is_some()) {
        FN_DEFAULT_LITS.with(|c| {
            c.borrow_mut().insert(name.to_string(), lits);
        });
    }
}

/// Extrai o valor numérico de um default literal (`1`, `-2`, `true`, `0`).
/// `None` para defaults complexos (expr, ref a param, string, etc).
fn default_lit_value(expr: &Expr) -> Option<f64> {
    match expr {
        Expr::Lit(swc_ecma_ast::Lit::Num(n)) => Some(n.value),
        Expr::Lit(swc_ecma_ast::Lit::Bool(b)) => Some(if b.value { 1.0 } else { 0.0 }),
        Expr::Unary(u) if matches!(u.op, swc_ecma_ast::UnaryOp::Minus) => {
            default_lit_value(&u.arg).map(|v| -v)
        }
        Expr::Paren(p) => default_lit_value(&p.expr),
        _ => None,
    }
}

/// (375) Preenche args omitidos numa chamada `super(...)` com os defaults do
/// constructor do pai. `super(name)` com `Animal`'s `constructor(name, e=100)`
/// vira `super(name, 100)`. Percorre o stmt procurando `Expr::Call` com callee
/// Super (top-level ou dentro de Expr/If/Block).
fn fill_super_call_defaults(
    stmt: &mut Stmt,
    param_names: &[String],
    defaults: &[Option<Box<Expr>>],
) {
    fn fill_in_expr(e: &mut Expr, param_names: &[String], defaults: &[Option<Box<Expr>>]) {
        if let Expr::Call(call) = e {
            if matches!(call.callee, Callee::Super(_)) {
                let provided = call.args.len();
                let total = defaults.len();
                if provided < total {
                    let mut bindings: std::collections::HashMap<String, Expr> =
                        std::collections::HashMap::new();
                    for (idx, arg) in call.args.iter().enumerate() {
                        if let Some(pn) = param_names.get(idx) {
                            bindings.insert(pn.clone(), (*arg.expr).clone());
                        }
                    }
                    for i in provided..total {
                        if let Some(def) = &defaults[i] {
                            let mut def_clone = (**def).clone();
                            substitute_param_refs(&mut def_clone, &bindings);
                            if let Some(pn) = param_names.get(i) {
                                bindings.insert(pn.clone(), def_clone.clone());
                            }
                            call.args.push(swc_ecma_ast::ExprOrSpread {
                                spread: None,
                                expr: Box::new(def_clone),
                            });
                        } else {
                            break;
                        }
                    }
                }
            }
        }
    }
    match stmt {
        Stmt::Expr(e) => fill_in_expr(&mut e.expr, param_names, defaults),
        Stmt::Block(b) => {
            for s in &mut b.stmts {
                fill_super_call_defaults(s, param_names, defaults);
            }
        }
        Stmt::If(i) => {
            fill_super_call_defaults(&mut i.cons, param_names, defaults);
            if let Some(alt) = i.alt.as_deref_mut() {
                fill_super_call_defaults(alt, param_names, defaults);
            }
        }
        _ => {}
    }
}

/// (#640) Substitui referencias a Ident(param_name) por exprs ja resolvidas
/// do callsite. Isso permite defaults que referenciam outros params funcionarem
/// quando o callsite preenche o slot indireto (ex: `rect(5)` com
/// `fn rect(w, h = w)` -> `rect(5, 5)` em vez de `rect(5, w)`).
fn substitute_param_refs(
    expr: &mut Expr,
    bindings: &std::collections::HashMap<String, Expr>,
) {
    match expr {
        Expr::Ident(id) => {
            if let Some(replacement) = bindings.get(id.sym.as_str()) {
                *expr = replacement.clone();
            }
        }
        Expr::Call(call) => {
            for a in call.args.iter_mut() {
                substitute_param_refs(&mut a.expr, bindings);
            }
            if let Callee::Expr(ce) = &mut call.callee {
                substitute_param_refs(ce, bindings);
            }
        }
        Expr::Bin(b) => {
            substitute_param_refs(&mut b.left, bindings);
            substitute_param_refs(&mut b.right, bindings);
        }
        Expr::Unary(u) => substitute_param_refs(&mut u.arg, bindings),
        Expr::Update(u) => substitute_param_refs(&mut u.arg, bindings),
        Expr::Cond(c) => {
            substitute_param_refs(&mut c.test, bindings);
            substitute_param_refs(&mut c.cons, bindings);
            substitute_param_refs(&mut c.alt, bindings);
        }
        Expr::Member(m) => substitute_param_refs(&mut m.obj, bindings),
        Expr::Paren(p) => substitute_param_refs(&mut p.expr, bindings),
        Expr::Tpl(t) => {
            for e in &mut t.exprs {
                substitute_param_refs(e, bindings);
            }
        }
        Expr::Array(a) => {
            for el in a.elems.iter_mut().flatten() {
                substitute_param_refs(&mut el.expr, bindings);
            }
        }
        Expr::Assign(a) => substitute_param_refs(&mut a.right, bindings),
        _ => {}
    }
}

pub(crate) fn expand_default_args(program: &mut Program) {
    use std::collections::HashMap;
    FN_DEFAULT_LITS.with(|c| c.borrow_mut().clear());

    // Mapa: nome → (param_names, defaults). param_names eh usado para
    // resolver refs cruzadas (#640): default que referencia outro param
    // (`fn rect(w, h = w)`) substitui a ref pelo expr ja resolvido no
    // callsite.
    let mut fn_defaults: HashMap<String, (Vec<String>, Vec<Option<Box<Expr>>>)> = HashMap::new();
    let mut method_defaults: HashMap<(String, String), (Vec<String>, Vec<Option<Box<Expr>>>)> = HashMap::new();
    // (375) Hierarquia + quais classes tem constructor proprio, pra propagar
    // o constructor herdado a subclasses sem `constructor` declarado.
    let mut class_super: HashMap<String, Option<String>> = HashMap::new();
    let mut has_own_ctor: std::collections::HashSet<String> = std::collections::HashSet::new();

    fn extract_names(params: &[Parameter]) -> Vec<String> {
        params.iter().map(|p| p.name.clone()).collect()
    }

    for item in &program.items {
        match item {
            Item::Function(f) => {
                if f.parameters.iter().any(|p| p.default.is_some()) {
                    let names = extract_names(&f.parameters);
                    let defaults: Vec<Option<Box<Expr>>> =
                        f.parameters.iter().map(|p| p.default.clone()).collect();
                    // (cross-runtime closures) Guarda os defaults LITERAIS p/ o
                    // runtime aplicar em chamadas indiretas.
                    let lits: Vec<Option<f64>> = f
                        .parameters
                        .iter()
                        .map(|p| p.default.as_deref().and_then(default_lit_value))
                        .collect();
                    if lits.iter().any(|v| v.is_some()) {
                        FN_DEFAULT_LITS.with(|c| {
                            c.borrow_mut().insert(f.name.clone(), lits);
                        });
                    }
                    fn_defaults.insert(f.name.clone(), (names, defaults));
                }
            }
            Item::Class(c) => {
                class_super.insert(c.name.clone(), c.super_class.clone());
                if c.members.iter().any(|m| matches!(m, ClassMember::Constructor(_))) {
                    has_own_ctor.insert(c.name.clone());
                }
                for m in &c.members {
                    if let ClassMember::Method(method) = m {
                        if method.parameters.iter().any(|p| p.default.is_some()) {
                            let names = extract_names(&method.parameters);
                            let defaults: Vec<Option<Box<Expr>>> = method
                                .parameters
                                .iter()
                                .map(|p| p.default.clone())
                                .collect();
                            method_defaults.insert(
                                (c.name.clone(), method.name.clone()),
                                (names, defaults),
                            );
                        }
                    }
                    // (375) Constructor com default params: registra sob chave
                    // especial `(Class, "<constructor>")` pra que `new Class()`
                    // preencha os slots omitidos. Sem isso `constructor(e=100)`
                    // + `new A()` deixava `e` como sentinel/0.
                    if let ClassMember::Constructor(ctor) = m {
                        if ctor.parameters.iter().any(|p| p.default.is_some()) {
                            let names = extract_names(&ctor.parameters);
                            let defaults: Vec<Option<Box<Expr>>> = ctor
                                .parameters
                                .iter()
                                .map(|p| p.default.clone())
                                .collect();
                            method_defaults.insert(
                                (c.name.clone(), "<constructor>".to_string()),
                                (names, defaults),
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // (375) Propaga o constructor herdado: subclasse sem `constructor`
    // proprio usa os defaults do constructor do ancestral mais proximo que
    // tem um. `class Dog extends Animal {}` + `new Dog("Rex")` aplica os
    // defaults de `Animal`'s constructor.
    let class_names: Vec<String> = class_super.keys().cloned().collect();
    for cn in class_names {
        if has_own_ctor.contains(&cn) {
            continue;
        }
        if method_defaults.contains_key(&(cn.clone(), "<constructor>".to_string())) {
            continue;
        }
        // Caminha ancestrais ate achar um com constructor-defaults.
        let mut cur = class_super.get(&cn).cloned().flatten();
        let mut guard = 0;
        while let Some(parent) = cur {
            guard += 1;
            if guard > 64 {
                break;
            }
            if let Some(defs) =
                method_defaults.get(&(parent.clone(), "<constructor>".to_string())).cloned()
            {
                method_defaults.insert((cn.clone(), "<constructor>".to_string()), defs);
                break;
            }
            if has_own_ctor.contains(&parent) {
                // Ancestral tem constructor proprio mas sem defaults — para.
                break;
            }
            cur = class_super.get(&parent).cloned().flatten();
        }
    }

    if fn_defaults.is_empty() && method_defaults.is_empty() {
        return;
    }

    // Reescreve callsites.
    for item in program.items.iter_mut() {
        match item {
            Item::Function(f) => {
                for s in f.body.iter_mut() {
                    let Statement::Raw(raw) = s;
                    if let Some(stmt) = raw.stmt.as_mut() {
                        expand_in_stmt(stmt, &fn_defaults, &method_defaults);
                    }
                }
            }
            Item::Class(c) => {
                // (375) Defaults do constructor do pai pra preencher
                // `super(...)` com args omitidos.
                let parent_ctor_defaults = class_super
                    .get(&c.name)
                    .cloned()
                    .flatten()
                    .and_then(|p| {
                        method_defaults.get(&(p, "<constructor>".to_string())).cloned()
                    });
                for m in c.members.iter_mut() {
                    match m {
                        ClassMember::Constructor(ctor) => {
                            for s in ctor.body.iter_mut() {
                                let Statement::Raw(raw) = s;
                                if let Some(stmt) = raw.stmt.as_mut() {
                                    expand_in_stmt(stmt, &fn_defaults, &method_defaults);
                                    if let Some((pnames, pdefs)) = &parent_ctor_defaults {
                                        fill_super_call_defaults(stmt, pnames, pdefs);
                                    }
                                }
                            }
                        }
                        ClassMember::Method(method) => {
                            for s in method.body.iter_mut() {
                                let Statement::Raw(raw) = s;
                                if let Some(stmt) = raw.stmt.as_mut() {
                                    expand_in_stmt(stmt, &fn_defaults, &method_defaults);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Item::Statement(Statement::Raw(raw)) => {
                if let Some(stmt) = raw.stmt.as_mut() {
                    expand_in_stmt(stmt, &fn_defaults, &method_defaults);
                }
            }
            _ => {}
        }
    }
}

fn expand_in_stmt(
    stmt: &mut Stmt,
    fn_defaults: &std::collections::HashMap<String, (Vec<String>, Vec<Option<Box<Expr>>>)>,
    method_defaults: &std::collections::HashMap<(String, String), (Vec<String>, Vec<Option<Box<Expr>>>)>,
) {
    use swc_ecma_ast::Stmt::*;
    match stmt {
        Expr(e) => expand_in_expr(&mut e.expr, fn_defaults, method_defaults),
        Return(r) => {
            if let Some(a) = r.arg.as_deref_mut() {
                expand_in_expr(a, fn_defaults, method_defaults);
            }
        }
        If(i) => {
            expand_in_expr(&mut i.test, fn_defaults, method_defaults);
            expand_in_stmt(&mut i.cons, fn_defaults, method_defaults);
            if let Some(alt) = i.alt.as_deref_mut() {
                expand_in_stmt(alt, fn_defaults, method_defaults);
            }
        }
        Block(b) => {
            for s in &mut b.stmts {
                expand_in_stmt(s, fn_defaults, method_defaults);
            }
        }
        While(w) => {
            expand_in_expr(&mut w.test, fn_defaults, method_defaults);
            expand_in_stmt(&mut w.body, fn_defaults, method_defaults);
        }
        DoWhile(w) => {
            expand_in_expr(&mut w.test, fn_defaults, method_defaults);
            expand_in_stmt(&mut w.body, fn_defaults, method_defaults);
        }
        For(f) => {
            if let Some(init) = f.init.as_mut() {
                if let swc_ecma_ast::VarDeclOrExpr::VarDecl(vd) = init {
                    for d in &mut vd.decls {
                        if let Some(e) = d.init.as_deref_mut() {
                            expand_in_expr(e, fn_defaults, method_defaults);
                        }
                    }
                }
            }
            if let Some(t) = f.test.as_deref_mut() {
                expand_in_expr(t, fn_defaults, method_defaults);
            }
            if let Some(u) = f.update.as_deref_mut() {
                expand_in_expr(u, fn_defaults, method_defaults);
            }
            expand_in_stmt(&mut f.body, fn_defaults, method_defaults);
        }
        ForOf(f) => {
            expand_in_expr(&mut f.right, fn_defaults, method_defaults);
            expand_in_stmt(&mut f.body, fn_defaults, method_defaults);
        }
        Decl(swc_ecma_ast::Decl::Var(v)) => {
            for d in &mut v.decls {
                if let Some(e) = d.init.as_deref_mut() {
                    expand_in_expr(e, fn_defaults, method_defaults);
                }
            }
        }
        Try(t) => {
            for s in &mut t.block.stmts {
                expand_in_stmt(s, fn_defaults, method_defaults);
            }
            if let Some(h) = t.handler.as_mut() {
                for s in &mut h.body.stmts {
                    expand_in_stmt(s, fn_defaults, method_defaults);
                }
            }
            if let Some(f) = t.finalizer.as_mut() {
                for s in &mut f.stmts {
                    expand_in_stmt(s, fn_defaults, method_defaults);
                }
            }
        }
        _ => {}
    }
}

fn expand_in_expr(
    expr: &mut Expr,
    fn_defaults: &std::collections::HashMap<String, (Vec<String>, Vec<Option<Box<Expr>>>)>,
    method_defaults: &std::collections::HashMap<(String, String), (Vec<String>, Vec<Option<Box<Expr>>>)>,
) {
    // Recurse primeiro para que callsites internos também sejam expandidos.
    match expr {
        Expr::Call(call) => {
            // Recurse em args primeiro (call aninhado).
            for a in call.args.iter_mut() {
                expand_in_expr(&mut a.expr, fn_defaults, method_defaults);
            }
            if let Callee::Expr(callee_expr) = &mut call.callee {
                expand_in_expr(callee_expr, fn_defaults, method_defaults);
            }
            // Detecta callee:
            //   - Ident("f") → fn_defaults["f"]
            //   - Member(this, "m") em método de classe → não temos
            //     contexto da classe aqui, então skip; será tratado em
            //     dispatch virtual posterior.
            //   - Member(obj_local, "m") onde obj_local é Ident — skip por
            //     mesmo motivo.
            //
            // Cobertura: defaults de fns top-level. Defaults em métodos
            // ficam parcialmente cobertos no codegen futuro (não nesse
            // pass).
            let fn_name = if let Callee::Expr(ce) = &call.callee {
                if let Expr::Ident(id) = ce.as_ref() {
                    Some(id.sym.to_string())
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(name) = fn_name {
                if let Some((param_names, defaults)) = fn_defaults.get(&name) {
                    let provided = call.args.len();
                    let total = defaults.len();
                    if provided < total {
                        // (#640) Build bindings: param_name -> expr ja
                        // resolvido no callsite. Permite que defaults
                        // que referenciam outros params (`h = w`)
                        // substituam para o expr correto.
                        let mut bindings: std::collections::HashMap<String, Expr> =
                            std::collections::HashMap::new();
                        for (idx, arg) in call.args.iter().enumerate() {
                            if let Some(pn) = param_names.get(idx) {
                                bindings.insert(pn.clone(), (*arg.expr).clone());
                            }
                        }
                        for i in provided..total {
                            if let Some(def) = &defaults[i] {
                                let mut def_clone = (**def).clone();
                                substitute_param_refs(&mut def_clone, &bindings);
                                expand_in_expr(&mut def_clone, fn_defaults, method_defaults);
                                // Default recem-resolvido tambem entra
                                // como binding pro proximo param.
                                if let Some(pn) = param_names.get(i) {
                                    bindings.insert(pn.clone(), def_clone.clone());
                                }
                                call.args.push(swc_ecma_ast::ExprOrSpread {
                                    spread: None,
                                    expr: Box::new(def_clone),
                                });
                            } else {
                                // Param sem default — TS exige que callsite
                                // proveja, codegen vai dar erro mais claro.
                                break;
                            }
                        }
                    }
                }
            }
        }
        Expr::Member(m) => expand_in_expr(&mut m.obj, fn_defaults, method_defaults),
        Expr::Bin(b) => {
            expand_in_expr(&mut b.left, fn_defaults, method_defaults);
            expand_in_expr(&mut b.right, fn_defaults, method_defaults);
        }
        Expr::Unary(u) => expand_in_expr(&mut u.arg, fn_defaults, method_defaults),
        Expr::Update(u) => expand_in_expr(&mut u.arg, fn_defaults, method_defaults),
        Expr::Assign(a) => {
            if let swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(m)) =
                &mut a.left
            {
                expand_in_expr(&mut m.obj, fn_defaults, method_defaults);
            }
            expand_in_expr(&mut a.right, fn_defaults, method_defaults);
        }
        Expr::New(n) => {
            // Recurse nos args providos primeiro.
            if let Some(args) = n.args.as_mut() {
                for a in args {
                    expand_in_expr(&mut a.expr, fn_defaults, method_defaults);
                }
            }
            // (375) Preenche defaults do constructor da classe quando o
            // callsite omite args. `new A()` com `constructor(e = 100)` ->
            // `new A(100)`.
            let class_name = if let Expr::Ident(id) = n.callee.as_ref() {
                Some(id.sym.to_string())
            } else {
                None
            };
            if let Some(cn) = class_name {
                if let Some((param_names, defaults)) =
                    method_defaults.get(&(cn, "<constructor>".to_string()))
                {
                    let args_vec = n.args.get_or_insert_with(Vec::new);
                    let provided = args_vec.len();
                    let total = defaults.len();
                    if provided < total {
                        let mut bindings: std::collections::HashMap<String, Expr> =
                            std::collections::HashMap::new();
                        for (idx, arg) in args_vec.iter().enumerate() {
                            if let Some(pn) = param_names.get(idx) {
                                bindings.insert(pn.clone(), (*arg.expr).clone());
                            }
                        }
                        for i in provided..total {
                            if let Some(def) = &defaults[i] {
                                let mut def_clone = (**def).clone();
                                substitute_param_refs(&mut def_clone, &bindings);
                                expand_in_expr(&mut def_clone, fn_defaults, method_defaults);
                                if let Some(pn) = param_names.get(i) {
                                    bindings.insert(pn.clone(), def_clone.clone());
                                }
                                args_vec.push(swc_ecma_ast::ExprOrSpread {
                                    spread: None,
                                    expr: Box::new(def_clone),
                                });
                            } else {
                                break;
                            }
                        }
                    }
                }
            }
        }
        Expr::Cond(c) => {
            expand_in_expr(&mut c.test, fn_defaults, method_defaults);
            expand_in_expr(&mut c.cons, fn_defaults, method_defaults);
            expand_in_expr(&mut c.alt, fn_defaults, method_defaults);
        }
        Expr::Paren(p) => expand_in_expr(&mut p.expr, fn_defaults, method_defaults),
        Expr::Tpl(t) => {
            for e in &mut t.exprs {
                expand_in_expr(e, fn_defaults, method_defaults);
            }
        }
        Expr::Array(a) => {
            for el in a.elems.iter_mut().flatten() {
                expand_in_expr(&mut el.expr, fn_defaults, method_defaults);
            }
        }
        _ => {}
    }
}
