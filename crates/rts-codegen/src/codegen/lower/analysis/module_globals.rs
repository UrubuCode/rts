//! Coleta o conjunto de variaveis top-level que precisam de storage
//! global (data segment Cranelift) por serem referenciadas por user fns
//! ou class methods. As demais sao promovidas a Variables locais a
//! `__RTS_MAIN` (sem load/store em hot loops top-level — 5x speedup
//! mensurado).

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, anyhow};
use cranelift_module::{DataDescription, Linkage, Module};
use swc_ecma_ast::{Decl, Pat, Stmt};

use crate::parser::ast::{ClassMember, Item, Program, Statement};

use super::super::ctx::{GlobalVar, ValTy};
use super::types::{infer_expr_ty, sanitize_symbol, ts_type_to_val_ty};

pub(crate) fn collect_module_globals(
    program: &Program,
    module: &mut dyn Module,
) -> Result<HashMap<String, GlobalVar>> {
    let mut globals = HashMap::<String, GlobalVar>::new();
    let mut counter = 0usize;

    // Identifica vars top-level cujo identificador eh referenciado por
    // alguma user fn (ou class method). Essas precisam de storage global
    // pra que outras fns possam ler/escrever atraves do mesmo data_id.
    // Vars NAO referenciadas viram Cranelift Variables locais a __RTS_MAIN
    // (sem load/store em hot loops top-level — 5x speedup mensurado).
    let referenced = collect_idents_referenced_in_user_fns(program);

    for item in &program.items {
        let Item::Statement(Statement::Raw(raw)) = item else {
            continue;
        };
        let Some(Stmt::Decl(Decl::Var(var_decl))) = raw.stmt.as_ref() else {
            continue;
        };

        for decl in &var_decl.decls {
            let name = match &decl.name {
                Pat::Ident(id) => id.sym.as_str().to_string(),
                _ => {
                    return Err(anyhow!(
                        "unsupported top-level binding pattern in global decl"
                    ));
                }
            };

            if globals.contains_key(&name) {
                continue;
            }

            // Promote-to-local: se nenhuma user fn referencia o nome,
            // pula a alocacao do data global. lower_var_decl em
            // module_scope cai em declare_local_kind quando has_global
            // retorna false.
            if !referenced.contains(&name) {
                continue;
            }

            let ann_ty = match &decl.name {
                Pat::Ident(id) => id
                    .type_ann
                    .as_ref()
                    .and_then(|ann| ts_type_to_val_ty(&ann.type_ann)),
                _ => None,
            };
            let inferred = infer_expr_ty(decl.init.as_deref());
            // (#305) Espelha o `promote_to_f64` de lower_var_decl: var MUTAVEL
            // (`let`/`var`) sem anotacao, inicializada com LITERAL inteiro, vira
            // F64. Sem isto o global ficava i32/i64 e `acc = acc * 10` num loop
            // top-level (referenciado por user fn -> vira global) truncava em 32
            // bits (`20000000000` virava -1474836480). JS: number eh f64.
            let is_mutable_kind = matches!(
                var_decl.kind,
                swc_ecma_ast::VarDeclKind::Let | swc_ecma_ast::VarDeclKind::Var
            );
            let init_is_int_lit = matches!(
                decl.init.as_deref(),
                Some(swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Num(_)))
            ) && matches!(inferred, ValTy::I32 | ValTy::I64);
            let ty = if ann_ty.is_none() && is_mutable_kind && init_is_int_lit {
                ValTy::F64
            } else {
                ann_ty.unwrap_or(inferred)
            };

            let symbol = format!("__rts_global_{}_{}", sanitize_symbol(&name), counter);
            counter += 1;
            let data_id = module
                .declare_data(&symbol, Linkage::Local, true, false)
                .with_context(|| format!("failed to declare module global `{name}`"))?;

            let size = match ty {
                ValTy::I32 => 4,
                _ => 8,
            };
            let mut desc = DataDescription::new();
            desc.define(vec![0u8; size].into_boxed_slice());
            module
                .define_data(data_id, &desc)
                .with_context(|| format!("failed to define module global `{name}`"))?;

            // Issue #407 / epic #419 — globals com tipo Handle precisam
            // ser tratadas como roots GC, senao o sweep coleta o handle
            // armazenado. Registra o DataId pra `jit.rs` resolver pra
            // endereco real apos `finalize_definitions`.
            //
            // (cross-runtime #344) Also register I64/U64 globals: an ambiguous
            // i64/handle global (e.g. a generator object from an indirect call,
            // or a capture promoted-to-global) can hold a live GC handle. The
            // conservative scanner's `gen != 0` check + mark_handle validity
            // filter non-handle numeric values, so this is safe — without it a
            // GC tick sweeps a handle stored in an i64-typed global (344's
            // __awaiter generator was collected mid-drive → infinite loop).
            //
            // (cross-runtime #344 cont.) Also register F64 globals: a
            // promoted-to-global capture is emitted as `let g = 0.0` (Number
            // literal init), so collect_module_globals infers F64 — yet at
            // runtime it holds a generator handle. A real f64 bit-pattern almost
            // never has gen != 0 AND decodes to a live handle, so the scanner's
            // gen-check + mark_handle validity filter still reject genuine
            // floats; cost is only an occasional harmless false-positive mark.
            // Bool excluded (0/1, never a handle).
            if matches!(ty, ValTy::Handle | ValTy::I64 | ValTy::U64 | ValTy::F64) {
                use cranelift_module::DataId;
                let _ = DataId::from_u32; // garante o tipo
                crate::namespaces::gc::global_roots::push_pending_data_id(
                    data_id.as_u32(),
                );
            }

            globals.insert(name, GlobalVar { data_id, ty });
        }
    }

    Ok(globals)
}

/// Coleta nomes de identifiers usados em qualquer body de user fn ou
/// class method (incl. arrows lifted, callbacks). Vars top-level cujo
/// nome esta nesse set precisam de storage global; as demais sao
/// seguras pra promover a Variables locais a __RTS_MAIN.
///
/// Conservador: nao distingue read de write, nao olha shadow scoping
/// (params/locais com mesmo nome contam como referencia). Falsos
/// positivos so pioram em deixar var como global (caminho ja-padrao).
/// Falsos negativos seriam graves — varredura completa de Expr.
fn collect_idents_referenced_in_user_fns(program: &Program) -> HashSet<String> {
    use swc_ecma_ast::{Expr, Pat, Stmt};

    let mut out: HashSet<String> = HashSet::new();

    fn walk_expr(e: &Expr, out: &mut HashSet<String>) {
        match e {
            Expr::Ident(id) => {
                out.insert(id.sym.as_str().to_string());
            }
            Expr::Array(a) => {
                for elem in a.elems.iter().flatten() {
                    walk_expr(&elem.expr, out);
                }
            }
            Expr::Object(o) => {
                for prop in &o.props {
                    if let swc_ecma_ast::PropOrSpread::Prop(p) = prop {
                        if let swc_ecma_ast::Prop::KeyValue(kv) = p.as_ref() {
                            walk_expr(&kv.value, out);
                        }
                    } else if let swc_ecma_ast::PropOrSpread::Spread(sp) = prop {
                        walk_expr(&sp.expr, out);
                    }
                }
            }
            Expr::Unary(u) => walk_expr(&u.arg, out),
            Expr::Update(u) => walk_expr(&u.arg, out),
            Expr::Bin(b) => {
                walk_expr(&b.left, out);
                walk_expr(&b.right, out);
            }
            Expr::Assign(a) => {
                if let swc_ecma_ast::AssignTarget::Simple(s) = &a.left {
                    if let swc_ecma_ast::SimpleAssignTarget::Ident(id) = s {
                        out.insert(id.id.sym.as_str().to_string());
                    } else if let swc_ecma_ast::SimpleAssignTarget::Member(m) = s {
                        walk_expr(&m.obj, out);
                    }
                }
                walk_expr(&a.right, out);
            }
            Expr::Member(m) => {
                walk_expr(&m.obj, out);
                if let swc_ecma_ast::MemberProp::Computed(c) = &m.prop {
                    walk_expr(&c.expr, out);
                }
            }
            Expr::Cond(c) => {
                walk_expr(&c.test, out);
                walk_expr(&c.cons, out);
                walk_expr(&c.alt, out);
            }
            Expr::Call(c) => {
                if let swc_ecma_ast::Callee::Expr(callee) = &c.callee {
                    walk_expr(callee, out);
                }
                for a in &c.args {
                    walk_expr(&a.expr, out);
                }
            }
            Expr::New(n) => {
                walk_expr(&n.callee, out);
                if let Some(args) = n.args.as_ref() {
                    for a in args {
                        walk_expr(&a.expr, out);
                    }
                }
            }
            Expr::Seq(s) => {
                for e in &s.exprs {
                    walk_expr(e, out);
                }
            }
            Expr::Tpl(t) => {
                for e in &t.exprs {
                    walk_expr(e, out);
                }
            }
            Expr::Paren(p) => walk_expr(&p.expr, out),
            Expr::TsAs(a) => walk_expr(&a.expr, out),
            Expr::TsTypeAssertion(a) => walk_expr(&a.expr, out),
            Expr::TsConstAssertion(a) => walk_expr(&a.expr, out),
            Expr::TsNonNull(n) => walk_expr(&n.expr, out),
            Expr::Arrow(a) => {
                use swc_ecma_ast::BlockStmtOrExpr;
                match a.body.as_ref() {
                    BlockStmtOrExpr::BlockStmt(b) => {
                        for s in &b.stmts {
                            walk_stmt(s, out);
                        }
                    }
                    BlockStmtOrExpr::Expr(e) => walk_expr(e, out),
                }
            }
            Expr::Fn(f) => {
                if let Some(body) = f.function.body.as_ref() {
                    for s in &body.stmts {
                        walk_stmt(s, out);
                    }
                }
            }
            _ => {}
        }
    }

    fn walk_stmt(s: &Stmt, out: &mut HashSet<String>) {
        match s {
            Stmt::Expr(e) => walk_expr(&e.expr, out),
            Stmt::Block(b) => {
                for s in &b.stmts {
                    walk_stmt(s, out);
                }
            }
            Stmt::Return(r) => {
                if let Some(e) = r.arg.as_ref() {
                    walk_expr(e, out);
                }
            }
            Stmt::If(i) => {
                walk_expr(&i.test, out);
                walk_stmt(&i.cons, out);
                if let Some(alt) = i.alt.as_ref() {
                    walk_stmt(alt, out);
                }
            }
            Stmt::While(w) => {
                walk_expr(&w.test, out);
                walk_stmt(&w.body, out);
            }
            Stmt::DoWhile(d) => {
                walk_expr(&d.test, out);
                walk_stmt(&d.body, out);
            }
            Stmt::For(f) => {
                if let Some(init) = f.init.as_ref() {
                    match init {
                        swc_ecma_ast::VarDeclOrExpr::Expr(e) => walk_expr(e, out),
                        swc_ecma_ast::VarDeclOrExpr::VarDecl(vd) => {
                            for d in &vd.decls {
                                if let Some(e) = d.init.as_ref() {
                                    walk_expr(e, out);
                                }
                            }
                        }
                    }
                }
                if let Some(t) = f.test.as_ref() {
                    walk_expr(t, out);
                }
                if let Some(u) = f.update.as_ref() {
                    walk_expr(u, out);
                }
                walk_stmt(&f.body, out);
            }
            Stmt::ForOf(f) => {
                walk_expr(&f.right, out);
                walk_stmt(&f.body, out);
            }
            Stmt::ForIn(f) => {
                walk_expr(&f.right, out);
                walk_stmt(&f.body, out);
            }
            Stmt::Switch(s) => {
                walk_expr(&s.discriminant, out);
                for c in &s.cases {
                    if let Some(t) = c.test.as_ref() {
                        walk_expr(t, out);
                    }
                    for s in &c.cons {
                        walk_stmt(s, out);
                    }
                }
            }
            Stmt::Throw(t) => walk_expr(&t.arg, out),
            Stmt::Try(t) => {
                for s in &t.block.stmts {
                    walk_stmt(s, out);
                }
                if let Some(h) = t.handler.as_ref() {
                    if let Some(Pat::Ident(id)) = h.param.as_ref() {
                        out.insert(id.id.sym.as_str().to_string());
                    }
                    for s in &h.body.stmts {
                        walk_stmt(s, out);
                    }
                }
                if let Some(f) = t.finalizer.as_ref() {
                    for s in &f.stmts {
                        walk_stmt(s, out);
                    }
                }
            }
            Stmt::Decl(swc_ecma_ast::Decl::Var(vd)) => {
                for d in &vd.decls {
                    if let Some(e) = d.init.as_ref() {
                        walk_expr(e, out);
                    }
                }
            }
            Stmt::Labeled(l) => walk_stmt(&l.body, out),
            _ => {}
        }
    }

    for item in &program.items {
        match item {
            Item::Function(f) => {
                for stmt_raw in &f.body {
                    let Statement::Raw(raw) = stmt_raw;
                    if let Some(s) = raw.stmt.as_ref() {
                        walk_stmt(s, &mut out);
                    }
                }
            }
            Item::Class(c) => {
                for m in &c.members {
                    match m {
                        ClassMember::Method(method) => {
                            for stmt_raw in &method.body {
                                let Statement::Raw(raw) = stmt_raw;
                                if let Some(s) = raw.stmt.as_ref() {
                                    walk_stmt(s, &mut out);
                                }
                            }
                        }
                        ClassMember::Constructor(ctor) => {
                            for stmt_raw in &ctor.body {
                                let Statement::Raw(raw) = stmt_raw;
                                if let Some(s) = raw.stmt.as_ref() {
                                    walk_stmt(s, &mut out);
                                }
                            }
                        }
                        ClassMember::Property(p) => {
                            if let Some(init) = p.initializer.as_ref() {
                                walk_expr(init, &mut out);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    out
}
