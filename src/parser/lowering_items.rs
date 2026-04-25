fn lower_program(cm: &Lrc<SourceMap>, source: &SwcProgram) -> Program {
    let mut program = Program::default();

    match source {
        SwcProgram::Module(module) => {
            for item in &module.body {
                lower_module_item(cm, item, &mut program.items);
            }
        }
        SwcProgram::Script(script) => {
            for stmt in &script.body {
                lower_stmt(cm, stmt, &mut program.items);
            }
        }
    }

    program
}

fn lower_module_item(cm: &Lrc<SourceMap>, item: &ModuleItem, out: &mut Vec<Item>) {
    match item {
        ModuleItem::ModuleDecl(decl) => lower_module_decl(cm, decl, out),
        ModuleItem::Stmt(stmt) => lower_stmt(cm, stmt, out),
    }
}

fn lower_module_decl(cm: &Lrc<SourceMap>, decl: &ModuleDecl, out: &mut Vec<Item>) {
    match decl {
        ModuleDecl::Import(import_decl) => {
            out.push(Item::Import(lower_import_decl(cm, import_decl)));
        }
        ModuleDecl::ExportDecl(export_decl) => {
            lower_decl(cm, &export_decl.decl, out);
        }
        ModuleDecl::ExportDefaultDecl(default_decl) => match &default_decl.decl {
            DefaultDecl::Class(class_expr) => {
                if let Some(name) = class_expr.ident.as_ref().map(|ident| ident.sym.to_string()) {
                    out.push(Item::Class(lower_class(
                        cm,
                        &name,
                        &class_expr.class,
                        class_expr.span(),
                    )));
                } else {
                    push_raw_statement(cm, decl.span(), out);
                }
            }
            DefaultDecl::Fn(fn_expr) => {
                if let Some(name) = fn_expr.ident.as_ref().map(|ident| ident.sym.to_string()) {
                    out.push(Item::Function(lower_function(
                        cm,
                        &name,
                        &fn_expr.function,
                        fn_expr.function.span,
                    )));
                } else {
                    push_raw_statement(cm, decl.span(), out);
                }
            }
            DefaultDecl::TsInterfaceDecl(interface_decl) => {
                out.push(Item::Interface(lower_interface_decl(cm, interface_decl)));
            }
        },
        _ => push_raw_statement(cm, decl.span(), out),
    }
}

fn lower_stmt(cm: &Lrc<SourceMap>, stmt: &Stmt, out: &mut Vec<Item>) {
    match stmt {
        Stmt::Decl(decl) => lower_decl(cm, decl, out),
        _ => push_raw_statement_with_stmt(cm, stmt.span(), Some(stmt), out),
    }
}

fn lower_decl(cm: &Lrc<SourceMap>, decl: &Decl, out: &mut Vec<Item>) {
    match decl {
        Decl::Class(class_decl) => {
            out.push(Item::Class(lower_class_decl(cm, class_decl)));
            // Decorators TC39: emite chamada a cada decorator com target=0
            // (handle nominal). Resultado eh descartado (registration-style
            // decorators tem efeito por side-effect). Decoradores de
            // metodo/param sao parseados mas tambem ignorados ate ter
            // metadata real.
            // Decorators TS executam bottom-up (do mais perto da classe
            // para o mais distante).
            for dec in class_decl.class.decorators.iter().rev() {
                emit_decorator_call_stmt(cm, &dec.expr, dec.span, out);
            }
        }
        Decl::Fn(fn_decl) => {
            out.push(Item::Function(lower_fn_decl(cm, fn_decl)));
        }
        Decl::TsInterface(interface_decl) => {
            out.push(Item::Interface(lower_interface_decl(cm, interface_decl)));
        }
        Decl::TsEnum(enum_decl) => {
            // Desugar `enum E { A, B = 5, C }` em
            // `const E = { A: 0, B: 5, C: 6 };` — objeto literal que o
            // codegen já trata via path normal de member access.
            //
            // Numeric enums: auto-incremento começando em 0; init explícito
            // (numérico) reseta o contador.
            // String enums: init obrigatório, valor literal.
            // Mistos seguem a regra do membro vigente.
            if let Some(stmt) = lower_ts_enum_to_const(enum_decl) {
                push_raw_statement_with_stmt(cm, enum_decl.span, Some(&stmt), out);
            }
        }
        Decl::TsModule(module_decl) => {
            // \`namespace Foo { export function f() {} ... }\`
            // Desugar:
            //   - Cada \`export function bar(...)\` vira \`function __ns_Foo_bar(...)\`
            //     no top-level (mangled).
            //   - Cada \`export class C {}\` vira \`class __ns_Foo_C {}\`.
            //   - Cada \`export const x = ...\` vira \`const __ns_Foo_x = ...\`.
            //   - Por fim, gera \`const Foo = { bar: __ns_Foo_bar, ... }\` pra
            //     habilitar \`Foo.bar()\` via member access + call_indirect.
            lower_ts_namespace(cm, module_decl, out);
        }
        Decl::Var(var_decl) if try_lower_fn_expr_decl(cm, var_decl, out) => {
            // All declarators were function/arrow expressions and have been
            // emitted as Item::Function above.
        }
        _ => {
            // Preserve non-function/class declarations (e.g. let/const) as a
            // real SWC statement so codegen can lower module-scope globals.
            let stmt = Stmt::Decl(decl.clone());
            push_raw_statement_with_stmt(cm, decl.span(), Some(&stmt), out);
        }
    }
}

/// Rewrites `const NAME = function(...) { ... }` (or arrow with block body)
/// into a synthetic `Item::Function` so callers can invoke it like a regular
/// named function. Returns true only if *every* declarator was a supported
/// function expression; otherwise the caller falls back to the statement path.
fn try_lower_fn_expr_decl(cm: &Lrc<SourceMap>, var_decl: &VarDecl, out: &mut Vec<Item>) -> bool {
    let mut pending = Vec::new();
    for decl in &var_decl.decls {
        let Pat::Ident(binding) = &decl.name else {
            return false;
        };
        let Some(init) = &decl.init else {
            return false;
        };
        let name = binding.id.sym.to_string();

        match init.as_ref() {
            Expr::Fn(fn_expr) => {
                let span = fn_expr.function.span;
                pending.push(lower_function(cm, &name, &fn_expr.function, span));
            }
            Expr::Arrow(arrow) => {
                let synthetic = arrow_to_function(arrow);
                pending.push(lower_function(cm, &name, &synthetic, arrow.span));
            }
            _ => return false,
        }
    }
    for fn_decl in pending {
        out.push(Item::Function(fn_decl));
    }
    true
}

/// Builds a `swc_ecma_ast::Function` from an `ArrowExpr` so it can flow
/// through the same lowering path as regular function declarations.
///
/// For expression-bodied arrows (`(x) => x * 2`) the single expression is
/// wrapped in a synthetic `{ return <expr>; }` so downstream codegen only
/// needs to know how to handle block-bodied functions.
fn arrow_to_function(arrow: &ArrowExpr) -> SwcFunction {
    let body = match &*arrow.body {
        swc_ecma_ast::BlockStmtOrExpr::BlockStmt(block) => Some(block.clone()),
        swc_ecma_ast::BlockStmtOrExpr::Expr(expr) => {
            let return_stmt = Stmt::Return(swc_ecma_ast::ReturnStmt {
                span: arrow.span,
                arg: Some(expr.clone()),
            });
            Some(BlockStmt {
                span: arrow.span,
                ctxt: arrow.ctxt,
                stmts: vec![return_stmt],
            })
        }
    };
    let params = arrow
        .params
        .iter()
        .map(|pat| swc_ecma_ast::Param {
            span: pat.span(),
            decorators: Vec::new(),
            pat: pat.clone(),
        })
        .collect();
    SwcFunction {
        params,
        decorators: Vec::new(),
        span: arrow.span,
        ctxt: arrow.ctxt,
        body,
        is_generator: false,
        is_async: arrow.is_async,
        type_params: arrow.type_params.clone(),
        return_type: arrow.return_type.clone(),
    }
}

fn lower_import_decl(cm: &Lrc<SourceMap>, import_decl: &SwcImportDecl) -> ImportDecl {
    let mut names = Vec::new();
    let mut default_name = None;

    for specifier in &import_decl.specifiers {
        match specifier {
            ImportSpecifier::Named(named) => {
                let name = if let Some(imported) = &named.imported {
                    module_export_name(imported)
                } else {
                    named.local.sym.to_string()
                };
                names.push(name);
            }
            ImportSpecifier::Default(def) => {
                default_name = Some(def.local.sym.to_string());
            }
            ImportSpecifier::Namespace(_) => {}
        }
    }

    ImportDecl {
        names,
        default_name,
        from: import_decl.src.value.to_string_lossy().to_string(),
        span: convert_span(cm, import_decl.span),
    }
}

fn lower_interface_decl(cm: &Lrc<SourceMap>, interface_decl: &SwcTsInterfaceDecl) -> InterfaceDecl {
    let mut fields = Vec::new();

    for member in &interface_decl.body.body {
        if let TsTypeElement::TsPropertySignature(property) = member {
            if let Some(name) = property_key_name(&property.key, cm) {
                let field = FieldDecl {
                    name,
                    type_annotation: property
                        .type_ann
                        .as_ref()
                        .map(|annotation| normalize_type_annotation(cm, annotation))
                        .unwrap_or_else(|| "any".to_string()),
                    span: convert_span(cm, property.span),
                };
                fields.push(field);
            }
        }
    }

    InterfaceDecl {
        name: interface_decl.id.sym.to_string(),
        fields,
        span: convert_span(cm, interface_decl.span),
    }
}

/// Desugar `enum E { A, B = 5 }` em `const E = { A: 0, B: 5 };`.
fn lower_ts_enum_to_const(enum_decl: &swc_ecma_ast::TsEnumDecl) -> Option<Stmt> {
    use swc_ecma_ast::*;

    let enum_name = enum_decl.id.sym.to_string();
    let mut props: Vec<PropOrSpread> = Vec::with_capacity(enum_decl.members.len());
    // Auto-counter pra membros numéricos sem init.
    let mut next_numeric: i64 = 0;

    for member in &enum_decl.members {
        let key_str = match &member.id {
            TsEnumMemberId::Ident(id) => id.sym.to_string(),
            TsEnumMemberId::Str(s) => s.value.to_string_lossy().to_string(),
        };

        // Determina o valor: usa init se presente, senão auto-incremento.
        let value_expr: Expr = if let Some(init) = &member.init {
            // Quando init é Lit::Num, atualiza o counter.
            if let Expr::Lit(Lit::Num(n)) = init.as_ref() {
                next_numeric = n.value as i64 + 1;
            }
            (**init).clone()
        } else {
            let val = next_numeric;
            next_numeric += 1;
            Expr::Lit(Lit::Num(Number {
                span: Default::default(),
                value: val as f64,
                raw: Some(format!("{val}").into()),
            }))
        };

        let prop = PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
            key: PropName::Ident(IdentName {
                span: Default::default(),
                sym: key_str.into(),
            }),
            value: Box::new(value_expr),
        })));
        props.push(prop);
    }

    let obj_lit = Expr::Object(ObjectLit {
        span: Default::default(),
        props,
    });

    let var_decl = VarDecl {
        span: Default::default(),
        ctxt: Default::default(),
        kind: VarDeclKind::Const,
        declare: false,
        decls: vec![VarDeclarator {
            span: Default::default(),
            name: Pat::Ident(BindingIdent {
                id: Ident {
                    span: Default::default(),
                    ctxt: Default::default(),
                    sym: enum_name.into(),
                    optional: false,
                },
                type_ann: None,
            }),
            init: Some(Box::new(obj_lit)),
            definite: false,
        }],
    };
    Some(Stmt::Decl(Decl::Var(Box::new(var_decl))))
}

/// Desugar \`namespace Foo { export function f() {} }\`:
/// 1. Members exportados viram top-level com nome mangled \`__ns_<NS>_<member>\`.
/// 2. Gera \`const <NS> = { member: __ns_<NS>_member, ... }\` no fim
///    pra habilitar \`<NS>.member()\` via member access + call_indirect.
fn lower_ts_namespace(
    cm: &Lrc<SourceMap>,
    module_decl: &swc_ecma_ast::TsModuleDecl,
    out: &mut Vec<Item>,
) {
    use swc_ecma_ast::*;

    // Pega o nome do namespace (skip strings — só Ident).
    let ns_name: String = match &module_decl.id {
        TsModuleName::Ident(id) => id.sym.to_string(),
        TsModuleName::Str(_) => return, // ambient module string — skip MVP
    };

    // Body é \`TsModuleBlock\` ou \`TsNamespaceDecl\` (nested).
    let block: &TsModuleBlock = match module_decl.body.as_ref() {
        Some(TsNamespaceBody::TsModuleBlock(b)) => b,
        Some(TsNamespaceBody::TsNamespaceDecl(_)) => {
            // Nested namespace (`namespace A.B {}`) — não suportado MVP.
            return;
        }
        None => return,
    };

    // Coleta nomes dos membros pra gerar o objeto const final.
    let mut member_names: Vec<String> = Vec::new();

    for item in &block.body {
        match item {
            ModuleItem::Stmt(Stmt::Decl(decl)) => {
                process_namespace_member(cm, &ns_name, decl, &mut member_names, out);
            }
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(ed)) => {
                process_namespace_member(cm, &ns_name, &ed.decl, &mut member_names, out);
            }
            _ => {}
        }
    }

    // Gera \`const <NS> = { member: __ns_<NS>_member, ... };\`
    if !member_names.is_empty() {
        let mut props: Vec<PropOrSpread> = Vec::with_capacity(member_names.len());
        for member in &member_names {
            let mangled = format!("__ns_{ns_name}_{member}");
            let prop = PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                key: PropName::Ident(IdentName {
                    span: Default::default(),
                    sym: member.as_str().into(),
                }),
                value: Box::new(Expr::Ident(Ident {
                    span: Default::default(),
                    ctxt: Default::default(),
                    sym: mangled.into(),
                    optional: false,
                })),
            })));
            props.push(prop);
        }
        let obj_lit = Expr::Object(ObjectLit {
            span: Default::default(),
            props,
        });
        let var_decl = VarDecl {
            span: Default::default(),
            ctxt: Default::default(),
            kind: VarDeclKind::Const,
            declare: false,
            decls: vec![VarDeclarator {
                span: Default::default(),
                name: Pat::Ident(BindingIdent {
                    id: Ident {
                        span: Default::default(),
                        ctxt: Default::default(),
                        sym: ns_name.clone().into(),
                        optional: false,
                    },
                    type_ann: None,
                }),
                init: Some(Box::new(obj_lit)),
                definite: false,
            }],
        };
        let stmt = Stmt::Decl(Decl::Var(Box::new(var_decl)));
        push_raw_statement_with_stmt(cm, module_decl.span, Some(&stmt), out);
    }
}

fn process_namespace_member(
    cm: &Lrc<SourceMap>,
    ns_name: &str,
    decl: &swc_ecma_ast::Decl,
    member_names: &mut Vec<String>,
    out: &mut Vec<Item>,
) {
    use swc_ecma_ast::*;
    match decl {
        Decl::Fn(fn_decl) => {
            // Renomeia para \`__ns_<NS>_<name>\`.
            let original_name = fn_decl.ident.sym.to_string();
            let mangled = format!("__ns_{ns_name}_{original_name}");
            // Constrói uma cópia do FnDecl com o ident renomeado.
            let mut renamed = fn_decl.clone();
            renamed.ident.sym = mangled.into();
            out.push(Item::Function(lower_fn_decl(cm, &renamed)));
            member_names.push(original_name);
        }
        Decl::Class(class_decl) => {
            let original_name = class_decl.ident.sym.to_string();
            let mangled = format!("__ns_{ns_name}_{original_name}");
            let mut renamed = class_decl.clone();
            renamed.ident.sym = mangled.into();
            out.push(Item::Class(lower_class_decl(cm, &renamed)));
            // Classes não vão para o objeto namespace porque \`Foo.C\` não
            // é \`new\` direto sem suporte adicional. Documentamos como
            // limitação. Por enquanto, ainda registramos o nome para
            // que o usuário possa fazer \`Foo.C\` (mas \`new Foo.C()\` não
            // funciona — usar \`new __ns_Foo_C()\` ou alias).
            // Skip do member_names: melhor não confundir.
            let _ = original_name;
        }
        Decl::Var(var_decl) => {
            // \`export const x = ...\` ou \`let\`/\`var\`. Renomeia cada decl.
            for d in &var_decl.decls {
                if let Pat::Ident(id) = &d.name {
                    let original_name = id.id.sym.to_string();
                    let mangled = format!("__ns_{ns_name}_{original_name}");
                    let new_decl = VarDeclarator {
                        span: d.span,
                        name: Pat::Ident(BindingIdent {
                            id: Ident {
                                span: Default::default(),
                                ctxt: Default::default(),
                                sym: mangled.into(),
                                optional: false,
                            },
                            type_ann: id.type_ann.clone(),
                        }),
                        init: d.init.clone(),
                        definite: d.definite,
                    };
                    let renamed_decl = VarDecl {
                        span: var_decl.span,
                        ctxt: var_decl.ctxt,
                        kind: var_decl.kind,
                        declare: var_decl.declare,
                        decls: vec![new_decl],
                    };
                    let stmt = Stmt::Decl(Decl::Var(Box::new(renamed_decl)));
                    push_raw_statement_with_stmt(cm, var_decl.span, Some(&stmt), out);
                    member_names.push(original_name);
                }
            }
        }
        Decl::TsEnum(enum_decl) => {
            // Enum interno: gera com nome mangled e adiciona ao namespace.
            let original_name = enum_decl.id.sym.to_string();
            let mut renamed = enum_decl.clone();
            renamed.id.sym = format!("__ns_{ns_name}_{original_name}").into();
            if let Some(stmt) = lower_ts_enum_to_const(&renamed) {
                push_raw_statement_with_stmt(cm, enum_decl.span, Some(&stmt), out);
                member_names.push(original_name);
            }
        }
        _ => {}
    }
}

/// Emite a chamada do decorator como statement de side-effect:
/// `decoratorExpr(0);`. Resultado descartado (decorators TC39 com
/// retorno modificando target nao sao suportados em runtime).
fn emit_decorator_call_stmt(
    cm: &Lrc<SourceMap>,
    decorator_expr: &Expr,
    span: SwcSpan,
    out: &mut Vec<Item>,
) {
    use swc_ecma_ast::*;
    // Se o decorator ja e uma chamada (factory: @tag("x")), executa direto.
    // Caso contrario (@log), envolve com (target=0).
    let call_expr = if let Expr::Call(_) = decorator_expr {
        decorator_expr.clone()
    } else {
        Expr::Call(CallExpr {
            span,
            ctxt: Default::default(),
            callee: Callee::Expr(Box::new(decorator_expr.clone())),
            args: vec![ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Lit(Lit::Num(Number {
                    span,
                    value: 0.0,
                    raw: Some("0".into()),
                }))),
            }],
            type_args: None,
        })
    };
    let stmt = Stmt::Expr(ExprStmt {
        span,
        expr: Box::new(call_expr),
    });
    push_raw_statement_with_stmt(cm, span, Some(&stmt), out);
}

fn lower_class_decl(cm: &Lrc<SourceMap>, class_decl: &SwcClassDecl) -> ClassDecl {
    lower_class(
        cm,
        &class_decl.ident.sym.to_string(),
        &class_decl.class,
        class_decl.class.span,
    )
}

