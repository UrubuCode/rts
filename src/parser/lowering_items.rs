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
        _ => push_raw_statement(cm, decl.span(), out),
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

