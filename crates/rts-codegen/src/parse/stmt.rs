//! Statements and declarations.
//!
//! Split out of `item.rs` for this crate's 1000-line ceiling (rule 8), which
//! that file was 395 lines past. The cut is where the subject changes: this is
//! what a statement can BE, and `item.rs` keeps what a function and a class
//! are — the two things a statement can declare but not the same question.

use swc_common::Spanned;
use swc_ecma_ast as swc;

use super::expr::expr;
use super::item::{
    class_parts, decorated_class_declaration, enum_declaration, function_parts,
    namespace_declaration, type_of,
};
use super::pat::{binding, target};
use super::{Cx, Result, position, unsupported};
use crate::syntax::{
    Class, Function,
    Binding, BindingKind, Catch, ForEachSource, ForEachTarget, ForInit, Stmt, StmtKind,
    SwitchClause,
};
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

pub(super) fn stmts(cx: &mut Cx, list: &[swc::Stmt]) -> Result<Vec<Stmt>> {
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
pub(super) fn decl(cx: &mut Cx, declaration: &swc::Decl) -> Result<Stmt> {
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

