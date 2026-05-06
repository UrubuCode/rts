fn span_snippet(cm: &Lrc<SourceMap>, span: SwcSpan) -> Option<String> {
    if span.is_dummy() {
        return None;
    }
    cm.span_to_snippet(span).ok()
}

fn convert_span(cm: &Lrc<SourceMap>, span: SwcSpan) -> Span {
    if span.is_dummy() {
        return Span::default();
    }

    let start = cm.lookup_char_pos(span.lo());
    let end = cm.lookup_char_pos(span.hi());

    Span {
        start: Position {
            line: start.line,
            column: start.col_display.saturating_add(1),
        },
        end: Position {
            line: end.line,
            column: end.col_display.saturating_add(1),
        },
        file: None,
    }
}

fn ts_syntax() -> Syntax {
    Syntax::Typescript(TsSyntax {
        tsx: false,
        decorators: true,
        ..Default::default()
    })
}

fn es_syntax() -> Syntax {
    Syntax::Es(EsSyntax {
        jsx: false,
        decorators: true,
        ..Default::default()
    })
}
