use crate::diagnostics::source_store::FileId;
use crate::parser::span::Span;

use super::annotations::TypeAnnotation;

/// Statement no HIR. Carrega o texto original (util para diagnosticos e
/// para o fallback `RuntimeEval`) e o `swc_ecma_ast::Stmt` ja parseado
/// quando disponivel. O MIR prefere o Stmt estruturado; so cai no texto
/// quando o parser interno nao tem o Stmt (casos raros de lowering
/// sintetico).
///
/// Antes desta etapa, o HIR armazenava apenas `Vec<String>` como body e
/// o MIR re-parseava cada string com um parser SWC local (ciclo
/// parse -> string -> reparse). Com `HirStmt` estruturado a cadeia vira:
/// SWC parse -> HIR (Stmt clonado) -> MIR lower, sem nenhum re-parse.
#[derive(Debug, Clone)]
pub struct HirStmt {
    pub text: String,
    pub stmt: Option<swc_ecma_ast::Stmt>,
}

impl HirStmt {
    pub fn new(text: String, stmt: Option<swc_ecma_ast::Stmt>) -> Self {
        Self { text, stmt }
    }
}

/// Localização no arquivo TypeScript original.
/// Propagada do AST do SWC pelo lower e preservada até o codegen.
#[derive(Debug, Clone, Default)]
pub struct SourceLocation {
    pub file: String,
    pub file_id: Option<FileId>,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

impl SourceLocation {
    /// Constroi uma `SourceLocation` a partir de um `Span` do parser.
    /// O `file` textual e resolvido via `source_store` quando disponivel.
    pub fn from_span(span: Span) -> Self {
        let file_label = span
            .file
            .and_then(crate::diagnostics::source_store::path_of)
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        Self {
            file: file_label,
            file_id: span.file,
            line: span.start.line as u32,
            column: span.start.column as u32,
            end_line: span.end.line as u32,
            end_column: span.end.column as u32,
        }
    }

    /// Converte de volta para `Span` — util quando emitindo diagnosticos.
    pub fn to_span(&self) -> Span {
        Span {
            start: crate::parser::span::Position {
                line: self.line as usize,
                column: self.column as usize,
            },
            end: crate::parser::span::Position {
                line: self.end_line as usize,
                column: self.end_column as usize,
            },
            file: self.file_id,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct HirModule {
    pub items: Vec<HirItem>,
    pub imports: Vec<HirImport>,
    pub classes: Vec<HirClass>,
    pub functions: Vec<HirFunction>,
    pub interfaces: Vec<HirInterface>,
}

#[derive(Debug, Clone)]
pub enum HirItem {
    Import(HirImport),
    Function(HirFunction),
    Interface(HirInterface),
    Class(HirClass),
    /// Top-level statement representado como AST do SWC ja parseado.
    /// O MIR consome direto, sem re-parse.
    Statement(HirStmt),
}

#[derive(Debug, Clone, Default)]
pub struct HirImport {
    pub names: Vec<String>,
    pub default_name: Option<String>,
    pub from: String,
    /// Localização da declaração no arquivo TypeScript original.
    pub loc: Option<SourceLocation>,
}

#[derive(Debug, Clone, Default)]
pub struct HirClass {
    pub name: String,
    pub fields: Vec<HirField>,
    pub methods: Vec<HirFunction>,
    /// Localização da declaração no arquivo TypeScript original.
    pub loc: Option<SourceLocation>,
}

#[derive(Debug, Clone, Default)]
pub struct HirInterface {
    pub name: String,
    pub fields: Vec<HirField>,
}

#[derive(Debug, Clone, Default)]
pub struct HirField {
    pub name: String,
    pub type_annotation: TypeAnnotation,
}

#[derive(Debug, Clone, Default)]
pub struct HirFunction {
    pub name: String,
    pub parameters: Vec<HirParameter>,
    pub return_type: Option<TypeAnnotation>,
    /// Corpo da funcao como stmts SWC ja parseados. Antes era
    /// `Vec<String>` e o MIR re-parseava cada string com um SourceMap
    /// local — ciclo eliminado na Etapa 6.
    pub body: Vec<HirStmt>,
    /// Localização da declaração no arquivo TypeScript original.
    pub loc: Option<SourceLocation>,
}

#[derive(Debug, Clone, Default)]
pub struct HirParameter {
    pub name: String,
    pub type_annotation: Option<TypeAnnotation>,
    pub variadic: bool,
}
