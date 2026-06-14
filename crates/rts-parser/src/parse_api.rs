pub fn parse_source(source: &str) -> Result<Program> {
    parse_source_with_mode(source, FrontendMode::Native)
}

/// Parse sem arquivo conhecido. Registra um fonte anonimo no `SourceStore`
/// para que diagnosticos ainda consigam renderizar snippets.
pub fn parse_source_with_mode(source: &str, mode: FrontendMode) -> Result<Program> {
    let stripped = strip_shebang(source);
    let file = source_store::register_anonymous("eval", stripped);
    parse_source_with_file(stripped, mode, file)
}

/// Remove shebang (\`#!...\`) na primeira linha. (#285) Permite scripts
/// executaveis com \`#!/usr/bin/env rts\` no topo. Mantem o \\n pra
/// preservar line numbers nos diagnosticos.
pub(crate) fn strip_shebang(source: &str) -> &str {
    if let Some(rest) = source.strip_prefix("#!") {
        if let Some(nl) = rest.find('\n') {
            return &source[nl + 2..];
        }
        return "";
    }
    source
}

/// Parse com `FileId` conhecido — todos os spans do AST resultante
/// carregam esse file id.
pub fn parse_source_with_file(
    source: &str,
    mode: FrontendMode,
    file: FileId,
) -> Result<Program> {
    let [primary, fallback] = match mode {
        FrontendMode::Native => [ts_syntax(), es_syntax()],
        FrontendMode::Compat => [es_syntax(), ts_syntax()],
    };

    // Try primary syntax silently first. If it succeeds → done.
    // If it fails, try fallback silently. Only emit rich diagnostics when
    // both fail, using the primary (TS) errors which are more relevant.
    if let Ok((parsed, cm)) = try_parse_syntax(source, primary) {
        let mut p = lower_program(&cm, &parsed);
        assign_file_to_program(&mut p, file);
        return Ok(p);
    }

    if let Ok((parsed, cm)) = try_parse_syntax(source, fallback) {
        let mut p = lower_program(&cm, &parsed);
        assign_file_to_program(&mut p, file);
        return Ok(p);
    }

    // Both failed — emit primary (TS) errors with rich snippets.
    parse_with_rich_errors(source, primary, file)
}

/// Parse `source` to a raw swc `Module`, WITHOUT lowering to the internal AST and
/// without emitting diagnostics. Returns `None` on a parse error or a script-mode
/// program. Additive helper (P5.11): the new engine's destructuring desugar needs
/// the swc binding `Pat` of function parameters, which the lowered `rts-ast` does
/// not carry. TS syntax first, ES fallback (matching `parse_source`'s default).
pub fn parse_swc_module(source: &str) -> Option<swc_ecma_ast::Module> {
    let stripped = strip_shebang(source);
    for syntax in [ts_syntax(), es_syntax()] {
        if let Ok((parsed, _cm)) = try_parse_syntax(stripped, syntax) {
            if let swc_ecma_ast::Program::Module(m) = parsed {
                return Some(m);
            }
        }
    }
    None
}

/// Parse sem emitir diagnósticos — retorna `(SwcProgram, SourceMap)` para reuso.
fn try_parse_syntax(
    source: &str,
    syntax: Syntax,
) -> Result<(swc_ecma_ast::Program, Lrc<SourceMap>)> {
    let cm: Lrc<SourceMap> = Default::default();
    let fm = cm.new_source_file(
        Lrc::new(FileName::Custom("rts-input.ts".into())),
        source.to_string(),
    );
    let lexer = Lexer::new(syntax, Default::default(), StringInput::from(&*fm), None);
    let mut parser = Parser::new_from(lexer);
    let parsed = parser.parse_program().map_err(|e| anyhow!("{}", e.kind().msg()))?;
    if let Some(err) = parser.take_errors().into_iter().next() {
        return Err(anyhow!("{}", err.kind().msg()));
    }
    Ok((parsed, cm))
}

/// Parse emitindo diagnósticos ricos para cada erro de sintaxe.
fn parse_with_rich_errors(source: &str, syntax: Syntax, file: FileId) -> Result<Program> {
    let cm: Lrc<SourceMap> = Default::default();
    let fm = cm.new_source_file(
        Lrc::new(FileName::Custom("rts-input.ts".into())),
        source.to_string(),
    );
    let lexer = Lexer::new(syntax, Default::default(), StringInput::from(&*fm), None);
    let mut parser = Parser::new_from(lexer);

    let result = parser.parse_program();
    let extra_errors = parser.take_errors();

    match result {
        Ok(parsed) => {
            for error in extra_errors {
                make_parse_error(&cm, &error, file);
            }
            let mut program = lower_program(&cm, &parsed);
            assign_file_to_program(&mut program, file);
            Ok(program)
        }
        Err(error) => {
            make_parse_error(&cm, &error, file);
            for error in extra_errors {
                make_parse_error(&cm, &error, file);
            }
            Err(anyhow!("syntax error"))
        }
    }
}


fn make_parse_error(
    cm: &Lrc<SourceMap>,
    error: &swc_ecma_parser::error::Error,
    file: FileId,
) -> anyhow::Error {
    let message = error.kind().msg().into_owned();
    let swc_span = error.span();

    if swc_span.is_dummy() {
        let diag = rts_diagnostics::reporter::RichDiagnostic::error("E0001", &message);
        rts_diagnostics::reporter::emit(diag);
        return anyhow!("{}", message);
    }

    let lo = cm.lookup_char_pos(swc_span.lo());
    let hi = cm.lookup_char_pos(swc_span.hi());

    let span = Span {
        start: Position { line: lo.line, column: lo.col_display + 1 },
        end: Position { line: hi.line, column: hi.col_display + 1 },
        file: Some(file),
    };

    let diag = rts_diagnostics::reporter::RichDiagnostic::error("E0001", &message)
        .with_span(span);
    rts_diagnostics::reporter::emit(diag);

    anyhow!("syntax error")
}

/// Percorre o AST do parser e preenche `Span.file` com o `FileId` informado
/// em todos os nodes. Chamado apos o lowering SWC -> AST interno.
fn assign_file_to_program(program: &mut Program, file: FileId) {
    for item in &mut program.items {
        match item {
            Item::Import(decl) => {
                decl.span.file = Some(file);
            }
            Item::ExportNamespace(decl) => {
                decl.span.file = Some(file);
            }
            Item::Interface(decl) => {
                decl.span.file = Some(file);
                for field in &mut decl.fields {
                    field.span.file = Some(file);
                }
            }
            Item::Class(decl) => {
                decl.span.file = Some(file);
                for member in &mut decl.members {
                    match member {
                        ClassMember::Constructor(ctor) => {
                            ctor.span.file = Some(file);
                            for param in &mut ctor.parameters {
                                param.span.file = Some(file);
                            }
                            for stmt in &mut ctor.body {
                                assign_file_to_statement(stmt, file);
                            }
                        }
                        ClassMember::Method(method) => {
                            method.span.file = Some(file);
                            for param in &mut method.parameters {
                                param.span.file = Some(file);
                            }
                            for stmt in &mut method.body {
                                assign_file_to_statement(stmt, file);
                            }
                        }
                        ClassMember::Property(prop) => {
                            prop.span.file = Some(file);
                        }
                    }
                }
            }
            Item::Function(decl) => {
                decl.span.file = Some(file);
                for param in &mut decl.parameters {
                    param.span.file = Some(file);
                }
                for stmt in &mut decl.body {
                    assign_file_to_statement(stmt, file);
                }
            }
            Item::Statement(stmt) => assign_file_to_statement(stmt, file),
        }
    }
}

fn assign_file_to_statement(stmt: &mut Statement, file: FileId) {
    match stmt {
        Statement::Raw(raw) => {
            raw.span.file = Some(file);
        }
    }
}


