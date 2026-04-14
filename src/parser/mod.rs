pub mod ast;
pub mod lexer;
pub mod span;

use anyhow::{Result, anyhow};
use swc_common::{FileName, SourceMap, SourceMapper, Span as SwcSpan, Spanned, sync::Lrc};
use swc_ecma_ast::{
    Accessibility, BlockStmt, Class as SwcClass, ClassDecl as SwcClassDecl,
    ClassMember as SwcClassMember, Decl, DefaultDecl, Expr, FnDecl as SwcFnDecl,
    Function as SwcFunction, ImportDecl as SwcImportDecl, ImportSpecifier, Lit, ModuleDecl,
    ModuleExportName, ModuleItem, Param as SwcParam, ParamOrTsParamProp, Pat,
    Program as SwcProgram, PropName, Stmt, TsInterfaceDecl as SwcTsInterfaceDecl, TsParamProp,
    TsParamPropParam, TsTypeAnn, TsTypeElement,
};
use swc_ecma_parser::{EsSyntax, Parser, StringInput, Syntax, TsSyntax, lexer::Lexer};

use crate::compile_options::FrontendMode;
use crate::diagnostics::source_store::{self, FileId};

use ast::{
    ClassDecl, ClassMember, ConstructorDecl, FieldDecl, FunctionDecl, ImportDecl, InterfaceDecl,
    Item, MemberModifiers, MethodDecl, Parameter, Program, PropertyDecl, RawStmt, Statement,
    Visibility,
};
use span::{Position, Span};

include!("parse_api.rs");
include!("lowering_items.rs");
include!("lowering_decls.rs");
include!("lowering_helpers.rs");
include!("location_and_syntax.rs");
