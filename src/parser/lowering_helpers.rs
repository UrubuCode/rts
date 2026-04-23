fn pat_name(pat: &Pat, cm: &Lrc<SourceMap>) -> Option<String> {
    match pat {
        Pat::Ident(ident) => Some(ident.id.sym.to_string()),
        Pat::Assign(assign) => pat_name(&assign.left, cm),
        Pat::Rest(rest) => pat_name(&rest.arg, cm),
        Pat::Expr(expr) => match &**expr {
            Expr::Ident(ident) => Some(ident.sym.to_string()),
            _ => span_snippet(cm, expr.span()),
        },
        _ => span_snippet(cm, pat.span()),
    }
}

fn pat_type_annotation(cm: &Lrc<SourceMap>, pat: &Pat) -> Option<String> {
    match pat {
        Pat::Ident(ident) => ident
            .type_ann
            .as_ref()
            .map(|annotation| normalize_type_annotation(cm, annotation)),
        Pat::Array(array) => array
            .type_ann
            .as_ref()
            .map(|annotation| normalize_type_annotation(cm, annotation)),
        Pat::Object(object) => object
            .type_ann
            .as_ref()
            .map(|annotation| normalize_type_annotation(cm, annotation)),
        Pat::Rest(rest) => rest
            .type_ann
            .as_ref()
            .map(|annotation| normalize_type_annotation(cm, annotation)),
        Pat::Assign(assign) => pat_type_annotation(cm, &assign.left),
        _ => None,
    }
}

fn normalize_type_annotation(cm: &Lrc<SourceMap>, annotation: &TsTypeAnn) -> String {
    let snippet = span_snippet(cm, annotation.span())
        .unwrap_or_else(|| "any".to_string())
        .trim()
        .to_string();

    let stripped = snippet
        .strip_prefix(':')
        .map(str::trim)
        .unwrap_or(&snippet)
        .to_string();

    if stripped.is_empty() {
        "any".to_string()
    } else {
        stripped
    }
}

fn property_key_name(key: &Expr, cm: &Lrc<SourceMap>) -> Option<String> {
    match key {
        Expr::Ident(ident) => Some(ident.sym.to_string()),
        Expr::Lit(Lit::Str(text)) => Some(text.value.to_string_lossy().to_string()),
        Expr::Lit(Lit::Num(number)) => Some(number.value.to_string()),
        Expr::Lit(Lit::BigInt(number)) => Some(number.value.to_string()),
        _ => span_snippet(cm, key.span()),
    }
}

fn prop_name_to_string(name: &PropName, cm: &Lrc<SourceMap>) -> String {
    match name {
        PropName::Ident(ident) => ident.sym.to_string(),
        PropName::Str(text) => text.value.to_string_lossy().to_string(),
        PropName::Num(number) => number.value.to_string(),
        PropName::BigInt(number) => number.value.to_string(),
        PropName::Computed(computed) => span_snippet(cm, computed.expr.span()).unwrap_or_default(),
    }
}

fn key_to_string(key: &swc_ecma_ast::Key, cm: &Lrc<SourceMap>) -> String {
    match key {
        swc_ecma_ast::Key::Private(name) => format!("#{}", name.name),
        swc_ecma_ast::Key::Public(name) => prop_name_to_string(name, cm),
    }
}

fn module_export_name(name: &ModuleExportName) -> String {
    match name {
        ModuleExportName::Ident(ident) => ident.sym.to_string(),
        ModuleExportName::Str(value) => value.value.to_string_lossy().to_string(),
    }
}

fn map_accessibility(accessibility: Option<Accessibility>) -> Option<Visibility> {
    match accessibility {
        Some(Accessibility::Public) => Some(Visibility::Public),
        Some(Accessibility::Protected) => Some(Visibility::Protected),
        Some(Accessibility::Private) => Some(Visibility::Private),
        None => None,
    }
}

fn push_raw_statement(cm: &Lrc<SourceMap>, span: SwcSpan, out: &mut Vec<Item>) {
    push_raw_statement_with_stmt(cm, span, None, out);
}

fn push_raw_statement_with_stmt(
    cm: &Lrc<SourceMap>,
    span: SwcSpan,
    stmt: Option<&Stmt>,
    out: &mut Vec<Item>,
) {
    if let Some(statement) = raw_statement(cm, span, stmt) {
        out.push(Item::Statement(statement));
    }
}

/// Constroi um `Statement::Raw` a partir do texto do snippet SWC,
/// opcionalmente carregando o `Stmt` parseado para consumo direto
/// pelo MIR. Quando `stmt` e `None`, o MIR caira em `RuntimeEval`.
fn raw_statement(
    cm: &Lrc<SourceMap>,
    span: SwcSpan,
    stmt: Option<&Stmt>,
) -> Option<Statement> {
    let snippet = span_snippet(cm, span)?;
    let text = snippet.trim();
    if text.is_empty() {
        return None;
    }

    let mut raw = RawStmt::new(text.to_string(), convert_span(cm, span));
    if let Some(stmt) = stmt {
        raw = raw.with_stmt(stmt.clone());
    }
    Some(Statement::Raw(raw))
}

fn lower_block_body(cm: &Lrc<SourceMap>, body: Option<&BlockStmt>) -> Vec<Statement> {
    body.map(|block| {
        block
            .stmts
            .iter()
            .filter_map(|stmt| raw_statement(cm, stmt.span(), Some(stmt)))
            .collect::<Vec<_>>()
    })
    .unwrap_or_default()
}

