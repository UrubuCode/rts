//! Diagnosticos estruturados com codigo, severidade, span de fonte e snippet.
//!
//! O `DiagnosticEngine` coleta `RichDiagnostic`s sem abortar no primeiro erro;
//! o CLI decide quando renderizar e abortar (via `has_errors`).

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::diagnostics::source_store::{self, FileId};
use crate::parser::span::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

impl Severity {
    fn label(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
        }
    }

    fn ansi_color(self) -> &'static str {
        match self {
            Severity::Error => "\x1b[1;31m",   // bold red
            Severity::Warning => "\x1b[1;33m", // bold yellow
            Severity::Note => "\x1b[1;36m",    // bold cyan
        }
    }
}

/// Diagnostico rico com localizacao de fonte e rendering estilo rustc.
#[derive(Debug, Clone)]
pub struct RichDiagnostic {
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    pub primary_span: Option<Span>,
    pub notes: Vec<String>,
    pub suggestion: Option<String>,
}

impl RichDiagnostic {
    pub fn error(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            severity: Severity::Error,
            message: message.into(),
            primary_span: None,
            notes: Vec::new(),
            suggestion: None,
        }
    }

    pub fn warning(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            severity: Severity::Warning,
            message: message.into(),
            primary_span: None,
            notes: Vec::new(),
            suggestion: None,
        }
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.primary_span = Some(span);
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }
}

/// Engine que acumula diagnosticos durante a compilacao e os renderiza no final.
///
/// Compartilhado via `Arc<Mutex<>>` para permitir emissao de multiplos sitios
/// (parser, type checker, typed) sem passar `&mut` por toda parte.
#[derive(Debug, Default, Clone)]
pub struct DiagnosticEngine {
    inner: Arc<Mutex<EngineInner>>,
}

#[derive(Debug, Default)]
struct EngineInner {
    diagnostics: Vec<RichDiagnostic>,
}

impl DiagnosticEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn emit(&self, diagnostic: RichDiagnostic) {
        let mut guard = self.inner.lock().expect("diagnostic engine poisoned");
        guard.diagnostics.push(diagnostic);
    }

    pub fn errors_count(&self) -> usize {
        let guard = self.inner.lock().expect("diagnostic engine poisoned");
        guard
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count()
    }

    pub fn warnings_count(&self) -> usize {
        let guard = self.inner.lock().expect("diagnostic engine poisoned");
        guard
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .count()
    }

    pub fn has_errors(&self) -> bool {
        self.errors_count() > 0
    }

    pub fn is_empty(&self) -> bool {
        let guard = self.inner.lock().expect("diagnostic engine poisoned");
        guard.diagnostics.is_empty()
    }

    /// Renderiza todos os diagnosticos acumulados. `use_color` ativa ANSI.
    pub fn render_all(&self, use_color: bool) -> String {
        let guard = self.inner.lock().expect("diagnostic engine poisoned");
        let mut output = String::new();
        for diag in &guard.diagnostics {
            output.push_str(&render_one(diag, use_color));
            output.push('\n');
        }
        output
    }

    /// Drena o estado interno, devolvendo o vetor de diagnosticos.
    /// Util para testes e para snapshots.
    pub fn drain(&self) -> Vec<RichDiagnostic> {
        let mut guard = self.inner.lock().expect("diagnostic engine poisoned");
        std::mem::take(&mut guard.diagnostics)
    }
}

fn render_one(diag: &RichDiagnostic, use_color: bool) -> String {
    let reset = if use_color { "\x1b[0m" } else { "" };
    let bold = if use_color { "\x1b[1m" } else { "" };
    let dim = if use_color { "\x1b[2m" } else { "" };
    let color = if use_color { diag.severity.ansi_color() } else { "" };

    let mut out = String::new();

    // Header: "error[E001]: mensagem"
    let _ = write!(
        out,
        "{color}{label}[{code}]{reset}{bold}: {msg}{reset}\n",
        color = color,
        label = diag.severity.label(),
        code = diag.code,
        reset = reset,
        bold = bold,
        msg = diag.message,
    );

    // Span header: "  --> path:line:col"
    if let Some(span) = diag.primary_span {
        if let Some(file_id) = span.file {
            if let Some(path) = source_store::path_of(file_id) {
                let _ = write!(
                    out,
                    "{dim}  --> {reset}{path}:{line}:{col}\n",
                    dim = dim,
                    reset = reset,
                    path = display_path(&path),
                    line = span.start.line,
                    col = span.start.column,
                );

                // Snippet: "  N | <linha>"
                render_snippet(&mut out, file_id, span, use_color);
            } else {
                let _ = write!(
                    out,
                    "{dim}  --> {reset}<unknown>:{line}:{col}\n",
                    dim = dim,
                    reset = reset,
                    line = span.start.line,
                    col = span.start.column,
                );
            }
        } else {
            let _ = write!(
                out,
                "{dim}  --> {reset}<no file>:{line}:{col}\n",
                dim = dim,
                reset = reset,
                line = span.start.line,
                col = span.start.column,
            );
        }
    }

    // Notes: "  = nota"
    for note in &diag.notes {
        let _ = write!(
            out,
            "{dim}  = {reset}{bold}note{reset}: {note}\n",
            dim = dim,
            reset = reset,
            bold = bold,
            note = note,
        );
    }

    // Suggestion: "  = sugestao: ..."
    if let Some(suggestion) = &diag.suggestion {
        let _ = write!(
            out,
            "{dim}  = {reset}{bold}sugestao{reset}: {suggestion}\n",
            dim = dim,
            reset = reset,
            bold = bold,
            suggestion = suggestion,
        );
    }

    out
}

fn render_snippet(out: &mut String, file_id: FileId, span: Span, use_color: bool) {
    let dim = if use_color { "\x1b[2m" } else { "" };
    let reset = if use_color { "\x1b[0m" } else { "" };
    let red = if use_color { "\x1b[1;31m" } else { "" };

    let line_num = span.start.line;
    let Some(text) = source_store::line_text(file_id, line_num) else {
        return;
    };

    let num_width = line_num.to_string().len().max(3);
    let padding = " ".repeat(num_width);

    // Linha em branco para respiro:  "   |"
    let _ = write!(out, "{dim}{padding} | {reset}\n", dim = dim, padding = padding, reset = reset);

    // Linha com numero:  " 12 | texto"
    let _ = write!(
        out,
        "{dim}{line:>width$} | {reset}{text}\n",
        dim = dim,
        reset = reset,
        line = line_num,
        width = num_width,
        text = text,
    );

    // Seta:  "   |    ^^^^"
    let start_col = span.start.column.saturating_sub(1);
    let end_col = if span.end.line == span.start.line {
        span.end.column.saturating_sub(1).max(start_col + 1)
    } else {
        // Span multi-linha: marca ate o fim da linha visivel.
        text.chars().count().max(start_col + 1)
    };
    let spaces = " ".repeat(start_col);
    let carets = "^".repeat(end_col.saturating_sub(start_col).max(1));
    let _ = write!(
        out,
        "{dim}{padding} | {reset}{spaces}{red}{carets}{reset}\n",
        dim = dim,
        padding = padding,
        reset = reset,
        spaces = spaces,
        red = red,
        carets = carets,
    );
}

fn display_path(path: &std::path::Path) -> String {
    // Tenta path relativo ao cwd; se falhar, absoluto.
    if let Ok(cwd) = std::env::current_dir() {
        if let Ok(rel) = path.strip_prefix(&cwd) {
            return rel.display().to_string();
        }
    }
    path.display().to_string()
}

/// Helper: determina se a saida deve usar ANSI color.
pub fn stderr_supports_color() -> bool {
    use std::io::IsTerminal;
    std::io::stderr().is_terminal()
}

// ---- Engine global -----------------------------------------------------
//
// Usamos um singleton global (OnceLock + Mutex interno no engine) para
// permitir que qualquer modulo do compilador emita diagnosticos sem precisar
// passar o engine por parametro. Compatível com rayon: o `Mutex` dentro do
// engine serializa emissoes concorrentes, e cada build zera o engine antes
// de comecar via `reset_global_engine()`.

use std::sync::OnceLock;

fn global_engine_slot() -> &'static std::sync::Mutex<DiagnosticEngine> {
    static SLOT: OnceLock<std::sync::Mutex<DiagnosticEngine>> = OnceLock::new();
    SLOT.get_or_init(|| std::sync::Mutex::new(DiagnosticEngine::new()))
}

/// Retorna um clone do engine global. Clonar e barato (`Arc<Mutex<>>` interno).
pub fn global_engine() -> DiagnosticEngine {
    global_engine_slot()
        .lock()
        .expect("global engine mutex poisoned")
        .clone()
}

/// Zera o engine global. Chamar antes de cada novo build.
pub fn reset_global_engine() {
    let mut guard = global_engine_slot()
        .lock()
        .expect("global engine mutex poisoned");
    *guard = DiagnosticEngine::new();
}

/// Helper: emite um diagnostico no engine global em um unico call.
pub fn emit(diagnostic: RichDiagnostic) {
    global_engine().emit(diagnostic);
}

// ---- API legado -------------------------------------------------------
//
// Mantemos `Diagnostic`/`render` antigos pois ainda nao ha consumidores,
// mas os mantemos como aliases simples para nao quebrar `suggestions.rs`.

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: Option<Span>,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
            span: None,
        }
    }
}

#[allow(dead_code)]
pub fn render(diagnostic: &Diagnostic) -> String {
    let severity = diagnostic.severity.label();
    if let Some(span) = diagnostic.span {
        format!(
            "{}: {} at {}:{}",
            severity, diagnostic.message, span.start.line, span.start.column
        )
    } else {
        format!("{}: {}", severity, diagnostic.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::span::{Position, Span};

    fn make_span(file: FileId, line: usize, col_start: usize, col_end: usize) -> Span {
        Span {
            start: Position {
                line,
                column: col_start,
            },
            end: Position {
                line,
                column: col_end,
            },
            file: Some(file),
        }
    }

    #[test]
    fn renders_error_with_snippet() {
        let file = source_store::register(
            "test_reporter_renders_snippet.ts",
            "const x = 1;\nconst y = 2;\n",
        );
        let engine = DiagnosticEngine::new();
        engine.emit(
            RichDiagnostic::error("E001", "test error")
                .with_span(make_span(file, 2, 7, 8))
                .with_note("test note"),
        );
        let out = engine.render_all(false);
        assert!(out.contains("error[E001]"), "output:\n{out}");
        assert!(out.contains("test error"), "output:\n{out}");
        assert!(
            out.contains("test_reporter_renders_snippet.ts:2:7"),
            "output:\n{out}"
        );
        assert!(out.contains("const y = 2;"), "output:\n{out}");
        assert!(out.contains("^"), "output:\n{out}");
        assert!(out.contains("note: test note"), "output:\n{out}");
    }

    #[test]
    fn counts_severities() {
        let engine = DiagnosticEngine::new();
        engine.emit(RichDiagnostic::error("E001", "e1"));
        engine.emit(RichDiagnostic::warning("W001", "w1"));
        engine.emit(RichDiagnostic::warning("W002", "w2"));
        assert_eq!(engine.errors_count(), 1);
        assert_eq!(engine.warnings_count(), 2);
        assert!(engine.has_errors());
    }

    #[test]
    fn render_suggestion_appears() {
        let engine = DiagnosticEngine::new();
        engine.emit(
            RichDiagnostic::error("E010", "unknown symbol")
                .with_suggestion("voce quis dizer 'print'?"),
        );
        let out = engine.render_all(false);
        assert!(out.contains("sugestao"));
        assert!(out.contains("print"));
    }
}

#[allow(unused_imports)]
use std::path::Path as _PathImport;

// Ensure PathBuf is not flagged as unused when running without tests.
#[allow(dead_code)]
fn _force_use_pathbuf(_: PathBuf) {}
