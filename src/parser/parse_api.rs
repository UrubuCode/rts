pub fn parse_source(source: &str) -> Result<Program> {
    parse_source_with_mode(source, FrontendMode::Native)
}

/// Parse sem arquivo conhecido. Registra um fonte anonimo no `SourceStore`
/// para que diagnosticos ainda consigam renderizar snippets.
pub fn parse_source_with_mode(source: &str, mode: FrontendMode) -> Result<Program> {
    let file = source_store::register_anonymous("eval", source);
    parse_source_with_file(source, mode, file)
}

/// Parse com `FileId` conhecido — todos os spans do AST resultante
/// carregam esse file id.
pub fn parse_source_with_file(
    source: &str,
    mode: FrontendMode,
    file: FileId,
) -> Result<Program> {
    let syntax_order = match mode {
        FrontendMode::Native => [ts_syntax(), es_syntax()],
        FrontendMode::Compat => [es_syntax(), ts_syntax()],
    };

    let mut first_error = None::<String>;

    for syntax in syntax_order {
        match parse_with_syntax(source, syntax, file) {
            Ok(program) => return Ok(program),
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error.to_string());
                }
            }
        }
    }

    Err(anyhow!(
        "failed to parse source in {} mode: {}",
        mode,
        first_error.unwrap_or_else(|| "unknown parser error".to_string())
    ))
}

fn parse_with_syntax(source: &str, syntax: Syntax, file: FileId) -> Result<Program> {
    let cm: Lrc<SourceMap> = Default::default();
    let fm = cm.new_source_file(
        Lrc::new(FileName::Custom("rts-input.ts".into())),
        source.to_string(),
    );

    let lexer = Lexer::new(syntax, Default::default(), StringInput::from(&*fm), None);
    let mut parser = Parser::new_from(lexer);

    let parsed = parser
        .parse_program()
        .map_err(|error| anyhow!(format_parser_error(&cm, &error)))?;

    if let Some(error) = parser.take_errors().into_iter().next() {
        return Err(anyhow!(format_parser_error(&cm, &error)));
    }

    let mut program = lower_program(&cm, &parsed);
    assign_file_to_program(&mut program, file);
    Ok(program)
}

/// Percorre o AST do parser e preenche `Span.file` com o `FileId` informado
/// em todos os nodes. Chamado apos o lowering SWC -> AST interno.
fn assign_file_to_program(program: &mut Program, file: FileId) {
    for item in &mut program.items {
        match item {
            Item::Import(decl) => {
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

fn format_parser_error(cm: &Lrc<SourceMap>, error: &swc_ecma_parser::error::Error) -> String {
    let message = error.kind().msg();
    let span = error.span();

    if span.is_dummy() {
        return message.into_owned();
    }

    let loc = cm.lookup_char_pos(span.lo());
    format!(
        "{} at {}:{}",
        message,
        loc.line,
        loc.col_display.saturating_add(1)
    )
}

