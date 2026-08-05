//! A class, lowered into what a class already was.
//!
//! # Why there is no class in the runtime
//!
//! Because there is no class in the language either. A class is a constructor
//! function, an object hanging off its `prototype` property holding the methods,
//! and two prototype links when there is an `extends` — every one of which this
//! engine already had. Adding a runtime notion of "class" would be inventing a
//! second kind of object to express something the first kind already expresses,
//! and every operation that reached one would then have to know about both.
//!
//! So this module is a **lowering** and not a feature. What it produces is
//! indistinguishable from what the equivalent hand-written function produces,
//! which is also what makes `class X extends RegExp {}` work without anything in
//! the regular-expression module knowing that classes exist.
//!
//! # The four links, and why each one is needed
//!
//! ```text
//!   B.prototype.__proto__ = A.prototype     an instance finds A's methods
//!   B.__proto__           = A               B finds A's STATIC methods
//!   instance.__proto__    = B.prototype     `new` already does this
//!   home                  = B.prototype     `super.m` starts one link above it
//! ```
//!
//! The second is the one an implementation forgets, and forgetting it is
//! invisible until a program calls an inherited static method.
//!
//! # How `super` is reached
//!
//! Through an ordinary environment. The class builds one at definition time
//! holding the parent constructor and the home object, links it to the
//! environment the class was written in, and emits every method against it — so
//! `super.m()` is an environment read and a property read, and `super()` is an
//! environment read and a call. Nothing about the calling convention changes,
//! and a method that mentions neither pays for neither.
//!
//! That is also what the specification describes: a method's `[[HomeObject]]` is
//! an internal slot of the function, and an environment entry is the same fact
//! stored where this engine can already store facts.

use std::collections::BTreeSet;

use rts_cranelift::ir::{FuncBuilder, ValueId};

use super::{Ctx, EmitError, EmitResult, Scope};
use super::{binding, expr, function};
use crate::names::Name;
use crate::runtime::RuntimeOp;
use crate::syntax::{
    AssignOp, AssignTarget, Class, ClassElement, ClassKey, Expr, ExprKind, Function, FunctionBody,
    Method, MethodKind, PropertyKey, Stmt, StmtKind,
};

/// The name the parent constructor is held under.
///
/// Spelled so a program cannot write it. A collision would be harmless anyway —
/// a class environment is never handed to JavaScript — but naming it once is
/// what keeps the writer and the reader agreeing.
const SUPER: &str = "__rts_super";

/// The name the home object is held under.
const HOME: &str = "__rts_home";

/// Emits a class and answers the constructor value.
pub(super) fn emit_class(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    class: &Class,
) -> EmitResult<ValueId> {
    refuse_what_is_not_built(class)?;

    // The heritage is evaluated first and exactly once, before anything else in
    // the body — including a computed key, which is why the order is the
    // semantics rather than the tidy way to write it.
    let parent = match &class.heritage {
        Some(heritage) => Some(super::emit_expr(builder, scope, ctx, heritage)?),
        None => None,
    };

    let mut inner = class_scope(builder, scope, ctx, parent)?;
    let constructor = emit_constructor(builder, &mut inner, ctx, class)?;

    // `ClosureNew` already made a `prototype` object, because a function that
    // could not be constructed with would be a different kind of function. So
    // this reads what exists rather than making a second one — two would be the
    // classic bug where methods land on an object no instance inherits from.
    let prototype_name = ctx.names.intern("prototype");
    let prototype = expr::emit_read(builder, ctx, constructor, prototype_name)?;

    if let Some(parent) = parent {
        let parent_prototype = expr::emit_read(builder, ctx, parent, prototype_name)?;
        expr::call(
            builder,
            ctx,
            RuntimeOp::SetPrototype,
            &[prototype, parent_prototype],
        )?;
        // The link an implementation forgets, and whose absence shows only when
        // a program calls an inherited STATIC method.
        expr::call(builder, ctx, RuntimeOp::SetPrototype, &[constructor, parent])?;
    }

    // Written now rather than with the parent, because the prototype did not
    // exist until the constructor did.
    if let Some(environment) = inner.environment() {
        let home = ctx.names.intern(HOME);
        expr::emit_write(builder, ctx, environment, home, prototype)?;
    }

    for element in &class.body {
        match element {
            ClassElement::Method(method) if !method.is_constructor(&ctx.names) => {
                let name = named_key(&method.key)?;
                let closure = function::emit_closure(builder, &inner, ctx, &method.function)?;
                let target = if method.is_static { constructor } else { prototype };
                // An accessor is not written as a property: it is a pair of
                // functions the read has to CALL, and a getter stored in the
                // layout would be returned by the cache instead of run.
                match method.kind {
                    MethodKind::Normal => {
                        expr::emit_write(builder, ctx, target, name, closure)?;
                    }
                    MethodKind::Getter => {
                        super::object::define_accessor(builder, ctx, target, name, closure, true)?;
                    }
                    MethodKind::Setter => {
                        super::object::define_accessor(builder, ctx, target, name, closure, false)?;
                    }
                }
            }
            // A static field's initialiser runs once, here, with the class
            // already linked — which is what lets `static all = new X()` work.
            ClassElement::Field(field) if field.is_static => {
                let name = named_key(&field.key)?;
                let value = match &field.value {
                    Some(value) => super::emit_expr(builder, &mut inner, ctx, value)?,
                    None => expr::undefined(builder, ctx),
                };
                expr::emit_write(builder, ctx, constructor, name, value)?;
            }
            // An instance field runs per construction, so it is not emitted
            // here at all — see `with_fields`.
            _ => {}
        }
    }

    Ok(constructor)
}

/// The constructor function, declared or supplied.
///
/// # Why a class with no constructor still gets one
///
/// Because `new X()` has to call something, and because a derived class must
/// pass its arguments on. The specification supplies `constructor(...args) {
/// super(...args) }` for a derived class and an empty body otherwise, and this
/// supplies the same thing as a synthesised tree rather than as a special case
/// in the emitter — so the supplied constructor is emitted by exactly the code
/// that emits a written one.
fn emit_constructor(
    builder: &mut FuncBuilder,
    inner: &mut Scope,
    ctx: &mut Ctx,
    class: &Class,
) -> EmitResult<ValueId> {
    let declared = class.constructor(&ctx.names).map(|method| &method.function);
    let supplied;
    let function = match declared {
        Some(function) => {
            supplied = with_fields(class, function)?;
            &supplied
        }
        None => {
            let default = default_constructor(ctx, class);
            supplied = with_fields(class, &default)?;
            &supplied
        }
    };
    function::emit_closure(builder, inner, ctx, function)
}

/// A constructor with the instance fields written in front of its body.
///
/// # Why the fields are statements rather than a second mechanism
///
/// `x = 1` in a class body means `this.x = 1` per instance, and this engine
/// already emits that. Synthesising the assignment reuses the whole of property
/// writing, `this`, and expression emission — where a field initialiser
/// mechanism of its own would be a second path to the same store, differing
/// wherever somebody forgot to update both.
///
/// # The divergence, named
///
/// They run at the **start** of the constructor. The specification runs them
/// after `super()` returns in a derived class, because until then there is no
/// `this` — but there is one here, since `construct` makes the object before
/// calling. So a derived field initialiser that reads a property the parent
/// constructor sets reads it too early, and that is the one program this order
/// gets wrong.
fn with_fields(class: &Class, function: &Function) -> EmitResult<Function> {
    let mut prologue = Vec::new();
    for element in &class.body {
        let ClassElement::Field(field) = element else {
            continue;
        };
        if field.is_static {
            continue;
        }
        let name = named_key(&field.key)?;
        let value = match &field.value {
            Some(value) => value.clone(),
            // Declaring `x;` with no initialiser is not the same as never
            // declaring it: the property exists, which is what fixes the layout.
            None => Expr {
                kind: ExprKind::Literal(crate::syntax::Literal::Singleton(
                    crate::values::Singleton::Undefined,
                )),
                at: class.at,
            },
        };
        prologue.push(Stmt {
            kind: StmtKind::Expr(Expr {
                kind: ExprKind::Assign {
                    target: AssignTarget::Place(Box::new(Expr {
                        kind: ExprKind::Member {
                            object: Box::new(Expr {
                                kind: ExprKind::This,
                                at: class.at,
                            }),
                            property: name,
                            optional: false,
                        },
                        at: class.at,
                    })),
                    value: Box::new(value),
                    op: AssignOp::Plain,
                },
                at: class.at,
            }),
            at: class.at,
        });
    }
    if prologue.is_empty() {
        return Ok(function.clone());
    }
    let FunctionBody::Block(body) = &function.body else {
        // A constructor is never a concise arrow body — that is a grammar fact
        // rather than something to handle.
        return Err(EmitError::Unsupported {
            construct: "a class constructor written as an expression body",
        });
    };
    prologue.extend(body.iter().cloned());
    let mut with = function.clone();
    with.body = FunctionBody::Block(prologue);
    Ok(with)
}

/// `constructor(...) { super(...) }`, or an empty one.
///
/// The arguments are forwarded by name through parameters nothing else can
/// spell, which is what makes `new Derived(1, 2)` reach the parent at all. Four
/// of them, because that is the arity a call carries — a class whose parent
/// wanted a fifth is refused at the call rather than losing it here.
fn default_constructor(ctx: &mut Ctx, class: &Class) -> Function {
    let names: Vec<Name> = (0..crate::runtime::ARGUMENT_SLOTS)
        .map(|at| ctx.names.intern(&format!("__rts_arg{at}")))
        .collect();
    let body = match class.is_derived() {
        true => vec![Stmt {
            kind: StmtKind::Expr(Expr {
                kind: ExprKind::SuperCall {
                    arguments: names
                        .iter()
                        .map(|name| {
                            crate::syntax::Spreadable::Single(Expr {
                                kind: ExprKind::Ident(*name),
                                at: class.at,
                            })
                        })
                        .collect(),
                },
                at: class.at,
            }),
            at: class.at,
        }],
        false => Vec::new(),
    };
    Function {
        name: class.name,
        parameters: names
            .into_iter()
            .map(|name| crate::syntax::Parameter {
                target: crate::syntax::Pattern::Name(name),
                default: None,
                claim: None,
            })
            .collect(),
        rest_parameter: None,
        directives: Vec::new(),
        body: FunctionBody::Block(body),
        returns: None,
        captures_this: false,
        is_async: false,
        is_generator: false,
        at: class.at,
    }
}

/// The environment a class body is emitted against.
///
/// Built only for a derived class: `super` is a syntax error outside one, so a
/// plain class would allocate an object nothing could read.
fn class_scope(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    parent: Option<ValueId>,
) -> EmitResult<Scope> {
    let Some(parent) = parent else {
        // Not a copy of the enclosing scope: methods are emitted against it, and
        // `Scope` is not `Clone`. Rebuilt from what it can reach, which is
        // exactly what a nested function sees.
        return Ok(Scope::for_function(
            scope.environment(),
            BTreeSet::new(),
            &scope.reachable(),
        ));
    };

    let environment = expr::call(builder, ctx, RuntimeOp::ObjectNew, &[])?[0];
    let outer = binding::outer_link(ctx);
    let handed = match scope.environment() {
        Some(environment) => environment,
        None => expr::undefined(builder, ctx),
    };
    expr::emit_write(builder, ctx, environment, outer, handed)?;

    let super_name = ctx.names.intern(SUPER);
    expr::emit_write(builder, ctx, environment, super_name, parent)?;
    let home_name = ctx.names.intern(HOME);

    // One link further out for everything the enclosing scope could reach,
    // because this environment sits between it and the methods.
    let reachable: Vec<(Name, u32)> = scope
        .reachable()
        .into_iter()
        .map(|(name, hops)| (name, hops + 1))
        .collect();
    let mut held = BTreeSet::new();
    held.insert(super_name);
    held.insert(home_name);
    Ok(Scope::for_function(Some(environment), held, &reachable))
}

/// `super.x` — read from above the home object, with `this` as the receiver.
pub(super) fn emit_super_member(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    property: &PropertyKey,
) -> EmitResult<ValueId> {
    let PropertyKey::Named(name) = property else {
        return Err(EmitError::Unsupported {
            construct: "a computed `super[e]`",
        });
    };
    let home = ctx.names.intern(HOME);
    let home = binding::read(builder, scope, ctx, home)?;
    // Above the home object, not on it. `super.m()` inside `m` must not find
    // `m` again, which is the whole reason the home object exists rather than
    // the read starting at `this`.
    let above = expr::call(builder, ctx, RuntimeOp::GetPrototype, &[home])?[0];
    expr::emit_read(builder, ctx, above, *name)
}

/// `super(...)` — the parent constructor, run against the object that exists.
///
/// Not `new`: the instance was already made by `construct`, with the derived
/// class's prototype. What is missing is the parent's own initialisation, and
/// running its body with this receiver is exactly that.
pub(super) fn emit_super_call(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    arguments: &[crate::syntax::Spreadable],
) -> EmitResult<ValueId> {
    let parent = ctx.names.intern(SUPER);
    let parent = binding::read(builder, scope, ctx, parent)?;
    let Some(receiver) = scope.this_value() else {
        return Err(EmitError::Unsupported {
            construct: "`super()` outside a method",
        });
    };
    super::call::emit_call_with(builder, scope, ctx, parent, receiver, arguments)
}

/// The name a member is installed under.
fn named_key(key: &ClassKey) -> EmitResult<Name> {
    match key {
        ClassKey::Public(PropertyKey::Named(name)) => Ok(*name),
        ClassKey::Public(PropertyKey::Computed(_)) => Err(EmitError::Unsupported {
            construct: "a computed class member name",
        }),
        ClassKey::Private(_) => Err(EmitError::Unsupported {
            construct: "a private class member",
        }),
    }
}

/// What this lowering does not express, refused by name before anything is
/// emitted.
///
/// Up front rather than as each is reached, so a class is either wholly emitted
/// or wholly refused — a half-built one would leave a constructor whose methods
/// are missing, which runs and is wrong.
fn refuse_what_is_not_built(class: &Class) -> EmitResult<()> {
    for element in &class.body {
        match element {
            ClassElement::StaticBlock(_) => {
                return Err(EmitError::Unsupported {
                    construct: "a class static block",
                });
            }
            // A constructor that is a getter is a grammar error rather than
            // something to emit, and `is_constructor` already answers false for
            // one — so the only accessors reaching the loop above are real
            // ones.
            ClassElement::Method(Method { .. }) => {}
            _ => {}
        }
        if let Some(key) = element.key() {
            named_key(key)?;
        }
    }
    Ok(())
}
