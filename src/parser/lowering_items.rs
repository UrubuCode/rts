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
        }
        Decl::Fn(fn_decl) => {
            out.push(Item::Function(lower_fn_decl(cm, fn_decl)));
        }
        Decl::TsInterface(interface_decl) => {
            out.push(Item::Interface(lower_interface_decl(cm, interface_decl)));
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
            Expr::Arrow(arrow) if matches!(&*arrow.body, swc_ecma_ast::BlockStmtOrExpr::BlockStmt(_)) => {
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
fn arrow_to_function(arrow: &ArrowExpr) -> SwcFunction {
    let body = match &*arrow.body {
        swc_ecma_ast::BlockStmtOrExpr::BlockStmt(block) => Some(block.clone()),
        _ => None,
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

fn lower_class_decl(cm: &Lrc<SourceMap>, class_decl: &SwcClassDecl) -> ClassDecl {
    lower_class(
        cm,
        &class_decl.ident.sym.to_string(),
        &class_decl.class,
        class_decl.class.span,
    )
}

