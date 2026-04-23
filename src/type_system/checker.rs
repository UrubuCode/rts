use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};

use crate::diagnostics::reporter::{self, RichDiagnostic};
use crate::parser::ast::{ClassMember, Item, Program};
use crate::parser::span::Span;

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
    // Coleta todos os erros de import antes de abortar — permite ao usuario
    // ver todos os problemas de uma vez, nao um por execucao.
    let mut first_error: Option<anyhow::Error> = None;
    for item in &program.items {
        if let Item::Import(import_decl) = item {
            if let Err(err) = check_import(import_decl, import_exports) {
                if first_error.is_none() {
                    first_error = Some(err);
                }
            }
        }
    }

    if let Some(err) = first_error {
        return Err(err);
    }
    Ok(())
}

pub fn collect_type_declarations(program: &Program) -> Result<Vec<TypeDeclaration>> {
    let mut declared_in_module = BTreeSet::new();
    let mut declarations = Vec::new();

    for item in &program.items {
        match item {
            Item::Interface(interface_decl) => {
                ensure_local_name_available(
                    &interface_decl.name,
                    interface_decl.span,
                    &mut declared_in_module,
                )?;

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
                ensure_local_name_available(
                    &class_decl.name,
                    class_decl.span,
                    &mut declared_in_module,
                )?;

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
        reporter::emit(
            RichDiagnostic::error(
                "E010",
                format!("modulo '{}' nao encontrado", import_decl.from),
            )
            .with_span(import_decl.span)
            .with_note(
                "o modulo nao foi carregado pelo compilador \
                 — verifique o especificador do import",
            ),
        );
        bail!("unknown module import: {}", import_decl.from);
    };

    for symbol in &import_decl.names {
        if !exports.contains(symbol) {
            let suggestion = suggest_similar(symbol, exports);
            let mut diag = RichDiagnostic::error(
                "E011",
                format!(
                    "modulo '{}' nao exporta o simbolo '{}'",
                    import_decl.from, symbol
                ),
            )
            .with_span(import_decl.span)
            .with_note(format!(
                "exports disponiveis: {}",
                exports.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
            if let Some(s) = suggestion {
                diag = diag.with_suggestion(format!("voce quis dizer '{s}'?"));
            }
            reporter::emit(diag);
            bail!(
                "module '{}' does not export symbol '{}': available exports = {}",
                import_decl.from,
                symbol,
                exports.iter().cloned().collect::<Vec<_>>().join(", ")
            );
        }
    }

    if import_decl.default_name.is_some() && !exports.contains("default") {
        reporter::emit(
            RichDiagnostic::error(
                "E012",
                format!("modulo '{}' nao possui export default", import_decl.from),
            )
            .with_span(import_decl.span)
            .with_note("use imports nomeados ({ foo, bar }) em vez de default"),
        );
        bail!("module '{}' has no default export", import_decl.from);
    }

    Ok(())
}

fn ensure_local_name_available(
    name: &str,
    span: Span,
    declared_in_module: &mut BTreeSet<String>,
) -> Result<()> {
    if is_primitive_name(name) {
        reporter::emit(
            RichDiagnostic::error(
                "E013",
                format!("'{name}' e um nome de tipo primitivo reservado"),
            )
            .with_span(span)
            .with_note(
                "escolha outro nome — tipos primitivos (number, string, bool, \
                 void, any, null, undefined, unknown, never) nao podem ser redeclarados",
            ),
        );
        bail!("'{}' is reserved as primitive type name", name);
    }

    if !declared_in_module.insert(name.to_string()) {
        reporter::emit(
            RichDiagnostic::error(
                "E014",
                format!("tipo '{name}' declarado mais de uma vez no mesmo modulo"),
            )
            .with_span(span)
            .with_note("remova a declaracao duplicada ou renomeie uma delas"),
        );
        bail!("duplicated type declaration in module: {}", name);
    }

    Ok(())
}

/// Helper: sugere o simbolo mais proximo (distancia Levenshtein <= 2) em um
/// conjunto de exports. Retorna None se nenhum candidato for proximo o suficiente.
fn suggest_similar(target: &str, exports: &BTreeSet<String>) -> Option<String> {
    exports
        .iter()
        .filter_map(|candidate| {
            let dist = levenshtein(target, candidate);
            if dist <= 2 && dist < target.len() {
                Some((dist, candidate.clone()))
            } else {
                None
            }
        })
        .min_by_key(|(dist, _)| *dist)
        .map(|(_, candidate)| candidate)
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let m = a.len();
    let n = b.len();
    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
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
