//! Helpers compartilhados pelo pipeline de compilacao:
//! - `is_lifted_callback` + `user_call_conv` decidem call conv de
//!   user fns (lifted vs C ABI vs Tail).
//! - `collect_var_decls` coleta `var x` para hoisting.

use cranelift_codegen::isa::CallConv;
use cranelift_module::Module;
use swc_ecma_ast::{Decl, ForHead, Pat, Stmt};

pub(crate) fn is_lifted_callback(name: &str) -> bool {
    // Trampolins simples (sem captura de `this`): `__lifted_arrow_N`.
    // Trampolins de classe (capturam `this`/`super`): `__class_C_lifted_arrow_N`.
    // Ambos atravessam a fronteira C ABI quando invocados pelo FLTK.
    if name.starts_with("__lifted_arrow_") {
        return true;
    }
    if let Some(rest) = name.strip_prefix("__class_") {
        if rest.contains("_lifted_arrow_") {
            return true;
        }
    }
    false
}

/// User-defined functions generally use the Tail calling convention so codegen
/// can emit `return_call` for tail-position invocations (#93). Lifted UI
/// callbacks are the exception: they cross a native C ABI boundary, e
/// fns cujo endereço é tomado (passadas a APIs nativas como
/// `thread.spawn`, FFI, etc — #206).
pub(crate) fn user_call_conv(module: &dyn Module, fn_name: &str, address_taken: bool) -> CallConv {
    if is_lifted_callback(fn_name) || address_taken {
        module.isa().default_call_conv()
    } else {
        CallConv::Tail
    }
}

pub(crate) fn collect_var_decls(stmt: &Stmt, out: &mut Vec<String>) {
    match stmt {
        Stmt::Decl(Decl::Var(vd)) => {
            if matches!(vd.kind, swc_ecma_ast::VarDeclKind::Var) {
                for d in &vd.decls {
                    if let Pat::Ident(id) = &d.name {
                        out.push(id.id.sym.as_str().to_string());
                    }
                }
            }
        }
        Stmt::Block(b) => {
            for s in &b.stmts {
                collect_var_decls(s, out);
            }
        }
        Stmt::If(i) => {
            collect_var_decls(&i.cons, out);
            if let Some(alt) = &i.alt {
                collect_var_decls(alt, out);
            }
        }
        Stmt::For(f) => {
            if let Some(swc_ecma_ast::VarDeclOrExpr::VarDecl(vd)) = &f.init {
                if matches!(vd.kind, swc_ecma_ast::VarDeclKind::Var) {
                    for d in &vd.decls {
                        if let Pat::Ident(id) = &d.name {
                            out.push(id.id.sym.as_str().to_string());
                        }
                    }
                }
            }
            collect_var_decls(&f.body, out);
        }
        Stmt::ForIn(f) => {
            if let ForHead::VarDecl(vd) = &f.left {
                if matches!(vd.kind, swc_ecma_ast::VarDeclKind::Var) {
                    for d in &vd.decls {
                        if let Pat::Ident(id) = &d.name {
                            out.push(id.id.sym.as_str().to_string());
                        }
                    }
                }
            }
            collect_var_decls(&f.body, out);
        }
        Stmt::ForOf(f) => {
            if let ForHead::VarDecl(vd) = &f.left {
                if matches!(vd.kind, swc_ecma_ast::VarDeclKind::Var) {
                    for d in &vd.decls {
                        if let Pat::Ident(id) = &d.name {
                            out.push(id.id.sym.as_str().to_string());
                        }
                    }
                }
            }
            collect_var_decls(&f.body, out);
        }
        Stmt::While(w) => collect_var_decls(&w.body, out),
        Stmt::DoWhile(d) => collect_var_decls(&d.body, out),
        Stmt::Try(t) => {
            for s in &t.block.stmts {
                collect_var_decls(s, out);
            }
            if let Some(h) = &t.handler {
                for s in &h.body.stmts {
                    collect_var_decls(s, out);
                }
            }
            if let Some(f) = &t.finalizer {
                for s in &f.stmts {
                    collect_var_decls(s, out);
                }
            }
        }
        Stmt::Switch(sw) => {
            for case in &sw.cases {
                for s in &case.cons {
                    collect_var_decls(s, out);
                }
            }
        }
        Stmt::Labeled(l) => collect_var_decls(&l.body, out),
        Stmt::With(w) => collect_var_decls(&w.body, out),
        // function/arrow/class declarations dentro do body criam novo
        // scope — nao recursa.
        _ => {}
    }
}



