//! The environment a method reads its `[[HomeObject]]` out of.
//!
//! # Why this is not inside `class.rs`
//!
//! Because a class body is not the only place a method has a home object. An
//! object literal's shorthand method has one too — `{ m() { return super.m(); } }`
//! is legal, and `super` there means "one link above the LITERAL" — and the two
//! were emitted by different modules, so the literal simply had no `__rts_home`
//! to read and every such program died with `ReferenceError: __rts_home is not
//! defined`.
//!
//! Rule 3 of this crate's README is the reason the answer landed here rather
//! than being written a second time in `object.rs`: "a method resolves `super`
//! through an environment chained to the one it was written in" is one semantic
//! rule, and a second statement of it is the one that goes stale.
//!
//! # What a home object is NOT
//!
//! It is not the object `super` reads from — it is the object `super` reads
//! from the PROTOTYPE OF, looked up at call time rather than at definition
//! time. That is why the environment holds the literal (or the class's
//! `prototype`) itself and `class::super_lookup_root_and_receiver` walks one
//! link up: `Object.setPrototypeOf(literal, other)` after the fact retargets
//! every `super` written inside it, which is what the language says and what a
//! captured parent would have frozen.

use std::collections::BTreeSet;

use rts_cranelift::ir::{FuncBuilder, ValueId};

use super::{Ctx, EmitResult, Scope, binding, expr};
use crate::names::Name;
use crate::runtime::RuntimeOp;
use crate::syntax::{Expr, ExprKind, Function, FunctionBody, Stmt};

/// The name the parent constructor is held under.
///
/// Spelled so a program cannot write it. A collision would be harmless anyway —
/// such an environment is never handed to JavaScript — but naming it once is
/// what keeps the writer and the reader agreeing.
pub(super) const SUPER: &str = "__rts_super";

/// The name the home object is held under.
pub(super) const HOME: &str = "__rts_home";

/// The name the home object of a STATIC member is held under.
///
/// A second name rather than a second value under the first, because both are
/// live at once: a class body has instance methods and static ones, and each
/// resolves `super` against its own home. The specification says the same in
/// its own words — `[[HomeObject]]` is per function, and a static method's is
/// the constructor.
pub(super) const STATIC_HOME: &str = "__rts_static_home";

/// Builds an environment holding `entries`, chained to the one in force, and
/// answers the [`Scope`] a separately compiled method body is emitted against.
///
/// An entry whose value is `None` is HELD but not written: the class's home
/// object does not exist until the constructor has been made, so `class.rs`
/// writes it afterwards and needs the name bound at zero hops all the same —
/// a name written into an environment that no `Scope` records is invisible to
/// `Scope::for_function`, which seeds a nested body's bindings from
/// `reachable()`.
pub(super) fn environment_holding(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    entries: &[(Name, Option<ValueId>)],
) -> EmitResult<Scope> {
    let zero = builder.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
        repr: rts_cranelift::repr::Repr::I64,
        bits: rts_cranelift::ir::ScalarBits(0),
    });
    let zero = builder.use_const(zero);
    let environment = expr::call(builder, ctx, RuntimeOp::ObjectNew, &[zero])?[0];
    let outer = binding::outer_link(ctx);
    let handed = match scope.environment() {
        Some(environment) => environment,
        None => expr::undefined(builder, ctx),
    };
    super::property::emit_write(builder, ctx, environment, outer, handed)?;

    let mut held = BTreeSet::new();
    for &(name, value) in entries {
        if let Some(value) = value {
            super::property::emit_write(builder, ctx, environment, name, value)?;
        }
        held.insert(name);
    }

    // One link further out for everything the enclosing scope could reach,
    // because this environment sits between it and the methods.
    let reachable: Vec<(Name, u32)> = scope
        .reachable()
        .into_iter()
        .map(|(name, hops)| (name, hops + 1))
        .collect();
    // `held` IS the own level, and the two sets being one is the point rather
    // than a shortcut: every name in it is written into `environment` here or
    // by the caller, and there is no nested block to over-include from.
    Ok(Scope::for_function(
        Some(environment),
        held.clone(),
        &held,
        &reachable,
    ))
}

/// Whether a method body reaches `super` at all.
///
/// Asked so that an object literal pays for a home-object environment only when
/// one of its methods can read it — an allocation and two property writes per
/// literal is not a cost the overwhelmingly common `{ m() { … } }` should carry.
///
/// **Over-approximates, in the safe direction.** It descends into nested
/// functions and nested classes, which have `super` bindings of their own, so a
/// literal whose method contains an unrelated class with a `super` in it builds
/// an environment nothing reads. That costs an allocation; the opposite
/// over-approximation costs a `ReferenceError`, which is the failure this whole
/// module exists to remove.
pub(super) fn reaches_super(function: &Function) -> bool {
    let mut found = false;
    match &function.body {
        FunctionBody::Block(body) => {
            for statement in body {
                in_stmt(statement, &mut found);
            }
        }
        FunctionBody::Expression(value) => in_expr(value, &mut found),
    }
    found
}

fn in_stmt(statement: &Stmt, found: &mut bool) {
    if *found {
        return;
    }
    super::capture::walk_stmt(statement, &mut |child| match child {
        super::capture::StmtChild::Stmt(inner) => in_stmt(inner, found),
        super::capture::StmtChild::Expr(expr) => in_expr(expr, found),
        super::capture::StmtChild::Binding(binding) => {
            if let Some(value) = &binding.value {
                in_expr(value, found);
            }
        }
        super::capture::StmtChild::Catch(catch) => {
            for inner in &catch.body {
                in_stmt(inner, found);
            }
        }
        super::capture::StmtChild::Function(function) => {
            *found = *found || reaches_super(function);
        }
        super::capture::StmtChild::Class(class) => in_class(class, found),
    });
}

fn in_expr(expr: &Expr, found: &mut bool) {
    if *found {
        return;
    }
    if matches!(
        expr.kind,
        ExprKind::SuperMember { .. } | ExprKind::SuperCall { .. }
    ) {
        *found = true;
        return;
    }
    super::capture::walk_expr(expr, &mut |child| match child {
        super::capture::Child::Expr(inner) => in_expr(inner, found),
        super::capture::Child::Function(function) => {
            *found = *found || reaches_super(function);
        }
        super::capture::Child::Class(class) => in_class(class, found),
    });
}

fn in_class(class: &crate::syntax::Class, found: &mut bool) {
    for element in &class.body {
        match element {
            crate::syntax::ClassElement::Method(method) => {
                *found = *found || reaches_super(&method.function);
            }
            crate::syntax::ClassElement::Field(field) => {
                if let Some(value) = &field.value {
                    in_expr(value, found);
                }
            }
            crate::syntax::ClassElement::StaticBlock(body) => {
                for statement in body {
                    in_stmt(statement, found);
                }
            }
        }
    }
}
