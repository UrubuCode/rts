use crate::parser::ast::{ClassMember, Item, Program, Statement};
use crate::type_system::resolver::TypeResolver;

use super::annotations::TypeAnnotation;
use super::nodes::{
    HirClass, HirField, HirFunction, HirImport, HirInterface, HirItem, HirModule, HirParameter,
};

pub fn lower(program: &Program, resolver: &TypeResolver) -> HirModule {
    let mut module = HirModule::default();

    for item in &program.items {
        match item {
            Item::Import(import_decl) => {
                let import = HirImport {
                    names: import_decl.names.clone(),
                    default_name: import_decl.default_name.clone(),
                    from: import_decl.from.clone(),
                };

                module.items.push(HirItem::Import(import.clone()));
                module.imports.push(import);
            }
            Item::Interface(interface_decl) => {
                let fields = interface_decl
                    .fields
                    .iter()
                    .map(|field| HirField {
                        name: field.name.clone(),
                        type_annotation: annotate(&field.type_annotation, resolver),
                    })
                    .collect::<Vec<_>>();

                let interface = HirInterface {
                    name: interface_decl.name.clone(),
                    fields,
                };

                module.items.push(HirItem::Interface(interface.clone()));
                module.interfaces.push(interface);
            }
            Item::Class(class_decl) => {
                let mut class = HirClass {
                    name: class_decl.name.clone(),
                    fields: Vec::new(),
                    methods: Vec::new(),
                    loc: None,
                };

                for member in &class_decl.members {
                    match member {
                        ClassMember::Constructor(ctor) => {
                            let parameters = ctor
                                .parameters
                                .iter()
                                .map(|param| HirParameter {
                                    name: param.name.clone(),
                                    type_annotation: param
                                        .type_annotation
                                        .as_ref()
                                        .map(|name| annotate(name, resolver)),
                                    variadic: param.variadic,
                                })
                                .collect::<Vec<_>>();

                            for param in &ctor.parameters {
                                if param.modifiers.visibility.is_some() {
                                    class.fields.push(HirField {
                                        name: param.name.clone(),
                                        type_annotation: param
                                            .type_annotation
                                            .as_ref()
                                            .map(|name| annotate(name, resolver))
                                            .unwrap_or_else(|| TypeAnnotation::unresolved("any")),
                                    });
                                }
                            }

                            let ctor_fn = HirFunction {
                                name: format!("{}::constructor", class_decl.name),
                                parameters,
                                return_type: None,
                                body: Vec::new(),
                                loc: None,
                            };

                            module.functions.push(ctor_fn.clone());
                            class.methods.push(ctor_fn);
                        }
                        ClassMember::Method(method) => {
                            let parameters = method
                                .parameters
                                .iter()
                                .map(|param| HirParameter {
                                    name: param.name.clone(),
                                    type_annotation: param
                                        .type_annotation
                                        .as_ref()
                                        .map(|name| annotate(name, resolver)),
                                    variadic: param.variadic,
                                })
                                .collect::<Vec<_>>();

                            let function = HirFunction {
                                name: format!("{}::{}", class_decl.name, method.name),
                                parameters,
                                return_type: method
                                    .return_type
                                    .as_ref()
                                    .map(|name| annotate(name, resolver)),
                                body: Vec::new(),
                                loc: None,
                            };

                            module.functions.push(function.clone());
                            class.methods.push(function);
                        }
                        ClassMember::Property(property) => {
                            class.fields.push(HirField {
                                name: property.name.clone(),
                                type_annotation: property
                                    .type_annotation
                                    .as_ref()
                                    .map(|name| annotate(name, resolver))
                                    .unwrap_or_else(|| TypeAnnotation::unresolved("any")),
                            });
                        }
                    }
                }

                module.items.push(HirItem::Class(class.clone()));
                module.classes.push(class);
            }
            Item::Function(function_decl) => {
                let parameters = function_decl
                    .parameters
                    .iter()
                    .map(|param| HirParameter {
                        name: param.name.clone(),
                        type_annotation: param
                            .type_annotation
                            .as_ref()
                            .map(|name| annotate(name, resolver)),
                        variadic: param.variadic,
                    })
                    .collect::<Vec<_>>();

                let function = HirFunction {
                    name: function_decl.name.clone(),
                    parameters,
                    return_type: function_decl
                        .return_type
                        .as_ref()
                        .map(|name| annotate(name, resolver)),
                    body: function_decl
                        .body
                        .iter()
                        .map(|statement| match statement {
                            Statement::Raw(raw) => raw.value.clone(),
                        })
                        .collect(),
                    loc: None,
                };

                module.items.push(HirItem::Function(function.clone()));
                module.functions.push(function);
            }
            Item::Statement(statement) => {
                let text = match statement {
                    Statement::Raw(raw) => raw.value.clone(),
                };

                module.items.push(HirItem::Statement(text));
            }
        }
    }

    module
}

fn annotate(type_name: &str, resolver: &TypeResolver) -> TypeAnnotation {
    if let Some(id) = resolver.resolve(type_name) {
        TypeAnnotation::resolved(type_name, id.0)
    } else {
        TypeAnnotation::unresolved(type_name)
    }
}
