use super::span::Span;

#[derive(Debug, Clone, Default)]
pub struct Program {
    pub items: Vec<Item>,
}

#[derive(Debug, Clone)]
pub enum Item {
    Import(ImportDecl),
    Interface(InterfaceDecl),
    Class(ClassDecl),
    Function(FunctionDecl),
    Statement(Statement),
}

#[derive(Debug, Clone)]
pub struct ImportDecl {
    /// Named imports: `import { foo, bar } from "…"`
    pub names: Vec<String>,
    /// Default import local name: `import io from "…"`
    pub default_name: Option<String>,
    pub from: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct InterfaceDecl {
    pub name: String,
    pub fields: Vec<FieldDecl>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FieldDecl {
    pub name: String,
    pub type_annotation: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ClassDecl {
    pub name: String,
    pub members: Vec<ClassMember>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ClassMember {
    Constructor(ConstructorDecl),
    Method(MethodDecl),
    Property(PropertyDecl),
}

#[derive(Debug, Clone)]
pub struct ConstructorDecl {
    pub parameters: Vec<Parameter>,
    pub body: Vec<Statement>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MethodDecl {
    pub name: String,
    pub modifiers: MemberModifiers,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<String>,
    pub body: Vec<Statement>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct PropertyDecl {
    pub name: String,
    pub modifiers: MemberModifiers,
    pub type_annotation: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, Default)]
pub struct MemberModifiers {
    pub visibility: Option<Visibility>,
    pub readonly: bool,
    pub is_static: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Private,
    Protected,
}

#[derive(Debug, Clone)]
pub struct FunctionDecl {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<String>,
    pub body: Vec<Statement>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub type_annotation: Option<String>,
    pub modifiers: MemberModifiers,
    pub variadic: bool,
    pub span: Span,
}

/// Statement no AST interno do parser. Cada statement carrega o texto
/// original (ainda util para diagnosticos e para o fallback `RuntimeEval`)
/// e o `swc_ecma_ast::Stmt` ja parseado, que o MIR consome direto sem
/// re-parse — eliminando o ciclo parse -> string -> reparse.
#[derive(Debug, Clone)]
pub enum Statement {
    Raw(RawStmt),
}

#[derive(Debug, Clone)]
pub struct RawStmt {
    pub text: String,
    pub span: Span,
    /// AST SWC ja parseado deste statement. Clonado diretamente do
    /// parser SWC — sem re-parse posterior. Pode ser `None` apenas em
    /// casos limite onde o lowerer do parser interno nao tem acesso
    /// ao Stmt original (ex: construcoes sinteticas).
    pub stmt: Option<swc_ecma_ast::Stmt>,
}

impl RawStmt {
    pub fn new(text: String, span: Span) -> Self {
        Self {
            text,
            span,
            stmt: None,
        }
    }

    pub fn with_stmt(mut self, stmt: swc_ecma_ast::Stmt) -> Self {
        self.stmt = Some(stmt);
        self
    }
}
