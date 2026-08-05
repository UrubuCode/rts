//! Statements, functions, classes and modules.

use swc_common::Spanned;
use swc_ecma_ast as swc;

use super::expr::{expr, property_key};
use super::pat::{binding, binding_element, target};
use super::{Cx, Result, position, unsupported};
use crate::syntax::{
    Binding, BindingKind, Catch, Claim, Class, ClassElement, ClassKey, Directive, Export,
    ExportDefault, ExportKind, ExportSpecifier, Field, ForEachSource, ForEachTarget, ForInit,
    Function, FunctionBody, Goal, Import, ImportAttribute, ImportBinding, Method, MethodKind,
    ModuleItem, Parameter, Pattern, Program, Stmt, StmtKind, SwitchClause,
};

/// A whole program.
pub(crate) fn program(cx: &mut Cx, parsed: &swc::Program, goal: Goal) -> Result<Program> {
    let mut items = Vec::new();
    let mut directives = Vec::new();

    match parsed {
        swc::Program::Module(module) => {
            let mut in_prologue = true;
            for item in &module.body {
                if in_prologue
                    && let swc::ModuleItem::Stmt(statement) = item
                    && let Some(directive) = as_directive(statement)
                {
                    directives.push(directive);
                    continue;
                }
                in_prologue = false;
                items.push(module_item(cx, item)?);
            }
        }
        swc::Program::Script(script) => {
            let mut in_prologue = true;
            for statement in &script.body {
                if in_prologue && let Some(directive) = as_directive(statement) {
                    directives.push(directive);
                    continue;
                }
                in_prologue = false;
                items.push(ModuleItem::Stmt(stmt(cx, statement)?));
            }
        }
    }

    Ok(Program {
        goal,
        directives,
        body: items,
    })
}

/// A statement that is a bare string literal, and so may be a directive.
///
/// Read from the raw text: `"use strict"` is a string statement, not a
/// cleverly-spelled directive, and checking the cooked value would accept it.
fn as_directive(statement: &swc::Stmt) -> Option<Directive> {
    let swc::Stmt::Expr(expression) = statement else {
        return None;
    };
    let swc::Expr::Lit(swc::Lit::Str(string)) = &*expression.expr else {
        return None;
    };
    let raw = string.raw.as_ref()?;
    let text = raw.as_str();
    let unquoted = text.get(1..text.len() - 1)?;
    Some(Directive {
        raw: unquoted.to_owned(),
    })
}

fn module_item(cx: &mut Cx, item: &swc::ModuleItem) -> Result<ModuleItem> {
    Ok(match item {
        swc::ModuleItem::Stmt(statement) => ModuleItem::Stmt(stmt(cx, statement)?),
        swc::ModuleItem::ModuleDecl(declaration) => match declaration {
            swc::ModuleDecl::Import(import) => ModuleItem::Import(import_decl(cx, import)?),
            other => ModuleItem::Export(export_decl(cx, other)?),
        },
    })
}

fn import_decl(cx: &mut Cx, import: &swc::ImportDecl) -> Result<Import> {
    let bindings = import
        .specifiers
        .iter()
        .map(|specifier| {
            Ok(match specifier {
                swc::ImportSpecifier::Default(default) => {
                    ImportBinding::Default(cx.name(&default.local.sym))
                }
                swc::ImportSpecifier::Namespace(namespace) => {
                    ImportBinding::Namespace(cx.name(&namespace.local.sym))
                }
                swc::ImportSpecifier::Named(named) => ImportBinding::Named {
                    exported: match &named.imported {
                        Some(swc::ModuleExportName::Ident(ident)) => ident.sym.to_string(),
                        Some(swc::ModuleExportName::Str(string)) => {
                            string.value.to_string_lossy().to_string()
                        }
                        None => named.local.sym.to_string(),
                    },
                    local: cx.name(&named.local.sym),
                },
            })
        })
        .collect::<Result<_>>()?;

    Ok(Import {
        bindings,
        source: import.src.value.to_string_lossy().to_string(),
        attributes: attributes(import.with.as_deref()),
        at: position(import.span),
    })
}

/// `with { type: "json" }`.
fn attributes(with: Option<&swc::ObjectLit>) -> Vec<ImportAttribute> {
    let Some(object) = with else {
        return Vec::new();
    };
    object
        .props
        .iter()
        .filter_map(|property| {
            let swc::PropOrSpread::Prop(prop) = property else {
                return None;
            };
            let swc::Prop::KeyValue(pair) = &**prop else {
                return None;
            };
            let key = match &pair.key {
                swc::PropName::Ident(ident) => ident.sym.to_string(),
                swc::PropName::Str(string) => string.value.to_string_lossy().to_string(),
                _ => return None,
            };
            let swc::Expr::Lit(swc::Lit::Str(value)) = &*pair.value else {
                return None;
            };
            Some(ImportAttribute {
                key,
                value: value.value.to_string_lossy().to_string(),
            })
        })
        .collect()
}

fn export_decl(cx: &mut Cx, declaration: &swc::ModuleDecl) -> Result<Export> {
    let at = position(declaration.span());
    let kind = match declaration {
        swc::ModuleDecl::ExportDecl(export) => {
            ExportKind::Declaration(Box::new(decl(cx, &export.decl)?))
        }

        swc::ModuleDecl::ExportNamed(named) => ExportKind::Named {
            specifiers: named
                .specifiers
                .iter()
                .map(|specifier| match specifier {
                    swc::ExportSpecifier::Named(entry) => Ok(ExportSpecifier {
                        local: export_name(&entry.orig),
                        exported: entry
                            .exported
                            .as_ref()
                            .map(export_name)
                            .unwrap_or_else(|| export_name(&entry.orig)),
                    }),
                    swc::ExportSpecifier::Default(entry) => Ok(ExportSpecifier {
                        local: entry.exported.sym.to_string(),
                        exported: "default".to_owned(),
                    }),
                    swc::ExportSpecifier::Namespace(entry) => Ok(ExportSpecifier {
                        local: "*".to_owned(),
                        exported: export_name(&entry.name),
                    }),
                })
                .collect::<Result<_>>()?,
            source: named
                .src
                .as_ref()
                .map(|s| s.value.to_string_lossy().to_string()),
            attributes: attributes(named.with.as_deref()),
        },

        swc::ModuleDecl::ExportDefaultDecl(default) => {
            ExportKind::Default(ExportDefault::Declaration(Box::new(match &default.decl {
                swc::DefaultDecl::Fn(function) => Stmt::new(
                    StmtKind::Function(Box::new(function_expr(cx, function)?)),
                    at,
                ),
                swc::DefaultDecl::Class(class) => {
                    Stmt::new(StmtKind::Class(Box::new(class_expr(cx, class)?)), at)
                }
                swc::DefaultDecl::TsInterfaceDecl(interface) => {
                    return unsupported("an exported interface", position(interface.span));
                }
            })))
        }

        swc::ModuleDecl::ExportDefaultExpr(default) => {
            ExportKind::Default(ExportDefault::Expr(expr(cx, &default.expr)?))
        }

        swc::ModuleDecl::ExportAll(all) => ExportKind::All {
            source: all.src.value.to_string_lossy().to_string(),
            alias: None,
            attributes: attributes(all.with.as_deref()),
        },

        swc::ModuleDecl::Import(_) => return unsupported("an import reached as an export", at),
        swc::ModuleDecl::TsImportEquals(_)
        | swc::ModuleDecl::TsExportAssignment(_)
        | swc::ModuleDecl::TsNamespaceExport(_) => {
            return unsupported("a TypeScript-only module declaration", at);
        }
    };

    Ok(Export { kind, at })
}

fn export_name(name: &swc::ModuleExportName) -> String {
    match name {
        swc::ModuleExportName::Ident(ident) => ident.sym.to_string(),
        swc::ModuleExportName::Str(string) => string.value.to_string_lossy().to_string(),
    }
}

/// One statement.
pub(crate) fn stmt(cx: &mut Cx, statement: &swc::Stmt) -> Result<Stmt> {
    let at = position(statement.span());
    let kind = match statement {
        swc::Stmt::Expr(expression) => StmtKind::Expr(expr(cx, &expression.expr)?),
        swc::Stmt::Empty(_) => StmtKind::Empty,
        swc::Stmt::Debugger(_) => StmtKind::Debugger,

        swc::Stmt::Block(block) => StmtKind::Block(stmts(cx, &block.stmts)?),

        swc::Stmt::If(if_) => StmtKind::If {
            condition: expr(cx, &if_.test)?,
            then_branch: Box::new(stmt(cx, &if_.cons)?),
            else_branch: match &if_.alt {
                Some(alt) => Some(Box::new(stmt(cx, alt)?)),
                None => None,
            },
        },

        swc::Stmt::While(while_) => StmtKind::While {
            condition: expr(cx, &while_.test)?,
            body: Box::new(stmt(cx, &while_.body)?),
        },

        swc::Stmt::DoWhile(do_while) => StmtKind::DoWhile {
            body: Box::new(stmt(cx, &do_while.body)?),
            condition: expr(cx, &do_while.test)?,
        },

        swc::Stmt::For(for_) => StmtKind::For {
            init: match &for_.init {
                Some(swc::VarDeclOrExpr::VarDecl(declaration)) => Some(ForInit::Declare {
                    kind: binding_kind(declaration.kind),
                    bindings: declarators(cx, &declaration.decls)?,
                }),
                Some(swc::VarDeclOrExpr::Expr(expression)) => {
                    Some(ForInit::Expr(expr(cx, expression)?))
                }
                None => None,
            },
            test: match &for_.test {
                Some(test) => Some(expr(cx, test)?),
                None => None,
            },
            update: match &for_.update {
                Some(update) => Some(expr(cx, update)?),
                None => None,
            },
            body: Box::new(stmt(cx, &for_.body)?),
        },

        swc::Stmt::ForIn(for_in) => StmtKind::ForEach {
            source: ForEachSource::In,
            target: for_head(cx, &for_in.left)?,
            subject: expr(cx, &for_in.right)?,
            body: Box::new(stmt(cx, &for_in.body)?),
        },

        swc::Stmt::ForOf(for_of) => StmtKind::ForEach {
            source: if for_of.is_await {
                ForEachSource::AwaitOf
            } else {
                ForEachSource::Of
            },
            target: for_head(cx, &for_of.left)?,
            subject: expr(cx, &for_of.right)?,
            body: Box::new(stmt(cx, &for_of.body)?),
        },

        swc::Stmt::Return(return_) => StmtKind::Return(match &return_.arg {
            Some(value) => Some(expr(cx, value)?),
            None => None,
        }),

        swc::Stmt::Break(break_) => StmtKind::Break(break_.label.as_ref().map(|l| cx.name(&l.sym))),
        swc::Stmt::Continue(continue_) => {
            StmtKind::Continue(continue_.label.as_ref().map(|l| cx.name(&l.sym)))
        }

        swc::Stmt::Labeled(labeled) => StmtKind::Labelled {
            label: cx.name(&labeled.label.sym),
            body: Box::new(stmt(cx, &labeled.body)?),
        },

        swc::Stmt::Switch(switch) => StmtKind::Switch {
            discriminant: expr(cx, &switch.discriminant)?,
            clauses: switch
                .cases
                .iter()
                .map(|case| {
                    Ok(SwitchClause {
                        test: match &case.test {
                            Some(test) => Some(expr(cx, test)?),
                            None => None,
                        },
                        body: stmts(cx, &case.cons)?,
                    })
                })
                .collect::<Result<_>>()?,
        },

        swc::Stmt::Throw(throw) => StmtKind::Throw(expr(cx, &throw.arg)?),

        swc::Stmt::Try(try_) => StmtKind::Try {
            body: stmts(cx, &try_.block.stmts)?,
            catch: match &try_.handler {
                Some(handler) => Some(Catch {
                    binding: match &handler.param {
                        Some(parameter) => Some(binding(cx, parameter)?),
                        None => None,
                    },
                    body: stmts(cx, &handler.body.stmts)?,
                }),
                None => None,
            },
            finally: match &try_.finalizer {
                Some(block) => Some(stmts(cx, &block.stmts)?),
                None => None,
            },
        },

        swc::Stmt::With(with) => StmtKind::With {
            object: expr(cx, &with.obj)?,
            body: Box::new(stmt(cx, &with.body)?),
        },

        swc::Stmt::Decl(declaration) => return decl(cx, declaration),
    };

    Ok(Stmt::new(kind, at))
}

fn stmts(cx: &mut Cx, list: &[swc::Stmt]) -> Result<Vec<Stmt>> {
    list.iter().map(|s| stmt(cx, s)).collect()
}

fn for_head(cx: &mut Cx, head: &swc::ForHead) -> Result<ForEachTarget> {
    Ok(match head {
        swc::ForHead::VarDecl(declaration) => {
            let Some(first) = declaration.decls.first() else {
                return unsupported(
                    "a for-head that declares nothing",
                    position(declaration.span),
                );
            };
            ForEachTarget::Declare {
                kind: binding_kind(declaration.kind),
                target: binding(cx, &first.name)?,
            }
        }
        // `target`, not `binding`: a for-head with no declaration assigns to
        // places that already exist, so `for ([a, obj.b] of xs)` is as legal as
        // `[a, obj.b] = xs`. Reading it in the binding role refused every member
        // target in a for-head — the same shape read in the wrong one of the two
        // roles this module exists to keep apart.
        swc::ForHead::Pat(pattern) => ForEachTarget::Assign(target(cx, pattern)?),
        swc::ForHead::UsingDecl(using) => {
            return unsupported("`using` in a for-head", position(using.span));
        }
    })
}

/// A declaration, which is a statement here.
fn decl(cx: &mut Cx, declaration: &swc::Decl) -> Result<Stmt> {
    let at = position(declaration.span());
    let kind = match declaration {
        swc::Decl::Var(variables) => StmtKind::Declare {
            kind: binding_kind(variables.kind),
            bindings: declarators(cx, &variables.decls)?,
        },
        swc::Decl::Fn(function) => StmtKind::Function(Box::new(Function {
            name: Some(cx.name(&function.ident.sym)),
            ..function_parts(cx, &function.function)?
        })),
        swc::Decl::Class(class) => StmtKind::Class(Box::new(Class {
            name: Some(cx.name(&class.ident.sym)),
            ..class_parts(cx, &class.class)?
        })),
        swc::Decl::Using(using) => StmtKind::Using {
            bindings: declarators(cx, &using.decls)?,
            is_async: using.is_await,
        },
        swc::Decl::TsInterface(_)
        | swc::Decl::TsTypeAlias(_)
        | swc::Decl::TsEnum(_)
        | swc::Decl::TsModule(_) => {
            // Types are erased, so an interface or an alias contributes nothing
            // to what runs. An enum and a namespace DO, and refusing them is
            // honest until they are lowered.
            match declaration {
                swc::Decl::TsInterface(_) | swc::Decl::TsTypeAlias(_) => StmtKind::Empty,
                _ => return unsupported("a TypeScript enum or namespace", at),
            }
        }
    };
    Ok(Stmt::new(kind, at))
}

fn declarators(cx: &mut Cx, declarators: &[swc::VarDeclarator]) -> Result<Vec<Binding>> {
    declarators
        .iter()
        .map(|declarator| {
            Ok(Binding {
                claim: type_of(cx, &declarator.name),
                target: binding(cx, &declarator.name)?,
                value: match &declarator.init {
                    Some(value) => Some(expr(cx, value)?),
                    None => None,
                },
            })
        })
        .collect()
}

fn binding_kind(kind: swc::VarDeclKind) -> BindingKind {
    match kind {
        swc::VarDeclKind::Var => BindingKind::Var,
        swc::VarDeclKind::Let => BindingKind::Let,
        swc::VarDeclKind::Const => BindingKind::Const,
    }
}

/// A function declaration or expression.
pub(crate) fn function_expr(cx: &mut Cx, function: &swc::FnExpr) -> Result<Function> {
    Ok(Function {
        name: function.ident.as_ref().map(|i| cx.name(&i.sym)),
        ..function_parts(cx, &function.function)?
    })
}

/// A method's function, which has no name of its own.
pub(crate) fn function(cx: &mut Cx, function: &swc::Function) -> Result<Function> {
    function_parts(cx, function)
}

fn function_parts(cx: &mut Cx, function: &swc::Function) -> Result<Function> {
    let (parameters, rest_parameter) = parameters(cx, function.params.iter().map(|p| &p.pat))?;
    let (directives, body) = block_body(cx, function.body.as_ref())?;

    Ok(Function {
        name: None,
        parameters,
        rest_parameter,
        directives,
        body,
        returns: function
            .return_type
            .as_deref()
            .map(|t| claim(cx, &t.type_ann)),
        captures_this: false,
        is_async: function.is_async,
        is_generator: function.is_generator,
        at: position(function.span),
    })
}

/// An arrow, whose one real difference is where `this` comes from.
pub(crate) fn arrow(cx: &mut Cx, arrow: &swc::ArrowExpr) -> Result<Function> {
    let (parameters, rest_parameter) = parameters(cx, arrow.params.iter())?;

    let (directives, body) = match &*arrow.body {
        swc::BlockStmtOrExpr::BlockStmt(block) => block_body(cx, Some(block))?,
        swc::BlockStmtOrExpr::Expr(expression) => (
            Vec::new(),
            FunctionBody::Expression(Box::new(expr(cx, expression)?)),
        ),
    };

    Ok(Function {
        name: None,
        parameters,
        rest_parameter,
        directives,
        body,
        returns: arrow.return_type.as_deref().map(|t| claim(cx, &t.type_ann)),
        captures_this: true,
        is_async: arrow.is_async,
        is_generator: arrow.is_generator,
        at: position(arrow.span),
    })
}

/// A getter or setter, which has a body and at most one parameter.
pub(crate) fn accessor(
    cx: &mut Cx,
    body: Option<&swc::BlockStmt>,
    params: &[swc::Pat],
    at: rts_cranelift::fault::Position,
) -> Result<Function> {
    let (parameters, rest_parameter) = parameters(cx, params.iter())?;
    let (directives, body) = block_body(cx, body)?;

    Ok(Function {
        name: None,
        parameters,
        rest_parameter,
        directives,
        body,
        returns: None,
        captures_this: false,
        is_async: false,
        is_generator: false,
        at,
    })
}

fn block_body(
    cx: &mut Cx,
    block: Option<&swc::BlockStmt>,
) -> Result<(Vec<Directive>, FunctionBody)> {
    let Some(block) = block else {
        return Ok((Vec::new(), FunctionBody::Block(Vec::new())));
    };

    let mut directives = Vec::new();
    let mut body = Vec::new();
    let mut in_prologue = true;
    for statement in &block.stmts {
        if in_prologue && let Some(directive) = as_directive(statement) {
            directives.push(directive);
            continue;
        }
        in_prologue = false;
        body.push(stmt(cx, statement)?);
    }

    Ok((directives, FunctionBody::Block(body)))
}

/// The declared parameters, and the rest parameter pulled out of the list.
fn parameters<'a>(
    cx: &mut Cx,
    params: impl Iterator<Item = &'a swc::Pat>,
) -> Result<(Vec<Parameter>, Option<Pattern>)> {
    let mut declared = Vec::new();
    let mut rest = None;

    for parameter in params {
        if let swc::Pat::Rest(spread) = parameter {
            rest = Some(binding(cx, &spread.arg)?);
            continue;
        }
        declared.push(parameter_of(cx, parameter)?);
    }

    Ok((declared, rest))
}

/// What a pattern's annotation claimed, if it carried one.
fn type_of(cx: &mut Cx, pattern: &swc::Pat) -> Option<Claim> {
    let annotation = match pattern {
        swc::Pat::Ident(ident) => ident.type_ann.as_deref(),
        swc::Pat::Array(array) => array.type_ann.as_deref(),
        swc::Pat::Object(object) => object.type_ann.as_deref(),
        swc::Pat::Rest(rest) => rest.type_ann.as_deref(),
        _ => None,
    }?;
    Some(claim(cx, &annotation.type_ann))
}

/// A TypeScript annotation, read for what it claims about a representation.
///
/// Deliberately shallow. This crate reads annotations to decide representations,
/// so a type it cannot turn into one is [`Claim::Unknown`] — an honest answer
/// that proves nothing, rather than a partial model of TypeScript's type
/// language that would have to be kept in step with the real one.
pub(crate) fn claim(cx: &mut Cx, annotation: &swc::TsType) -> Claim {
    match annotation {
        swc::TsType::TsKeywordType(keyword) => match keyword.kind {
            swc::TsKeywordTypeKind::TsNumberKeyword => Claim::Number,
            swc::TsKeywordTypeKind::TsStringKeyword => Claim::Str,
            swc::TsKeywordTypeKind::TsBooleanKeyword => Claim::Boolean,
            swc::TsKeywordTypeKind::TsUndefinedKeyword => Claim::Undefined,
            swc::TsKeywordTypeKind::TsNullKeyword => Claim::Null,
            _ => Claim::Unknown,
        },
        swc::TsType::TsArrayType(array) => Claim::Array(Box::new(claim(cx, &array.elem_type))),
        swc::TsType::TsTypeRef(reference) => match &reference.type_name {
            swc::TsEntityName::Ident(ident) => Claim::Object(cx.name(&ident.sym)),
            swc::TsEntityName::TsQualifiedName(_) => Claim::Unknown,
        },
        swc::TsType::TsUnionOrIntersectionType(swc::TsUnionOrIntersectionType::TsUnionType(
            union,
        )) => Claim::Union(union.types.iter().map(|t| claim(cx, t)).collect()),
        swc::TsType::TsParenthesizedType(paren) => claim(cx, &paren.type_ann),
        _ => Claim::Unknown,
    }
}

/// A class expression.
pub(crate) fn class_expr(cx: &mut Cx, class: &swc::ClassExpr) -> Result<Class> {
    Ok(Class {
        name: class.ident.as_ref().map(|i| cx.name(&i.sym)),
        ..class_parts(cx, &class.class)?
    })
}

fn class_parts(cx: &mut Cx, class: &swc::Class) -> Result<Class> {
    let mut body = Vec::new();

    for member in &class.body {
        match member {
            swc::ClassMember::Method(method) => body.push(ClassElement::Method(Method {
                key: ClassKey::Public(property_key(cx, &method.key)?),
                kind: method_kind(method.kind),
                function: Box::new(function(cx, &method.function)?),
                is_static: method.is_static,
            })),

            swc::ClassMember::PrivateMethod(method) => body.push(ClassElement::Method(Method {
                key: ClassKey::Private(cx.name(&method.key.name)),
                kind: method_kind(method.kind),
                function: Box::new(function(cx, &method.function)?),
                is_static: method.is_static,
            })),

            swc::ClassMember::Constructor(constructor) => {
                body.push(ClassElement::Method(constructor_member(cx, constructor)?));
            }

            swc::ClassMember::ClassProp(property) => body.push(ClassElement::Field(Field {
                key: ClassKey::Public(property_key(cx, &property.key)?),
                value: match &property.value {
                    Some(value) => Some(expr(cx, value)?),
                    None => None,
                },
                is_static: property.is_static,
            })),

            swc::ClassMember::PrivateProp(property) => body.push(ClassElement::Field(Field {
                key: ClassKey::Private(cx.name(&property.key.name)),
                value: match &property.value {
                    Some(value) => Some(expr(cx, value)?),
                    None => None,
                },
                is_static: property.is_static,
            })),

            swc::ClassMember::StaticBlock(block) => {
                body.push(ClassElement::StaticBlock(stmts(cx, &block.body.stmts)?));
            }

            swc::ClassMember::Empty(_) => {}

            swc::ClassMember::AutoAccessor(accessor) => {
                return unsupported("an auto-accessor", position(accessor.span));
            }
            swc::ClassMember::TsIndexSignature(_) => {}
        }
    }

    Ok(Class {
        name: None,
        heritage: match &class.super_class {
            Some(parent) => Some(expr(cx, parent)?),
            None => None,
        },
        body,
        at: position(class.span),
    })
}

/// The constructor, which SWC gives its own node and the tree does not.
fn constructor_member(cx: &mut Cx, constructor: &swc::Constructor) -> Result<Method> {
    let mut declared = Vec::new();
    let mut rest = None;

    for parameter in &constructor.params {
        let pattern = match parameter {
            swc::ParamOrTsParamProp::Param(param) => &param.pat,
            swc::ParamOrTsParamProp::TsParamProp(property) => {
                // `constructor(private x: number)` declares a field as a side
                // effect of a parameter. Refused rather than approximated: it
                // adds a class element the tree would not show.
                return unsupported("a parameter property", position(property.span));
            }
        };
        if let swc::Pat::Rest(spread) = pattern {
            rest = Some(binding(cx, &spread.arg)?);
            continue;
        }
        declared.push(parameter_of(cx, pattern)?);
    }

    let (directives, body) = block_body(cx, constructor.body.as_ref())?;

    Ok(Method {
        key: ClassKey::Public(property_key(cx, &constructor.key)?),
        kind: MethodKind::Normal,
        function: Box::new(Function {
            name: None,
            parameters: declared,
            rest_parameter: rest,
            directives,
            body,
            returns: None,
            captures_this: false,
            is_async: false,
            is_generator: false,
            at: position(constructor.span),
        }),
        is_static: false,
    })
}

fn method_kind(kind: swc::MethodKind) -> MethodKind {
    match kind {
        swc::MethodKind::Method => MethodKind::Normal,
        swc::MethodKind::Getter => MethodKind::Getter,
        swc::MethodKind::Setter => MethodKind::Setter,
    }
}

/// One declared parameter: its pattern, its default, and what it claims to be.
///
/// Shared by the ordinary parameter reader and the constructor's, which stated
/// it identically. The three lines look mechanical and are not: the claim is
/// read from the pattern *before* the pattern is consumed, and a copy that
/// reordered those two would take the annotation off the wrong node.
fn parameter_of(cx: &mut Cx, pattern: &swc::Pat) -> Result<Parameter> {
    let claim = type_of(cx, pattern);
    let element = binding_element(cx, pattern)?;
    Ok(Parameter {
        target: element.pattern,
        default: element.default,
        claim,
    })
}
