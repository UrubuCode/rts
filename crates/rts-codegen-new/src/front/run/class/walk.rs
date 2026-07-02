//! `this`-rewriting + field-collection + `this`-usage walkers for the class
//! synthesizer (P4.9 / P5.1). Split out of [`super::synth`] to keep both files
//! under the 500-line module rule.
//!
//! - [`rewrite_this_block`] turns swc's `Raw("This(…)")` nodes into
//!   `Ident("this")` (the lowerer binds `this` to a0);
//! - [`collect_this_assign_fields`] gathers the `this.x = …` targets that become
//!   instance slots;
//! - [`body_uses_this`] detects a `this` reference (used to refuse `this` in a
//!   static method / static-field initializer).

use rts_hir::ir::{HirExpr, HirExprKind, HirStmt, HirType};

use super::THIS;

/// `this.<field> = <value>` as an HIR statement (an `Assign` to a `Member` on the
/// `this` identifier). Used for property initializers in the constructor prologue.
pub(super) fn this_field_assign(field: &str, value: HirExpr) -> HirStmt {
    let this = HirExpr::new(HirExprKind::Ident(THIS.to_string()), HirType::Unknown);
    let member = HirExpr::new(
        HirExprKind::Member {
            object: Box::new(this),
            prop: field.to_string(),
        },
        HirType::Unknown,
    );
    HirStmt::Expr(HirExpr::new(
        HirExprKind::Assign {
            target: Box::new(member),
            value: Box::new(value),
        },
        HirType::Unknown,
    ))
}

/// Append `name` to `fields` if not already present (first-seen-order union).
pub(super) fn push_unique(fields: &mut Vec<String>, name: &str) {
    if !fields.iter().any(|f| f == name) {
        fields.push(name.to_string());
    }
}

/// Walk a (constructor) HIR body collecting every `this.<field> = …` target field
/// not already in `fields`, in first-seen order.
pub(super) fn collect_this_assign_fields(stmts: &[HirStmt], fields: &mut Vec<String>) {
    for s in stmts {
        walk_stmt_fields(s, fields);
    }
}

fn walk_stmt_fields(s: &HirStmt, fields: &mut Vec<String>) {
    match s {
        HirStmt::Expr(e) | HirStmt::Throw(e) => walk_expr_fields(e, fields),
        HirStmt::Return(Some(e)) => walk_expr_fields(e, fields),
        HirStmt::Let { init: Some(e), .. } => walk_expr_fields(e, fields),
        HirStmt::Const { init, .. } => walk_expr_fields(init, fields),
        HirStmt::If { cond, then, else_ } => {
            walk_expr_fields(cond, fields);
            then.iter().for_each(|s| walk_stmt_fields(s, fields));
            if let Some(e) = else_ {
                e.iter().for_each(|s| walk_stmt_fields(s, fields));
            }
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { cond, body } => {
            walk_expr_fields(cond, fields);
            body.iter().for_each(|s| walk_stmt_fields(s, fields));
        }
        HirStmt::Block(b) => b.iter().for_each(|s| walk_stmt_fields(s, fields)),
        HirStmt::For { body, .. } | HirStmt::ForOf { body, .. } | HirStmt::ForIn { body, .. } => {
            body.iter().for_each(|s| walk_stmt_fields(s, fields));
        }
        _ => {}
    }
}

fn walk_expr_fields(e: &HirExpr, fields: &mut Vec<String>) {
    if let HirExprKind::Assign { target, value } = &e.kind {
        if let HirExprKind::Member { object, prop } = &target.kind {
            if matches!(&object.kind, HirExprKind::Ident(n) if n == THIS) {
                push_unique(fields, prop);
            }
        }
        walk_expr_fields(value, fields);
        return;
    }
    match &e.kind {
        HirExprKind::Bin { lhs, rhs, .. } => {
            walk_expr_fields(lhs, fields);
            walk_expr_fields(rhs, fields);
        }
        HirExprKind::AssignOp { value, .. } => walk_expr_fields(value, fields),
        HirExprKind::Call { args, .. } | HirExprKind::MethodCall { args, .. } => {
            args.iter().for_each(|a| walk_expr_fields(a, fields));
        }
        HirExprKind::Ternary { then, else_, .. } => {
            walk_expr_fields(then, fields);
            walk_expr_fields(else_, fields);
        }
        _ => {}
    }
}

/// Whether a (lowered, `this`-rewritten) body references `Ident("this")`.
pub(super) fn body_uses_this(stmts: &[HirStmt]) -> bool {
    stmts.iter().any(stmt_uses_this)
}

fn stmt_uses_this(s: &HirStmt) -> bool {
    match s {
        HirStmt::Expr(e) | HirStmt::Throw(e) => expr_uses_this(e),
        HirStmt::Return(opt) => opt.as_ref().is_some_and(expr_uses_this),
        HirStmt::Let { init, .. } => init.as_ref().is_some_and(expr_uses_this),
        HirStmt::Const { init, .. } => expr_uses_this(init),
        HirStmt::If { cond, then, else_ } => {
            expr_uses_this(cond)
                || then.iter().any(stmt_uses_this)
                || else_.as_ref().is_some_and(|e| e.iter().any(stmt_uses_this))
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { cond, body } => {
            expr_uses_this(cond) || body.iter().any(stmt_uses_this)
        }
        HirStmt::Block(b) => b.iter().any(stmt_uses_this),
        _ => false,
    }
}

fn expr_uses_this(e: &HirExpr) -> bool {
    if super::is_raw_this(e) {
        return true;
    }
    if matches!(&e.kind, HirExprKind::Ident(n) if n == THIS) {
        return true;
    }
    match &e.kind {
        HirExprKind::Bin { lhs, rhs, .. } => expr_uses_this(lhs) || expr_uses_this(rhs),
        HirExprKind::Unary { operand, .. } => expr_uses_this(operand),
        HirExprKind::Assign { target, value } | HirExprKind::AssignOp { target, value, .. } => {
            expr_uses_this(target) || expr_uses_this(value)
        }
        HirExprKind::Call { callee, args } => {
            expr_uses_this(callee) || args.iter().any(expr_uses_this)
        }
        HirExprKind::MethodCall { object, args, .. } => {
            expr_uses_this(object) || args.iter().any(expr_uses_this)
        }
        HirExprKind::Member { object, .. } => expr_uses_this(object),
        HirExprKind::Index { object, index } => expr_uses_this(object) || expr_uses_this(index),
        HirExprKind::Ternary { cond, then, else_ } => {
            expr_uses_this(cond) || expr_uses_this(then) || expr_uses_this(else_)
        }
        HirExprKind::Array(elems) => elems.iter().any(expr_uses_this),
        HirExprKind::New { args, .. } => args.iter().any(expr_uses_this),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// PRE-rewrite `this` detection: scan a NOT-yet-rewritten body for swc's
// `Raw("This(…)")` node. Used by the free-function `this`-transform (Phase 1) to
// decide whether a top-level `function`/function-expression references `this`
// and so needs a synthesized leading `this` param. Mirrors the `rewrite_this_*`
// traversal exactly so it sees `this` everywhere the rewrite would change it.
// ---------------------------------------------------------------------------

/// Whether `stmts` (a NOT-yet-`this`-rewritten body) references `this` via swc's
/// `Raw("This(…)")` node anywhere the [`rewrite_this_block`] traversal reaches.
pub(crate) fn body_uses_raw_this(stmts: &[HirStmt]) -> bool {
    stmts.iter().any(raw_this_stmt)
}

fn raw_this_stmt(s: &HirStmt) -> bool {
    match s {
        HirStmt::Expr(e) | HirStmt::Throw(e) => raw_this_expr(e),
        HirStmt::Return(opt) => opt.as_ref().is_some_and(raw_this_expr),
        HirStmt::Let { init, .. } => init.as_ref().is_some_and(raw_this_expr),
        HirStmt::Const { init, .. } => raw_this_expr(init),
        HirStmt::If { cond, then, else_ } => {
            raw_this_expr(cond)
                || then.iter().any(raw_this_stmt)
                || else_.as_ref().is_some_and(|e| e.iter().any(raw_this_stmt))
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { cond, body } => {
            raw_this_expr(cond) || body.iter().any(raw_this_stmt)
        }
        HirStmt::For {
            cond, update, body, ..
        } => {
            cond.as_ref().is_some_and(raw_this_expr)
                || update.as_ref().is_some_and(raw_this_expr)
                || body.iter().any(raw_this_stmt)
        }
        HirStmt::ForOf { iterable, body, .. } => {
            raw_this_expr(iterable) || body.iter().any(raw_this_stmt)
        }
        HirStmt::ForIn { object, body, .. } => {
            raw_this_expr(object) || body.iter().any(raw_this_stmt)
        }
        HirStmt::Block(b) => b.iter().any(raw_this_stmt),
        _ => false,
    }
}

fn raw_this_expr(e: &HirExpr) -> bool {
    if super::is_raw_this(e) {
        return true;
    }
    match &e.kind {
        HirExprKind::Bin { lhs, rhs, .. } => raw_this_expr(lhs) || raw_this_expr(rhs),
        HirExprKind::Unary { operand, .. } => raw_this_expr(operand),
        HirExprKind::Assign { target, value } | HirExprKind::AssignOp { target, value, .. } => {
            raw_this_expr(target) || raw_this_expr(value)
        }
        HirExprKind::Call { callee, args } => {
            raw_this_expr(callee) || args.iter().any(raw_this_expr)
        }
        HirExprKind::MethodCall { object, args, .. } => {
            raw_this_expr(object) || args.iter().any(raw_this_expr)
        }
        HirExprKind::Member { object, .. } => raw_this_expr(object),
        HirExprKind::Index { object, index } => raw_this_expr(object) || raw_this_expr(index),
        HirExprKind::Ternary { cond, then, else_ } => {
            raw_this_expr(cond) || raw_this_expr(then) || raw_this_expr(else_)
        }
        HirExprKind::Array(elems) => elems.iter().any(raw_this_expr),
        HirExprKind::Object(fields) => fields.iter().any(|(_, v)| raw_this_expr(v)),
        HirExprKind::Await(inner)
        | HirExprKind::Spread(inner)
        | HirExprKind::Cast { expr: inner, .. } => raw_this_expr(inner),
        HirExprKind::PreInc(t)
        | HirExprKind::PreDec(t)
        | HirExprKind::PostInc(t)
        | HirExprKind::PostDec(t) => raw_this_expr(t),
        HirExprKind::Seq(items) => items.iter().any(raw_this_expr),
        HirExprKind::New { args, .. } => args.iter().any(raw_this_expr),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// `this` rewriting: swc lowers `this` to a Raw node; turn each into Ident("this").
// ---------------------------------------------------------------------------

pub(crate) fn rewrite_this_block(stmts: &mut [HirStmt]) {
    for s in stmts {
        rewrite_this_stmt(s);
    }
}

fn rewrite_this_stmt(s: &mut HirStmt) {
    match s {
        HirStmt::Expr(e) | HirStmt::Throw(e) => rewrite_this_expr(e),
        HirStmt::Return(opt) => {
            if let Some(e) = opt {
                rewrite_this_expr(e);
            }
        }
        HirStmt::Let { init, .. } => {
            if let Some(e) = init {
                rewrite_this_expr(e);
            }
        }
        HirStmt::Const { init, .. } => rewrite_this_expr(init),
        HirStmt::If { cond, then, else_ } => {
            rewrite_this_expr(cond);
            rewrite_this_block(then);
            if let Some(e) = else_ {
                rewrite_this_block(e);
            }
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { cond, body } => {
            rewrite_this_expr(cond);
            rewrite_this_block(body);
        }
        HirStmt::For {
            cond, update, body, ..
        } => {
            if let Some(c) = cond {
                rewrite_this_expr(c);
            }
            if let Some(u) = update {
                rewrite_this_expr(u);
            }
            rewrite_this_block(body);
        }
        HirStmt::ForOf { iterable, body, .. } => {
            rewrite_this_expr(iterable);
            rewrite_this_block(body);
        }
        HirStmt::ForIn { object, body, .. } => {
            rewrite_this_expr(object);
            rewrite_this_block(body);
        }
        HirStmt::Block(b) => rewrite_this_block(b),
        _ => {}
    }
}

fn rewrite_this_expr(e: &mut HirExpr) {
    if super::is_raw_this(e) {
        e.kind = HirExprKind::Ident(THIS.to_string());
        e.ty = HirType::Unknown;
        return;
    }
    match &mut e.kind {
        HirExprKind::Bin { lhs, rhs, .. } => {
            rewrite_this_expr(lhs);
            rewrite_this_expr(rhs);
        }
        HirExprKind::Unary { operand, .. } => rewrite_this_expr(operand),
        HirExprKind::Assign { target, value } | HirExprKind::AssignOp { target, value, .. } => {
            rewrite_this_expr(target);
            rewrite_this_expr(value);
        }
        HirExprKind::Call { callee, args } => {
            rewrite_this_expr(callee);
            args.iter_mut().for_each(rewrite_this_expr);
        }
        HirExprKind::MethodCall { object, args, .. } => {
            rewrite_this_expr(object);
            args.iter_mut().for_each(rewrite_this_expr);
        }
        HirExprKind::Member { object, .. } => rewrite_this_expr(object),
        HirExprKind::Index { object, index } => {
            rewrite_this_expr(object);
            rewrite_this_expr(index);
        }
        HirExprKind::Ternary { cond, then, else_ } => {
            rewrite_this_expr(cond);
            rewrite_this_expr(then);
            rewrite_this_expr(else_);
        }
        HirExprKind::Array(elems) => elems.iter_mut().for_each(rewrite_this_expr),
        HirExprKind::Object(fields) => fields.iter_mut().for_each(|(_, v)| rewrite_this_expr(v)),
        HirExprKind::Await(inner)
        | HirExprKind::Spread(inner)
        | HirExprKind::Cast { expr: inner, .. } => rewrite_this_expr(inner),
        HirExprKind::PreInc(t)
        | HirExprKind::PreDec(t)
        | HirExprKind::PostInc(t)
        | HirExprKind::PostDec(t) => rewrite_this_expr(t),
        HirExprKind::Seq(items) => items.iter_mut().for_each(rewrite_this_expr),
        HirExprKind::New { args, .. } => args.iter_mut().for_each(rewrite_this_expr),
        // An ARROW inside a method body: its lexical `this` IS the method's
        // `this` param, so rewrite the arrow body too — the extraction pass then
        // sees `this` as an ordinary capturable outer local (capture-by-value is
        // exact: `this` never reassigns). Without this arm a `() => this.x`
        // kept `Raw("This(..)")` and bailed the whole arrow.
        HirExprKind::Arrow { body, .. } => match body {
            rts_hir::ir::HirArrowBody::Expr(e) => rewrite_this_expr(e),
            rts_hir::ir::HirArrowBody::Block(stmts) => rewrite_this_block(stmts),
        },
        _ => {}
    }
}

/// Mangle every PRIVATE property name (`#x`) in `stmts` to its per-class form
/// (`#x@Class`) — TS private fields are visible ONLY in the declaring class, so
/// two classes in one inheritance chain may each declare `#v` and they must be
/// DISTINCT slots (the flattened field list would otherwise collide by name).
/// Covers Member/Index reads+writes through the same expr walk shape as the
/// `this` rewrite.
pub(crate) fn mangle_private_block(stmts: &mut [HirStmt], class: &str) {
    for s in stmts {
        mangle_private_stmt(s, class);
    }
}

/// The mangled per-class name of a private field `#x` declared in `class`.
pub(crate) fn mangle_private_name(name: &str, class: &str) -> String {
    format!("{name}@{class}")
}

fn mangle_private_stmt(s: &mut HirStmt, class: &str) {
    match s {
        HirStmt::Expr(e) | HirStmt::Throw(e) => mangle_private_expr(e, class),
        HirStmt::Return(Some(e)) => mangle_private_expr(e, class),
        HirStmt::Let { init: Some(e), .. } => mangle_private_expr(e, class),
        HirStmt::Const { init, .. } => mangle_private_expr(init, class),
        HirStmt::If { cond, then, else_ } => {
            mangle_private_expr(cond, class);
            mangle_private_block(then, class);
            if let Some(e) = else_ {
                mangle_private_block(e, class);
            }
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { cond, body } => {
            mangle_private_expr(cond, class);
            mangle_private_block(body, class);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                mangle_private_stmt(i, class);
            }
            if let Some(c) = cond {
                mangle_private_expr(c, class);
            }
            if let Some(u) = update {
                mangle_private_expr(u, class);
            }
            mangle_private_block(body, class);
        }
        HirStmt::ForOf { iterable, body, .. } => {
            mangle_private_expr(iterable, class);
            mangle_private_block(body, class);
        }
        HirStmt::ForIn { object, body, .. } => {
            mangle_private_expr(object, class);
            mangle_private_block(body, class);
        }
        HirStmt::Block(b) => mangle_private_block(b, class),
        HirStmt::Try {
            body,
            catch,
            finally,
        } => {
            mangle_private_block(body, class);
            if let Some(c) = catch {
                mangle_private_block(&mut c.body, class);
            }
            if let Some(f) = finally {
                mangle_private_block(f, class);
            }
        }
        _ => {}
    }
}

fn mangle_private_expr(e: &mut HirExpr, class: &str) {
    if let HirExprKind::Member { object, prop } = &mut e.kind {
        if prop.starts_with('#') && !prop.contains('@') {
            *prop = mangle_private_name(prop, class);
        }
        mangle_private_expr(object, class);
        return;
    }
    match &mut e.kind {
        HirExprKind::Bin { lhs, rhs, .. } => {
            mangle_private_expr(lhs, class);
            mangle_private_expr(rhs, class);
        }
        HirExprKind::Unary { operand, .. } => mangle_private_expr(operand, class),
        HirExprKind::Assign { target, value } | HirExprKind::AssignOp { target, value, .. } => {
            mangle_private_expr(target, class);
            mangle_private_expr(value, class);
        }
        HirExprKind::Call { callee, args } => {
            mangle_private_expr(callee, class);
            args.iter_mut().for_each(|a| mangle_private_expr(a, class));
        }
        HirExprKind::MethodCall { object, args, .. } => {
            mangle_private_expr(object, class);
            args.iter_mut().for_each(|a| mangle_private_expr(a, class));
        }
        HirExprKind::Index { object, index } => {
            mangle_private_expr(object, class);
            mangle_private_expr(index, class);
        }
        HirExprKind::Ternary { cond, then, else_ } => {
            mangle_private_expr(cond, class);
            mangle_private_expr(then, class);
            mangle_private_expr(else_, class);
        }
        HirExprKind::Array(elems) => elems.iter_mut().for_each(|x| mangle_private_expr(x, class)),
        HirExprKind::Object(fields) => fields
            .iter_mut()
            .for_each(|(_, v)| mangle_private_expr(v, class)),
        HirExprKind::New { args, .. } => {
            args.iter_mut().for_each(|a| mangle_private_expr(a, class));
        }
        HirExprKind::Await(inner) | HirExprKind::Spread(inner) => {
            mangle_private_expr(inner, class);
        }
        HirExprKind::Seq(items) => items.iter_mut().for_each(|x| mangle_private_expr(x, class)),
        HirExprKind::PreInc(t)
        | HirExprKind::PreDec(t)
        | HirExprKind::PostInc(t)
        | HirExprKind::PostDec(t) => mangle_private_expr(t, class),
        HirExprKind::Arrow { body, .. } => match body {
            rts_hir::ir::HirArrowBody::Expr(inner) => mangle_private_expr(inner, class),
            rts_hir::ir::HirArrowBody::Block(stmts) => mangle_private_block(stmts, class),
        },
        _ => {}
    }
}
