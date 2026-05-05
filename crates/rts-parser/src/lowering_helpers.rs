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
        PropName::Computed(computed) => {
            // Resolução em compile-time:
            //   ["foo"]      → "foo"      (literal string)
            //   [42]         → "42"       (literal numérico)
            //   [`tpl`]      → conteúdo do template sem interpolação
            //   [a + b]      → fallback: snippet do código (mantém comportamento
            //                  anterior pra não quebrar casos não-críticos).
            //                  Computed dinâmico real (com expressões não-const)
            //                  fica fora do MVP — os snippets não são únicos
            //                  e podem colidir. Documenta-se em #153.
            if let Some(s) = computed_to_static_str(computed.expr.as_ref()) {
                return s;
            }
            span_snippet(cm, computed.expr.span()).unwrap_or_default()
        }
    }
}

/// Resolve a expressão de um nome computed (`[expr]`) para uma string
/// estática, quando possível. Cobre literais string/número e templates
/// sem interpolação. Outros casos retornam None — caller decide o
/// fallback (no parser, snippet textual; no codegen, erro).
fn computed_to_static_str(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Lit(Lit::Str(s)) => Some(s.value.to_string_lossy().to_string()),
        Expr::Lit(Lit::Num(n)) => Some(n.value.to_string()),
        Expr::Lit(Lit::BigInt(n)) => Some(n.value.to_string()),
        Expr::Tpl(tpl) if tpl.exprs.is_empty() => {
            // Template literal sem interpolação: junta os quasis.
            let mut out = String::new();
            for q in &tpl.quasis {
                out.push_str(q.raw.as_ref());
            }
            Some(out)
        }
        Expr::Paren(p) => computed_to_static_str(&p.expr),
        _ => None,
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

/// True quando o pattern faz destructuring (object/array), incluindo
/// quando vem embrulhado em `Pat::Assign` (parametro com default).
fn is_destructure_pat(pat: &Pat) -> bool {
    match pat {
        Pat::Object(_) | Pat::Array(_) => true,
        Pat::Assign(assign) => is_destructure_pat(&assign.left),
        _ => false,
    }
}

/// Nome sintetico do parametro destructured no slot `index`.
fn synth_destructure_param_name(index: usize) -> String {
    format!("__rts_param_destruct_{}", index)
}

/// Parseia um statement sintetico (`const {a,b} = src;`) usando o
/// parser SWC TS. Retorna `None` se o snippet falhar ao parsear — caller
/// loga e segue (o codegen exibira erro de var indefinida no body).
fn parse_synthetic_stmt(text: &str) -> Option<Stmt> {
    let cm: Lrc<SourceMap> = Default::default();
    let fm = cm.new_source_file(
        Lrc::new(FileName::Custom("rts-synth.ts".into())),
        text.to_string(),
    );
    let lexer = Lexer::new(
        Syntax::Typescript(TsSyntax::default()),
        Default::default(),
        StringInput::from(&*fm),
        None,
    );
    let mut parser = Parser::new_from(lexer);
    let program = parser.parse_program().ok()?;
    match program {
        SwcProgram::Module(m) => m.body.into_iter().find_map(|item| match item {
            ModuleItem::Stmt(stmt) => Some(stmt),
            _ => None,
        }),
        SwcProgram::Script(s) => s.body.into_iter().next(),
    }
}

/// Para uma lista de parametros SWC, retorna a lista lowered de
/// `Parameter` (com nomes sinteticos quando o pattern usa destructuring)
/// e a lista de statements de prologo a serem prepended ao body da fn.
///
/// `let { a, b } = __rts_param_destruct_0;` — reusa o pipeline de
/// destructuring de `let/const` que ja existe em `expand_destruct_decl`
/// (codegen).
fn lower_params_with_destructure(
    cm: &Lrc<SourceMap>,
    params: &[SwcParam],
) -> (Vec<Parameter>, Vec<Statement>) {
    let mut lowered = Vec::with_capacity(params.len());
    let mut prologue: Vec<Statement> = Vec::new();
    for (i, param) in params.iter().enumerate() {
        let needs_destructure = is_destructure_pat(&param.pat);
        if needs_destructure {
            // Pattern original (pode estar dentro de Pat::Assign quando ha default).
            let (pat_for_text, default_text) = match &param.pat {
                Pat::Assign(assign) => (
                    &*assign.left,
                    span_snippet(cm, assign.right.span()),
                ),
                _ => (&param.pat, None),
            };
            let pat_text = match span_snippet(cm, pat_for_text.span()) {
                Some(t) => t,
                None => continue,
            };
            let synth_name = synth_destructure_param_name(i);
            // Prologo: const <pat> = <synth>; — ?? <default> quando aplicavel.
            let init_expr = match &default_text {
                Some(d) => format!("({} ?? {})", synth_name, d),
                None => synth_name.clone(),
            };
            let snippet = format!("const {} = {};", pat_text.trim(), init_expr);
            if let Some(stmt) = parse_synthetic_stmt(&snippet) {
                let span = convert_span(cm, param.span);
                let mut raw = RawStmt::new(snippet, span);
                raw = raw.with_stmt(stmt);
                prologue.push(Statement::Raw(raw));
            }
            // Parametro real entra como ident sintetico.
            let type_annotation = pat_type_annotation(cm, pat_for_text);
            lowered.push(Parameter {
                name: synth_name,
                type_annotation,
                modifiers: MemberModifiers::default(),
                variadic: false,
                default: None,
                span: convert_span(cm, param.span),
            });
        } else if let Some(p) = lower_param(cm, param, MemberModifiers::default()) {
            lowered.push(p);
        }
    }
    (lowered, prologue)
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

