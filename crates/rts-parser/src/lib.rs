//! RTS parser crate: SWC-based TypeScript/JavaScript parser.
//!
//! Exposes `parse_source`, `parse_source_with_mode`, and
//! `parse_source_with_file` as the public API. Also re-exports
//! `FrontendMode` and the lexer module.

pub mod generator_desugar;
pub mod generator_sm;
pub mod lexer;

use anyhow::{Result, anyhow};
use swc_common::{FileName, SourceMap, SourceMapper, Span as SwcSpan, Spanned, sync::Lrc};
use swc_ecma_ast::{
    Accessibility, ArrowExpr, BlockStmt, Class as SwcClass, ClassDecl as SwcClassDecl,
    ClassMember as SwcClassMember, Decl, DefaultDecl, Expr, FnDecl as SwcFnDecl,
    Function as SwcFunction, ImportDecl as SwcImportDecl, ImportSpecifier, Lit, ModuleDecl,
    ModuleExportName, ModuleItem, Param as SwcParam, ParamOrTsParamProp, Pat,
    Program as SwcProgram, PropName, Stmt, TsInterfaceDecl as SwcTsInterfaceDecl, TsParamProp,
    TsParamPropParam, TsTypeAnn, TsTypeElement, VarDecl,
};
use swc_ecma_parser::{EsSyntax, Parser, StringInput, Syntax, TsSyntax, lexer::Lexer};

use rts_diagnostics::source_store::{self, FileId};

use rts_ast::ast::{
    ClassDecl, ClassMember, ConstructorDecl, ExportNamespaceDecl, FieldDecl, FunctionDecl,
    ImportDecl, ImportName, InterfaceDecl, Item, MemberModifiers, MethodDecl, MethodRole,
    Parameter, Program, PropertyDecl, RawStmt, Statement, Visibility,
};
use rts_ast::span::{Position, Span};

/// Controls which syntax dialect the parser tries first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FrontendMode {
    /// TypeScript syntax first, ES fallback.
    #[default]
    Native,
    /// ES syntax first, TypeScript fallback.
    Compat,
}

impl FrontendMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Compat => "compat",
        }
    }
}

impl std::fmt::Display for FrontendMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

include!("location_and_syntax.rs");
include!("lowering_helpers.rs");
include!("lowering_decls.rs");
include!("lowering_items.rs");
include!("parse_api.rs");
