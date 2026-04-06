use super::annotations::TypeAnnotation;

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
    pub from: String,
}

#[derive(Debug, Clone, Default)]
pub struct HirClass {
    pub name: String,
    pub fields: Vec<HirField>,
    pub methods: Vec<HirFunction>,
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
}

#[derive(Debug, Clone, Default)]
pub struct HirParameter {
    pub name: String,
    pub type_annotation: Option<TypeAnnotation>,
    pub variadic: bool,
}
