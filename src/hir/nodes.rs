use super::annotations::TypeAnnotation;

/// Localização no arquivo TypeScript original.
/// Propagada do AST do SWC pelo lower e preservada até o codegen.
#[derive(Debug, Clone, Default)]
pub struct SourceLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
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
    Statement(String),
}

#[derive(Debug, Clone, Default)]
pub struct HirImport {
    pub names: Vec<String>,
    pub default_name: Option<String>,
    pub from: String,
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
    pub body: Vec<String>,
    /// Localização da declaração no arquivo TypeScript original.
    pub loc: Option<SourceLocation>,
}

#[derive(Debug, Clone, Default)]
pub struct HirParameter {
    pub name: String,
    pub type_annotation: Option<TypeAnnotation>,
    pub variadic: bool,
}
