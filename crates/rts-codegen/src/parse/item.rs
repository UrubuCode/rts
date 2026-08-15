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
use crate::syntax::{AssignOp, AssignTarget, Expr, ExprKind, Literal, LogicalOp, Spreadable};

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
        // Read rather than refused. The bridge had nowhere to put it, which
        // made a construct SWC reads perfectly well look like one the front end
        // could not parse — and moved the gap away from where it is. Where it
        // is, is disposal: emission refuses it by name.
        swc::ForHead::UsingDecl(using) => {
            let Some(first) = using.decls.first() else {
                return unsupported(
                    "a `using` for-head that declares nothing",
                    position(using.span),
                );
            };
            let swc::Pat::Ident(ident) = &first.name else {
                return unsupported("a `using` for-head with a pattern", position(using.span));
            };
            ForEachTarget::Dispose {
                target: cx.name(&ident.id.sym),
                is_async: using.is_await,
            }
        }
    })
}

/// A declaration, which is a statement here.
fn decl(cx: &mut Cx, declaration: &swc::Decl) -> Result<Stmt> {
    let at = position(declaration.span());
    let kind = match declaration {
        // `declare const x: T` states that something EXISTS elsewhere; it
        // introduces no binding and emits nothing, which is the whole meaning of
        // the keyword. Lowering it as an ordinary declaration bound `x` to
        // `undefined` in the enclosing scope — and since the thing being
        // declared is almost always a global, the binding SHADOWED the very
        // value it was announcing.
        //
        // The failure reads as the global not existing: `declare const print`
        // followed by `print(x)` died with "print is not a function", while the
        // same call one line above the declaration worked. `declare function f`
        // was never affected, because a function with no body is already
        // nothing to emit — which is why this looked like a global that only
        // sometimes existed.
        swc::Decl::Var(variables) if variables.declare => StmtKind::Empty,
        swc::Decl::Var(variables) => StmtKind::Declare {
            kind: binding_kind(variables.kind),
            bindings: declarators(cx, &variables.decls)?,
        },
        swc::Decl::Fn(function) => StmtKind::Function(Box::new(Function {
            name: Some(cx.name(&function.ident.sym)),
            ..function_parts(cx, &function.function)?
        })),
        swc::Decl::Class(class) if class.class.decorators.is_empty() => {
            StmtKind::Class(Box::new(Class {
                name: Some(cx.name(&class.ident.sym)),
                ..class_parts(cx, &class.class)?
            }))
        }
        swc::Decl::Class(class) => return decorated_class_declaration(cx, class, at),
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
                swc::Decl::TsEnum(held) => return enum_declaration(cx, held),
                swc::Decl::TsModule(module) => return namespace_declaration(cx, module, at),
                _ => unreachable!("all `Decl` variants are matched above"),
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
///
/// A leading `this` is dropped, because TypeScript's `function f(this: T, x: U)`
/// declares the type of the RECEIVER and not a first argument. Keeping it shifted
/// every real parameter one slot along, and the damage was quiet: `f.call(o, 1)`
/// bound `this` to `1` and `x` to nothing, so `this.a + x` answered `NaN` instead
/// of the sum — a wrong number from a function whose body reads correctly. It was
/// mistaken for a `bind` defect first, because `bind` is where it was noticed.
///
/// Only the first one, and only spelled `this`: the language reserves the word,
/// so no JavaScript program can declare a parameter by that name and there is
/// nothing else this can swallow. Dropped in this function rather than at the
/// three call sites — a function, an arrow and a method all arrive here, and the
/// arrow is exactly the one that cannot have a receiver parameter at all.
fn parameters<'a>(
    cx: &mut Cx,
    params: impl Iterator<Item = &'a swc::Pat>,
) -> Result<(Vec<Parameter>, Option<Pattern>)> {
    let mut declared = Vec::new();
    let mut rest = None;

    for (position, parameter) in params.enumerate() {
        if position == 0 && is_receiver_annotation(parameter) {
            continue;
        }
        if let swc::Pat::Rest(spread) = parameter {
            rest = Some(binding(cx, &spread.arg)?);
            continue;
        }
        declared.push(parameter_of(cx, parameter)?);
    }

    Ok((declared, rest))
}

/// Whether this parameter is TypeScript's `this` annotation rather than an
/// argument.
fn is_receiver_annotation(parameter: &swc::Pat) -> bool {
    matches!(parameter, swc::Pat::Ident(ident) if &*ident.id.sym == "this")
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
    cx.enter_class(private_names_of(class));
    let parts = class_members(cx, class);
    cx.leave_class();
    parts
}

/// The private names a class body declares, before any of it is parsed.
///
/// Its own pass because the order a body is written in is not the order it is
/// resolved in: `class C { m() { return this.#y; } #y = 1; }` reads `#y` in a
/// member parsed before the one that declares it, and a set filled as members
/// are walked would be empty at that read.
fn private_names_of(class: &swc::Class) -> std::collections::HashSet<String> {
    class
        .body
        .iter()
        .filter_map(|member| match member {
            swc::ClassMember::PrivateMethod(method) => Some(method.key.name.to_string()),
            swc::ClassMember::PrivateProp(property) => Some(property.key.name.to_string()),
            _ => None,
        })
        .collect()
}

fn class_members(cx: &mut Cx, class: &swc::Class) -> Result<Class> {
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
                key: ClassKey::Private(cx.private_name(&method.key.name)),
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
                claim: super::members::field_claim(cx, property.type_ann.as_deref()),
            })),

            swc::ClassMember::PrivateProp(property) => body.push(ClassElement::Field(Field {
                key: ClassKey::Private(cx.private_name(&property.key.name)),
                value: match &property.value {
                    Some(value) => Some(expr(cx, value)?),
                    None => None,
                },
                is_static: property.is_static,
                claim: super::members::field_claim(cx, property.type_ann.as_deref()),
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
    // The names a parameter property declares, in the order written — which is
    // the order they are assigned, and therefore the order two of them writing
    // the same field resolve in.
    let mut assigned: Vec<crate::names::Name> = Vec::new();

    for parameter in &constructor.params {
        let pattern = match parameter {
            swc::ParamOrTsParamProp::Param(param) => &param.pat,
            // `constructor(private x: number)` declares a field as a side
            // effect of a parameter. It IS a parameter and an assignment, and
            // that is what it becomes: the parameter is declared like any other
            // and `this.x = x` is prepended to the body.
            //
            // A desugaring here rather than a class element in the tree, for the
            // reason the enum above gives: the rule is settled before anything
            // runs, and a node would carry it into the emitter to be decided a
            // second time.
            swc::ParamOrTsParamProp::TsParamProp(property) => {
                match &property.param {
                    swc::TsParamPropParam::Ident(ident) => {
                        assigned.push(cx.names.intern(ident.id.sym.as_ref()));
                        &swc::Pat::Ident(ident.clone())
                    }
                    // `private x = 1` — the default belongs to the parameter,
                    // and the assignment reads whatever the parameter ended up
                    // holding, so the two compose without either knowing about
                    // the other.
                    swc::TsParamPropParam::Assign(assign) => {
                        let swc::Pat::Ident(ident) = &*assign.left else {
                            return unsupported(
                                "a parameter property that destructures",
                                position(property.span),
                            );
                        };
                        assigned.push(cx.names.intern(ident.id.sym.as_ref()));
                        &swc::Pat::Assign(assign.clone())
                    }
                }
            }
        };
        if let swc::Pat::Rest(spread) = pattern {
            rest = Some(binding(cx, &spread.arg)?);
            continue;
        }
        declared.push(parameter_of(cx, pattern)?);
    }

    let (directives, mut body) = block_body(cx, constructor.body.as_ref())?;
    if !assigned.is_empty() {
        let at = position(constructor.span);
        let writes: Vec<Stmt> = assigned
            .into_iter()
            .map(|name| field_assignment(name, at))
            .collect();
        // AFTER a leading `super(...)` and not before it. In a derived class
        // `this` does not exist until `super()` answers one, so an assignment
        // ahead of it would write into nothing — and TypeScript places them
        // exactly here for that reason. With no `super()` there is nothing to
        // follow, so they go first.
        // A constructor always has a block body — a concise one is not legal —
        // so anything else here is a defect rather than a program.
        if let FunctionBody::Block(statements) = &mut body {
            let after = match statements.first() {
                Some(first) if is_super_call(first) => 1,
                _ => 0,
            };
            statements.splice(after..after, writes);
        }
    }

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

/// A TypeScript `enum`, as the statements it means.
///
/// # Why this is a desugaring in the parser and not a node in the tree
///
/// Because an enum is not a runtime concept: TypeScript's own emitter turns it
/// into an object and a series of assignments, and every rule about it — the
/// auto-increment, which members get a reverse mapping — is settled before
/// anything runs. A node would carry those rules into the emitter, where the
/// same decisions would be made a second time.
///
/// ```text
/// enum E { A, B = 5, C = "s" }
/// ```
/// becomes
/// ```text
/// var E = {};
/// E["A"] = 0;  E[0] = "A";
/// E["B"] = 5;  E[5] = "B";
/// E["C"] = "s";
/// ```
///
/// # The two rules a reader would get wrong
///
/// A REVERSE mapping exists only for a numeric member. A string member gets the
/// forward direction and nothing else, which is what makes `E[E.C]` `undefined`
/// there and `"B"` for a numeric one.
///
/// `var` and not `let`: the block below is a scope, and TypeScript's own output
/// puts the binding in the enclosing function. A `let` would make the enum
/// invisible one line after it was written.
///
/// # What is refused, and why by name
///
/// A member whose value is neither a numeric nor a string literal — `A = 1 << 2`
/// or `A = B`. TypeScript settles those at compile time through constant
/// folding this parser does not do, and emitting the expression instead would
/// be right for the forward direction and wrong for the reverse one, which
/// needs the VALUE to index by. A member that is right in one direction and
/// missing in the other is the shape a program finds out about late.
fn enum_declaration(cx: &mut Cx, declaration: &swc::TsEnumDecl) -> Result<Stmt> {
    let at = position(declaration.span);
    let name = cx.names.intern(declaration.id.sym.as_ref());
    let mut properties: Vec<crate::syntax::Property> = Vec::new();
    let mut next = 0.0;
    for member in &declaration.members {
        let member_name = match &member.id {
            swc::TsEnumMemberId::Ident(id) => id.sym.to_string(),
            swc::TsEnumMemberId::Str(text) => text.value.to_string_lossy().to_string(),
        };
        let (value, numeric) = match &member.init {
            None => (Literal::Number(next), Some(next)),
            Some(init) => match &**init {
                swc::Expr::Lit(swc::Lit::Num(number)) => {
                    (Literal::Number(number.value), Some(number.value))
                }
                swc::Expr::Lit(swc::Lit::Str(text)) => (
                    // Units, for the reason `parse::expr::text` states: an enum
                    // member's value is a string literal like any other, and a
                    // lossy conversion here would make `E.A` a different string
                    // from the one written.
                    Literal::String(
                        text.value
                            .as_wtf8()
                            .to_ill_formed_utf16()
                            .collect::<Vec<u16>>()
                            .into(),
                    ),
                    None,
                ),
                _ => {
                    return unsupported("an enum member that is not a literal", at);
                }
            },
        };
        if let Some(held) = numeric {
            next = held + 1.0;
        }
        properties.push(crate::syntax::Property::Value {
            key: crate::syntax::PropertyKey::Named(cx.names.intern(&member_name)),
            value: Expr {
                kind: ExprKind::Literal(value),
                at,
            },
            shorthand: false,
        });
        // Only a NUMERIC member is reachable by its value. A string one gets the
        // forward direction and nothing else, which is what makes `E[E.C]`
        // undefined for a string member and the member name for a numeric one.
        //
        // The key is the number as text because that is what a property key is:
        // `E[0]` and `E["0"]` are the same lookup, and writing the digits keeps
        // this a plain object rather than something with elements.
        if let Some(held) = numeric {
            properties.push(crate::syntax::Property::Value {
                key: crate::syntax::PropertyKey::Named(
                    cx.names.intern(&crate::syntax::number_to_key(held)),
                ),
                value: Expr {
                    kind: ExprKind::Literal(Literal::String(member_name.into())),
                    at,
                },
                shorthand: false,
            });
        }
    }
    // ONE statement, and a `var` rather than a block of assignments: a block is
    // a scope here, and a `var` inside one did not reach past it — the enum was
    // unbound on the next line. Every value is a literal, so the whole thing is
    // an object literal and the question does not arise.
    Ok(Stmt {
        kind: StmtKind::Declare {
            kind: BindingKind::Var,
            bindings: vec![Binding {
                target: Pattern::Name(name),
                value: Some(Expr {
                    kind: ExprKind::Object { properties },
                    at,
                }),
                claim: None,
            }],
        },
        at,
    })
}


/// `@d1 @d2 class C { … }` — the **legacy** `experimentalDecorators` design,
/// established from the fixtures rather than assumed: `decorator_method_tolerated`
/// requires method/property/parameter decorators to parse and do nothing at
/// runtime, and a class decorator receives one `i64` (the class, tagged
/// through the same untyped lane every native uses) and RETURNS one — both
/// are the legacy shape. The ES2022 standard form is a different, incompatible
/// contract (a class decorator there receives `(value, context)`, a `context`
/// object nothing here constructs), so it is not what these fixtures ask for.
/// `npx tsc` was not available in this environment ([reported: not on PATH]),
/// so this reads the two proposals' shapes from ECMA-262 / the TypeScript
/// decorators RFC rather than checking a real compiler's output.
///
/// Lowered to `var C; C = class C { … }; C = dN(C); … ; C = d1(C);` — a `var`
/// so the binding escapes a block the same way a namespace's does (see
/// `namespace_declaration`), and reassignment because a legacy decorator's
/// return value replaces the class it decorated. Applied bottom-up: the
/// decorator nearest the declaration runs first, which is what
/// `decorator_multiple` pins ("third", "second", "first").
///
/// One decision this MVP takes and states rather than hides: a decorator
/// **factory call** (`@entity("usuario")`) is evaluated once and NOT invoked a
/// second time on the class. Full legacy semantics call the factory to get
/// the actual decorator function and then call THAT on the class — but
/// `decorator_factory`'s own `entity` returns `0`, which is not callable, so
/// reading the fixture literally would make every factory-decorated class
/// throw. Read as the fixture's comment states it ("recebe args em
/// compile-time"), the call form is treated as the whole decoration: it runs
/// for its side effect and the class passes through unchanged. A bare
/// identifier (`@register`) has no such call to be "the whole decoration", so
/// it is invoked directly on the class, which is the one case that still
/// matches full legacy semantics exactly.
fn decorated_class_declaration(
    cx: &mut Cx,
    class: &swc::ClassDecl,
    at: rts_cranelift::fault::Position,
) -> Result<Stmt> {
    let name = cx.name(&class.ident.sym);
    let class_value = Class {
        name: Some(name),
        ..class_parts(cx, &class.class)?
    };

    let mut stmts = vec![
        Stmt::new(
            StmtKind::Declare {
                kind: BindingKind::Var,
                bindings: vec![Binding {
                    target: Pattern::Name(name),
                    value: None,
                    claim: None,
                }],
            },
            at,
        ),
        Stmt::new(
            StmtKind::Expr(Expr {
                kind: ExprKind::Assign {
                    target: AssignTarget::Place(Box::new(Expr {
                        kind: ExprKind::Ident(name),
                        at,
                    })),
                    value: Box::new(Expr {
                        kind: ExprKind::Class(Box::new(class_value)),
                        at,
                    }),
                    op: AssignOp::Plain,
                },
                at,
            }),
            at,
        ),
    ];

    for decorator in class.class.decorators.iter().rev() {
        let decorator_at = position(decorator.span);
        let converted = expr(cx, &decorator.expr)?;
        let is_factory_call = matches!(&*decorator.expr, swc::Expr::Call(_));
        let application = if is_factory_call {
            Stmt::new(StmtKind::Expr(converted), decorator_at)
        } else {
            Stmt::new(
                StmtKind::Expr(Expr {
                    kind: ExprKind::Assign {
                        target: AssignTarget::Place(Box::new(Expr {
                            kind: ExprKind::Ident(name),
                            at: decorator_at,
                        })),
                        value: Box::new(Expr {
                            kind: ExprKind::Call {
                                callee: Box::new(converted),
                                arguments: vec![Spreadable::Single(Expr {
                                    kind: ExprKind::Ident(name),
                                    at: decorator_at,
                                })],
                                optional: false,
                            },
                            at: decorator_at,
                        }),
                        op: AssignOp::Plain,
                    },
                    at: decorator_at,
                }),
                decorator_at,
            )
        };
        stmts.push(application);
    }

    Ok(Stmt::new(StmtKind::Block(stmts), at))
}

/// `namespace N { … }` lowers to what TypeScript itself lowers it to: an IIFE
/// that receives the namespace object and assigns each exported member onto
/// it, called with `N || (N = {})` — TypeScript's own emit for this — so a
/// second block with the same name merges into the first object instead of
/// replacing it. `var N;` is only emitted for the first block with a given
/// name (`Cx::declared_namespaces` tracks that across the whole file):
/// merging needs exactly one hoisted declaration, and a second, unconditional
/// one would still be legal `var` re-declaration but is pointless to repeat.
///
/// Not attempted: a namespace referring to its own exports from inside its own
/// body (`namespace Net { export const A = 1; export const B = A; }`) — `A`
/// resolves to the outer, module-scope binding only after this lowering,
/// which is correct for these fixtures because none of them do that, and
/// wrong in general. Fixing it means rewriting internal references to the
/// object parameter, which is `scope/`'s job (absent — see this crate's
/// README) and not attempted here as a workaround.
fn namespace_declaration(
    cx: &mut Cx,
    module: &swc::TsModuleDecl,
    at: rts_cranelift::fault::Position,
) -> Result<Stmt> {
    if module.global {
        return unsupported("a `declare global` augmentation", at);
    }
    let name = match &module.id {
        swc::TsModuleName::Ident(ident) => cx.name(&ident.sym),
        swc::TsModuleName::Str(_) => {
            return unsupported("a string-named TypeScript module", at);
        }
    };
    let Some(body) = &module.body else {
        return unsupported("an ambient namespace with no body", at);
    };
    let swc::TsNamespaceBody::TsModuleBlock(block) = body else {
        return unsupported("a dotted namespace name (`namespace A.B`)", at);
    };

    let mut body_stmts = Vec::new();
    for item in &block.body {
        match item {
            swc::ModuleItem::Stmt(statement) => body_stmts.push(stmt(cx, statement)?),
            swc::ModuleItem::ModuleDecl(swc::ModuleDecl::ExportDecl(export)) => {
                let declared = decl(cx, &export.decl)?;
                let exported_names = names_bound_by(&declared.kind);
                body_stmts.push(declared);
                for exported in exported_names {
                    body_stmts.push(namespace_member_assignment(name, exported, at));
                }
            }
            _ => return unsupported("a namespace member that is not a declaration", at),
        }
    }

    let iife = Expr {
        kind: ExprKind::Call {
            callee: Box::new(Expr {
                kind: ExprKind::Function(Box::new(Function {
                    name: None,
                    parameters: vec![Parameter {
                        target: Pattern::Name(name),
                        default: None,
                        claim: None,
                    }],
                    rest_parameter: None,
                    directives: Vec::new(),
                    body: FunctionBody::Block(body_stmts),
                    returns: None,
                    captures_this: false,
                    is_async: false,
                    is_generator: false,
                    at,
                })),
                at,
            }),
            arguments: vec![Spreadable::Single(Expr {
                kind: ExprKind::Logical {
                    op: LogicalOp::Or,
                    left: Box::new(Expr {
                        kind: ExprKind::Ident(name),
                        at,
                    }),
                    right: Box::new(Expr {
                        kind: ExprKind::Assign {
                            target: AssignTarget::Place(Box::new(Expr {
                                kind: ExprKind::Ident(name),
                                at,
                            })),
                            value: Box::new(Expr {
                                kind: ExprKind::Object {
                                    properties: Vec::new(),
                                },
                                at,
                            }),
                            op: AssignOp::Plain,
                        },
                        at,
                    }),
                },
                at,
            })],
            optional: false,
        },
        at,
    };
    let call_stmt = Stmt::new(StmtKind::Expr(iife), at);

    if cx.declared_namespaces.insert(name) {
        let hoist = Stmt::new(
            StmtKind::Declare {
                kind: BindingKind::Var,
                bindings: vec![Binding {
                    target: Pattern::Name(name),
                    value: None,
                    claim: None,
                }],
            },
            at,
        );
        Ok(Stmt::new(StmtKind::Block(vec![hoist, call_stmt]), at))
    } else {
        Ok(call_stmt)
    }
}

/// Every name a declaration introduces at its own scope — what an `export`
/// inside a namespace body means: attach that name onto the namespace object
/// under the same key.
fn names_bound_by(kind: &StmtKind) -> Vec<crate::names::Name> {
    match kind {
        StmtKind::Declare { bindings, .. } => bindings
            .iter()
            .filter_map(|binding| match &binding.target {
                Pattern::Name(bound) => Some(*bound),
                _ => None,
            })
            .collect(),
        StmtKind::Function(function) => function.name.iter().copied().collect(),
        StmtKind::Class(class) => class.name.iter().copied().collect(),
        _ => Vec::new(),
    }
}

/// `N.member = member;` — publishing one exported name onto the namespace
/// object, by the parameter it is bound to inside the IIFE.
fn namespace_member_assignment(
    namespace: crate::names::Name,
    member: crate::names::Name,
    at: rts_cranelift::fault::Position,
) -> Stmt {
    Stmt {
        kind: StmtKind::Expr(Expr {
            kind: ExprKind::Assign {
                target: AssignTarget::Place(Box::new(Expr {
                    kind: ExprKind::Member {
                        object: Box::new(Expr {
                            kind: ExprKind::Ident(namespace),
                            at,
                        }),
                        property: member,
                        optional: false,
                    },
                    at,
                })),
                value: Box::new(Expr {
                    kind: ExprKind::Ident(member),
                    at,
                }),
                op: AssignOp::Plain,
            },
            at,
        }),
        at,
    }
}

/// `this.name = name;`, for a parameter property.
fn field_assignment(name: crate::names::Name, at: rts_cranelift::fault::Position) -> Stmt {
    Stmt {
        kind: StmtKind::Expr(Expr {
            kind: ExprKind::Assign {
                target: crate::syntax::AssignTarget::Place(Box::new(Expr {
                    kind: ExprKind::Member {
                        object: Box::new(Expr {
                            kind: ExprKind::This,
                            at,
                        }),
                        property: name,
                        optional: false,
                    },
                    at,
                })),
                value: Box::new(Expr {
                    kind: ExprKind::Ident(name),
                    at,
                }),
                op: crate::syntax::AssignOp::Plain,
            },
            at,
        }),
        at,
    }
}

/// Whether a statement is a bare `super(...)` call.
///
/// Read from the tree rather than from a flag on the constructor, because what
/// decides where a parameter property is assigned is whether the body STARTS
/// with one — a derived class whose author put a statement first is a program
/// TypeScript itself rejects, so there is no third case to get wrong.
fn is_super_call(statement: &Stmt) -> bool {
    let StmtKind::Expr(expr) = &statement.kind else {
        return false;
    };
    matches!(expr.kind, ExprKind::SuperCall { .. })
}
