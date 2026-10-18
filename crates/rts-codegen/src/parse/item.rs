//! Statements, functions, classes and modules.

use swc_ecma_ast as swc;

use super::expr::{expr, property_key};
use super::pat::{binding, binding_element};
use super::stmt::{decl, stmt, stmts};
use super::{Cx, Result, position, unsupported};
use crate::syntax::{AssignOp, AssignTarget, Expr, ExprKind, Literal, LogicalOp, Spreadable};
use crate::syntax::{
    Binding, BindingKind, Claim, Class, ClassElement, ClassKey, Directive, Field, Function,
    FunctionBody, Goal, Method, MethodKind, Parameter, Pattern, Program, Stmt, StmtKind,
};

/// A whole program.
pub(crate) fn program(cx: &mut Cx, parsed: &swc::Program, goal: Goal) -> Result<Program> {
    let mut items = Vec::new();
    let mut directives = Vec::new();

    match parsed {
        swc::Program::Module(module) => {
            cx.operators = operators_import(cx, module);
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
                // `None` where TypeScript erases the statement — see `import_decl`.
                if let Some(built) = super::module::module_item(cx, item)? {
                    items.push(built);
                }
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
                items.push(crate::syntax::ModuleItem::Stmt(super::stmt::stmt(
                    cx, statement,
                )?));
            }
        }
    }

    // `@operator("+")` in a module that never imported `operators`: the key it desugars to
    // needs the binding, so the module gets the import it would otherwise have had to write.
    if cx.operators_injected
        && let Some(local) = cx.operators
    {
        items.insert(
            0,
            crate::syntax::ModuleItem::Import(crate::syntax::Import {
                bindings: vec![crate::syntax::ImportBinding::Named {
                    exported: "operators".to_owned(),
                    local,
                }],
                source: "rts".to_owned(),
                attributes: Vec::new(),
                at: rts_cranelift::fault::Position::UNKNOWN,
            }),
        );
    }

    Ok(Program {
        goal,
        directives,
        body: items,
    })
}

/// The local a module's own `import { operators } from "rts"` binds, if it has one.
///
/// Read before any member is parsed, because a class written above the import (imports
/// hoist) still desugars `@operator` against it. Reusing the user's binding, rather than
/// always injecting a second one, keeps the module at exactly one `operators` import --
/// the shape `emit::operator_direct` requires to fire.
fn operators_import(cx: &mut Cx, module: &swc::Module) -> Option<crate::names::Name> {
    module.body.iter().find_map(|item| {
        let swc::ModuleItem::ModuleDecl(swc::ModuleDecl::Import(import)) = item else {
            return None;
        };
        if import.type_only || import.src.value.to_string_lossy() != "rts" {
            return None;
        }
        import.specifiers.iter().find_map(|specifier| match specifier {
            swc::ImportSpecifier::Named(named) if !named.is_type_only => {
                let imported = match &named.imported {
                    Some(swc::ModuleExportName::Ident(ident)) => ident.sym.to_string(),
                    Some(swc::ModuleExportName::Str(string)) => {
                        string.value.to_string_lossy().to_string()
                    }
                    None => named.local.sym.to_string(),
                };
                (imported == "operators").then(|| cx.name(&named.local.sym))
            }
            _ => None,
        })
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

/// A function declaration or expression.
pub(super) fn function_expr(cx: &mut Cx, function: &swc::FnExpr) -> Result<Function> {
    Ok(Function {
        name: function.ident.as_ref().map(|i| cx.name(&i.sym)),
        ..function_parts(cx, &function.function)?
    })
}

/// A method's function, which has no name of its own.
pub(crate) fn function(cx: &mut Cx, function: &swc::Function) -> Result<Function> {
    function_parts(cx, function)
}

pub(super) fn function_parts(cx: &mut Cx, function: &swc::Function) -> Result<Function> {
    let (mut parameters, rest_parameter) = parameters(cx, function.params.iter().map(|p| &p.pat))?;
    // `@repr(i32)` ON A PARAMETER: `function f(@repr(i32) x: number)`. A parameter decorator
    // is the spelling the parser carries cleanly -- swc keeps it on `Param`, beside the
    // pattern -- and it names the one value it applies to, where a function decorator
    // would have to name parameters by string. `parameters` skips a `this` receiver and a
    // rest element, so the flags are matched to the survivors in the same order.
    let mut declared = function
        .params
        .iter()
        .enumerate()
        .filter(|(position, p)| {
            !(*position == 0 && is_receiver_annotation(&p.pat)) && !matches!(p.pat, swc::Pat::Rest(_))
        })
        .map(|(_, p)| p);
    for parameter in parameters.iter_mut() {
        let Some(param) = declared.next() else {
            break;
        };
        parameter.repr_i32 = repr_i32_of(&param.decorators)?;
    }
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
        advertised_length: None,
        intrinsic: None,
        inline: false,
        is_generator: function.is_generator,
        at: position(function.span),
    })
}

/// Whether a parameter carries `@repr(i32)` (the argument may also be the string `"i32"`).
///
/// Any other decorator is left alone, as `intrinsic_of` leaves them. A `@repr(...)` naming
/// a representation this compiler does not have is a parse error, never a silent drop: a
/// misspelt `@repr(i23)` would otherwise compile to ordinary f64 arithmetic while the
/// author believed it wraps.
fn repr_i32_of(decorators: &[swc::Decorator]) -> Result<bool> {
    let mut found = false;
    for decorator in decorators {
        let swc::Expr::Call(call) = &*decorator.expr else {
            continue;
        };
        let swc::Callee::Expr(callee) = &call.callee else {
            continue;
        };
        let swc::Expr::Ident(ident) = &**callee else {
            continue;
        };
        if &*ident.sym != "repr" {
            continue;
        }
        let malformed = || {
            super::ParseError::Syntax(
                "@repr takes exactly one argument naming a representation, e.g. @repr(i32)".into(),
            )
        };
        let [argument] = call.args.as_slice() else {
            return Err(malformed());
        };
        if argument.spread.is_some() {
            return Err(malformed());
        }
        let named = match &*argument.expr {
            swc::Expr::Ident(name) => name.sym.to_string(),
            swc::Expr::Lit(swc::Lit::Str(name)) => name.value.to_string_lossy().into_owned(),
            _ => return Err(malformed()),
        };
        if named != "i32" {
            return Err(super::ParseError::Syntax(format!("@repr({named}) names no representation (only i32)")));
        }
        found = true;
    }
    Ok(found)
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
        advertised_length: None,
        intrinsic: None,
        inline: false,
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
        advertised_length: None,
        intrinsic: None,
        inline: false,
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

    let body = super::using::lower(cx, body);
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
pub(super) fn type_of(cx: &mut Cx, pattern: &swc::Pat) -> Option<Claim> {
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
/// A class written as an EXPRESSION.
///
/// A decorator on one is refused rather than dropped. TypeScript refuses the
/// same program — decorators are a declaration form there — and dropping one
/// silently is the failure this engine had: a registry that never happens and a
/// program that carries on. `parse::decorator`'s desugaring needs a binding to
/// reassign, which an expression has not got.
pub(crate) fn class_expr(cx: &mut Cx, class: &swc::ClassExpr) -> Result<Class> {
    if super::decorator::is_decorated_class(&class.class) {
        return unsupported("a decorator on a class expression", position(class.class.span));
    }
    Ok(Class {
        name: class.ident.as_ref().map(|i| cx.name(&i.sym)),
        ..class_parts(cx, &class.class)?
    })
}

pub(crate) fn class_parts(cx: &mut Cx, class: &swc::Class) -> Result<Class> {
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

/// Whether a class member is only a type, and so contributes nothing that runs.
///
/// A method with no BODY is an overload signature, or a member of a
/// `declare class`. TypeScript erases it and keeps the implementation, which is
/// written last — so the member that stays is the one the signatures describe.
///
/// This is the rule `parse::stmt::decl` applies to a bodyless function
/// declaration, stated twice because a class member is a different SWC node
/// rather than a different rule. Until 2026-10-03 neither place applied it, and
/// `block_body` gave the signature `FunctionBody::Block(vec![])` — a real
/// method returning `undefined`.
///
/// That answered WRONGLY with nothing failing. The signatures put one key into
/// the class's shape several times, and which function ran depended on how the
/// call was compiled rather than on what was written. Measured 2026-10-03
/// against bun 1.4.0, one signature over one implementation:
///
/// ```text
/// new K().go("a")                 → "impl:a"   — resolved by name, last wins
/// const k = new K(); k.go("a")    → undefined  — a cached access, first slot
/// ```
///
/// Rejected: making a duplicate shape key keep the last slot. That describes a
/// class body which cannot exist once the signatures are erased, and the shape
/// is the machine's question anyway (rule 2) — erasure is the language's.
fn member_is_signature_only(member: &swc::ClassMember) -> bool {
    match member {
        swc::ClassMember::Method(method) => method.function.body.is_none(),
        swc::ClassMember::PrivateMethod(method) => method.function.body.is_none(),
        swc::ClassMember::Constructor(constructor) => constructor.body.is_none(),
        _ => false,
    }
}

fn class_members(cx: &mut Cx, class: &swc::Class) -> Result<Class> {
    let mut body = Vec::new();

    for member in &class.body {
        if member_is_signature_only(member) {
            continue;
        }
        match member {
            swc::ClassMember::Method(method) => body.push(ClassElement::Method(Method {
                key: match super::decorator::operator_key(cx, method)? {
                    Some(key) => ClassKey::Public(crate::syntax::PropertyKey::Computed(key)),
                    None => ClassKey::Public(property_key(cx, &method.key)?),
                },
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
            advertised_length: None,
            intrinsic: None,
            inline: false,
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
        repr_i32: false,
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
/// # What folds, what runs, what is refused
///
/// `fold_enum` evaluates a constant initializer the way TypeScript does (see
/// there for the forms). Every member known now and a first declaration: the
/// object literal above. A computed member (`F = "abc".length`) or a block
/// merging into an enum already declared: TypeScript's own IIFE emit over
/// `E || {}`, which `enum_run_time` builds. Refused: a `const enum` member that
/// does not fold (TS2474), and a member with no initializer after a computed one.
pub(super) fn enum_declaration(cx: &mut Cx, declaration: &swc::TsEnumDecl) -> Result<Stmt> {
    let at = position(declaration.span);
    let name = cx.names.intern(declaration.id.sym.as_ref());
    let enum_name = declaration.id.sym.to_string();

    // The constant folding TypeScript does, member by member. A member is either
    // a value known now or `None`, which is a run-time initializer.
    let mut folded: Vec<(String, Option<Folded>)> = Vec::new();
    let mut next: Option<f64> = Some(0.0);
    for member in &declaration.members {
        let member_name = match &member.id {
            swc::TsEnumMemberId::Ident(id) => id.sym.to_string(),
            swc::TsEnumMemberId::Str(text) => text.value.to_string_lossy().to_string(),
        };
        let value = match &member.init {
            None => match next {
                Some(held) => Some(Folded::Number(held)),
                None => {
                    return unsupported("an enum member with no initializer after a computed one", at);
                }
            },
            Some(init) => fold_enum(init, &enum_name, &folded),
        };
        if declaration.is_const && value.is_none() {
            // TypeScript refuses it too (TS2474): a const enum is erased, so its
            // members must all be known while compiling.
            return unsupported("a const enum member that is not a constant expression", at);
        }
        next = match &value {
            Some(Folded::Number(held)) => Some(held + 1.0),
            _ => None,
        };
        folded.push((member_name, value));
    }

    let first = cx.declared_namespaces.insert(name);
    if first && folded.iter().all(|(_, value)| value.is_some()) {
        return Ok(enum_object_literal(cx, name, &folded, at));
    }
    enum_run_time(cx, name, declaration, &folded, at)
}

/// A value an enum member's initializer folded to.
#[derive(Clone)]
enum Folded {
    Number(f64),
    Text(Vec<u16>),
}

/// Folds an enum initializer to the value TypeScript's own constant evaluation
/// gives it, or `None` where it does not fold and is left to run time.
///
/// What folds: numeric and string literals, a template with no substitutions,
/// parentheses, unary `+ - ~`, binary `+ - * / % << >> >>> & | ^ **` (and `+`
/// joining text), and a reference to an EARLIER member of this block, bare or
/// as `E.A` / `E["A"]`. A result that is not finite is left to run time, which
/// is what TypeScript does with it.
fn fold_enum(node: &swc::Expr, enum_name: &str, earlier: &[(String, Option<Folded>)]) -> Option<Folded> {
    let lookup = |wanted: &str| -> Option<Folded> {
        earlier
            .iter()
            .rev()
            .find(|(held, _)| held == wanted)
            .and_then(|(_, value)| value.clone())
    };
    let folded = match node {
        swc::Expr::Lit(swc::Lit::Num(number)) => Folded::Number(number.value),
        swc::Expr::Lit(swc::Lit::Str(text)) => Folded::Text(
            text.value.as_wtf8().to_ill_formed_utf16().collect::<Vec<u16>>(),
        ),
        swc::Expr::Tpl(template) if template.exprs.is_empty() && template.quasis.len() == 1 => {
            let cooked = template.quasis[0].cooked.as_ref()?;
            Folded::Text(cooked.as_wtf8().to_ill_formed_utf16().collect::<Vec<u16>>())
        }
        swc::Expr::Paren(inner) => fold_enum(&inner.expr, enum_name, earlier)?,
        swc::Expr::Ident(id) => lookup(id.sym.as_ref())?,
        swc::Expr::Member(member) => {
            let swc::Expr::Ident(object) = &*member.obj else {
                return None;
            };
            if object.sym.as_ref() != enum_name {
                return None;
            }
            match &member.prop {
                swc::MemberProp::Ident(prop) => lookup(prop.sym.as_ref())?,
                swc::MemberProp::Computed(computed) => match &*computed.expr {
                    swc::Expr::Lit(swc::Lit::Str(text)) => lookup(&text.value.to_string_lossy())?,
                    _ => return None,
                },
                _ => return None,
            }
        }
        swc::Expr::Unary(unary) => {
            let Folded::Number(operand) = fold_enum(&unary.arg, enum_name, earlier)? else {
                return None;
            };
            Folded::Number(match unary.op {
                swc::UnaryOp::Plus => operand,
                swc::UnaryOp::Minus => -operand,
                swc::UnaryOp::Tilde => f64::from(!to_int32(operand)),
                _ => return None,
            })
        }
        swc::Expr::Bin(binary) => {
            let left = fold_enum(&binary.left, enum_name, earlier)?;
            let right = fold_enum(&binary.right, enum_name, earlier)?;
            match (left, right) {
                (Folded::Number(a), Folded::Number(b)) => Folded::Number(match binary.op {
                    swc::BinaryOp::Add => a + b,
                    swc::BinaryOp::Sub => a - b,
                    swc::BinaryOp::Mul => a * b,
                    swc::BinaryOp::Div => a / b,
                    swc::BinaryOp::Mod => a % b,
                    swc::BinaryOp::LShift => f64::from(to_int32(a).wrapping_shl(to_int32(b) as u32 & 31)),
                    swc::BinaryOp::RShift => f64::from(to_int32(a) >> (to_int32(b) as u32 & 31)),
                    swc::BinaryOp::ZeroFillRShift => {
                        f64::from((to_int32(a) as u32) >> (to_int32(b) as u32 & 31))
                    }
                    swc::BinaryOp::BitAnd => f64::from(to_int32(a) & to_int32(b)),
                    swc::BinaryOp::BitOr => f64::from(to_int32(a) | to_int32(b)),
                    swc::BinaryOp::BitXor => f64::from(to_int32(a) ^ to_int32(b)),
                    swc::BinaryOp::Exp => {
                        // `powf` answers 1 for `1 ** Infinity` and for a NaN
                        // exponent where JavaScript answers NaN.
                        if b.is_nan() || (a.abs() == 1.0 && b.is_infinite()) {
                            f64::NAN
                        } else {
                            a.powf(b)
                        }
                    }
                    _ => return None,
                }),
                (Folded::Text(a), Folded::Text(b)) if matches!(binary.op, swc::BinaryOp::Add) => {
                    Folded::Text([a, b].concat())
                }
                _ => return None,
            }
        }
        _ => return None,
    };
    match folded {
        Folded::Number(held) if !held.is_finite() => None,
        other => Some(other),
    }
}

/// `ToInt32`: the number modulo 2^32, as a signed 32-bit integer.
fn to_int32(value: f64) -> i32 {
    if !value.is_finite() {
        return 0;
    }
    (value.trunc().rem_euclid(4294967296.0) as u32) as i32
}

fn folded_literal(value: &Folded) -> Literal {
    match value {
        Folded::Number(held) => Literal::Number(*held),
        Folded::Text(units) => Literal::String(units.clone().into()),
    }
}

/// Every member known now: the enum is one object literal.
fn enum_object_literal(
    cx: &mut Cx,
    name: crate::names::Name,
    members: &[(String, Option<Folded>)],
    at: rts_cranelift::fault::Position,
) -> Stmt {
    let mut properties: Vec<crate::syntax::Property> = Vec::new();
    for (member_name, value) in members {
        let Some(value) = value else { continue };
        properties.push(crate::syntax::Property::Value {
            key: crate::syntax::PropertyKey::Named(cx.names.intern(member_name)),
            value: Expr {
                kind: ExprKind::Literal(folded_literal(value)),
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
        if let Folded::Number(held) = value {
            properties.push(crate::syntax::Property::Value {
                key: crate::syntax::PropertyKey::Named(
                    cx.names.intern(&crate::syntax::number_to_key(*held)),
                ),
                value: Expr {
                    kind: ExprKind::Literal(Literal::String(member_name.as_str().into())),
                    at,
                },
                shorthand: false,
            });
        }
    }
    // ONE statement, and a `var` rather than a block of assignments: a block is
    // a scope here, and a `var` inside one did not reach past it — the enum was
    // unbound on the next line.
    Stmt {
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
    }
}

/// A computed member, or a block merging into an enum already declared: TypeScript's
/// own emit,
///
/// ```text
/// var E = (function (E) { ...; return E; })(E || {});
/// ```
///
/// where each member is `var A = v; E["A"] = A; E[A] = "A";`. The local `var`
/// is what lets a later initializer name an earlier member bare, as the source
/// does. A known string member gets no reverse entry; a computed one always does,
/// because its type is not known until it runs.
fn enum_run_time(
    cx: &mut Cx,
    name: crate::names::Name,
    declaration: &swc::TsEnumDecl,
    members: &[(String, Option<Folded>)],
    at: rts_cranelift::fault::Position,
) -> Result<Stmt> {
    let ident = |name| Expr {
        kind: ExprKind::Ident(name),
        at,
    };
    let string = |text: &str| Expr {
        kind: ExprKind::Literal(Literal::String(text.into())),
        at,
    };
    let assign = |target: Expr, value: Expr| Expr {
        kind: ExprKind::Assign {
            target: AssignTarget::Place(Box::new(target)),
            value: Box::new(value),
            op: AssignOp::Plain,
        },
        at,
    };
    let index = |object: Expr, key: Expr| Expr {
        kind: ExprKind::Index {
            object: Box::new(object),
            index: Box::new(key),
            optional: false,
        },
        at,
    };
    let mut body = Vec::new();
    for (member, (member_name, value)) in declaration.members.iter().zip(members) {
        // A local only where the member is a plain identifier and cannot shadow
        // the enum object itself.
        let local = match &member.id {
            swc::TsEnumMemberId::Ident(id) if id.sym.as_ref() != declaration.id.sym.as_ref() => {
                Some(cx.names.intern(id.sym.as_ref()))
            }
            _ => None,
        };
        let initial = match value {
            Some(value) => Expr {
                kind: ExprKind::Literal(folded_literal(value)),
                at,
            },
            None => match &member.init {
                Some(init) => expr(cx, init)?,
                None => return unsupported("an enum member with no initializer", at),
            },
        };
        let held = match local {
            Some(local) => {
                body.push(Stmt::new(
                    StmtKind::Declare {
                        kind: BindingKind::Var,
                        bindings: vec![Binding {
                            target: Pattern::Name(local),
                            value: Some(initial),
                            claim: None,
                        }],
                    },
                    at,
                ));
                ident(local)
            }
            None => {
                // No local to hold it: assign the object slot and read it back.
                let slot = assign(index(ident(name), string(member_name)), initial);
                body.push(Stmt::new(StmtKind::Expr(slot), at));
                index(ident(name), string(member_name))
            }
        };
        if local.is_some() {
            body.push(Stmt::new(
                StmtKind::Expr(assign(index(ident(name), string(member_name)), held)),
                at,
            ));
        }
        if !matches!(value, Some(Folded::Text(_))) {
            let key = index(ident(name), string(member_name));
            body.push(Stmt::new(
                StmtKind::Expr(assign(index(ident(name), key), string(member_name))),
                at,
            ));
        }
    }
    body.push(Stmt::new(StmtKind::Return(Some(ident(name))), at));
    let function = Expr {
        kind: ExprKind::Function(Box::new(Function {
            name: None,
            parameters: vec![Parameter {
                target: Pattern::Name(name),
                default: None,
                claim: None,
                repr_i32: false,
            }],
            rest_parameter: None,
            directives: Vec::new(),
            body: FunctionBody::Block(body),
            returns: None,
            captures_this: false,
            is_async: false,
            advertised_length: None,
            intrinsic: None,
            inline: false,
            is_generator: false,
            at,
        })),
        at,
    };
    let argument = Expr {
        kind: ExprKind::Logical {
            op: LogicalOp::Or,
            left: Box::new(ident(name)),
            right: Box::new(Expr {
                kind: ExprKind::Object {
                    properties: Vec::new(),
                },
                at,
            }),
        },
        at,
    };
    Ok(Stmt {
        kind: StmtKind::Declare {
            kind: BindingKind::Var,
            bindings: vec![Binding {
                target: Pattern::Name(name),
                value: Some(Expr {
                    kind: ExprKind::Call {
                        callee: Box::new(function),
                        arguments: vec![Spreadable::Single(argument)],
                        optional: false,
                    },
                    at,
                }),
                claim: None,
            }],
        },
        at,
    })
}

/// Whether this module or namespace declaration is AMBIENT — a type-only
/// declaration TypeScript erases whole, body included.
///
/// Three forms are ambient, and the first was the one being refused:
/// `declare module "name" { … }` (module augmentation, which is the only way
/// a library extends an interface it does not own), `declare global { … }`,
/// and anything carrying the `declare` keyword (`declare namespace N`,
/// `declare module Legado`). A string-named module is ambient even without
/// the keyword, which is how a `.d.ts` writes one.
///
/// Erasing the WHOLE declaration is the point rather than a shortcut: an
/// `export const n: number` inside an ambient body declares a type and
/// nothing else, so lowering the body would bind `n` at run time where bun
/// and node leave it unbound. Measured 2026-10-03 against bun 1.4.0 —
/// `declare namespace N { const n: number }` answers `typeof N ===
/// "undefined"` there, where this engine used to build the namespace object.
fn is_ambient_module(module: &swc::TsModuleDecl) -> bool {
    module.declare || module.global || matches!(module.id, swc::TsModuleName::Str(_))
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
///
/// An AMBIENT declaration never reaches any of that: `is_ambient_module`
/// erases it first.
pub(super) fn namespace_declaration(
    cx: &mut Cx,
    module: &swc::TsModuleDecl,
    at: rts_cranelift::fault::Position,
) -> Result<Stmt> {
    if is_ambient_module(module) {
        return Ok(Stmt::new(StmtKind::Empty, at));
    }
    let name = match &module.id {
        swc::TsModuleName::Ident(ident) => cx.name(&ident.sym),
        swc::TsModuleName::Str(_) => {
            // Unreachable: a string-named module is ambient by construction and
            // `is_ambient_module` erased it above. Kept as a refusal rather than
            // an `unreachable!` because the discriminator is swc's and a parser
            // upgrade is free to admit a form this one has not seen.
            return unsupported("a string-named TypeScript module", at);
        }
    };
    let Some(body) = &module.body else {
        return unsupported("an ambient namespace with no body", at);
    };
    lower_namespace(cx, name, body, None, at)
}

/// One namespace level. `parent` is the enclosing namespace's parameter when
/// this level is itself a member (`namespace A.B.C`, or `export namespace B`
/// inside `A`): TypeScript's emit then receives `B = A.B || (A.B = {})` and
/// declares `var B` inside the enclosing IIFE, so a second `namespace A.B`
/// merges into the same `A.B` object.
///
/// A dotted name `A.B.C { body }` is `A { B { C { body } } }`, each inner level
/// exported into its parent, and only the innermost carries the body.
fn lower_namespace(
    cx: &mut Cx,
    name: crate::names::Name,
    body: &swc::TsNamespaceBody,
    parent: Option<crate::names::Name>,
    at: rts_cranelift::fault::Position,
) -> Result<Stmt> {
    let mut body_stmts = Vec::new();
    let block = match body {
        swc::TsNamespaceBody::TsModuleBlock(block) => block,
        swc::TsNamespaceBody::TsNamespaceDecl(inner) => {
            let inner_name = cx.name(&inner.id.sym);
            body_stmts.push(lower_namespace(
                cx,
                inner_name,
                &inner.body,
                Some(name),
                at,
            )?);
            return finish_namespace(cx, name, body_stmts, parent, at);
        }
    };
    for item in &block.body {
        match item {
            swc::ModuleItem::Stmt(statement) => body_stmts.push(stmt(cx, statement)?),
            swc::ModuleItem::ModuleDecl(swc::ModuleDecl::ExportDecl(export))
                if matches!(&export.decl, swc::Decl::TsModule(_)) =>
            {
                let swc::Decl::TsModule(nested) = &export.decl else {
                    unreachable!()
                };
                if is_ambient_module(nested) {
                    continue;
                }
                let swc::TsModuleName::Ident(ident) = &nested.id else {
                    return unsupported("a string-named TypeScript module", at);
                };
                let Some(nested_body) = &nested.body else {
                    return unsupported("an ambient namespace with no body", at);
                };
                let nested_name = cx.name(&ident.sym);
                body_stmts.push(lower_namespace(
                    cx,
                    nested_name,
                    nested_body,
                    Some(name),
                    at,
                )?);
            }
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
    finish_namespace(cx, name, body_stmts, parent, at)
}

/// Wraps a lowered namespace body in its IIFE and the hoisted `var`.
fn finish_namespace(
    cx: &mut Cx,
    name: crate::names::Name,
    body_stmts: Vec<Stmt>,
    parent: Option<crate::names::Name>,
    at: rts_cranelift::fault::Position,
) -> Result<Stmt> {
    let member_of = |parent: crate::names::Name| Expr {
        kind: ExprKind::Member {
            object: Box::new(Expr {
                kind: ExprKind::Ident(parent),
                at,
            }),
            property: name,
            optional: false,
        },
        at,
    };
    let argument = match parent {
        // `B = A.B || (A.B = {})`
        Some(parent) => Expr {
            kind: ExprKind::Assign {
                target: AssignTarget::Place(Box::new(Expr {
                    kind: ExprKind::Ident(name),
                    at,
                })),
                value: Box::new(Expr {
                    kind: ExprKind::Logical {
                        op: LogicalOp::Or,
                        left: Box::new(member_of(parent)),
                        right: Box::new(Expr {
                            kind: ExprKind::Assign {
                                target: AssignTarget::Place(Box::new(member_of(parent))),
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
                }),
                op: AssignOp::Plain,
            },
            at,
        },
        None => Expr {
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
        },
    };

    let iife = Expr {
        kind: ExprKind::Call {
            callee: Box::new(Expr {
                kind: ExprKind::Function(Box::new(Function {
                    name: None,
                    parameters: vec![Parameter {
                        target: Pattern::Name(name),
                        default: None,
                        claim: None,
                        repr_i32: false,
                    }],
                    rest_parameter: None,
                    directives: Vec::new(),
                    body: FunctionBody::Block(body_stmts),
                    returns: None,
                    captures_this: false,
                    is_async: false,
                    advertised_length: None,
                    intrinsic: None,
                    inline: false,
                    is_generator: false,
                    at,
                })),
                at,
            }),
            arguments: vec![Spreadable::Single(argument)],
            optional: false,
        },
        at,
    };
    let call_stmt = Stmt::new(StmtKind::Expr(iife), at);

    if parent.is_some() || cx.declared_namespaces.insert(name) {
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
