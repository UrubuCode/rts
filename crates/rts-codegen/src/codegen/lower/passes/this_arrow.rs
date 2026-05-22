//! Arrow callback lifting + `this` rewriting/reverting.
//!
//! `lift_arrow_callbacks` percorre o programa identificando arrows
//! passados como callback I64 a APIs ABI (FLTK, thread.spawn, fs.watch,
//! etc.) e os promove para `Item::Function` sintetico — `__lifted_arrow_N`
//! ou `__class_C_lifted_arrow_N` se a arrow captura `this`/super da classe
//! envolvente.
//!
//! Helpers `arrow_uses_this` / `stmt_uses_this` / `expr_uses_this`
//! detectam o uso de `this`. `rewrite_*` substitui `this.foo` por
//! `__rts_under_this.foo` durante o lifting, e o conjunto `revert_*`
//! reverte essa substituicao em arrows aninhadas (que tem o proprio
//! escopo de `this`). `make_this_local` / `make_slot_assign` geram
//! prologos de slot pra atravessar o trampolim C.

use std::collections::{HashMap, HashSet};

use swc_ecma_ast::{Callee, Decl, Expr, Lit, MemberProp, Pat, Stmt, TsType, TsTypeRef};

use crate::parser::ast::{
    ClassMember, FunctionDecl, Item, MemberModifiers, Parameter, Program, RawStmt, Statement,
};
use crate::parser::span::Span;

use super::super::analysis::captures::{
    collect_captures_in_body, collect_local_decls, make_sync_param_to_global,
    promote_local_to_global, rename_uses_in_body,
};

/// (cross-runtime #41/#195) Promove `let` declaradas em escopo de
/// for/while/block top-level cujos identificadores sao capturados por
/// arrows lifted. Reescreve as referencias para um nome global.
///
/// Heuristica simples: para cada Stmt::For top-level com init = let,
/// se o corpo do for contem uma arrow que referencia esse ident,
/// promove a global `__cap_<i>_<var>`. Reescreve os usos no for inteiro.
fn promote_top_level_captures(program: &mut Program, new_globals: &mut Vec<String>) {
    use std::collections::HashSet;
    let mut counter: u32 = 0;
    let mut promoted_globals: Vec<(String, ())> = Vec::new();
    for item in program.items.iter_mut() {
        let Item::Statement(Statement::Raw(raw)) = item else { continue };
        let Some(stmt) = raw.stmt.as_mut() else { continue };
        scan_and_promote_stmt(stmt, &mut counter, new_globals, &mut promoted_globals);
    }
    // Declarar os globals como `let __cap_..._x = 0` no inicio do program,
    // antes da primeira fn/class/statement com referencia.
    if !promoted_globals.is_empty() {
        let mut new_decls: Vec<Item> = Vec::new();
        for (name, _) in &promoted_globals {
            // let <name> = 0; (top-level promove a global mutable via codegen)
            let var_decl = swc_ecma_ast::VarDecl {
                span: Default::default(),
                ctxt: Default::default(),
                kind: swc_ecma_ast::VarDeclKind::Let,
                declare: false,
                decls: vec![swc_ecma_ast::VarDeclarator {
                    span: Default::default(),
                    name: swc_ecma_ast::Pat::Ident(swc_ecma_ast::BindingIdent {
                        id: swc_ecma_ast::Ident {
                            span: Default::default(),
                            ctxt: Default::default(),
                            sym: name.as_str().into(),
                            optional: false,
                        },
                        type_ann: None,
                    }),
                    init: Some(Box::new(Expr::Lit(swc_ecma_ast::Lit::Num(swc_ecma_ast::Number {
                        span: Default::default(),
                        value: 0.0,
                        raw: None,
                    })))),
                    definite: false,
                }],
            };
            let stmt = Stmt::Decl(Decl::Var(Box::new(var_decl)));
            new_decls.push(Item::Statement(Statement::Raw(
                RawStmt::new("<promoted-global>".to_string(), Span::default()).with_stmt(stmt),
            )));
        }
        for (i, decl) in new_decls.into_iter().enumerate() {
            program.items.insert(i, decl);
        }
    }
}

/// Scaneia um Stmt procurando for(let var; ...) cujo body tem arrow
/// que captura var. Se encontrou, promove var → global e reescreve.
fn scan_and_promote_stmt(
    stmt: &mut Stmt,
    counter: &mut u32,
    new_globals: &mut Vec<String>,
    promoted_globals: &mut Vec<(String, ())>,
) {
    use std::collections::HashSet;
    // Caso direto: Stmt::For com init=let.
    if let Stmt::For(for_stmt) = stmt {
        if let Some(swc_ecma_ast::VarDeclOrExpr::VarDecl(init_vd)) = for_stmt.init.as_ref() {
            if matches!(init_vd.kind, swc_ecma_ast::VarDeclKind::Let | swc_ecma_ast::VarDeclKind::Var)
                && init_vd.decls.len() == 1
            {
                if let Pat::Ident(b) = &init_vd.decls[0].name {
                    let var_name = b.id.sym.to_string();
                    // Verifica se body contem arrow capturando var_name.
                    if arrow_inside_captures(&for_stmt.body, &var_name) {
                        let global_name = format!("__cap_{}_{}", counter, var_name);
                        *counter += 1;
                        new_globals.push(global_name.clone());
                        promoted_globals.push((global_name.clone(), ()));
                        // Reescreve init: substitui pelo assign ao global.
                        let init_expr = init_vd.decls[0].init.clone().unwrap_or_else(|| {
                            Box::new(Expr::Lit(swc_ecma_ast::Lit::Num(swc_ecma_ast::Number {
                                span: Default::default(),
                                value: 0.0,
                                raw: None,
                            })))
                        });
                        let assign = Expr::Assign(swc_ecma_ast::AssignExpr {
                            span: Default::default(),
                            op: swc_ecma_ast::AssignOp::Assign,
                            left: swc_ecma_ast::AssignTarget::Simple(
                                swc_ecma_ast::SimpleAssignTarget::Ident(
                                    swc_ecma_ast::BindingIdent {
                                        id: swc_ecma_ast::Ident {
                                            span: Default::default(),
                                            ctxt: Default::default(),
                                            sym: global_name.clone().into(),
                                            optional: false,
                                        },
                                        type_ann: None,
                                    },
                                ),
                            ),
                            right: init_expr,
                        });
                        for_stmt.init = Some(swc_ecma_ast::VarDeclOrExpr::Expr(Box::new(assign)));
                        // Reescreve refs em test/update/body.
                        if let Some(test) = for_stmt.test.as_mut() {
                            rename_ident_in_expr_local(test, &var_name, &global_name);
                        }
                        if let Some(update) = for_stmt.update.as_mut() {
                            rename_ident_in_expr_local(update, &var_name, &global_name);
                        }
                        rename_ident_in_stmt_local(&mut for_stmt.body, &var_name, &global_name);
                    }
                }
            }
        }
    }
    // Recursa em sub-statements (block, if, while, do-while, for-of, try).
    match stmt {
        Stmt::Block(b) => for s in &mut b.stmts { scan_and_promote_stmt(s, counter, new_globals, promoted_globals); }
        Stmt::If(i) => {
            scan_and_promote_stmt(&mut i.cons, counter, new_globals, promoted_globals);
            if let Some(a) = i.alt.as_mut() { scan_and_promote_stmt(a, counter, new_globals, promoted_globals); }
        }
        Stmt::While(w) => scan_and_promote_stmt(&mut w.body, counter, new_globals, promoted_globals),
        Stmt::DoWhile(w) => scan_and_promote_stmt(&mut w.body, counter, new_globals, promoted_globals),
        Stmt::For(f) => scan_and_promote_stmt(&mut f.body, counter, new_globals, promoted_globals),
        Stmt::ForIn(f) => scan_and_promote_stmt(&mut f.body, counter, new_globals, promoted_globals),
        Stmt::ForOf(f) => scan_and_promote_stmt(&mut f.body, counter, new_globals, promoted_globals),
        _ => {}
    }
}

/// Procura recursivamente por Arrow Expr cujo body captura `var_name`.
fn arrow_inside_captures(stmt: &Stmt, var_name: &str) -> bool {
    let mut found = false;
    fn scan_expr(e: &Expr, var: &str, found: &mut bool) {
        if *found { return; }
        if let Expr::Arrow(arrow) = e {
            // Verifica se body referencia var (sem ser shadowed por param).
            let shadowed = arrow.params.iter().any(|p| {
                if let Pat::Ident(b) = p {
                    b.id.sym.as_str() == var
                } else { false }
            });
            if !shadowed && arrow_body_refs(arrow, var) {
                *found = true;
            }
            return;
        }
        // Recursa.
        match e {
            Expr::Bin(b) => { scan_expr(&b.left, var, found); scan_expr(&b.right, var, found); }
            Expr::Unary(u) => scan_expr(&u.arg, var, found),
            Expr::Update(u) => scan_expr(&u.arg, var, found),
            Expr::Cond(c) => { scan_expr(&c.test, var, found); scan_expr(&c.cons, var, found); scan_expr(&c.alt, var, found); }
            Expr::Call(c) => {
                if let Callee::Expr(ce) = &c.callee { scan_expr(ce, var, found); }
                for a in &c.args { scan_expr(&a.expr, var, found); }
            }
            Expr::Assign(a) => scan_expr(&a.right, var, found),
            Expr::Paren(p) => scan_expr(&p.expr, var, found),
            Expr::Member(m) => scan_expr(&m.obj, var, found),
            Expr::Tpl(t) => for e in &t.exprs { scan_expr(e, var, found); }
            Expr::Array(arr) => for el in arr.elems.iter().flatten() { scan_expr(&el.expr, var, found); }
            _ => {}
        }
    }
    fn scan_stmt(s: &Stmt, var: &str, found: &mut bool) {
        if *found { return; }
        match s {
            Stmt::Expr(e) => scan_expr(&e.expr, var, found),
            Stmt::Return(r) => { if let Some(e) = r.arg.as_deref() { scan_expr(e, var, found); } }
            Stmt::Block(b) => for s in &b.stmts { scan_stmt(s, var, found); }
            Stmt::If(i) => { scan_expr(&i.test, var, found); scan_stmt(&i.cons, var, found); if let Some(a) = i.alt.as_deref() { scan_stmt(a, var, found); } }
            Stmt::Decl(Decl::Var(v)) => for d in &v.decls { if let Some(init) = d.init.as_deref() { scan_expr(init, var, found); } }
            Stmt::For(f) => { if let Some(t) = f.test.as_deref() { scan_expr(t, var, found); } scan_stmt(&f.body, var, found); }
            Stmt::ForOf(f) => { scan_expr(&f.right, var, found); scan_stmt(&f.body, var, found); }
            Stmt::While(w) => { scan_expr(&w.test, var, found); scan_stmt(&w.body, var, found); }
            _ => {}
        }
    }
    scan_stmt(stmt, var_name, &mut found);
    found
}

fn arrow_body_refs(arrow: &swc_ecma_ast::ArrowExpr, var: &str) -> bool {
    let body_stmts: Vec<Stmt> = match arrow.body.as_ref() {
        swc_ecma_ast::BlockStmtOrExpr::BlockStmt(b) => b.stmts.clone(),
        swc_ecma_ast::BlockStmtOrExpr::Expr(e) => vec![Stmt::Return(swc_ecma_ast::ReturnStmt {
            span: Default::default(),
            arg: Some(e.clone()),
        })],
    };
    let mut found = false;
    fn scan_expr(e: &Expr, var: &str, found: &mut bool) {
        if *found { return; }
        match e {
            Expr::Ident(id) => if id.sym.as_str() == var { *found = true; }
            Expr::Bin(b) => { scan_expr(&b.left, var, found); scan_expr(&b.right, var, found); }
            Expr::Unary(u) => scan_expr(&u.arg, var, found),
            Expr::Update(u) => scan_expr(&u.arg, var, found),
            Expr::Cond(c) => { scan_expr(&c.test, var, found); scan_expr(&c.cons, var, found); scan_expr(&c.alt, var, found); }
            Expr::Call(c) => { if let Callee::Expr(ce) = &c.callee { scan_expr(ce, var, found); } for a in &c.args { scan_expr(&a.expr, var, found); } }
            Expr::Assign(a) => scan_expr(&a.right, var, found),
            Expr::Paren(p) => scan_expr(&p.expr, var, found),
            Expr::Member(m) => scan_expr(&m.obj, var, found),
            Expr::Tpl(t) => for e in &t.exprs { scan_expr(e, var, found); }
            _ => {}
        }
    }
    fn scan_stmt(s: &Stmt, var: &str, found: &mut bool) {
        if *found { return; }
        match s {
            Stmt::Expr(e) => scan_expr(&e.expr, var, found),
            Stmt::Return(r) => { if let Some(e) = r.arg.as_deref() { scan_expr(e, var, found); } }
            Stmt::Block(b) => for s in &b.stmts { scan_stmt(s, var, found); }
            Stmt::If(i) => { scan_expr(&i.test, var, found); scan_stmt(&i.cons, var, found); if let Some(a) = i.alt.as_deref() { scan_stmt(a, var, found); } }
            Stmt::Decl(Decl::Var(v)) => for d in &v.decls { if let Some(init) = d.init.as_deref() { scan_expr(init, var, found); } }
            _ => {}
        }
    }
    for s in &body_stmts {
        scan_stmt(s, var, &mut found);
        if found { break; }
    }
    found
}

fn rename_ident_in_expr_local(e: &mut Expr, old: &str, new: &str) {
    match e {
        Expr::Ident(id) => { if id.sym.as_str() == old { id.sym = new.into(); } }
        Expr::Bin(b) => { rename_ident_in_expr_local(&mut b.left, old, new); rename_ident_in_expr_local(&mut b.right, old, new); }
        Expr::Unary(u) => rename_ident_in_expr_local(&mut u.arg, old, new),
        Expr::Update(u) => rename_ident_in_expr_local(&mut u.arg, old, new),
        Expr::Cond(c) => { rename_ident_in_expr_local(&mut c.test, old, new); rename_ident_in_expr_local(&mut c.cons, old, new); rename_ident_in_expr_local(&mut c.alt, old, new); }
        Expr::Call(c) => {
            if let Callee::Expr(ce) = &mut c.callee { rename_ident_in_expr_local(ce, old, new); }
            for a in &mut c.args { rename_ident_in_expr_local(&mut a.expr, old, new); }
        }
        Expr::Assign(a) => {
            if let swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Ident(b)) = &mut a.left {
                if b.id.sym.as_str() == old { b.id.sym = new.into(); }
            }
            rename_ident_in_expr_local(&mut a.right, old, new);
        }
        Expr::Paren(p) => rename_ident_in_expr_local(&mut p.expr, old, new),
        Expr::Member(m) => rename_ident_in_expr_local(&mut m.obj, old, new),
        Expr::Tpl(t) => for e in &mut t.exprs { rename_ident_in_expr_local(e, old, new); }
        Expr::Array(arr) => for el in arr.elems.iter_mut().flatten() { rename_ident_in_expr_local(&mut el.expr, old, new); }
        Expr::Arrow(a) => {
            // So' renomeia se nao for shadowed por param da arrow.
            let shadowed = a.params.iter().any(|p| if let Pat::Ident(b) = p { b.id.sym.as_str() == old } else { false });
            if !shadowed {
                match a.body.as_mut() {
                    swc_ecma_ast::BlockStmtOrExpr::BlockStmt(b) => for s in &mut b.stmts { rename_ident_in_stmt_local(s, old, new); }
                    swc_ecma_ast::BlockStmtOrExpr::Expr(e) => rename_ident_in_expr_local(e, old, new),
                }
            }
        }
        _ => {}
    }
}

fn rename_ident_in_stmt_local(s: &mut Stmt, old: &str, new: &str) {
    match s {
        Stmt::Expr(e) => rename_ident_in_expr_local(&mut e.expr, old, new),
        Stmt::Return(r) => { if let Some(e) = r.arg.as_mut() { rename_ident_in_expr_local(e, old, new); } }
        Stmt::Block(b) => for s in &mut b.stmts { rename_ident_in_stmt_local(s, old, new); }
        Stmt::If(i) => {
            rename_ident_in_expr_local(&mut i.test, old, new);
            rename_ident_in_stmt_local(&mut i.cons, old, new);
            if let Some(a) = i.alt.as_mut() { rename_ident_in_stmt_local(a, old, new); }
        }
        Stmt::Decl(Decl::Var(v)) => for d in &mut v.decls { if let Some(init) = d.init.as_mut() { rename_ident_in_expr_local(init, old, new); } }
        Stmt::For(f) => {
            if let Some(t) = f.test.as_mut() { rename_ident_in_expr_local(t, old, new); }
            if let Some(u) = f.update.as_mut() { rename_ident_in_expr_local(u, old, new); }
            rename_ident_in_stmt_local(&mut f.body, old, new);
        }
        Stmt::While(w) => {
            rename_ident_in_expr_local(&mut w.test, old, new);
            rename_ident_in_stmt_local(&mut w.body, old, new);
        }
        _ => {}
    }
}

fn sanitize_for_symbol(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Lifts inline `() => { ... }` arrow expressions that appear as `I64`-typed
/// ABI arguments into synthetic top-level `FunctionDecl`s so codegen can
/// emit a `func_addr` pointer for them.
///
/// The arrow in the raw SWC statement is replaced with an `Ident` naming
/// the synthetic function. Runs before Phase 1 (declaration) so the lifted
/// functions go through the normal declare → compile path.
pub(crate) fn lift_arrow_callbacks(program: &mut Program) -> HashSet<String> {
    let mut user_fn_names: HashSet<String> = program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(f) => Some(f.name.clone()),
            _ => None,
        })
        .collect();
    let mut user_fn_arities: HashMap<String, usize> = program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(f) => Some((f.name.clone(), f.parameters.len())),
            _ => None,
        })
        .collect();
    // Tipo declarado do primeiro param (ou None se sem annotation /
    // sem params). Usado pelo lifter de thread.spawn pra decidir se
    // injeta `num.f64_from_bits` no trampolim quando worker pede f64.
    let mut user_fn_first_param_ty: HashMap<String, Option<String>> = program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(f) => Some((
                f.name.clone(),
                f.parameters.first().and_then(|p| p.type_annotation.clone()),
            )),
            _ => None,
        })
        .collect();

    // Top-level aliases: `const fp = worker as unknown as number;`
    //                  ou `const fp = getPointer(worker);`
    // Marca `fp` como alias da user fn para o lifter detectar idents
    // wrappados (necessario p/ thread.spawn, sync.once_call etc).
    fn peel_for_alias<'a>(e: &'a Expr) -> &'a Expr {
        match e {
            Expr::TsAs(a) => peel_for_alias(&a.expr),
            Expr::TsTypeAssertion(a) => peel_for_alias(&a.expr),
            Expr::TsConstAssertion(a) => peel_for_alias(&a.expr),
            Expr::Paren(p) => peel_for_alias(&p.expr),
            // getPointer(fn) → fn
            Expr::Call(c) => {
                if let Callee::Expr(callee) = &c.callee {
                    if let Expr::Ident(id) = callee.as_ref() {
                        if id.sym.as_str() == "getPointer" {
                            if let Some(arg) = c.args.first() {
                                if arg.spread.is_none() {
                                    return peel_for_alias(&arg.expr);
                                }
                            }
                        }
                    }
                }
                e
            }
            _ => e,
        }
    }
    let snapshot = user_fn_names.clone();
    let mut alias_to_real: HashMap<String, String> = HashMap::new();
    for item in program.items.iter() {
        let Item::Statement(Statement::Raw(raw)) = item else { continue };
        let Some(Stmt::Decl(swc_ecma_ast::Decl::Var(var_decl))) = raw.stmt.as_ref() else { continue };
        for d in var_decl.decls.iter() {
            let Some(init) = d.init.as_deref() else { continue };
            let Expr::Ident(id) = peel_for_alias(init) else { continue };
            if !snapshot.contains(id.sym.as_str()) { continue; }
            let swc_ecma_ast::Pat::Ident(name) = &d.name else { continue };
            let alias = name.id.sym.to_string();
            user_fn_names.insert(alias.clone());
            if let Some(&arity) = user_fn_arities.get(id.sym.as_str()) {
                user_fn_arities.insert(alias.clone(), arity);
            }
            if let Some(ty) = user_fn_first_param_ty.get(id.sym.as_str()).cloned() {
                user_fn_first_param_ty.insert(alias.clone(), ty);
            }
            alias_to_real.insert(alias, id.sym.to_string());
        }
    }

    let mut acc = LiftAcc {
        counter: 0,
        new_fns: Vec::new(),
        new_globals: Vec::new(),
        user_fn_names,
        user_fn_arities,
        user_fn_first_param_ty,
        alias_to_real,
        needs_c_callconv: HashSet::new(),
    };

    // Pass 1: dentro de classes (constructors e métodos). Arrows que usam
    // `this` viram trampolins que leem o handle de uma global escrita no
    // callsite imediatamente antes do `widget_set_callback` (etc).
    for item in program.items.iter_mut() {
        let Item::Class(class) = item else { continue };
        let class_name = class.name.clone();
        for member in class.members.iter_mut() {
            match member {
                ClassMember::Constructor(ctor) => {
                    acc.lift_in_body(&class_name, &mut ctor.body, /*in_class=*/ true);
                }
                ClassMember::Method(method) if !method.modifiers.is_static => {
                    acc.lift_in_body(&class_name, &mut method.body, /*in_class=*/ true);
                }
                _ => {}
            }
        }
    }

    // Pass 1.5: funções user top-level. Arrows passados a callbacks ABI
    // dentro de uma fn capturam idents do escopo da fn (params + locais).
    // Para cada captura, criamos uma global `__cb_local_<fn>_<var>` e
    // reescrevemos *toda* referência ao ident na fn pra usar a global.
    // Limitação: múltiplas chamadas da mesma fn compartilham o estado
    // via global. OK pra fns que registram callback uma vez (setup
    // pattern); falha em recursão/reentrada.
    for item in program.items.iter_mut() {
        let Item::Function(f) = item else { continue };
        // Skip lifted/synthetic functions já processadas.
        if f.name.starts_with("__lifted_arrow_") || f.name.starts_with("__class_") {
            continue;
        }
        acc.lift_in_user_fn(f);
    }

    // (cross-runtime #41/#195) Pass 1.5: promote captures em statements
    // top-level que tem arrows capturando locais (e.g. for-let + arrow).
    // Sem isso, o lift abaixo gera `__lifted_arrow_N` sem acesso aos
    // locais e codegen falha com "undefined variable".
    promote_top_level_captures(program, &mut acc.new_globals);

    // Pass 2: top-level (arrows em script). Sem `this`. Mantém comportamento
    // anterior.
    let n = program.items.len();
    for i in 0..n {
        let Item::Statement(Statement::Raw(_)) = &program.items[i] else {
            continue;
        };
        // Extrair temporariamente para evitar conflito de borrow.
        let mut taken = std::mem::replace(
            &mut program.items[i],
            Item::Statement(Statement::Raw(RawStmt::new(String::new(), Span::default()))),
        );
        if let Item::Statement(Statement::Raw(raw)) = &mut taken {
            // Empacota num Vec<Statement> de 1 elemento e reaproveita a
            // varredura unificada.
            let placeholder = std::mem::replace(raw, RawStmt::new(String::new(), Span::default()));
            let mut body = vec![Statement::Raw(placeholder)];
            acc.lift_in_body("", &mut body, /*in_class=*/ false);
            // Reescreve o item top-level como o (possivelmente expandido) primeiro
            // statement; pré-statements do callsite (escrita do slot) vão como
            // Items adicionais a inserir.
            // Esperamos que body tenha 1+ statements; o primeiro vira o slot do
            // item original, o resto também vira items.
            let mut iter = body.into_iter();
            if let Some(first) = iter.next() {
                program.items[i] = Item::Statement(first);
                // Inserir os extras logo após. Coletamos num buffer e injetamos
                // depois pra não bagunçar o índice da iteração.
                for extra in iter {
                    acc.new_fns.push(Item::Statement(extra));
                }
            }
        }
    }

    // Globals dos slots `__cb_this_<id>` precisam ser declaradas top-level
    // antes de `collect_module_globals` rodar.
    let mut prepend: Vec<Item> = Vec::new();
    for global_name in acc.new_globals.into_iter() {
        // `let __cb_this_N: number = 0;`
        let var = swc_ecma_ast::VarDecl {
            span: Default::default(),
            ctxt: Default::default(),
            kind: swc_ecma_ast::VarDeclKind::Let,
            declare: false,
            decls: vec![swc_ecma_ast::VarDeclarator {
                span: Default::default(),
                name: Pat::Ident(swc_ecma_ast::BindingIdent {
                    id: swc_ecma_ast::Ident {
                        span: Default::default(),
                        ctxt: Default::default(),
                        sym: global_name.into(),
                        optional: false,
                    },
                    type_ann: Some(Box::new(swc_ecma_ast::TsTypeAnn {
                        span: Default::default(),
                        type_ann: Box::new(TsType::TsTypeRef(TsTypeRef {
                            span: Default::default(),
                            type_name: swc_ecma_ast::TsEntityName::Ident(swc_ecma_ast::Ident {
                                span: Default::default(),
                                ctxt: Default::default(),
                                sym: "i64".into(),
                                optional: false,
                            }),
                            type_params: None,
                        })),
                    })),
                }),
                init: Some(Box::new(Expr::Lit(Lit::Num(swc_ecma_ast::Number {
                    span: Default::default(),
                    value: 0.0,
                    raw: None,
                })))),
                definite: false,
            }],
        };
        let stmt = Stmt::Decl(Decl::Var(Box::new(var)));
        prepend.push(Item::Statement(Statement::Raw(
            RawStmt::new("<cb-slot>".to_string(), Span::default()).with_stmt(stmt),
        )));
    }

    // Funções lifted vão antes dos statements top-level pra fase 1 declará-las.
    for fn_item in acc.new_fns.into_iter().rev() {
        program.items.insert(0, fn_item);
    }
    for global_item in prepend.into_iter().rev() {
        program.items.insert(0, global_item);
    }
    acc.needs_c_callconv
}

struct LiftAcc {
    counter: u32,
    new_fns: Vec<Item>,
    /// Nomes de globais `__cb_this_N` a declarar como `let` top-level.
    new_globals: Vec<String>,
    user_fn_names: HashSet<String>,
    /// Aridade declarada de cada user fn / alias top-level — usada
    /// para que trampolins de `thread.spawn(fp, arg)` repassem o `arg`
    /// quando a worker fn aceita 1+ parâmetros (#206).
    user_fn_arities: HashMap<String, usize>,
    /// Tipo declarado do primeiro param (string raw da annotation, ex:
    /// "number", "i64") ou None. Quando worker de thread.spawn pede
    /// "number" (f64), o trampolim envolve `__rts_spawn_arg` em
    /// `num.f64_from_bits(...)` pra preservar o bit pattern.
    user_fn_first_param_ty: HashMap<String, Option<String>>,
    /// Mapa alias → user fn real para `const fp = worker as ...`. O
    /// trampolim deve invocar a fn real, não o alias (que vira const
    /// global e cai em call_indirect com sig errada).
    alias_to_real: HashMap<String, String>,
    /// User fns chamadas a partir de trampolins C-callconv (lifted)
    /// — devem ser declaradas com C callconv também para evitar
    /// corrupção de stack na fronteira (#206).
    needs_c_callconv: HashSet<String>,
}


impl LiftAcc {
    /// Processa uma função user (não-classe, não-lifted). Detecta locais
    /// capturadas em arrows passados a callbacks ABI, promove cada local
    /// pra global, e reescreve referências na fn inteira. Depois delega
    /// pra `lift_in_body` que faz o lift normal — nesse momento os idents
    /// capturados já apontam pra globais que existem em escopo do trampolim.
    fn lift_in_user_fn(&mut self, f: &mut FunctionDecl) {
        // Coleta locais declaradas e parâmetros — qualquer ident que
        // referencie um desses *dentro de um arrow* é uma captura.
        let mut locals: std::collections::HashSet<String> = std::collections::HashSet::new();
        for p in &f.parameters {
            locals.insert(p.name.clone());
        }
        collect_local_decls(&f.body, &mut locals);

        // Para cada arrow nos statements (recursivamente), descobre
        // quais idents da fn são capturados.
        let captured = collect_captures_in_body(&f.body, &locals);

        // Determina conjunto de parâmetros (vs locais declaradas).
        let param_names: std::collections::HashSet<String> =
            f.parameters.iter().map(|p| p.name.clone()).collect();

        // Promove cada captura pra global e reescreve toda a fn.
        // Insere as syncs de parâmetros no topo (em ordem reversa para
        // manter a ordem original).
        let mut param_syncs: Vec<(String, String)> = Vec::new(); // (global, param)
        for var in &captured {
            let global = format!("__cb_local_{}_{}", sanitize_for_symbol(&f.name), var);
            self.new_globals.push(global.clone());
            if param_names.contains(var) {
                // Parâmetro: precisa sincronizar valor inicial. A reescrita
                // não toca o param em si (continua recebendo o valor do
                // caller), mas todos os usos dentro da fn referem ao
                // global. Sync no topo: `<global> = <param>;`.
                param_syncs.push((global.clone(), var.clone()));
                // Reescreve usos no body (parâmetro permanece declarado).
                rename_uses_in_body(&mut f.body, var, &global);
            } else {
                // Local declarada: promote_local_to_global substitui o
                // `let <var> = expr` por `<global> = expr`.
                promote_local_to_global(&mut f.body, var, &global);
            }
        }

        // Insere syncs de parâmetros no início (ordem original preservada
        // via insert(0, ...) em ordem reversa).
        for (global, param) in param_syncs.iter().rev() {
            f.body.insert(0, make_sync_param_to_global(global, param));
        }

        // Agora roda o lift normal — idents nos arrows são globais,
        // resolvem sem problema.
        self.lift_in_body("", &mut f.body, /*in_class=*/ false);
    }

    /// Lift de uma arrow anônima (sem captura) para uma user fn sintética
    /// `__lifted_arrow_N`. Retorna o `Ident` que substitui a arrow no AST.
    /// Não trata captura de `this` — caller é responsável por garantir que
    /// a arrow não usa `this` (ou está fora de classe).
    fn lift_arrow_to_ident(
        &mut self,
        class_name: &str,
        arrow: &swc_ecma_ast::ArrowExpr,
        in_class: bool,
    ) -> swc_ecma_ast::Ident {
        let has_return_value = matches!(arrow.body.as_ref(), swc_ecma_ast::BlockStmtOrExpr::Expr(_));
        let raw_stmts = arrow_body_to_stmts(arrow);
        let mut body_stmts: Vec<Statement> = raw_stmts
            .into_iter()
            .map(|s| {
                Statement::Raw(
                    RawStmt::new("<lifted>".to_string(), Span::default()).with_stmt(s),
                )
            })
            .collect();

        let syn_name = format!("__lifted_arrow_{}", self.counter);
        self.counter += 1;

        // Recurse para arrows aninhadas.
        self.lift_in_body(class_name, &mut body_stmts, in_class);

        // Expression-body arrows always return a value; block-body arrows
        // with explicit `return` also do, but we can't easily detect that
        // here, so treat block-body as void (the common UI-callback case).
        let ret_ty = if has_return_value { Some("i64".to_string()) } else { Some("void".to_string()) };

        self.new_fns.push(Item::Function(FunctionDecl {
            name: syn_name.clone(),
            parameters: Vec::new(),
            return_type: ret_ty,
            body: body_stmts,
            span: Span::default(),
            is_async: false,
        }));

        swc_ecma_ast::Ident {
            span: Default::default(),
            ctxt: Default::default(),
            sym: syn_name.into(),
            optional: false,
        }
    }

    /// Recursa em sub-blocos procurando `const/let/var x = () => ...` e
    /// substitui o initializer por um `Ident` lifted. Permite que arrow
    /// em VarDecl dentro de fn user funcione (codegen direto só trata
    /// top-level). Capturas já estão promovidas pra global por
    /// `lift_in_user_fn` antes desta passagem.
    fn lift_vardecl_arrows_in_stmt(
        &mut self,
        class_name: &str,
        stmt: &mut Stmt,
        in_class: bool,
    ) {
        match stmt {
            Stmt::Decl(swc_ecma_ast::Decl::Var(var_decl)) => {
                for declr in var_decl.decls.iter_mut() {
                    if let Some(init) = declr.init.as_mut() {
                        if matches!(init.as_ref(), Expr::Arrow(_)) {
                            if let Expr::Arrow(arrow) = std::mem::replace(
                                init.as_mut(),
                                Expr::Invalid(swc_ecma_ast::Invalid { span: Default::default() }),
                            ) {
                                let ident = self.lift_arrow_to_ident(class_name, &arrow, in_class);
                                **init = Expr::Ident(ident);
                            }
                        }
                    }
                }
            }
            Stmt::If(i) => {
                self.lift_vardecl_arrows_in_stmt(class_name, &mut i.cons, in_class);
                if let Some(alt) = i.alt.as_mut() {
                    self.lift_vardecl_arrows_in_stmt(class_name, alt, in_class);
                }
            }
            Stmt::Block(b) => {
                for s in b.stmts.iter_mut() {
                    self.lift_vardecl_arrows_in_stmt(class_name, s, in_class);
                }
            }
            Stmt::While(w) => {
                self.lift_vardecl_arrows_in_stmt(class_name, &mut w.body, in_class);
            }
            Stmt::DoWhile(w) => {
                self.lift_vardecl_arrows_in_stmt(class_name, &mut w.body, in_class);
            }
            Stmt::For(f) => {
                self.lift_vardecl_arrows_in_stmt(class_name, &mut f.body, in_class);
            }
            Stmt::ForIn(f) => {
                self.lift_vardecl_arrows_in_stmt(class_name, &mut f.body, in_class);
            }
            Stmt::ForOf(f) => {
                self.lift_vardecl_arrows_in_stmt(class_name, &mut f.body, in_class);
            }
            Stmt::Try(t) => {
                for s in t.block.stmts.iter_mut() {
                    self.lift_vardecl_arrows_in_stmt(class_name, s, in_class);
                }
                if let Some(handler) = t.handler.as_mut() {
                    for s in handler.body.stmts.iter_mut() {
                        self.lift_vardecl_arrows_in_stmt(class_name, s, in_class);
                    }
                }
                if let Some(finalizer) = t.finalizer.as_mut() {
                    for s in finalizer.stmts.iter_mut() {
                        self.lift_vardecl_arrows_in_stmt(class_name, s, in_class);
                    }
                }
            }
            Stmt::Labeled(l) => {
                self.lift_vardecl_arrows_in_stmt(class_name, &mut l.body, in_class);
            }
            Stmt::Switch(sw) => {
                for case in sw.cases.iter_mut() {
                    for s in case.cons.iter_mut() {
                        self.lift_vardecl_arrows_in_stmt(class_name, s, in_class);
                    }
                }
            }
            _ => {}
        }
    }

    /// Recursa em sub-blocos (if/while/for/block/try) procurando `return arrow`
    /// e substitui a arrow por um `Ident` lifted.
    fn lift_return_arrows_in_stmt(
        &mut self,
        class_name: &str,
        stmt: &mut Stmt,
        in_class: bool,
    ) {
        match stmt {
            Stmt::Return(ret) => {
                if let Some(arg) = ret.arg.as_mut() {
                    if matches!(arg.as_ref(), Expr::Arrow(_)) {
                        if let Expr::Arrow(arrow) = std::mem::replace(
                            arg.as_mut(),
                            Expr::Invalid(swc_ecma_ast::Invalid { span: Default::default() }),
                        ) {
                            let ident = self.lift_arrow_to_ident(class_name, &arrow, in_class);
                            **arg = Expr::Ident(ident);
                        }
                    }
                }
            }
            Stmt::If(i) => {
                self.lift_return_arrows_in_stmt(class_name, &mut i.cons, in_class);
                if let Some(alt) = i.alt.as_mut() {
                    self.lift_return_arrows_in_stmt(class_name, alt, in_class);
                }
            }
            Stmt::Block(b) => {
                for s in b.stmts.iter_mut() {
                    self.lift_return_arrows_in_stmt(class_name, s, in_class);
                }
            }
            Stmt::While(w) => {
                self.lift_return_arrows_in_stmt(class_name, &mut w.body, in_class);
            }
            Stmt::DoWhile(w) => {
                self.lift_return_arrows_in_stmt(class_name, &mut w.body, in_class);
            }
            Stmt::For(f) => {
                self.lift_return_arrows_in_stmt(class_name, &mut f.body, in_class);
            }
            Stmt::ForIn(f) => {
                self.lift_return_arrows_in_stmt(class_name, &mut f.body, in_class);
            }
            Stmt::ForOf(f) => {
                self.lift_return_arrows_in_stmt(class_name, &mut f.body, in_class);
            }
            Stmt::Try(t) => {
                for s in t.block.stmts.iter_mut() {
                    self.lift_return_arrows_in_stmt(class_name, s, in_class);
                }
                if let Some(handler) = t.handler.as_mut() {
                    for s in handler.body.stmts.iter_mut() {
                        self.lift_return_arrows_in_stmt(class_name, s, in_class);
                    }
                }
                if let Some(finalizer) = t.finalizer.as_mut() {
                    for s in finalizer.stmts.iter_mut() {
                        self.lift_return_arrows_in_stmt(class_name, s, in_class);
                    }
                }
            }
            Stmt::Labeled(l) => {
                self.lift_return_arrows_in_stmt(class_name, &mut l.body, in_class);
            }
            Stmt::Switch(sw) => {
                for case in sw.cases.iter_mut() {
                    for s in case.cons.iter_mut() {
                        self.lift_return_arrows_in_stmt(class_name, s, in_class);
                    }
                }
            }
            _ => {}
        }
    }

    /// Varre `body` em busca de chamadas a funções do namespace ABI cujo arg
    /// I64 é um `ArrowExpr` ou `Ident` apontando pra user fn. Substitui in
    /// place pelo `Ident` da fn lifted, e injeta statements/fns de suporte.
    fn lift_in_body(&mut self, class_name: &str, body: &mut Vec<Statement>, in_class: bool) {
        use crate::abi::AbiType;

        let mut idx = 0usize;
        while idx < body.len() {
            // Lift de arrow em posições não-call: `return arrow` e
            // `const x = arrow`. Recursa em sub-blocos para cobrir
            // ocorrências dentro de control flow. Substitui pela
            // `Ident` da fn sintética; codegen materializa como
            // `func_addr` (i64). Capturas já estão promovidas pra
            // global por `lift_in_user_fn` antes desta passagem,
            // então a fn lifted lê/escreve via global.
            {
                let Statement::Raw(raw) = &mut body[idx];
                if let Some(stmt) = raw.stmt.as_mut() {
                    self.lift_return_arrows_in_stmt(class_name, stmt, in_class);
                    self.lift_vardecl_arrows_in_stmt(class_name, stmt, in_class);
                }
            }

            // Pega CallExpr do statement atual, se houver. Coletamos as
            // mutações separadas: substituições de args + statements a
            // injetar antes deste.
            let Statement::Raw(raw) = &mut body[idx];
            // Aceita tanto `expr_stmt.expr` quanto VarDecl initializer
            // como sede do CallExpr a inspecionar — assim const decls
            // do tipo `const t = thread.spawn(fp, 0)` tambem entram.
            let call: &mut swc_ecma_ast::CallExpr = match raw.stmt.as_mut() {
                Some(Stmt::Expr(expr_stmt)) => match expr_stmt.expr.as_mut() {
                    Expr::Call(c) => c,
                    _ => { idx += 1; continue; }
                },
                Some(Stmt::Decl(swc_ecma_ast::Decl::Var(var_decl))) => {
                    let mut found: Option<*mut swc_ecma_ast::CallExpr> = None;
                    for d in var_decl.decls.iter_mut() {
                        if let Some(init) = d.init.as_deref_mut() {
                            if let Expr::Call(c) = init {
                                found = Some(c as *mut _);
                                break;
                            }
                        }
                    }
                    match found {
                        // SAFETY: o ponteiro vem de um borrow vivo deste
                        // mesmo `var_decl` que persiste pela duracao do
                        // bloco; nenhuma realocacao acontece entre obter
                        // o ptr e usar.
                        Some(p) => unsafe { &mut *p },
                        None => { idx += 1; continue; }
                    }
                }
                _ => { idx += 1; continue; }
            };

            let ns_method = match &call.callee {
                Callee::Expr(ce) => match ce.as_ref() {
                    Expr::Member(m) => match (m.obj.as_ref(), &m.prop) {
                        (Expr::Ident(obj), MemberProp::Ident(prop)) => {
                            Some((obj.sym.to_string(), prop.sym.to_string()))
                        }
                        _ => None,
                    },
                    _ => None,
                },
                _ => None,
            };
            let Some((ns_name, method_name)) = ns_method else {
                // Direct function calls (user fns like describe/test) also need
                // arrow args lifted so codegen can emit a func_addr pointer.
                let is_direct = matches!(&call.callee, Callee::Expr(ce) if matches!(ce.as_ref(), Expr::Ident(_)));
                if is_direct {
                    for arg in call.args.iter_mut() {
                        let body_stmts: Vec<Statement> = match arg.expr.as_ref() {
                            Expr::Arrow(arrow) => arrow_body_to_stmts(arrow)
                                .into_iter()
                                .map(|s| Statement::Raw(
                                    RawStmt::new("<lifted>".to_string(), Span::default()).with_stmt(s),
                                ))
                                .collect(),
                            _ => continue,
                        };
                        let syn_name = format!("__lifted_arrow_{}", self.counter);
                        self.counter += 1;
                        let mut body_stmts = body_stmts;
                        self.lift_in_body(class_name, &mut body_stmts, in_class);
                        self.new_fns.push(Item::Function(FunctionDecl {
                            name: syn_name.clone(),
                            parameters: Vec::new(),
                            return_type: Some("void".to_string()),
                            body: body_stmts,
                            span: Span::default(),
                            is_async: false,
                        }));
                        *arg.expr = Expr::Ident(swc_ecma_ast::Ident {
                            span: Default::default(),
                            ctxt: Default::default(),
                            sym: syn_name.into(),
                            optional: false,
                        });
                    }
                }
                idx += 1;
                continue;
            };

            let qualified = format!("{ns_name}.{method_name}");
            let Some((_spec, member)) = crate::abi::lookup(&qualified) else {
                idx += 1;
                continue;
            };

            // `pre_stmts` sao statements a inserir antes do callsite (escrita
            // do slot `__cb_this_N = this`).
            let mut pre_stmts: Vec<Statement> = Vec::new();
            // Marca quando precisamos reescrever o callsite atual pra
            // chamar `widget_set_callback_with_ud` em vez de
            // `widget_set_callback`, adicionando `this` como 3º arg.
            let mut pending_userdata_rewrite = false;

            // thread.spawn (U64, U64): so o primeiro arg (fn_ptr) deve ser
            // tratado como callback candidato. Demais membros de ABI seguem
            // a regra padrao (apenas args I64).
            let is_thread_spawn = qualified == "thread.spawn";
            let is_parallel_map = qualified == "parallel.map";
            let is_parallel_for_each = qualified == "parallel.for_each";
            let is_parallel_reduce = qualified == "parallel.reduce";
            let is_parallel_op = is_parallel_map || is_parallel_for_each || is_parallel_reduce;
            for (arg_idx, (arg, &abi_ty)) in call.args.iter_mut().zip(member.args.iter()).enumerate() {
                let is_callback_slot = if is_thread_spawn {
                    arg_idx == 0
                } else if is_parallel_op {
                    // fn_ptr slot is U64 in parallel.* ABIs
                    abi_ty == AbiType::U64
                } else {
                    abi_ty == AbiType::I64
                };
                if !is_callback_slot {
                    continue;
                }

                // Decide qual variante:
                //  (a) Arrow capturando `this` dentro de classe → trampolim
                //      com slot global.
                //  (b) Arrow simples (sem `this`) → lift comum.
                //  (c) Ident apontando pra user fn → wrapper zero-arg.
                let arrow_uses_this = if in_class {
                    matches!(arg.expr.as_ref(), Expr::Arrow(arrow) if arrow_uses_this(arrow))
                } else {
                    false
                };

                let body_stmts: Vec<Statement>;
                let mut needs_this_slot: Option<String> = None; // slot global (path antigo)
                // Quando true: callsite será reescrito pra usar
                // `widget_set_callback_with_ud` passando `this` como
                // userdata. Trampolim recebe `this` como parâmetro
                // — sem slot global, sem limitação \"última vence\".
                let mut use_userdata_callback = false;
                let is_widget_set_callback = qualified == "ui.widget_set_callback";

                // Peel TsAs/TsTypeAssertion/TsConstAssertion/Paren para
                // detectar idents wrappados por type assertions (ex:
                // `worker as unknown as number` em thread.spawn).
                fn peel_ts<'a>(e: &'a Expr) -> &'a Expr {
                    match e {
                        Expr::TsAs(a) => peel_ts(&a.expr),
                        Expr::TsTypeAssertion(a) => peel_ts(&a.expr),
                        Expr::TsConstAssertion(a) => peel_ts(&a.expr),
                        Expr::Paren(p) => peel_ts(&p.expr),
                        _ => e,
                    }
                }
                match peel_ts(arg.expr.as_ref()) {
                    Expr::Arrow(arrow) if arrow_uses_this && is_widget_set_callback => {
                        // Path NOVO (#148): trampolim recebe `this` por
                        // parâmetro. O callsite é reescrito abaixo.
                        use_userdata_callback = true;
                        let raw_stmts = arrow_body_to_stmts(arrow);
                        body_stmts = raw_stmts
                            .into_iter()
                            .map(|s| {
                                Statement::Raw(
                                    RawStmt::new("<lifted>".to_string(), Span::default())
                                        .with_stmt(s),
                                )
                            })
                            .collect();
                    }
                    Expr::Arrow(arrow) if arrow_uses_this => {
                        // Path antigo (slot global): usado por callsites
                        // que não têm variante `_with_ud` no ABI ainda
                        // (window_set_callback, widget_set_draw,
                        // menubar_add). Mantém limitação \"última vence\".
                        let slot = format!("__cb_this_{}", self.counter);
                        needs_this_slot = Some(slot.clone());
                        let raw_stmts = arrow_body_to_stmts(arrow);
                        let prologue = make_this_local(class_name, &slot);
                        let mut stmts: Vec<swc_ecma_ast::Stmt> = raw_stmts;
                        stmts.insert(0, prologue);
                        body_stmts = stmts
                            .into_iter()
                            .map(|s| {
                                Statement::Raw(
                                    RawStmt::new("<lifted>".to_string(), Span::default())
                                        .with_stmt(s),
                                )
                            })
                            .collect();
                    }
                    Expr::Arrow(arrow) => {
                        let raw_stmts = arrow_body_to_stmts(arrow);
                        body_stmts = raw_stmts
                            .into_iter()
                            .map(|s| {
                                Statement::Raw(
                                    RawStmt::new("<lifted>".to_string(), Span::default())
                                        .with_stmt(s),
                                )
                            })
                            .collect();
                    }
                    Expr::Ident(id) if self.user_fn_names.contains(id.sym.as_str()) => {
                        // Resolve alias → fn real. Sem isso, trampolim
                        // chamaria o alias (const global i64), caindo em
                        // call_indirect com sig padrão divergente da fn
                        // real (#206).
                        let real_name = self
                            .alias_to_real
                            .get(id.sym.as_str())
                            .cloned()
                            .unwrap_or_else(|| id.sym.to_string());
                        let target_id = swc_ecma_ast::Ident {
                            span: id.span,
                            ctxt: id.ctxt,
                            sym: real_name.clone().into(),
                            optional: false,
                        };
                        let arity = self
                            .user_fn_arities
                            .get(real_name.as_str())
                            .copied()
                            .unwrap_or(0);
                        let pass_arg = is_thread_spawn && arity >= 1;
                        if is_thread_spawn {
                            self.needs_c_callconv.insert(real_name.clone());
                        }

                        // parallel.* trampolim: adapts i64 ABI to user fn.
                        // Rayon passes Vec<i64> elements as i64 (integer
                        // registers). User fns may declare `number` (f64)
                        // params — codegen coerces automatically via
                        // `lower_user_call`. Trampolim bridges the gap.
                        // (#776) parallel.map/for_each etc agora recebem
                        // callback (val, idx, array). Trampolim que adapta
                        // user fn nomeada passa apenas `val` (primeiro arg)
                        // — preserva compat com user fns 1-arg.
                        if is_parallel_op {
                            fn par_ident(sym: &str) -> Expr {
                                Expr::Ident(swc_ecma_ast::Ident {
                                    span: Default::default(),
                                    ctxt: Default::default(),
                                    sym: sym.to_string().into(),
                                    optional: false,
                                })
                            }
                            fn par_arg(sym: &str) -> swc_ecma_ast::ExprOrSpread {
                                swc_ecma_ast::ExprOrSpread {
                                    spread: None,
                                    expr: Box::new(par_ident(sym)),
                                }
                            }
                            let call_args: Vec<swc_ecma_ast::ExprOrSpread> =
                                if is_parallel_reduce {
                                    vec![par_arg("__par_acc"), par_arg("__par_x")]
                                } else {
                                    // (#776) Passa N args ao target conforme
                                    // a aridade declarada (1..=3): val, idx,
                                    // array. User fns 1-arg recebem so' val
                                    // (compat com codigo existente); fns
                                    // liftadas internamente (3 params) recebem
                                    // os 3.
                                    let target_arity = arity.max(1).min(3);
                                    let all = ["__par_x", "__par_idx", "__par_arr"];
                                    (0..target_arity as usize).map(|i| par_arg(all[i])).collect()
                                };
                            let call_expr = Expr::Call(swc_ecma_ast::CallExpr {
                                span: Default::default(),
                                ctxt: Default::default(),
                                callee: Callee::Expr(Box::new(Expr::Ident(target_id))),
                                args: call_args,
                                type_args: None,
                            });
                            let body_stmt = if is_parallel_for_each {
                                Stmt::Expr(swc_ecma_ast::ExprStmt {
                                    span: Default::default(),
                                    expr: Box::new(call_expr),
                                })
                            } else {
                                Stmt::Return(swc_ecma_ast::ReturnStmt {
                                    span: Default::default(),
                                    arg: Some(Box::new(call_expr)),
                                })
                            };
                            body_stmts = vec![Statement::Raw(
                                RawStmt::new("<par-tramp>".to_string(), Span::default())
                                    .with_stmt(body_stmt),
                            )];
                        } else {
                            // Decide nome do param: __rts_spawn_arg_f64
                            // se worker pede `number`, senao
                            // __rts_spawn_arg. Esse mesmo nome e usado
                            // tanto na decl do trampolim (acima) quanto
                            // no ident que passa pro worker.
                            let worker_wants_f64 = pass_arg && matches!(
                                self.user_fn_first_param_ty.get(real_name.as_str()),
                                Some(Some(ty)) if ty == "number" || ty == "f64"
                            );
                            let arg_name = if worker_wants_f64 {
                                "__rts_spawn_arg_f64"
                            } else {
                                "__rts_spawn_arg"
                            };
                            let args: Vec<swc_ecma_ast::ExprOrSpread> = if pass_arg {
                                vec![swc_ecma_ast::ExprOrSpread {
                                    spread: None,
                                    expr: Box::new(Expr::Ident(swc_ecma_ast::Ident {
                                        span: Default::default(),
                                        ctxt: Default::default(),
                                        sym: arg_name.into(),
                                        optional: false,
                                    })),
                                }]
                            } else {
                                Vec::new()
                            };
                            let call_stmt = Stmt::Expr(swc_ecma_ast::ExprStmt {
                                span: id.span,
                                expr: Box::new(Expr::Call(swc_ecma_ast::CallExpr {
                                    span: id.span,
                                    ctxt: id.ctxt,
                                    callee: Callee::Expr(Box::new(Expr::Ident(target_id))),
                                    args,
                                    type_args: None,
                                })),
                            });
                            body_stmts = vec![Statement::Raw(
                                RawStmt::new("<lifted>".to_string(), Span::default())
                                    .with_stmt(call_stmt),
                            )];
                        }
                    }
                    _ => continue,
                };

                // Nome mangled quando o trampolim captura `this` —
                // habilita `current_class` no codegen via
                // `extract_class_owner`, o que destrava `Expr::This`,
                // `super.method()` e dispatch virtual.
                let captures_this = needs_this_slot.is_some() || use_userdata_callback;
                let syn_name = if captures_this {
                    format!("__class_{}_lifted_arrow_{}", class_name, self.counter)
                } else {
                    format!("__lifted_arrow_{}", self.counter)
                };
                self.counter += 1;

                // Recurse pra arrows aninhadas no body do trampolim.
                let mut body_stmts = body_stmts;
                self.lift_in_body(class_name, &mut body_stmts, in_class);

                // Trampolim recebe `this: ClassName` como parâmetro
                // quando vamos passar `this` por userdata. Para
                // `thread.spawn(fp, arg)` com worker arity≥1, recebe
                // `__rts_spawn_arg: number`. Parallel ops recebem
                // parâmetros i64 (Rayon passa Vec<i64> elements).
                // Caso contrário: sem parâmetros (UI callbacks tradicionais).
                fn mk_i64_param(name: &str) -> Parameter {
                    Parameter {
                        name: name.to_string(),
                        type_annotation: Some("i64".to_string()),
                        modifiers: MemberModifiers::default(),
                        variadic: false,
                        default: None,
                        span: Span::default(),
                    }
                }
                let (parameters, tramp_return_type): (Vec<Parameter>, &'static str) =
                    if use_userdata_callback {
                        (
                            vec![Parameter {
                                name: "this".to_string(),
                                type_annotation: Some(class_name.to_string()),
                                modifiers: MemberModifiers::default(),
                                variadic: false,
                                default: None,
                                span: Span::default(),
                            }],
                            "void",
                        )
                    } else if is_parallel_reduce {
                        (vec![mk_i64_param("__par_acc"), mk_i64_param("__par_x")], "i64")
                    } else if is_parallel_map {
                        // (#776) trampolim com 3 params (val, idx, array) para casar
                        // com ABI runtime nova; passa apenas val ao user fn target.
                        (vec![mk_i64_param("__par_x"), mk_i64_param("__par_idx"), mk_i64_param("__par_arr")], "i64")
                    } else if is_parallel_for_each {
                        (vec![mk_i64_param("__par_x"), mk_i64_param("__par_idx"), mk_i64_param("__par_arr")], "void")
                    } else if is_thread_spawn
                        && matches!(peel_ts(arg.expr.as_ref()), Expr::Ident(id) if {
                            let real = self.alias_to_real.get(id.sym.as_str()).cloned()
                                .unwrap_or_else(|| id.sym.to_string());
                            self.user_fn_arities.get(real.as_str()).copied().unwrap_or(0) >= 1
                        })
                    {
                        // Worker pode pedir `number` (f64) ou `i64`. Pra
                        // f64, marcamos o param com nome especial
                        // `__rts_spawn_arg_f64` — `compile_user_fn` detecta
                        // o sufixo, gera bind com bitcast i64→f64 (caller
                        // passa bits via U64 extern arg, NAO numerico).
                        // Sem isso, codegen faria fcvt_from_sint e
                        // worker receberia valor errado.
                        let real_for_ty = match peel_ts(arg.expr.as_ref()) {
                            Expr::Ident(id) => self.alias_to_real.get(id.sym.as_str()).cloned()
                                .unwrap_or_else(|| id.sym.to_string()),
                            _ => String::new(),
                        };
                        let worker_wants_f64 = matches!(
                            self.user_fn_first_param_ty.get(real_for_ty.as_str()),
                            Some(Some(ty)) if ty == "number" || ty == "f64"
                        );
                        let pname = if worker_wants_f64 {
                            "__rts_spawn_arg_f64"
                        } else {
                            "__rts_spawn_arg"
                        };
                        (
                            vec![Parameter {
                                name: pname.to_string(),
                                type_annotation: Some("i64".to_string()),
                                modifiers: MemberModifiers::default(),
                                variadic: false,
                                default: None,
                                span: Span::default(),
                            }],
                            "void",
                        )
                    } else {
                        (Vec::new(), "void")
                    };

                self.new_fns.push(Item::Function(FunctionDecl {
                    name: syn_name.clone(),
                    parameters,
                    return_type: Some(tramp_return_type.to_string()),
                    body: body_stmts,
                    span: Span::default(),
                    is_async: false,
                }));

                if let Some(slot_name) = needs_this_slot {
                    self.new_globals.push(slot_name.clone());
                    pre_stmts.push(make_slot_assign(&slot_name));
                }

                *arg.expr = Expr::Ident(swc_ecma_ast::Ident {
                    span: Default::default(),
                    ctxt: Default::default(),
                    sym: syn_name.into(),
                    optional: false,
                });

                // Se vamos passar userdata, marca o callsite pra
                // reescrita posterior. Mais simples fazer fora do loop
                // de args — ver `pending_userdata_rewrite` abaixo.
                if use_userdata_callback {
                    pending_userdata_rewrite = true;
                }
            }

            // Reescrita do callsite quando o trampolim captura `this`
            // via parâmetro (path novo de #148). Substitui o callee
            // `ui.widget_set_callback` por `ui.widget_set_callback_with_ud`
            // e anexa `this` como 3º argumento.
            if pending_userdata_rewrite {
                if let Callee::Expr(callee_expr) = &mut call.callee {
                    if let Expr::Member(m) = callee_expr.as_mut() {
                        if let MemberProp::Ident(prop_id) = &mut m.prop {
                            prop_id.sym = "widget_set_callback_with_ud".into();
                        }
                    }
                }
                // Adiciona `this` como 3º arg.
                call.args.push(swc_ecma_ast::ExprOrSpread {
                    spread: None,
                    expr: Box::new(Expr::This(swc_ecma_ast::ThisExpr {
                        span: Default::default(),
                    })),
                });
            }

            // Injeta os pre_stmts antes do callsite atual.
            let pre_count = pre_stmts.len();
            if pre_count > 0 {
                for (k, s) in pre_stmts.into_iter().enumerate() {
                    body.insert(idx + k, s);
                }
                idx += pre_count;
            }
            idx += 1;
        }
    }
}

fn arrow_uses_this(arrow: &swc_ecma_ast::ArrowExpr) -> bool {
    use swc_ecma_ast::BlockStmtOrExpr;
    let mut found = false;
    match arrow.body.as_ref() {
        BlockStmtOrExpr::BlockStmt(block) => {
            for s in &block.stmts {
                if stmt_uses_this(s) {
                    found = true;
                    break;
                }
            }
        }
        BlockStmtOrExpr::Expr(expr) => {
            found = expr_uses_this(expr);
        }
    }
    found
}

fn stmt_uses_this(stmt: &Stmt) -> bool {
    use swc_ecma_ast::Stmt::*;
    match stmt {
        Expr(e) => expr_uses_this(&e.expr),
        Return(r) => r.arg.as_deref().map_or(false, expr_uses_this),
        If(i) => {
            expr_uses_this(&i.test)
                || stmt_uses_this(&i.cons)
                || i.alt.as_deref().map_or(false, stmt_uses_this)
        }
        Block(b) => b.stmts.iter().any(stmt_uses_this),
        While(w) => expr_uses_this(&w.test) || stmt_uses_this(&w.body),
        DoWhile(w) => expr_uses_this(&w.test) || stmt_uses_this(&w.body),
        For(f) => {
            f.init.as_ref().map_or(false, |init| match init {
                swc_ecma_ast::VarDeclOrExpr::Expr(e) => expr_uses_this(e),
                swc_ecma_ast::VarDeclOrExpr::VarDecl(v) => v
                    .decls
                    .iter()
                    .any(|d| d.init.as_deref().map_or(false, expr_uses_this)),
            }) || f.test.as_deref().map_or(false, expr_uses_this)
                || f.update.as_deref().map_or(false, expr_uses_this)
                || stmt_uses_this(&f.body)
        }
        ForOf(f) => expr_uses_this(&f.right) || stmt_uses_this(&f.body),
        Decl(swc_ecma_ast::Decl::Var(v)) => v
            .decls
            .iter()
            .any(|d| d.init.as_deref().map_or(false, expr_uses_this)),
        Try(t) => {
            t.block.stmts.iter().any(stmt_uses_this)
                || t.handler
                    .as_ref()
                    .map_or(false, |h| h.body.stmts.iter().any(stmt_uses_this))
                || t.finalizer
                    .as_ref()
                    .map_or(false, |f| f.stmts.iter().any(stmt_uses_this))
        }
        _ => false,
    }
}

fn expr_uses_this(expr: &Expr) -> bool {
    use swc_ecma_ast::Expr::*;
    match expr {
        This(_) => true,
        // `super.method(...)` e `super[...]` também precisam do contexto
        // de classe — tratá-los como uso de `this` força o trampolim a
        // virar `__class_C_lifted_arrow_N` (que popula current_class).
        SuperProp(_) => true,
        Member(m) => expr_uses_this(&m.obj),
        Bin(b) => expr_uses_this(&b.left) || expr_uses_this(&b.right),
        Unary(u) => expr_uses_this(&u.arg),
        Update(u) => expr_uses_this(&u.arg),
        Assign(a) => {
            let lhs = match &a.left {
                swc_ecma_ast::AssignTarget::Simple(s) => match s {
                    swc_ecma_ast::SimpleAssignTarget::Ident(_) => false,
                    swc_ecma_ast::SimpleAssignTarget::Member(m) => expr_uses_this(&m.obj),
                    _ => false,
                },
                _ => false,
            };
            lhs || expr_uses_this(&a.right)
        }
        Call(c) => {
            let callee_uses = match &c.callee {
                Callee::Expr(e) => expr_uses_this(e),
                Callee::Super(_) => true,
                _ => false,
            };
            callee_uses || c.args.iter().any(|a| expr_uses_this(&a.expr))
        }
        New(n) => n
            .args
            .as_ref()
            .map_or(false, |args| args.iter().any(|a| expr_uses_this(&a.expr))),
        Cond(c) => expr_uses_this(&c.test) || expr_uses_this(&c.cons) || expr_uses_this(&c.alt),
        Paren(p) => expr_uses_this(&p.expr),
        Tpl(t) => t.exprs.iter().any(|e| expr_uses_this(e)),
        Array(a) => a
            .elems
            .iter()
            .any(|e| e.as_ref().map_or(false, |el| expr_uses_this(&el.expr))),
        Seq(s) => s.exprs.iter().any(|e| expr_uses_this(e)),
        _ => false,
    }
}

fn arrow_body_to_stmts(arrow: &swc_ecma_ast::ArrowExpr) -> Vec<Stmt> {
    use swc_ecma_ast::BlockStmtOrExpr;
    match arrow.body.as_ref() {
        BlockStmtOrExpr::BlockStmt(block) => block.stmts.clone(),
        BlockStmtOrExpr::Expr(expr) => {
            vec![Stmt::Return(swc_ecma_ast::ReturnStmt {
                span: Default::default(),
                arg: Some(expr.clone()),
            })]
        }
    }
}

// NOTE: As funções `rewrite_*` e `revert_*` abaixo eram usadas pela
// estratégia anterior (renomear `this`→`__this` no body do trampolim).
// A estratégia atual usa nome mangled `__class_C_lifted_arrow_N` +
// `let this: C = ...` no prólogo, então `this` permanece intacto.
// Mantenho as funções marcadas como `#[allow(dead_code)]` por enquanto
// — limpeza num commit separado quando o approach se mostrar estável.

#[allow(dead_code)]
fn rewrite_this_to_under_this(mut s: Stmt) -> Stmt {
    rewrite_stmt(&mut s);
    s
}

#[allow(dead_code)]
fn rewrite_stmt(stmt: &mut Stmt) {
    use swc_ecma_ast::Stmt::*;
    match stmt {
        Expr(e) => rewrite_expr(&mut e.expr),
        Return(r) => {
            if let Some(a) = r.arg.as_deref_mut() {
                rewrite_expr(a);
            }
        }
        If(i) => {
            rewrite_expr(&mut i.test);
            rewrite_stmt(&mut i.cons);
            if let Some(alt) = i.alt.as_deref_mut() {
                rewrite_stmt(alt);
            }
        }
        Block(b) => {
            for s in &mut b.stmts {
                rewrite_stmt(s);
            }
        }
        While(w) => {
            rewrite_expr(&mut w.test);
            rewrite_stmt(&mut w.body);
        }
        DoWhile(w) => {
            rewrite_expr(&mut w.test);
            rewrite_stmt(&mut w.body);
        }
        For(f) => {
            if let Some(init) = f.init.as_mut() {
                match init {
                    swc_ecma_ast::VarDeclOrExpr::Expr(e) => rewrite_expr(e),
                    swc_ecma_ast::VarDeclOrExpr::VarDecl(v) => {
                        for d in &mut v.decls {
                            if let Some(e) = d.init.as_deref_mut() {
                                rewrite_expr(e);
                            }
                        }
                    }
                }
            }
            if let Some(t) = f.test.as_deref_mut() {
                rewrite_expr(t);
            }
            if let Some(u) = f.update.as_deref_mut() {
                rewrite_expr(u);
            }
            rewrite_stmt(&mut f.body);
        }
        ForOf(f) => {
            rewrite_expr(&mut f.right);
            rewrite_stmt(&mut f.body);
        }
        Decl(swc_ecma_ast::Decl::Var(v)) => {
            for d in &mut v.decls {
                if let Some(e) = d.init.as_deref_mut() {
                    rewrite_expr(e);
                }
            }
        }
        Try(t) => {
            for s in &mut t.block.stmts {
                rewrite_stmt(s);
            }
            if let Some(h) = t.handler.as_mut() {
                for s in &mut h.body.stmts {
                    rewrite_stmt(s);
                }
            }
            if let Some(f) = t.finalizer.as_mut() {
                for s in &mut f.stmts {
                    rewrite_stmt(s);
                }
            }
        }
        _ => {}
    }
}

#[allow(dead_code)]
fn rewrite_expr(expr: &mut Expr) {
    use swc_ecma_ast::Expr::*;
    // Substitui `this` por Ident("__this") in-place.
    if matches!(expr, This(_)) {
        *expr = Expr::Ident(swc_ecma_ast::Ident {
            span: Default::default(),
            ctxt: Default::default(),
            sym: "__this".into(),
            optional: false,
        });
        return;
    }
    match expr {
        Member(m) => rewrite_expr(&mut m.obj),
        Bin(b) => {
            rewrite_expr(&mut b.left);
            rewrite_expr(&mut b.right);
        }
        Unary(u) => rewrite_expr(&mut u.arg),
        Update(u) => rewrite_expr(&mut u.arg),
        Assign(a) => {
            if let swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(m)) =
                &mut a.left
            {
                rewrite_expr(&mut m.obj);
            }
            rewrite_expr(&mut a.right);
        }
        Call(c) => {
            if let Callee::Expr(e) = &mut c.callee {
                rewrite_expr(e);
            }
            for a in &mut c.args {
                rewrite_expr(&mut a.expr);
            }
        }
        New(n) => {
            if let Some(args) = n.args.as_mut() {
                for a in args {
                    rewrite_expr(&mut a.expr);
                }
            }
        }
        Cond(c) => {
            rewrite_expr(&mut c.test);
            rewrite_expr(&mut c.cons);
            rewrite_expr(&mut c.alt);
        }
        Paren(p) => rewrite_expr(&mut p.expr),
        Tpl(t) => {
            for e in &mut t.exprs {
                rewrite_expr(e);
            }
        }
        Array(a) => {
            for el in a.elems.iter_mut().flatten() {
                rewrite_expr(&mut el.expr);
            }
        }
        Seq(s) => {
            for e in &mut s.exprs {
                rewrite_expr(e);
            }
        }
        _ => {}
    }
}

/// Inside any nested `Expr::Arrow` found in `stmts`, revert `__this`
/// identifiers back to `this`. Used after the outer arrow's body had
/// `this`→`__this` rewritten: inner arrows kept the rewrite, but they
/// will be lifted to their own trampolines that re-bind `__this`
/// from their own slot, so they need to start with `this` again.
/// Statements outside arrows are left as is (the outer trampoline
/// owns those and binds `__this` itself).
#[allow(dead_code)]
fn revert_under_this_inside_arrows(stmts: &mut [Statement]) {
    for s in stmts.iter_mut() {
        let Statement::Raw(raw) = s;
        if let Some(stmt) = raw.stmt.as_mut() {
            revert_stmt_arrows(stmt);
        }
    }
}

#[allow(dead_code)]
fn revert_stmt_arrows(stmt: &mut Stmt) {
    use swc_ecma_ast::Stmt::*;
    match stmt {
        Expr(e) => revert_expr_arrows(&mut e.expr),
        Return(r) => {
            if let Some(a) = r.arg.as_deref_mut() {
                revert_expr_arrows(a);
            }
        }
        If(i) => {
            revert_expr_arrows(&mut i.test);
            revert_stmt_arrows(&mut i.cons);
            if let Some(alt) = i.alt.as_deref_mut() {
                revert_stmt_arrows(alt);
            }
        }
        Block(b) => {
            for s in &mut b.stmts {
                revert_stmt_arrows(s);
            }
        }
        While(w) => {
            revert_expr_arrows(&mut w.test);
            revert_stmt_arrows(&mut w.body);
        }
        DoWhile(w) => {
            revert_expr_arrows(&mut w.test);
            revert_stmt_arrows(&mut w.body);
        }
        For(f) => {
            if let Some(init) = f.init.as_mut() {
                match init {
                    swc_ecma_ast::VarDeclOrExpr::Expr(e) => revert_expr_arrows(e),
                    swc_ecma_ast::VarDeclOrExpr::VarDecl(v) => {
                        for d in &mut v.decls {
                            if let Some(e) = d.init.as_deref_mut() {
                                revert_expr_arrows(e);
                            }
                        }
                    }
                }
            }
            if let Some(t) = f.test.as_deref_mut() {
                revert_expr_arrows(t);
            }
            if let Some(u) = f.update.as_deref_mut() {
                revert_expr_arrows(u);
            }
            revert_stmt_arrows(&mut f.body);
        }
        ForOf(f) => {
            revert_expr_arrows(&mut f.right);
            revert_stmt_arrows(&mut f.body);
        }
        Decl(swc_ecma_ast::Decl::Var(v)) => {
            for d in &mut v.decls {
                if let Some(e) = d.init.as_deref_mut() {
                    revert_expr_arrows(e);
                }
            }
        }
        _ => {}
    }
}

#[allow(dead_code)]
fn revert_expr_arrows(expr: &mut Expr) {
    use swc_ecma_ast::Expr::*;
    match expr {
        Arrow(arrow) => {
            // Within the arrow's body, swap `__this` ident for `Expr::This`.
            match arrow.body.as_mut() {
                swc_ecma_ast::BlockStmtOrExpr::BlockStmt(b) => {
                    for s in &mut b.stmts {
                        revert_under_this_in_stmt(s);
                    }
                }
                swc_ecma_ast::BlockStmtOrExpr::Expr(e) => {
                    revert_under_this_in_expr(e);
                }
            }
        }
        Member(m) => revert_expr_arrows(&mut m.obj),
        Bin(b) => {
            revert_expr_arrows(&mut b.left);
            revert_expr_arrows(&mut b.right);
        }
        Unary(u) => revert_expr_arrows(&mut u.arg),
        Update(u) => revert_expr_arrows(&mut u.arg),
        Assign(a) => {
            if let swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(m)) =
                &mut a.left
            {
                revert_expr_arrows(&mut m.obj);
            }
            revert_expr_arrows(&mut a.right);
        }
        Call(c) => {
            if let Callee::Expr(e) = &mut c.callee {
                revert_expr_arrows(e);
            }
            for a in &mut c.args {
                revert_expr_arrows(&mut a.expr);
            }
        }
        New(n) => {
            if let Some(args) = n.args.as_mut() {
                for a in args {
                    revert_expr_arrows(&mut a.expr);
                }
            }
        }
        Cond(c) => {
            revert_expr_arrows(&mut c.test);
            revert_expr_arrows(&mut c.cons);
            revert_expr_arrows(&mut c.alt);
        }
        Paren(p) => revert_expr_arrows(&mut p.expr),
        Tpl(t) => {
            for e in &mut t.exprs {
                revert_expr_arrows(e);
            }
        }
        Array(a) => {
            for el in a.elems.iter_mut().flatten() {
                revert_expr_arrows(&mut el.expr);
            }
        }
        Seq(s) => {
            for e in &mut s.exprs {
                revert_expr_arrows(e);
            }
        }
        _ => {}
    }
}

#[allow(dead_code)]
fn revert_under_this_in_stmt(stmt: &mut Stmt) {
    use swc_ecma_ast::Stmt::*;
    match stmt {
        Expr(e) => revert_under_this_in_expr(&mut e.expr),
        Return(r) => {
            if let Some(a) = r.arg.as_deref_mut() {
                revert_under_this_in_expr(a);
            }
        }
        If(i) => {
            revert_under_this_in_expr(&mut i.test);
            revert_under_this_in_stmt(&mut i.cons);
            if let Some(alt) = i.alt.as_deref_mut() {
                revert_under_this_in_stmt(alt);
            }
        }
        Block(b) => {
            for s in &mut b.stmts {
                revert_under_this_in_stmt(s);
            }
        }
        While(w) => {
            revert_under_this_in_expr(&mut w.test);
            revert_under_this_in_stmt(&mut w.body);
        }
        DoWhile(w) => {
            revert_under_this_in_expr(&mut w.test);
            revert_under_this_in_stmt(&mut w.body);
        }
        For(f) => {
            if let Some(init) = f.init.as_mut() {
                match init {
                    swc_ecma_ast::VarDeclOrExpr::Expr(e) => revert_under_this_in_expr(e),
                    swc_ecma_ast::VarDeclOrExpr::VarDecl(v) => {
                        for d in &mut v.decls {
                            if let Some(e) = d.init.as_deref_mut() {
                                revert_under_this_in_expr(e);
                            }
                        }
                    }
                }
            }
            if let Some(t) = f.test.as_deref_mut() {
                revert_under_this_in_expr(t);
            }
            if let Some(u) = f.update.as_deref_mut() {
                revert_under_this_in_expr(u);
            }
            revert_under_this_in_stmt(&mut f.body);
        }
        ForOf(f) => {
            revert_under_this_in_expr(&mut f.right);
            revert_under_this_in_stmt(&mut f.body);
        }
        Decl(swc_ecma_ast::Decl::Var(v)) => {
            for d in &mut v.decls {
                if let Some(e) = d.init.as_deref_mut() {
                    revert_under_this_in_expr(e);
                }
            }
        }
        _ => {}
    }
}

#[allow(dead_code)]
fn revert_under_this_in_expr(expr: &mut Expr) {
    use swc_ecma_ast::Expr::*;
    if let Ident(id) = expr {
        if id.sym.as_ref() == "__this" {
            *expr = Expr::This(swc_ecma_ast::ThisExpr {
                span: Default::default(),
            });
            return;
        }
    }
    match expr {
        Member(m) => revert_under_this_in_expr(&mut m.obj),
        Bin(b) => {
            revert_under_this_in_expr(&mut b.left);
            revert_under_this_in_expr(&mut b.right);
        }
        Unary(u) => revert_under_this_in_expr(&mut u.arg),
        Update(u) => revert_under_this_in_expr(&mut u.arg),
        Assign(a) => {
            if let swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Member(m)) =
                &mut a.left
            {
                revert_under_this_in_expr(&mut m.obj);
            }
            revert_under_this_in_expr(&mut a.right);
        }
        Call(c) => {
            if let Callee::Expr(e) = &mut c.callee {
                revert_under_this_in_expr(e);
            }
            for a in &mut c.args {
                revert_under_this_in_expr(&mut a.expr);
            }
        }
        New(n) => {
            if let Some(args) = n.args.as_mut() {
                for a in args {
                    revert_under_this_in_expr(&mut a.expr);
                }
            }
        }
        Cond(c) => {
            revert_under_this_in_expr(&mut c.test);
            revert_under_this_in_expr(&mut c.cons);
            revert_under_this_in_expr(&mut c.alt);
        }
        Paren(p) => revert_under_this_in_expr(&mut p.expr),
        Arrow(arrow) => {
            // Recurse into arrow body too — same rule applies to nested
            // arrows: any `__this` they hold should revert to `this` so
            // their own lift sees the canonical form.
            match arrow.body.as_mut() {
                swc_ecma_ast::BlockStmtOrExpr::BlockStmt(b) => {
                    for s in &mut b.stmts {
                        revert_under_this_in_stmt(s);
                    }
                }
                swc_ecma_ast::BlockStmtOrExpr::Expr(e) => {
                    revert_under_this_in_expr(e);
                }
            }
        }
        Tpl(t) => {
            for e in &mut t.exprs {
                revert_under_this_in_expr(e);
            }
        }
        Array(a) => {
            for el in a.elems.iter_mut().flatten() {
                revert_under_this_in_expr(&mut el.expr);
            }
        }
        Seq(s) => {
            for e in &mut s.exprs {
                revert_under_this_in_expr(e);
            }
        }
        _ => {}
    }
}

/// `let this: ClassName = __cb_this_N;` — o nome do bind é `this`
/// para que `read_local("this")` no codegen retorne o handle da
/// instância. Combinado com o nome mangled `__class_C_lifted_arrow_N`
/// (que faz `current_class = Some("C")`), `Expr::This` e
/// `super.method()` funcionam normalmente dentro do trampolim.
fn make_this_local(class_name: &str, slot_name: &str) -> Stmt {
    let cls_ann = TsType::TsTypeRef(TsTypeRef {
        span: Default::default(),
        type_name: swc_ecma_ast::TsEntityName::Ident(swc_ecma_ast::Ident {
            span: Default::default(),
            ctxt: Default::default(),
            sym: class_name.into(),
            optional: false,
        }),
        type_params: None,
    });
    let init = Expr::Ident(swc_ecma_ast::Ident {
        span: Default::default(),
        ctxt: Default::default(),
        sym: slot_name.into(),
        optional: false,
    });
    let var = swc_ecma_ast::VarDecl {
        span: Default::default(),
        ctxt: Default::default(),
        kind: swc_ecma_ast::VarDeclKind::Let,
        declare: false,
        decls: vec![swc_ecma_ast::VarDeclarator {
            span: Default::default(),
            name: Pat::Ident(swc_ecma_ast::BindingIdent {
                id: swc_ecma_ast::Ident {
                    span: Default::default(),
                    ctxt: Default::default(),
                    sym: "this".into(),
                    optional: false,
                },
                type_ann: Some(Box::new(swc_ecma_ast::TsTypeAnn {
                    span: Default::default(),
                    type_ann: Box::new(cls_ann),
                })),
            }),
            init: Some(Box::new(init)),
            definite: false,
        }],
    };
    Stmt::Decl(Decl::Var(Box::new(var)))
}

/// `__cb_this_N = this;`
fn make_slot_assign(slot_name: &str) -> Statement {
    let rhs: Expr = Expr::This(swc_ecma_ast::ThisExpr {
        span: Default::default(),
    });
    let assign = Expr::Assign(swc_ecma_ast::AssignExpr {
        span: Default::default(),
        op: swc_ecma_ast::AssignOp::Assign,
        left: swc_ecma_ast::AssignTarget::Simple(swc_ecma_ast::SimpleAssignTarget::Ident(
            swc_ecma_ast::BindingIdent {
                id: swc_ecma_ast::Ident {
                    span: Default::default(),
                    ctxt: Default::default(),
                    sym: slot_name.into(),
                    optional: false,
                },
                type_ann: None,
            },
        )),
        right: Box::new(rhs),
    });
    let stmt = Stmt::Expr(swc_ecma_ast::ExprStmt {
        span: Default::default(),
        expr: Box::new(assign),
    });
    Statement::Raw(RawStmt::new("<cb-slot-set>".to_string(), Span::default()).with_stmt(stmt))
}
