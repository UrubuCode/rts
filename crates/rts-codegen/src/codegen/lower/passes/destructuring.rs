//! Pass que expande destructuring patterns em decls separadas.
//!
//! Cobertura:
//! - Array: `const [a, b] = arr` → `const _t = arr; const a = _t[0]; const b = _t[1]`
//! - Object: `const {x, y} = obj` → `const _t = obj; const x = _t["x"]; const y = _t["y"]`
//! - Aliasing: `const {x: a} = obj` → `const a = _t["x"]`
//! - Default: `const {x = 0} = obj` via `??`
//! - Rest array/object via helpers map_clone+map_delete e for-loop com push

use swc_ecma_ast::{Decl, Expr, Lit, MemberProp, Pat, Stmt};

use crate::parser::ast::{ClassMember, Item, Program, RawStmt, Statement};
use crate::parser::span::Span;

pub(crate) fn expand_destructuring(program: &mut Program) {
    // Counter global para gerar nomes de temporários únicos.
    let mut counter: u32 = 0;

    // Helper: processa um Vec<Statement>, expandindo decls de destructuring.
    // Cria novos statements e replace in place.
    fn process_body(body: &mut Vec<Statement>, counter: &mut u32) {
        let mut i = 0;
        while i < body.len() {
            let Statement::Raw(raw) = &body[i];
            let Some(stmt) = raw.stmt.as_ref() else {
                i += 1;
                continue;
            };
            // \`Stmt::Decl(Decl::Var)\` pode ter patterns.
            let Stmt::Decl(Decl::Var(var_decl)) = stmt else {
                i += 1;
                continue;
            };

            // Verifica se algum decl tem pattern.
            let has_pattern = var_decl
                .decls
                .iter()
                .any(|d| !matches!(d.name, Pat::Ident(_)));
            if !has_pattern {
                i += 1;
                continue;
            }

            // Expande: gera múltiplos statements.
            let kind = var_decl.kind;
            let mut new_stmts: Vec<Statement> = Vec::new();
            for decl in &var_decl.decls {
                expand_destruct_decl(
                    &decl.name,
                    decl.init.as_deref(),
                    kind,
                    counter,
                    &mut new_stmts,
                );
            }

            // Substitui o stmt atual pelos novos.
            body.remove(i);
            for (k, s) in new_stmts.into_iter().enumerate() {
                body.insert(i + k, s);
            }
            i += 1;
        }
    }

    for item in program.items.iter_mut() {
        match item {
            Item::Function(f) => process_body(&mut f.body, &mut counter),
            Item::Class(c) => {
                for m in c.members.iter_mut() {
                    match m {
                        ClassMember::Constructor(ctor) => {
                            process_body(&mut ctor.body, &mut counter);
                        }
                        ClassMember::Method(method) => {
                            process_body(&mut method.body, &mut counter);
                        }
                        _ => {}
                    }
                }
            }
            Item::Statement(_) => {
                // Top-level: tratado em loop separado abaixo.
            }
            _ => {}
        }
    }

    // Top-level: itera com índice mutável.
    let mut i = 0;
    while i < program.items.len() {
        let needs_expansion = matches!(
            &program.items[i],
            Item::Statement(Statement::Raw(raw))
                if matches!(
                    raw.stmt.as_ref(),
                    Some(Stmt::Decl(Decl::Var(v)))
                    if v.decls.iter().any(|d| !matches!(d.name, Pat::Ident(_)))
                )
        );
        if !needs_expansion {
            i += 1;
            continue;
        }
        let Item::Statement(Statement::Raw(raw)) = &program.items[i] else {
            i += 1;
            continue;
        };
        let Some(Stmt::Decl(Decl::Var(var_decl))) = raw.stmt.as_ref() else {
            i += 1;
            continue;
        };
        let kind = var_decl.kind;
        let mut new_stmts: Vec<Statement> = Vec::new();
        for decl in &var_decl.decls {
            expand_destruct_decl(
                &decl.name,
                decl.init.as_deref(),
                kind,
                &mut counter,
                &mut new_stmts,
            );
        }
        program.items.remove(i);
        for (k, s) in new_stmts.into_iter().enumerate() {
            program.items.insert(i + k, Item::Statement(s));
        }
        i += 1;
    }
}

/// Expande um decl com pattern para Vec<Statement>. Recurse em nested.
fn expand_destruct_decl(
    pat: &Pat,
    init: Option<&Expr>,
    kind: swc_ecma_ast::VarDeclKind,
    counter: &mut u32,
    out: &mut Vec<Statement>,
) {
    match pat {
        Pat::Ident(_) => {
            // Sem destructuring — apenas regenera o decl simples.
            let var_decl = swc_ecma_ast::VarDecl {
                span: Default::default(),
                ctxt: Default::default(),
                kind,
                declare: false,
                decls: vec![swc_ecma_ast::VarDeclarator {
                    span: Default::default(),
                    name: pat.clone(),
                    init: init.map(|e| Box::new(e.clone())),
                    definite: false,
                }],
            };
            let stmt = Stmt::Decl(Decl::Var(Box::new(var_decl)));
            out.push(Statement::Raw(
                RawStmt::new("<destruct>".to_string(), Span::default()).with_stmt(stmt),
            ));
        }
        Pat::Array(arr) => {
            let tmp_name = format!("__destruct_{}", *counter);
            *counter += 1;
            // Gera const __destruct_N = init;
            if let Some(init) = init {
                out.push(make_const_decl(&tmp_name, init.clone(), kind));
            }
            for (idx, elem) in arr.elems.iter().enumerate() {
                let Some(e) = elem else { continue }; // hole — pula

                // Rest element: copia tudo a partir de idx para um novo
                // array. Sem default ou nesting interno (caso atipico).
                if let Pat::Rest(rest_pat) = e {
                    let rest_name = match rest_pat.arg.as_ref() {
                        Pat::Ident(b) => b.id.sym.to_string(),
                        _ => continue,
                    };
                    push_array_rest_decl(&tmp_name, &rest_name, idx, kind, out);
                    break; // rest precisa ser ultimo
                }

                // Default em array: \`[a = 5]\` — Pat::Assign.
                if let Pat::Assign(ap) = e {
                    let access = make_index_access_with_default(
                        &tmp_name,
                        idx as f64,
                        ap.right.as_ref(),
                    );
                    expand_destruct_decl(ap.left.as_ref(), Some(&access), kind, counter, out);
                    continue;
                }

                // const elem_name = __destruct_N[idx];
                let access = make_index_access(&tmp_name, idx as f64);
                expand_destruct_decl(&e, Some(&access), kind, counter, out);
            }
        }
        Pat::Object(obj) => {
            let tmp_name = format!("__destruct_{}", *counter);
            *counter += 1;
            if let Some(init) = init {
                out.push(make_const_decl(&tmp_name, init.clone(), kind));
            }
            // Coleta keys explicitamente capturadas — usadas pra construir
            // o `rest` removendo-as do clone (#312).
            let mut consumed_keys: Vec<String> = Vec::new();
            let mut rest_pat: Option<&Pat> = None;
            for prop in &obj.props {
                match prop {
                    swc_ecma_ast::ObjectPatProp::Assign(ap) => {
                        // \`{ x }\` ou \`{ x = default }\`
                        let key = ap.key.id.sym.to_string();
                        consumed_keys.push(key.clone());
                        let inner_pat = Pat::Ident(swc_ecma_ast::BindingIdent {
                            id: ap.key.id.clone(),
                            type_ann: None,
                        });
                        let access = if let Some(default) = ap.value.as_ref() {
                            make_member_access_with_default(&tmp_name, &key, default.as_ref())
                        } else {
                            make_member_access(&tmp_name, &key)
                        };
                        expand_destruct_decl(&inner_pat, Some(&access), kind, counter, out);
                    }
                    swc_ecma_ast::ObjectPatProp::KeyValue(kvp) => {
                        // \`{ x: a }\` — alias; tambem \`{ x: a = default }\`
                        // (#551) SWC representa default em KeyValue como
                        // value=Pat::Assign{left,right=default}.
                        let key = match &kvp.key {
                            swc_ecma_ast::PropName::Ident(id) => id.sym.to_string(),
                            swc_ecma_ast::PropName::Str(s) => s.value.to_string_lossy().to_string(),
                            _ => continue,
                        };
                        consumed_keys.push(key.clone());
                        let (target_pat, access) = if let Pat::Assign(ap) = kvp.value.as_ref() {
                            let acc = make_member_access_with_default(
                                &tmp_name,
                                &key,
                                ap.right.as_ref(),
                            );
                            (ap.left.as_ref(), acc)
                        } else {
                            let acc = make_member_access(&tmp_name, &key);
                            (kvp.value.as_ref(), acc)
                        };
                        expand_destruct_decl(target_pat, Some(&access), kind, counter, out);
                    }
                    swc_ecma_ast::ObjectPatProp::Rest(rest) => {
                        rest_pat = Some(rest.arg.as_ref());
                    }
                }
            }
            // Object rest (#312): `const { a, b, ...rest } = obj` →
            //   const rest = collections.map_clone(__destruct_N);
            //   collections.map_delete(rest, "a");
            //   collections.map_delete(rest, "b");
            if let Some(pat) = rest_pat {
                if let Pat::Ident(rest_id) = pat {
                    let rest_name = rest_id.id.sym.to_string();
                    push_object_rest_decls(&rest_name, &tmp_name, &consumed_keys, kind, out);
                }
            }
        }
        _ => {
            // Outros patterns (Rest, Assign solo, etc) — emite decl direto
            // se for Ident interno; senão silencia.
        }
    }
}

/// `const <name> = <expr>;` (kind preservado).
fn make_const_decl(name: &str, expr: Expr, kind: swc_ecma_ast::VarDeclKind) -> Statement {
    let var = swc_ecma_ast::VarDecl {
        span: Default::default(),
        ctxt: Default::default(),
        kind,
        declare: false,
        decls: vec![swc_ecma_ast::VarDeclarator {
            span: Default::default(),
            name: Pat::Ident(swc_ecma_ast::BindingIdent {
                id: swc_ecma_ast::Ident {
                    span: Default::default(),
                    ctxt: Default::default(),
                    sym: name.into(),
                    optional: false,
                },
                type_ann: None,
            }),
            init: Some(Box::new(expr)),
            definite: false,
        }],
    };
    let stmt = Stmt::Decl(Decl::Var(Box::new(var)));
    Statement::Raw(RawStmt::new("<destruct-tmp>".to_string(), Span::default()).with_stmt(stmt))
}

/// `<obj>[<idx>]` como Expr.
fn make_index_access(obj_name: &str, idx: f64) -> Expr {
    Expr::Member(swc_ecma_ast::MemberExpr {
        span: Default::default(),
        obj: Box::new(Expr::Ident(swc_ecma_ast::Ident {
            span: Default::default(),
            ctxt: Default::default(),
            sym: obj_name.into(),
            optional: false,
        })),
        prop: MemberProp::Computed(swc_ecma_ast::ComputedPropName {
            span: Default::default(),
            expr: Box::new(Expr::Lit(Lit::Num(swc_ecma_ast::Number {
                span: Default::default(),
                value: idx,
                raw: Some(format!("{}", idx as i64).into()),
            }))),
        }),
    })
}

/// `<obj>.<key>` como Expr.
fn make_member_access(obj_name: &str, key: &str) -> Expr {
    Expr::Member(swc_ecma_ast::MemberExpr {
        span: Default::default(),
        obj: Box::new(Expr::Ident(swc_ecma_ast::Ident {
            span: Default::default(),
            ctxt: Default::default(),
            sym: obj_name.into(),
            optional: false,
        })),
        prop: MemberProp::Ident(swc_ecma_ast::IdentName {
            span: Default::default(),
            sym: key.into(),
        }),
    })
}

/// `<obj>[<idx>] ?? <default>` como Expr.
/// Em destructuring com default, queremos: se o slot eh nullish/missing,
/// usa default. Em RTS isso tambem dispara em valor 0 — JS distingue mas
/// custaria adicionar IR proprio so pra isso. Boa aproximacao pra MVP.
fn make_index_access_with_default(obj_name: &str, idx: f64, default: &Expr) -> Expr {
    let access = make_index_access(obj_name, idx);
    Expr::Bin(swc_ecma_ast::BinExpr {
        span: Default::default(),
        op: swc_ecma_ast::BinaryOp::NullishCoalescing,
        left: Box::new(access),
        right: Box::new(default.clone()),
    })
}

/// `<obj>.<key> ?? <default>` como Expr.
fn make_member_access_with_default(obj_name: &str, key: &str, default: &Expr) -> Expr {
    let access = make_member_access(obj_name, key);
    Expr::Bin(swc_ecma_ast::BinExpr {
        span: Default::default(),
        op: swc_ecma_ast::BinaryOp::NullishCoalescing,
        left: Box::new(access),
        right: Box::new(default.clone()),
    })
}

/// Empurra em `out` o desugar de object rest (#312):
///   `const <rest_name> = collections.map_clone(<src_name>);`
///   `collections.map_delete(<rest_name>, "<key>");` (uma por consumed_key)
///
/// Reusa o builtin `collections.map_clone` (registra entry novo no
/// HandleTable com clone do IndexMap) + `map_delete` ja existente.
fn push_object_rest_decls(
    rest_name: &str,
    src_name: &str,
    consumed_keys: &[String],
    kind: swc_ecma_ast::VarDeclKind,
    out: &mut Vec<Statement>,
) {
    use swc_ecma_ast::{CallExpr, ExprOrSpread, ExprStmt, Ident, MemberExpr, Stmt};

    fn ident(name: &str) -> Ident {
        Ident {
            span: Default::default(),
            ctxt: Default::default(),
            sym: name.into(),
            optional: false,
        }
    }

    // collections.map_clone(<src_name>)
    let clone_call = Expr::Call(CallExpr {
        span: Default::default(),
        ctxt: Default::default(),
        callee: swc_ecma_ast::Callee::Expr(Box::new(Expr::Member(MemberExpr {
            span: Default::default(),
            obj: Box::new(Expr::Ident(ident("collections"))),
            prop: MemberProp::Ident(swc_ecma_ast::IdentName {
                span: Default::default(),
                sym: "map_clone".into(),
            }),
        }))),
        args: vec![ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Ident(ident(src_name))),
        }],
        type_args: None,
    });
    out.push(make_const_decl(rest_name, clone_call, kind));

    // collections.map_delete(<rest_name>, "<key>"); para cada consumed_key
    for key in consumed_keys {
        let delete_call = Expr::Call(CallExpr {
            span: Default::default(),
            ctxt: Default::default(),
            callee: swc_ecma_ast::Callee::Expr(Box::new(Expr::Member(MemberExpr {
                span: Default::default(),
                obj: Box::new(Expr::Ident(ident("collections"))),
                prop: MemberProp::Ident(swc_ecma_ast::IdentName {
                    span: Default::default(),
                    sym: "map_delete".into(),
                }),
            }))),
            args: vec![
                ExprOrSpread {
                    spread: None,
                    expr: Box::new(Expr::Ident(ident(rest_name))),
                },
                ExprOrSpread {
                    spread: None,
                    expr: Box::new(Expr::Lit(Lit::Str(swc_ecma_ast::Str {
                        span: Default::default(),
                        value: key.as_str().into(),
                        raw: None,
                    }))),
                },
            ],
            type_args: None,
        });
        let stmt = Stmt::Expr(ExprStmt {
            span: Default::default(),
            expr: Box::new(delete_call),
        });
        let raw = crate::parser::ast::RawStmt::new(
            format!("collections.map_delete({rest_name}, \"{key}\");"),
            crate::parser::span::Span::default(),
        )
        .with_stmt(stmt);
        out.push(crate::parser::ast::Statement::Raw(raw));
    }
}

/// Empurra em \`out\`:
///   \`const <rest_name> = [];\`
///   \`for (let __i = <start_idx>; __i < <obj>.length; __i++) <rest_name>.push(<obj>[__i]);\`
fn push_array_rest_decl(
    obj_name: &str,
    rest_name: &str,
    start_idx: usize,
    kind: swc_ecma_ast::VarDeclKind,
    out: &mut Vec<Statement>,
) {
    use swc_ecma_ast::{
        AssignOp, BinaryOp, BlockStmt, ForStmt, Ident, MemberExpr, Stmt, UpdateExpr, UpdateOp,
        VarDecl, VarDeclKind, VarDeclOrExpr, VarDeclarator,
    };

    let counter_name = format!("__rest_i_{}", obj_name);

    fn ident(name: &str) -> Ident {
        Ident {
            span: Default::default(),
            ctxt: Default::default(),
            sym: name.into(),
            optional: false,
        }
    }

    let arr_init = Expr::Array(swc_ecma_ast::ArrayLit {
        span: Default::default(),
        elems: vec![],
    });

    // const <rest_name> = [];
    let rest_decl_stmt = make_const_decl(rest_name, arr_init, kind);

    // for (let __rest_i = start_idx; __rest_i < obj.length; __rest_i++)
    //   <rest_name>.push(obj[__rest_i]);
    let init = VarDeclOrExpr::VarDecl(Box::new(VarDecl {
        span: Default::default(),
        ctxt: Default::default(),
        kind: VarDeclKind::Let,
        declare: false,
        decls: vec![VarDeclarator {
            span: Default::default(),
            name: Pat::Ident(swc_ecma_ast::BindingIdent {
                id: ident(&counter_name),
                type_ann: None,
            }),
            init: Some(Box::new(Expr::Lit(Lit::Num(swc_ecma_ast::Number {
                span: Default::default(),
                value: start_idx as f64,
                raw: Some(format!("{}", start_idx).into()),
            })))),
            definite: false,
        }],
    }));

    let length_access = Expr::Member(MemberExpr {
        span: Default::default(),
        obj: Box::new(Expr::Ident(ident(obj_name))),
        prop: MemberProp::Ident(swc_ecma_ast::IdentName {
            span: Default::default(),
            sym: "length".into(),
        }),
    });

    let test = Expr::Bin(swc_ecma_ast::BinExpr {
        span: Default::default(),
        op: BinaryOp::Lt,
        left: Box::new(Expr::Ident(ident(&counter_name))),
        right: Box::new(length_access),
    });

    let update = Expr::Update(UpdateExpr {
        span: Default::default(),
        op: UpdateOp::PlusPlus,
        prefix: false,
        arg: Box::new(Expr::Ident(ident(&counter_name))),
    });

    // <rest_name>.push(<obj>[<counter>])
    let push_call = Expr::Call(swc_ecma_ast::CallExpr {
        span: Default::default(),
        ctxt: Default::default(),
        callee: swc_ecma_ast::Callee::Expr(Box::new(Expr::Member(MemberExpr {
            span: Default::default(),
            obj: Box::new(Expr::Ident(ident(rest_name))),
            prop: MemberProp::Ident(swc_ecma_ast::IdentName {
                span: Default::default(),
                sym: "push".into(),
            }),
        }))),
        args: vec![swc_ecma_ast::ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Member(MemberExpr {
                span: Default::default(),
                obj: Box::new(Expr::Ident(ident(obj_name))),
                prop: MemberProp::Computed(swc_ecma_ast::ComputedPropName {
                    span: Default::default(),
                    expr: Box::new(Expr::Ident(ident(&counter_name))),
                }),
            })),
        }],
        type_args: None,
    });

    let body_stmt = Stmt::Expr(swc_ecma_ast::ExprStmt {
        span: Default::default(),
        expr: Box::new(push_call),
    });

    let for_stmt = Stmt::For(ForStmt {
        span: Default::default(),
        init: Some(init),
        test: Some(Box::new(test)),
        update: Some(Box::new(update)),
        body: Box::new(Stmt::Block(BlockStmt {
            span: Default::default(),
            ctxt: Default::default(),
            stmts: vec![body_stmt],
        })),
    });

    let _ = AssignOp::Assign;
    out.push(rest_decl_stmt);
    out.push(Statement::Raw(
        RawStmt::new("<destruct-rest-loop>".to_string(), Span::default()).with_stmt(for_stmt),
    ));
}
