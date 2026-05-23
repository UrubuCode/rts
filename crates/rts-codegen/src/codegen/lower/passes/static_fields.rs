//! Pass que expande static fields de classe pra `let` globais.
//!
//! Cada `static x: T = init` vira uma `let __class_static_<C>_<x> = init`
//! no top-level, e todos os usos `C.x` (read/write/update) sao reescritos
//! para o ident global. Static blocks `static { ... }` viram statements
//! top-level apos as decls.
//!
//! Precondicao: roda antes de expand_destructuring/default_args/etc,
//! mas depois do parser. As classes ainda estao em Item::Class.

use std::collections::HashMap;

use swc_ecma_ast::{Decl, Expr, Lit, MemberProp, Stmt};

use crate::parser::ast::{ClassMember, Item, Program, RawStmt, Statement};
use crate::parser::span::Span;

pub(crate) fn expand_static_fields(program: &mut Program) {
    // Coleta (class, field, type_ann, init) e remove os Property static.
    struct StaticField {
        class: String,
        field: String,
        type_ann: Option<String>,
        init: Option<Box<Expr>>,
    }
    let mut fields: Vec<StaticField> = Vec::new();
    // Mapa: (class_name, field) -> nome global gerado. Usado pra
    // reescrita de C.field e pra detectar conflitos.
    let mut static_map: HashMap<(String, String), String> = HashMap::new();
    // Stmts dos `static { ... }` blocks, drenados das ClassDecls,
    // agrupados por classe pra re-emitir na posicao correta.
    // (#1077) preserva ordem source mantendo ligacao class->stmts.
    // (#1077 follow-up) Cada entry eh (count_of_static_fields_before, stmts)
    // pra reconstruir interleaving (field, block, field, ...).
    let mut static_init_by_class: HashMap<String, Vec<(usize, Vec<Statement>)>> = HashMap::new();

    for item in program.items.iter_mut() {
        let Item::Class(class) = item else { continue };
        let class_name = class.name.clone();
        class.members.retain(|m| {
            if let ClassMember::Property(p) = m {
                if p.modifiers.is_static {
                    // (cross-runtime #269) Privates (`#v`) viram `__priv_v`
                    // no nome global — o `#` no ident name confunde o
                    // lookup de var no codegen.
                    let safe_field = if let Some(rest) = p.name.strip_prefix('#') {
                        format!("__priv_{}", rest)
                    } else {
                        p.name.clone()
                    };
                    let global_name = format!("__class_static_{}_{}", class_name, safe_field);
                    static_map.insert((class_name.clone(), p.name.clone()), global_name);
                    fields.push(StaticField {
                        class: class_name.clone(),
                        field: p.name.clone(),
                        type_ann: p.type_annotation.clone(),
                        init: p.initializer.clone(),
                    });
                    return false; // remove
                }
            }
            true
        });
        // Drena `static { ... }` blocks. (#1077 follow-up) Usa
        // static_init_blocks (com indice de fields antes) pra interleaving.
        // static_init_body permanece para callers que ainda esperam flat list.
        if !class.static_init_blocks.is_empty() {
            static_init_by_class
                .entry(class_name.clone())
                .or_default()
                .extend(std::mem::take(&mut class.static_init_blocks));
            // Limpa o flat tb pra nao re-emit duplicado.
            class.static_init_body.clear();
        }
    }

    if fields.is_empty() && static_init_by_class.is_empty() {
        return;
    }

    // Reescreve C.F em todas as expressões. Suporta:
    //   - read:  Class.field           -> Ident(global)
    //   - write: Class.field = v       -> Ident(global) = v
    //   - update: Class.field++        -> via Ident
    //   - compound: Class.field += v   -> via Ident
    let static_keys: HashMap<String, HashMap<String, String>> = {
        let mut m: HashMap<String, HashMap<String, String>> = HashMap::new();
        for ((c, f), g) in &static_map {
            m.entry(c.clone()).or_default().insert(f.clone(), g.clone());
        }
        m
    };

    rewrite_static_in_program(program, &static_keys);

    // Insere `let __class_static_<C>_<F> = init;` antes de qualquer
    // statement no programa. Tem que rodar antes do uso, e o codegen
    // promove `let` top-level a global automaticamente.
    let mut decls: Vec<Item> = Vec::new();
    for sf in &fields {
        let safe_field = if let Some(rest) = sf.field.strip_prefix('#') {
            format!("__priv_{}", rest)
        } else {
            sf.field.clone()
        };
        let global_name = format!("__class_static_{}_{}", sf.class, safe_field);
        let mut init_expr = match &sf.init {
            Some(e) => (**e).clone(),
            None => default_expr_for_ann(sf.type_ann.as_deref()),
        };
        // (cross-runtime #269) init eh clone do AST original — precisa
        // passar pelo rewrite para que `C.field` interno (e.g. static x =
        // C.order.push(...)) seja substituido pelo ident global.
        rewrite_static_in_expr(&mut init_expr, &static_keys);
        let mut binding = swc_ecma_ast::BindingIdent {
            id: swc_ecma_ast::Ident {
                span: Default::default(),
                ctxt: Default::default(),
                sym: global_name.into(),
                optional: false,
            },
            type_ann: None,
        };
        if let Some(ann) = sf.type_ann.as_deref() {
            // Anota `let x: T = ...` para preservar tipo.
            binding.type_ann = build_type_ann(ann);
        }
        let var = swc_ecma_ast::VarDecl {
            span: Default::default(),
            ctxt: Default::default(),
            kind: swc_ecma_ast::VarDeclKind::Let,
            declare: false,
            decls: vec![swc_ecma_ast::VarDeclarator {
                span: Default::default(),
                name: swc_ecma_ast::Pat::Ident(binding),
                init: Some(Box::new(init_expr)),
                definite: false,
            }],
        };
        let stmt = Stmt::Decl(Decl::Var(Box::new(var)));
        let raw = RawStmt::new("<static-field>".to_string(), Span::default()).with_stmt(stmt);
        decls.push(Item::Statement(Statement::Raw(raw)));
    }

    // Anexa os stmts dos `static { }` blocks depois das declaracoes
    // das let. Eles vao por `rewrite_static_in_program` na proxima fase
    // — mas como ja' rodamos rewrite acima, precisamos rodar so' neles.
    for (_cls, blocks) in static_init_by_class.iter_mut() {
        for (_count_before, stmts) in blocks.iter_mut() {
            for stmt in stmts.iter_mut() {
                let Statement::Raw(r) = stmt;
                if let Some(s) = r.stmt.as_mut() {
                    rewrite_static_in_swc_stmt(s, &static_keys);
                }
            }
        }
    }

    // (cross-runtime #1077) Insere os decls de static field IMEDIATAMENTE
    // antes da Class declaration correspondente — preservando program order.
    // Antes a logica colocava tudo antes da PRIMEIRA classe, o que quebrava
    // quando o initializer referenciava idents declarados depois daquela
    // classe (ex: `class A {}; const slog = []; class S { static a = slog.push(...) }`
    // movia o `let __class_static_S_a = slog.push(...)` para antes de `const slog`).
    //
    // static_init_stmts (de `static { ... }` blocks) tambem sao agrupados
    // por classe — vao logo depois das decls da sua classe.
    // (#1077) Decls de fields por classe, preservando ordem de declaracao.
    let mut decls_by_class: HashMap<String, Vec<Item>> = HashMap::new();
    for sf_idx in 0..fields.len() {
        let sf = &fields[sf_idx];
        decls_by_class.entry(sf.class.clone()).or_default().push(decls[sf_idx].clone());
    }
    let mut new_items: Vec<Item> = Vec::with_capacity(program.items.len() + decls.len());
    for item in program.items.drain(..) {
        if let Item::Class(c) = &item {
            let class_name = c.name.clone();
            let class_field_decls = decls_by_class.remove(&class_name).unwrap_or_default();
            let class_blocks = static_init_by_class.remove(&class_name).unwrap_or_default();
            // (#1077 follow-up) Interleave fields e blocks pela ordem source.
            // class_blocks eh Vec<(count_fields_before, stmts)>. Para cada
            // field index N, emite todos os blocks com count_before == N
            // antes do field N+1.
            let mut block_iter = class_blocks.into_iter().peekable();
            for (i, decl) in class_field_decls.iter().enumerate() {
                // Blocks com count_before == i vao antes do field i.
                while let Some((cb, _)) = block_iter.peek() {
                    if *cb == i {
                        let (_, stmts) = block_iter.next().unwrap();
                        for s in stmts {
                            new_items.push(Item::Statement(s));
                        }
                    } else {
                        break;
                    }
                }
                new_items.push(decl.clone());
            }
            // Blocks que aparecem apos todos os fields (count_before >= len).
            for (_, stmts) in block_iter {
                for s in stmts {
                    new_items.push(Item::Statement(s));
                }
            }
            new_items.push(item);
        } else {
            new_items.push(item);
        }
    }
    // Defensivo: decls/blocks remanescentes (classe nao encontrada) — no fim.
    for (_cls, ds) in decls_by_class {
        for d in ds {
            new_items.push(d);
        }
    }
    for (_cls, blocks) in static_init_by_class {
        for (_, stmts) in blocks {
            for s in stmts {
                new_items.push(Item::Statement(s));
            }
        }
    }
    program.items = new_items;
}

/// Default value compativel com a anotacao do field. Static fields sem
/// initializer (ex: `static readonly X: string;`) ainda precisam de um
/// init na let global pra Cranelift nao reclamar de tipo. O valor real
/// chega via static {} block.
fn default_expr_for_ann(ann: Option<&str>) -> Expr {
    match ann.map(str::trim) {
        Some("string") => Expr::Lit(Lit::Str(swc_ecma_ast::Str {
            span: Default::default(),
            value: "".into(),
            raw: None,
        })),
        Some("boolean") | Some("bool") => Expr::Lit(Lit::Bool(swc_ecma_ast::Bool {
            span: Default::default(),
            value: false,
        })),
        _ => Expr::Lit(Lit::Num(swc_ecma_ast::Number {
            span: Default::default(),
            value: 0.0,
            raw: None,
        })),
    }
}

/// Constroi um TsTypeAnn minimo a partir de uma anotacao como "number"
/// ou "string". Tipos compostos sao deixados como undefined-ann; o
/// codegen ainda usara o tipo do initializer.
fn build_type_ann(ann: &str) -> Option<Box<swc_ecma_ast::TsTypeAnn>> {
    use swc_ecma_ast::{TsKeywordType, TsKeywordTypeKind, TsType, TsTypeAnn};
    let kind = match ann.trim() {
        "number" | "i64" | "f64" | "i32" => TsKeywordTypeKind::TsNumberKeyword,
        "string" => TsKeywordTypeKind::TsStringKeyword,
        "boolean" | "bool" => TsKeywordTypeKind::TsBooleanKeyword,
        _ => return None,
    };
    Some(Box::new(TsTypeAnn {
        span: Default::default(),
        type_ann: Box::new(TsType::TsKeywordType(TsKeywordType {
            span: Default::default(),
            kind,
        })),
    }))
}

/// Reescreve `C.F` -> `Ident(__class_static_C_F)` em todo o programa.
fn rewrite_static_in_program(
    program: &mut Program,
    map: &HashMap<String, HashMap<String, String>>,
) {
    for item in program.items.iter_mut() {
        match item {
            Item::Function(f) => {
                for stmt in f.body.iter_mut() {
                    rewrite_static_in_statement(stmt, map);
                }
            }
            Item::Class(c) => {
                for m in c.members.iter_mut() {
                    match m {
                        ClassMember::Constructor(ctor) => {
                            for s in ctor.body.iter_mut() {
                                rewrite_static_in_statement(s, map);
                            }
                        }
                        ClassMember::Method(method) => {
                            for s in method.body.iter_mut() {
                                rewrite_static_in_statement(s, map);
                            }
                        }
                        ClassMember::Property(p) => {
                            if let Some(init) = p.initializer.as_mut() {
                                rewrite_static_in_expr(init, map);
                            }
                        }
                    }
                }
            }
            Item::Statement(Statement::Raw(r)) => {
                if let Some(s) = r.stmt.as_mut() {
                    rewrite_static_in_swc_stmt(s, map);
                }
            }
            _ => {}
        }
    }
}

fn rewrite_static_in_statement(
    stmt: &mut Statement,
    map: &HashMap<String, HashMap<String, String>>,
) {
    let Statement::Raw(r) = stmt;
    if let Some(s) = r.stmt.as_mut() {
        rewrite_static_in_swc_stmt(s, map);
    }
}

fn rewrite_static_in_swc_stmt(s: &mut Stmt, map: &HashMap<String, HashMap<String, String>>) {
    match s {
        Stmt::Block(b) => {
            for s in b.stmts.iter_mut() {
                rewrite_static_in_swc_stmt(s, map);
            }
        }
        Stmt::Expr(e) => rewrite_static_in_expr(&mut e.expr, map),
        Stmt::Return(r) => {
            if let Some(e) = r.arg.as_mut() {
                rewrite_static_in_expr(e, map);
            }
        }
        Stmt::If(i) => {
            rewrite_static_in_expr(&mut i.test, map);
            rewrite_static_in_swc_stmt(&mut i.cons, map);
            if let Some(alt) = i.alt.as_mut() {
                rewrite_static_in_swc_stmt(alt, map);
            }
        }
        Stmt::While(w) => {
            rewrite_static_in_expr(&mut w.test, map);
            rewrite_static_in_swc_stmt(&mut w.body, map);
        }
        Stmt::DoWhile(d) => {
            rewrite_static_in_expr(&mut d.test, map);
            rewrite_static_in_swc_stmt(&mut d.body, map);
        }
        Stmt::For(f) => {
            if let Some(init) = f.init.as_mut() {
                match init {
                    swc_ecma_ast::VarDeclOrExpr::Expr(e) => rewrite_static_in_expr(e, map),
                    swc_ecma_ast::VarDeclOrExpr::VarDecl(vd) => {
                        for d in vd.decls.iter_mut() {
                            if let Some(e) = d.init.as_mut() {
                                rewrite_static_in_expr(e, map);
                            }
                        }
                    }
                }
            }
            if let Some(t) = f.test.as_mut() {
                rewrite_static_in_expr(t, map);
            }
            if let Some(u) = f.update.as_mut() {
                rewrite_static_in_expr(u, map);
            }
            rewrite_static_in_swc_stmt(&mut f.body, map);
        }
        Stmt::ForOf(f) => {
            rewrite_static_in_expr(&mut f.right, map);
            rewrite_static_in_swc_stmt(&mut f.body, map);
        }
        Stmt::ForIn(f) => {
            rewrite_static_in_expr(&mut f.right, map);
            rewrite_static_in_swc_stmt(&mut f.body, map);
        }
        Stmt::Switch(sw) => {
            rewrite_static_in_expr(&mut sw.discriminant, map);
            for c in sw.cases.iter_mut() {
                if let Some(t) = c.test.as_mut() {
                    rewrite_static_in_expr(t, map);
                }
                for s in c.cons.iter_mut() {
                    rewrite_static_in_swc_stmt(s, map);
                }
            }
        }
        Stmt::Throw(t) => rewrite_static_in_expr(&mut t.arg, map),
        Stmt::Try(t) => {
            for s in t.block.stmts.iter_mut() {
                rewrite_static_in_swc_stmt(s, map);
            }
            if let Some(h) = t.handler.as_mut() {
                for s in h.body.stmts.iter_mut() {
                    rewrite_static_in_swc_stmt(s, map);
                }
            }
            if let Some(f) = t.finalizer.as_mut() {
                for s in f.stmts.iter_mut() {
                    rewrite_static_in_swc_stmt(s, map);
                }
            }
        }
        Stmt::Decl(swc_ecma_ast::Decl::Var(v)) => {
            for d in v.decls.iter_mut() {
                if let Some(e) = d.init.as_mut() {
                    rewrite_static_in_expr(e, map);
                }
            }
        }
        Stmt::Labeled(l) => rewrite_static_in_swc_stmt(&mut l.body, map),
        _ => {}
    }
}

fn rewrite_static_in_expr(e: &mut Expr, map: &HashMap<String, HashMap<String, String>>) {
    // Caso 1: Member { obj: Ident(C), prop: Ident(F) } onde (C,F) sao
    // static field — substitui o Member inteiro por Ident(global).
    if let Expr::Member(m) = e {
        if let Expr::Ident(obj) = m.obj.as_ref() {
            // (cross-runtime #269) Aceita tanto Ident quanto PrivateName.
            // `C.field` e `C.#field` precisam ambos reescrever — o pass
            // armazena o nome do field como esta no source (com `#` para
            // privates) em static_map.
            let prop_name: Option<String> = match &m.prop {
                MemberProp::Ident(prop) => Some(prop.sym.to_string()),
                MemberProp::PrivateName(pn) => Some(format!("#{}", pn.name.as_ref())),
                _ => None,
            };
            if let Some(name) = prop_name {
                if let Some(fields) = map.get(obj.sym.as_ref()) {
                    if let Some(global) = fields.get(&name) {
                        *e = Expr::Ident(swc_ecma_ast::Ident {
                            span: m.span,
                            ctxt: Default::default(),
                            sym: global.clone().into(),
                            optional: false,
                        });
                        return;
                    }
                }
            }
        }
    }
    // Recursa em sub-expressões.
    match e {
        Expr::Bin(b) => {
            rewrite_static_in_expr(&mut b.left, map);
            rewrite_static_in_expr(&mut b.right, map);
        }
        Expr::Unary(u) => rewrite_static_in_expr(&mut u.arg, map),
        Expr::Update(u) => rewrite_static_in_expr(&mut u.arg, map),
        Expr::Assign(a) => {
            // LHS pode ser Class.field. swc tem AssignTarget — quando
            // for SimpleAssignTarget::Member com Class.field, substitui
            // por SimpleAssignTarget::Ident(global).
            if let swc_ecma_ast::AssignTarget::Simple(
                swc_ecma_ast::SimpleAssignTarget::Member(m),
            ) = &mut a.left
            {
                // (cross-runtime #269) Aceita Ident e PrivateName no LHS.
                let prop_name: Option<String> = if let Expr::Ident(_) = m.obj.as_ref() {
                    match &m.prop {
                        MemberProp::Ident(prop) => Some(prop.sym.to_string()),
                        MemberProp::PrivateName(pn) => Some(format!("#{}", pn.name.as_ref())),
                        _ => None,
                    }
                } else {
                    None
                };
                if let (Expr::Ident(obj), Some(name)) = (m.obj.as_ref(), prop_name) {
                    if let Some(fields) = map.get(obj.sym.as_ref()) {
                        if let Some(global) = fields.get(&name) {
                            a.left = swc_ecma_ast::AssignTarget::Simple(
                                swc_ecma_ast::SimpleAssignTarget::Ident(
                                    swc_ecma_ast::BindingIdent {
                                        id: swc_ecma_ast::Ident {
                                            span: m.span,
                                            ctxt: Default::default(),
                                            sym: global.clone().into(),
                                            optional: false,
                                        },
                                        type_ann: None,
                                    },
                                ),
                            );
                        } else {
                            rewrite_static_in_expr(&mut m.obj, map);
                        }
                    } else {
                        rewrite_static_in_expr(&mut m.obj, map);
                    }
                } else {
                    rewrite_static_in_expr(&mut m.obj, map);
                }
            }
            rewrite_static_in_expr(&mut a.right, map);
        }
        Expr::Cond(c) => {
            rewrite_static_in_expr(&mut c.test, map);
            rewrite_static_in_expr(&mut c.cons, map);
            rewrite_static_in_expr(&mut c.alt, map);
        }
        Expr::Call(c) => {
            if let swc_ecma_ast::Callee::Expr(callee) = &mut c.callee {
                rewrite_static_in_expr(callee, map);
            }
            for a in c.args.iter_mut() {
                rewrite_static_in_expr(&mut a.expr, map);
            }
        }
        Expr::New(n) => {
            rewrite_static_in_expr(&mut n.callee, map);
            if let Some(args) = n.args.as_mut() {
                for a in args.iter_mut() {
                    rewrite_static_in_expr(&mut a.expr, map);
                }
            }
        }
        Expr::Member(m) => {
            rewrite_static_in_expr(&mut m.obj, map);
            if let MemberProp::Computed(c) = &mut m.prop {
                rewrite_static_in_expr(&mut c.expr, map);
            }
        }
        Expr::Paren(p) => rewrite_static_in_expr(&mut p.expr, map),
        Expr::Seq(s) => {
            for e in s.exprs.iter_mut() {
                rewrite_static_in_expr(e, map);
            }
        }
        Expr::Array(a) => {
            for el in a.elems.iter_mut().flatten() {
                rewrite_static_in_expr(&mut el.expr, map);
            }
        }
        Expr::Object(o) => {
            for p in o.props.iter_mut() {
                if let swc_ecma_ast::PropOrSpread::Prop(p) = p {
                    if let swc_ecma_ast::Prop::KeyValue(kv) = p.as_mut() {
                        rewrite_static_in_expr(&mut kv.value, map);
                    }
                }
            }
        }
        Expr::Tpl(t) => {
            for e in t.exprs.iter_mut() {
                rewrite_static_in_expr(e, map);
            }
        }
        Expr::TsAs(a) => rewrite_static_in_expr(&mut a.expr, map),
        Expr::TsTypeAssertion(a) => rewrite_static_in_expr(&mut a.expr, map),
        Expr::TsNonNull(n) => rewrite_static_in_expr(&mut n.expr, map),
        Expr::TsConstAssertion(a) => rewrite_static_in_expr(&mut a.expr, map),
        Expr::Arrow(ar) => match ar.body.as_mut() {
            swc_ecma_ast::BlockStmtOrExpr::BlockStmt(b) => {
                for s in b.stmts.iter_mut() {
                    rewrite_static_in_swc_stmt(s, map);
                }
            }
            swc_ecma_ast::BlockStmtOrExpr::Expr(e) => rewrite_static_in_expr(e, map),
        },
        Expr::Fn(f) => {
            if let Some(body) = f.function.body.as_mut() {
                for s in body.stmts.iter_mut() {
                    rewrite_static_in_swc_stmt(s, map);
                }
            }
        }
        _ => {}
    }
}
