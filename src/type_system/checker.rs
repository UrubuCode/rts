use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};

use crate::parser::ast::{ClassMember, Item, Program};

use super::TypeRegistry;
use super::types::{TypeField, TypeKind};

pub type ImportExports = BTreeMap<String, BTreeSet<String>>;

#[derive(Debug, Clone)]
pub struct TypeDeclaration {
    pub name: String,
    pub kind: TypeKind,
}

pub fn check_program(
    program: &Program,
    registry: &mut TypeRegistry,
    import_exports: &ImportExports,
) -> Result<()> {
    seed_primitives(registry);
    check_imports(program, import_exports)?;
    let declarations = collect_type_declarations(program)?;
    register_type_declarations(registry, declarations);

    Ok(())
}

pub fn check_imports(program: &Program, import_exports: &ImportExports) -> Result<()> {
    for item in &program.items {
        if let Item::Import(import_decl) = item {
            check_import(import_decl, import_exports)?;
        }
    }

    Ok(())
}

pub fn collect_type_declarations(program: &Program) -> Result<Vec<TypeDeclaration>> {
    let mut declared_in_module = BTreeSet::new();
    let mut declarations = Vec::new();

    for item in &program.items {
        match item {
            Item::Interface(interface_decl) => {
                ensure_local_name_available(&interface_decl.name, &mut declared_in_module)?;

                let fields = interface_decl
                    .fields
                    .iter()
                    .map(|field| TypeField {
                        name: field.name.clone(),
                        type_name: field.type_annotation.clone(),
                    })
                    .collect();

                declarations.push(TypeDeclaration {
                    name: interface_decl.name.clone(),
                    kind: TypeKind::Interface { fields },
                });
            }
            Item::Class(class_decl) => {
                ensure_local_name_available(&class_decl.name, &mut declared_in_module)?;

                let mut fields = Vec::new();

                for member in &class_decl.members {
                    match member {
                        ClassMember::Property(prop) => {
                            fields.push(TypeField {
                                name: prop.name.clone(),
                                type_name: prop
                                    .type_annotation
                                    .clone()
                                    .unwrap_or_else(|| "any".to_string()),
                            });
                        }
                        ClassMember::Constructor(ctor) => {
                            for param in &ctor.parameters {
                                if param.modifiers.visibility.is_some() {
                                    fields.push(TypeField {
                                        name: param.name.clone(),
                                        type_name: param
                                            .type_annotation
                                            .clone()
                                            .unwrap_or_else(|| "any".to_string()),
                                    });
                                }
                            }
                        }
                        ClassMember::Method(_) => {}
                    }
                }

                declarations.push(TypeDeclaration {
                    name: class_decl.name.clone(),
                    kind: TypeKind::Class { fields },
                });
            }
            Item::Import(_) | Item::Function(_) | Item::Statement(_) => {}
        }
    }

    Ok(declarations)
}

pub fn register_type_declarations(
    registry: &mut TypeRegistry,
    declarations: impl IntoIterator<Item = TypeDeclaration>,
) {
    for declaration in declarations {
        registry.register(declaration.name, declaration.kind);
    }
}

fn check_import(
    import_decl: &crate::parser::ast::ImportDecl,
    import_exports: &ImportExports,
) -> Result<()> {
    let Some(exports) = import_exports.get(&import_decl.from) else {
        bail!("unknown module import: {}", import_decl.from);
    };

    for symbol in &import_decl.names {
        if !exports.contains(symbol) {
            bail!(
                "module '{}' does not export symbol '{}': available exports = {}",
                import_decl.from,
                symbol,
                exports.iter().cloned().collect::<Vec<_>>().join(", ")
            );
        }
    }

    if import_decl.default_name.is_some() && !exports.contains("default") {
        bail!("module '{}' has no default export", import_decl.from);
    }

    Ok(())
}

fn ensure_local_name_available(
    name: &str,
    declared_in_module: &mut BTreeSet<String>,
) -> Result<()> {
    if is_primitive_name(name) {
        bail!("'{}' is reserved as primitive type name", name);
    }

    if !declared_in_module.insert(name.to_string()) {
        bail!("duplicated type declaration in module: {}", name);
    }

    Ok(())
}

fn is_primitive_name(name: &str) -> bool {
    matches!(
        name,
        "number"
            | "string"
            | "boolean"
            | "void"
            | "any"
            | "null"
            | "undefined"
            | "unknown"
            | "never"
    )
}

pub fn seed_primitives(registry: &mut TypeRegistry) {
    for primitive in [
        "number",
        "string",
        "boolean",
        "void",
        "any",
        "null",
        "undefined",
        "unknown",
        "never",
    ] {
        if registry.get_by_name(primitive).is_none() {
            let _ = registry.register(primitive, TypeKind::Primitive);
        }
    }
}
