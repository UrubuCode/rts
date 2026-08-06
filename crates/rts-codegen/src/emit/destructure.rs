//! Lowering a [`Pattern`], in either of the roles it can hold.
//!
//! # Why this reuses statements and expressions instead of emitting IR by hand
//!
//! A destructuring target is a full `LeftHandSideExpression` — `({ a: obj.x } =
//! src)` is legal — and `expr.rs` already knows how to write a name, a member
//! and a computed index, in the order the specification gives. A second copy of
//! that here is exactly the failure rule 1 warns about: reaching around a layer
//! that already answers the question. So a leaf that writes to a place is
//! answered by building the ordinary `Expr::Assign` node for it and handing it
//! to [`super::expr::emit_expr`], the same way `foreach.rs` builds an ordinary
//! `for` out of `for-in`'s expansion rather than walking an array by hand.
//!
//! # Why there is no `scope.enter()`/`scope.leave()` around a pattern's own body
//!
//! The names a **binding** pattern introduces must land in the scope the
//! caller is already emitting into — `let [a, b] = xs;` has to leave `a` and
//! `b` reachable by the statement after it, not popped with a frame this
//! module made for its own bookkeeping. So every synthetic temporary below is
//! declared directly into the caller's current scope too, exactly beside the
//! names it computes for.
//!
//! What keeps that safe is [`depth`]: every temporary's name is suffixed with
//! how many pattern levels deep it was made at, so a nested pattern's own
//! `__rts_destructure_arr_1` cannot overwrite its parent's
//! `__rts_destructure_arr_0` while the parent still needs it — which it does,
//! for every element after the one that recursed. Two calls at the *same*
//! depth never overlap in time — a pattern's own elements run one at a time —
//! so reusing a name there is reusing a finished binding, not colliding with a
//! live one. The names are spelled the same way `binding.rs`'s `__rts_outer`
//! is: unspellable by a program, so nothing it writes can collide with them
//! either.
//!
//! # What each shape reads through
//!
//! An array pattern **iterates**. [`crate::runtime::RuntimeOp::Iterate`]
//! materialises the source as an array once — which is why `const [a] = new
//! Set([1])` works, and why a hole still consumes a slot: the whole sequence
//! was already walked before any element here is read. An object pattern
//! **reads properties**, by name or by a computed key evaluated once, with no
//! iterator and no length to consult.
//!
//! # The default, precisely
//!
//! `[a = f()] = [x]` calls `f` if and only if `x` is `undefined` — not `null`,
//! not falsy. [`apply_default`] is the one place that comparison is made, and it
//! is a branch rather than an operator: the machine has no representation for
//! "maybe evaluate this", the same reason `??` and `&&` live in `choice.rs`
//! rather than being folded into an arithmetic instruction.

use rts_cranelift::fault::Position;
use rts_cranelift::ir::{FuncBuilder, ValueId};

use super::loops::Loops;
use super::{Ctx, EmitResult, Scope, UNPROVEN};
use crate::names::Name;
use crate::runtime::RuntimeOp;
use crate::syntax::{
    ArrayPattern, AssignOp, AssignTarget, BinaryOp, Binding as SyntaxBinding, BindingKind, Expr,
    ExprKind, ForInit, Literal, LogicalOp, ObjectPattern, Pattern, PropertyKey, Spreadable, Stmt,
    StmtKind, UnaryOp, UpdateOp, UpdatePosition,
};
use crate::values::Singleton;

/// Binds every name a **binding** pattern introduces, from a value the caller
/// evaluated exactly once.
///
/// `at` is a position to blame IR built for the pattern's own bookkeeping on —
/// the temporaries and the synthetic loops below, none of which are anything a
/// program wrote.
///
/// # What this does not cover yet
///
/// Only the binding role: `let`/`const`/`var`, a `for`-head's own declaration,
/// and a `for-of`/`for-in` target with `let`/`const` — everywhere
/// [`Pattern::is_valid_binding`] holds, which is everywhere this is called from.
/// The assignment role (`[a, b] = xs`, `({ a } = o)`) is not wired to anything
/// yet: its entry point is `emit/expr.rs`'s `emit_assign`, a file this change
/// does not touch. [`Pattern::Name`] means something different there — a
/// write to an existing binding rather than a fresh one — so wiring it back in
/// needs that distinction restored here, not just a second caller of this
/// function.
pub fn declare(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    pattern: &Pattern,
    value: ValueId,
    at: Position,
) -> EmitResult<()> {
    place(builder, scope, ctx, pattern, value, at, 0, Role::Bind)
}

/// Writes to every place an **assignment** pattern names, from a value the
/// caller evaluated exactly once.
///
/// The same walk as [`declare`], and the difference is one leaf: a bare name
/// here is a WRITE to a binding that already exists, where in a declaration it
/// introduces one. Everything else — the iteration protocol, when a default
/// fires, how a rest is built — is the same rule, and two walks would be that
/// rule written twice.
pub fn assign(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    pattern: &Pattern,
    value: ValueId,
    at: Position,
) -> EmitResult<()> {
    place(builder, scope, ctx, pattern, value, at, 0, Role::Assign)
}

/// What a bare name in a pattern means.
///
/// The one thing the two roles disagree about, which is why it is a parameter
/// rather than two copies of the walk.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    /// `let [a] = xs` — `a` is introduced.
    Bind,
    /// `[a] = xs` — `a` is written, and must already exist.
    Assign,
}

/// The one recursion, carrying how many pattern levels deep it is — see the
/// module doc for why that number is what keeps every synthetic name here
/// from colliding with one an enclosing level still needs.
fn place(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    pattern: &Pattern,
    value: ValueId,
    at: Position,
    depth: u32,
    role: Role,
) -> EmitResult<()> {
    match pattern {
        // The one leaf the two roles disagree about: a declaration introduces
        // the name, an assignment writes a binding that already exists.
        Pattern::Name(name) => match role {
            Role::Bind => super::binding::declare(builder, scope, ctx, *name, value),
            // The value the write answers is the assignment's, and a pattern
            // produces the whole source rather than one element — so it is
            // dropped here and the caller answers.
            Role::Assign => super::binding::write(builder, scope, ctx, *name, value).map(|_| ()),
        },
        // The grammar never produces this leaf in the binding role —
        // `Pattern::is_valid_binding` is what a declaration's parser checks —
        // so nothing valid reaches it. Kept rather than `unreachable!()`
        // because a defect upstream should be a bug here, not a panic.
        Pattern::Target(target) => assign_target(builder, scope, ctx, target, value, at),
        Pattern::Object(object) => {
            object_pattern(builder, scope, ctx, object, value, at, depth, role)
        }
        Pattern::Array(array) => array_pattern(builder, scope, ctx, array, value, at, depth, role),
    }
}

/// Writes `value` to an arbitrary place, by building the ordinary assignment
/// node for it.
///
/// # Why a temporary binding rather than an expression to re-evaluate
///
/// `value` is already a [`ValueId`] — the result of a read this module already
/// performed — and there is no source expression left to hand `emit_expr`. A
/// synthetic identifier bound to it is the smallest expression that names a
/// [`ValueId`], which is what lets the ordinary `Expr::Assign` lowering do the
/// rest: evaluate the receiver, choose the named-key fast path, write.
///
/// Bracketed by its own scope, unlike everything below: nothing this leaf
/// declares needs to survive it, so the temporary is popped rather than left
/// beside whatever the pattern actually bound.
fn assign_target(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    target: &Expr,
    value: ValueId,
    at: Position,
) -> EmitResult<()> {
    scope.enter();
    let tmp = ctx.names.intern("__rts_destructure_tmp");
    super::binding::declare(builder, scope, ctx, tmp, value)?;
    let assign = Expr {
        kind: ExprKind::Assign {
            target: AssignTarget::Place(Box::new(target.clone())),
            value: Box::new(ident(tmp, at)),
            op: AssignOp::Plain,
        },
        at,
    };
    super::expr::emit_expr(builder, scope, ctx, &assign)?;
    scope.leave();
    Ok(())
}

/// `[a, b = 1, ...rest] = source` / `let [a, b = 1, ...rest] = source`.
fn array_pattern(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    pattern: &ArrayPattern,
    source: ValueId,
    at: Position,
    depth: u32,
    role: Role,
) -> EmitResult<()> {
    // Iteration, not indexing — `const [a] = new Set([1])` has no index to
    // read. `Iterate` walks the whole sequence once and materialises it as an
    // array, so every element below is an ordinary index read into something
    // that already exists, and a hole still consumed a step by having been
    // materialised at all.
    let materialized = super::expr::call(builder, ctx, RuntimeOp::Iterate, &[source])?[0];
    let arr = ctx.names.intern(&format!("__rts_destructure_arr_{depth}"));
    super::binding::declare(builder, scope, ctx, arr, materialized)?;

    for (position, element) in pattern.elements.iter().enumerate() {
        // A hole binds nothing. It already took its step above.
        let Some(element) = element else { continue };
        let read = index_expr(ident(arr, at), number(position as f64, at), at);
        let raw = super::expr::emit_expr(builder, scope, ctx, &read)?;
        let value = apply_default(builder, scope, ctx, raw, element.default.as_ref())?;
        place(builder, scope, ctx, &element.pattern, value, at, depth + 1, role)?;
    }

    if let Some(rest) = &pattern.rest {
        // `elements` holds a hole at the rest's own position too — the parser
        // maps `...tail` in `[head, ...tail]` to `None` there, the same
        // representation an ordinary hole gets, rather than shortening the
        // vector by one. So the count of slots the rest starts *after* is one
        // less than `elements.len()` whenever a rest is present at all — the
        // rest is always last, so it is always that one hole.
        let start = pattern.elements.len() - 1;
        let gathered = array_rest(builder, scope, ctx, arr, start, at)?;
        place(builder, scope, ctx, rest, gathered, at, depth + 1, role)?;
    }

    Ok(())
}

/// `...rest` of an array pattern: everything `Iterate` produced from `start`
/// on, as a fresh array.
///
/// # Why `.slice`, and not a hand-written copying loop
///
/// `ArrayNew` sizes its store once, at the length it is given — it is not a
/// growable vector, which is why `call.rs`'s own argument-vector builder grows
/// through `ArrayAppend` rather than through `SetIndexed` past the end. A
/// counted loop writing `rest[i] = arr[start + i]` into an `ArrayNew(0)` would
/// be writing past a store sized for nothing, silently, for every index —
/// which was tried here first and read back an empty array on every fixture
/// that named a rest element. `Array.prototype.slice` already exists, is
/// already exercised (`arrays.js`'s own `slice` case), and is the correct
/// answer to "everything from this index on" without this module deciding how
/// an array grows a second time.
fn array_rest(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    arr: Name,
    start: usize,
    at: Position,
) -> EmitResult<ValueId> {
    let slice = ctx.names.intern("slice");
    let call = Expr {
        kind: ExprKind::Call {
            callee: Box::new(member_expr(ident(arr, at), slice, at)),
            arguments: vec![Spreadable::Single(number(start as f64, at))],
            optional: false,
        },
        at,
    };
    super::expr::emit_expr(builder, scope, ctx, &call)
}

/// `{ a, b: c = 1, ...rest } = source` / `let { a, b: c = 1, ...rest } = source`.
fn object_pattern(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    pattern: &ObjectPattern,
    source: ValueId,
    at: Position,
    depth: u32,
    role: Role,
) -> EmitResult<()> {
    let src = ctx.names.intern(&format!("__rts_destructure_obj_{depth}"));
    super::binding::declare(builder, scope, ctx, src, source)?;

    // The keys this pattern already read, in source order — what `...rest`
    // must exclude. A computed key's expression runs exactly once, right
    // here, which is also the only value that could name it later.
    let mut excluded: Vec<Expr> = Vec::new();

    for (position, property) in pattern.properties.iter().enumerate() {
        let read = match &property.key {
            // `{a: b}` binds `b`, reading the property named `a` — the key is
            // never the name a value ends up bound to.
            PropertyKey::Named(name) => {
                excluded.push(Expr {
                    kind: ExprKind::Literal(Literal::String(ctx.names.text(*name).to_owned())),
                    at,
                });
                member_expr(ident(src, at), *name, at)
            }
            PropertyKey::Computed(key_expr) => {
                let key_value = super::expr::emit_expr(builder, scope, ctx, key_expr)?;
                let key_name = ctx
                    .names
                    .intern(&format!("__rts_destructure_key_{depth}_{position}"));
                super::binding::declare(builder, scope, ctx, key_name, key_value)?;
                excluded.push(ident(key_name, at));
                index_expr(ident(src, at), ident(key_name, at), at)
            }
        };
        let raw = super::expr::emit_expr(builder, scope, ctx, &read)?;
        let value = apply_default(builder, scope, ctx, raw, property.value.default.as_ref())?;
        place(builder, scope, ctx, &property.value.pattern, value, at, depth + 1, role)?;
    }

    if let Some(rest) = &pattern.rest {
        let gathered = object_rest(builder, scope, ctx, src, &excluded, at, depth)?;
        place(builder, scope, ctx, rest, gathered, at, depth + 1, role)?;
    }

    Ok(())
}

/// `...rest` of an object pattern: the own enumerable properties `OwnKeys`
/// finds, less whatever [`object_pattern`] already named, copied into a fresh
/// object.
///
/// # A named divergence
///
/// The exclusion the loop below builds is a chain of `===` against the keys
/// this call already knows — every [`PropertyKey::Named`] and every
/// [`PropertyKey::Computed`] this pattern read. That is exact for both. What is
/// NOT modelled is a key a *different* rest in a sibling pattern excluded, which
/// is not a thing the specification asks for either — each object pattern's
/// rest excludes only its own siblings.
fn object_rest(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    src: Name,
    excluded: &[Expr],
    at: Position,
    depth: u32,
) -> EmitResult<ValueId> {
    let rest_obj = super::expr::call(builder, ctx, RuntimeOp::ObjectNew, &[])?[0];
    let rest = ctx.names.intern(&format!("__rts_destructure_restobj_{depth}"));
    super::binding::declare(builder, scope, ctx, rest, rest_obj)?;

    let source_value = super::binding::read(builder, scope, ctx, src)?;
    let keys_arr = super::expr::call(builder, ctx, RuntimeOp::OwnKeys, &[source_value])?[0];
    let keys = ctx.names.intern(&format!("__rts_destructure_keys_{depth}"));
    super::binding::declare(builder, scope, ctx, keys, keys_arr)?;

    let index_name = ctx.names.intern(&format!("__rts_destructure_i_{depth}"));
    let key_name = ctx.names.intern(&format!("__rts_destructure_k_{depth}"));
    let length = ctx.names.intern("length");

    let init = ForInit::Declare {
        kind: BindingKind::Let,
        bindings: vec![SyntaxBinding {
            target: Pattern::Name(index_name),
            value: Some(number(0.0, at)),
            claim: None,
        }],
    };
    let test = Expr {
        kind: ExprKind::Binary {
            op: BinaryOp::Less,
            left: Box::new(ident(index_name, at)),
            right: Box::new(member_expr(ident(keys, at), length, at)),
        },
        at,
    };
    let update = Expr {
        kind: ExprKind::Update {
            op: UpdateOp::Increment,
            position: UpdatePosition::Postfix,
            target: Box::new(ident(index_name, at)),
        },
        at,
    };

    let key_decl = Stmt {
        kind: StmtKind::Declare {
            kind: BindingKind::Let,
            bindings: vec![SyntaxBinding {
                target: Pattern::Name(key_name),
                value: Some(index_expr(ident(keys, at), ident(index_name, at), at)),
                claim: None,
            }],
        },
        at,
    };

    let mut already_named: Option<Expr> = None;
    for key in excluded {
        let matches = Expr {
            kind: ExprKind::Binary {
                op: BinaryOp::StrictEqual,
                left: Box::new(key.clone()),
                right: Box::new(ident(key_name, at)),
            },
            at,
        };
        already_named = Some(match already_named {
            None => matches,
            Some(previous) => Expr {
                kind: ExprKind::Logical {
                    op: LogicalOp::Or,
                    left: Box::new(previous),
                    right: Box::new(matches),
                },
                at,
            },
        });
    }

    let copy = plain_assign_stmt(
        index_expr(ident(rest, at), ident(key_name, at), at),
        index_expr(ident(src, at), ident(key_name, at), at),
        at,
    );

    let body_stmts = match already_named {
        None => vec![key_decl, copy],
        Some(condition) => {
            let not_named = Expr {
                kind: ExprKind::Unary {
                    op: UnaryOp::Not,
                    operand: Box::new(condition),
                },
                at,
            };
            vec![
                key_decl,
                Stmt {
                    kind: StmtKind::If {
                        condition: not_named,
                        then_branch: Box::new(copy),
                        else_branch: None,
                    },
                    at,
                },
            ]
        }
    };
    let body = Stmt {
        kind: StmtKind::Block(body_stmts),
        at,
    };

    let mut loops = Loops::default();
    super::loops::emit_for(
        builder,
        scope,
        ctx,
        &mut loops,
        Some(&init),
        Some(&test),
        Some(&update),
        &body,
        None,
    )?;

    super::binding::read(builder, scope, ctx, rest)
}

/// Applies a slot's default, if it has one and the value read was `undefined`.
///
/// # Why `undefined` alone, and why a branch
///
/// `[a = 1] = [null]` binds `null` — a default fires on absence, not on
/// falsiness and not on `null`, which is a value that was there. That rules out
/// reusing `??`'s nullish test (`choice::branch_on_nullish`), which treats
/// `null` the same as `undefined` on purpose. And it has to be a branch rather
/// than an operator: `[a = f()] = [1]` must never call `f`, so the default
/// expression is only ever emitted on the path where it is needed — the same
/// reason `&&`, `||` and `??` are branches in `choice.rs` rather than
/// instructions.
fn apply_default(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    value: ValueId,
    default: Option<&Expr>,
) -> EmitResult<ValueId> {
    let Some(default) = default else {
        return Ok(value);
    };

    let undefined = super::expr::singleton(builder, ctx, Singleton::Undefined);
    let is_undefined =
        super::expr::call(builder, ctx, RuntimeOp::StrictEquals, &[value, undefined])?[0];

    let evaluate = builder.create_block();
    let skip = builder.create_block();
    let before = scope.snapshot();
    builder.branch(is_undefined, (evaluate, &[]), (skip, &[]))?;

    builder.switch_to(evaluate);
    let default_value = super::expr::emit_expr(builder, scope, ctx, default)?;
    let default_value = builder.widen(default_value);
    let evaluate_exit = builder.current();
    let evaluate_bindings = scope.snapshot();

    // The skip path starts from what the branch started from, not from what
    // the other arm left — `[a = (b = 2)] = [1]` must not see `b`'s new value
    // on the path that never ran the assignment.
    scope.restore(&before);
    builder.switch_to(skip);
    let skip_value = builder.widen(value);
    let skip_exit = skip;
    let skip_bindings = before;

    let join = builder.create_block();
    let result = builder.add_block_param(join, UNPROVEN);
    let merged = super::merge::disagreements(&evaluate_bindings, &skip_bindings);
    let params = super::merge::parameters(builder, join, &merged, &evaluate_bindings);

    builder.switch_to(evaluate_exit);
    let mut args = vec![default_value];
    args.extend(merged.iter().map(|&at| evaluate_bindings[at].value()));
    builder.jump(join, &args)?;

    builder.switch_to(skip_exit);
    let mut args = vec![skip_value];
    args.extend(merged.iter().map(|&at| skip_bindings[at].value()));
    builder.jump(join, &args)?;

    super::merge::settle(scope, evaluate_bindings, &merged, params);
    builder.switch_to(join);
    Ok(result)
}

/// `name`, as a synthetic identifier expression.
fn ident(name: Name, at: Position) -> Expr {
    Expr {
        kind: ExprKind::Ident(name),
        at,
    }
}

/// A number literal, as a synthetic expression.
fn number(value: f64, at: Position) -> Expr {
    Expr {
        kind: ExprKind::Literal(Literal::Number(value)),
        at,
    }
}

/// `object.property`, as a synthetic expression.
fn member_expr(object: Expr, property: Name, at: Position) -> Expr {
    Expr {
        kind: ExprKind::Member {
            object: Box::new(object),
            property,
            optional: false,
        },
        at,
    }
}

/// `object[index]`, as a synthetic expression.
fn index_expr(object: Expr, index: Expr, at: Position) -> Expr {
    Expr {
        kind: ExprKind::Index {
            object: Box::new(object),
            index: Box::new(index),
            optional: false,
        },
        at,
    }
}

/// `place = value;`, as a synthetic statement — the shared shape both rests'
/// copying loops need.
fn plain_assign_stmt(place: Expr, value: Expr, at: Position) -> Stmt {
    Stmt {
        kind: StmtKind::Expr(Expr {
            kind: ExprKind::Assign {
                target: AssignTarget::Place(Box::new(place)),
                value: Box::new(value),
                op: AssignOp::Plain,
            },
            at,
        }),
        at,
    }
}
