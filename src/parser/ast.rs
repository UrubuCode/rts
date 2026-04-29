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
    /// Classe pai em `class C extends P`. Resolve em codegen para
    /// dispatch estatico de super().
    pub super_class: Option<String>,
    pub members: Vec<ClassMember>,
    /// `abstract class C { ... }` — não pode ser instanciada via `new C()`.
    /// Subclasses concretas devem implementar todos os métodos abstract.
    pub is_abstract: bool,
    /// Stmts dos `static { ... }` blocks, na ordem em que apareceram.
    /// expand_static_fields prepend-os ao top-level depois das declaracoes
    /// de static fields, mantendo a ordem de inicializacao TS.
    pub static_init_body: Vec<Statement>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodRole {
    Method,
    Getter,
    Setter,
}

#[derive(Debug, Clone)]
pub struct MethodDecl {
    pub name: String,
    pub modifiers: MemberModifiers,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<String>,
    pub body: Vec<Statement>,
    pub role: MethodRole,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct PropertyDecl {
    pub name: String,
    pub modifiers: MemberModifiers,
    pub type_annotation: Option<String>,
    /// Inicializador opcional (`x: number = 42`). Quando presente,
    /// `synthesize_class_fns` injeta `this.<name> = <initializer>`
    /// no prólogo do constructor (depois de `super()`, antes do
    /// corpo do usuário).
    pub initializer: Option<Box<swc_ecma_ast::Expr>>,
    pub span: Span,
}

#[derive(Debug, Clone, Default)]
pub struct MemberModifiers {
    pub visibility: Option<Visibility>,
    pub readonly: bool,
    pub is_static: bool,
    /// `abstract method(): T` ou `abstract field: T` — sem implementação
    /// nesta classe; subclasses concretas devem prover. Só faz sentido
    /// dentro de uma `abstract class`.
    pub is_abstract: bool,
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
    /// Expressão de default (`(x = 42)`). Quando presente, callsites
    /// que omitem este argumento são expandidos com o valor default
    /// num pass do compilador.
    pub default: Option<Box<swc_ecma_ast::Expr>>,
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
