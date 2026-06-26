//! Inheritance support: parent-first processing order + `super` HIR rewriting
//! (P5.1).
//!
//! Two jobs, kept out of [`super::synth`] for the <500-line rule:
//! 1. [`topo_order`] — order the program's classes so a subclass is built AFTER
//!    its parent (its descriptor must already exist to flatten fields/methods).
//!    An `extends` of a class not declared in this program, or an inheritance
//!    cycle, BAILS.
//! 2. [`rewrite_super_block`] — after a constructor/method body is lowered to HIR
//!    and its `this` nodes rewritten, turn the two `super` shapes into ordinary
//!    user-function calls:
//!    - `super(args)` (lowered to `Call { callee: Raw("callee"), args }` because
//!      swc's `Callee::Super` has no dedicated HIR arm) → a call of the parent's
//!      synthesized constructor `__rtsn_ctor_Parent(this, args…)`;
//!    - `super.m(args)` (lowered to `Call { callee: Raw("SuperProp(…sym: \"m\"…)"),
//!      args }`) → a call of the parent-resolved method `__rtsn_method_R_m(this,
//!      args…)`, where `R` is the nearest ancestor declaring `m`.
//!
//!    Both prepend the in-scope `this` identifier as the receiver. A `super`
//!    inside a class with NO parent, or a `super.m` whose `m` is not found on any
//!    ancestor, BAILS (never a guess).

use std::collections::{HashMap, HashSet};

use rts_ast::ast::ClassDecl;
use rts_hir::ir::{HirExpr, HirExprKind, HirStmt, HirType};

use crate::front::error::{FrontResult, Unsupported};

use super::{ClassDesc, THIS};

/// Order `classes` parent-before-child. A class whose `extends` names a class not
/// in this program bails; a cycle bails. Classes with no (in-program) parent come
/// first, then each child once its parent has been emitted.
pub(super) fn topo_order<'a>(classes: &[&'a ClassDecl]) -> FrontResult<Vec<&'a ClassDecl>> {
    let names: HashSet<&str> = classes.iter().map(|c| c.name.as_str()).collect();
    let by_name: HashMap<&str, &ClassDecl> =
        classes.iter().map(|c| (c.name.as_str(), *c)).collect();

    let mut order: Vec<&ClassDecl> = Vec::with_capacity(classes.len());
    let mut done: HashSet<&str> = HashSet::new();

    // Visit each class, emitting its (in-program) parent chain first. `visiting`
    // detects a cycle.
    for &decl in classes {
        emit_class(
            decl,
            &by_name,
            &names,
            &mut order,
            &mut done,
            &mut HashSet::new(),
        )?;
    }
    Ok(order)
}

fn emit_class<'a>(
    decl: &'a ClassDecl,
    by_name: &HashMap<&str, &'a ClassDecl>,
    names: &HashSet<&str>,
    order: &mut Vec<&'a ClassDecl>,
    done: &mut HashSet<&'a str>,
    visiting: &mut HashSet<String>,
) -> FrontResult<()> {
    if done.contains(decl.name.as_str()) {
        return Ok(());
    }
    if !visiting.insert(decl.name.clone()) {
        return Err(Unsupported::new(format!(
            "inheritance cycle through class `{}`",
            decl.name
        )));
    }
    if let Some(parent) = &decl.super_class {
        if names.contains(parent.as_str()) {
            let pdecl = by_name[parent.as_str()];
            emit_class(pdecl, by_name, names, order, done, visiting)?;
        }
        // A parent NOT in the program is left for `collect_classes` to bail with a
        // precise message (it resolves the parent descriptor there).
    }
    visiting.remove(decl.name.as_str());
    // SAFETY of the `&'a str` insert: `decl.name` lives as long as `decl` (`'a`).
    let name_ref: &'a str = decl.name.as_str();
    if done.insert(name_ref) {
        order.push(decl);
    }
    Ok(())
}

/// Rewrite every `super(...)` / `super.m(...)` in a lowered constructor/method
/// body of a class whose parent descriptor is `parent`. With no parent, a `super`
/// use bails.
pub(super) fn rewrite_super_block(
    stmts: &mut [HirStmt],
    parent: Option<&ClassDesc>,
) -> FrontResult<()> {
    for s in stmts {
        rewrite_super_stmt(s, parent)?;
    }
    Ok(())
}

fn rewrite_super_stmt(s: &mut HirStmt, parent: Option<&ClassDesc>) -> FrontResult<()> {
    match s {
        HirStmt::Expr(e) | HirStmt::Throw(e) => rewrite_super_expr(e, parent),
        HirStmt::Return(opt) => {
            if let Some(e) = opt {
                rewrite_super_expr(e, parent)?;
            }
            Ok(())
        }
        HirStmt::Let { init, .. } => {
            if let Some(e) = init {
                rewrite_super_expr(e, parent)?;
            }
            Ok(())
        }
        HirStmt::Const { init, .. } => rewrite_super_expr(init, parent),
        HirStmt::If { cond, then, else_ } => {
            rewrite_super_expr(cond, parent)?;
            rewrite_super_block(then, parent)?;
            if let Some(e) = else_ {
                rewrite_super_block(e, parent)?;
            }
            Ok(())
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { cond, body } => {
            rewrite_super_expr(cond, parent)?;
            rewrite_super_block(body, parent)
        }
        HirStmt::Block(b) => rewrite_super_block(b, parent),
        _ => Ok(()),
    }
}

fn rewrite_super_expr(e: &mut HirExpr, parent: Option<&ClassDesc>) -> FrontResult<()> {
    // A BARE `super.x` FIELD/getter READ (not a call): rts-hir lowered the whole
    // `SuperPropExpr` to a `Raw("SuperProp(…)")` node. In this engine instance
    // fields are OWN properties and an inherited getter resolves through the
    // instance's class, so `super.x` reads the same slot/getter as `this.x` (for
    // the non-shadowed case the tests cover). Rewrite to `this.<name>`. A
    // `super.m(args)` CALL is handled below (the Raw is the call's *callee*, so it
    // is not matched here).
    if let HirExprKind::Raw(raw) = &e.kind {
        if raw.starts_with("SuperProp(") {
            let parent = parent.ok_or_else(|| {
                Unsupported::new("`super.x` in a class with no superclass".to_string())
            })?;
            let name = super_prop_method(raw)?;
            // If the PARENT (or an ancestor) declares a GETTER for `name`, `super.x`
            // must invoke THAT getter with `this` — bypassing any override on the
            // current class (so `super.x` ≠ the virtual `this.x`). Otherwise `x` is a
            // plain instance field (own property) → `this.x` reads the same slot.
            if let Some(getter) = parent.accessor(&name).and_then(|a| a.getter.clone()) {
                e.kind = HirExprKind::Call {
                    callee: Box::new(ident(&getter)),
                    args: vec![this_ident()],
                };
            } else {
                e.kind = HirExprKind::Member {
                    object: Box::new(this_ident()),
                    prop: name,
                };
            }
            return Ok(());
        }
    }
    // Detect a super-shaped Call and rewrite it in place.
    if let HirExprKind::Call { callee, args } = &mut e.kind {
        if let HirExprKind::Raw(raw) = &callee.kind {
            if raw == "callee" {
                return rewrite_super_ctor(e, parent);
            }
            if raw.starts_with("SuperProp(") {
                let method = super_prop_method(raw)?;
                return rewrite_super_method(e, &method, parent);
            }
        }
        // Descend into the (non-super) call's parts.
        rewrite_super_expr(callee, parent)?;
        for a in args {
            rewrite_super_expr(a, parent)?;
        }
        return Ok(());
    }
    descend_super_expr(e, parent)
}

/// `super(args)` → `__rtsn_ctor_Parent(this, args…)`.
fn rewrite_super_ctor(e: &mut HirExpr, parent: Option<&ClassDesc>) -> FrontResult<()> {
    let parent = parent.ok_or_else(|| {
        Unsupported::new("`super(...)` in a class with no superclass".to_string())
    })?;
    let HirExprKind::Call { args, .. } = &e.kind else {
        unreachable!()
    };
    let mut new_args = vec![this_ident()];
    for a in args {
        let mut a = a.clone();
        descend_super_expr(&mut a, Some(parent))?;
        new_args.push(a);
    }
    e.kind = HirExprKind::Call {
        callee: Box::new(ident(&parent.ctor)),
        args: new_args,
    };
    Ok(())
}

/// `super.m(args)` → `__rtsn_method_R_m(this, args…)` where `R` is the ancestor
/// declaring `m` (resolved via the parent's flattened method map).
fn rewrite_super_method(
    e: &mut HirExpr,
    method: &str,
    parent: Option<&ClassDesc>,
) -> FrontResult<()> {
    let parent = parent.ok_or_else(|| {
        Unsupported::new(format!(
            "`super.{method}(...)` in a class with no superclass"
        ))
    })?;
    let fn_name = parent
        .method_fn(method)
        .map(str::to_string)
        .ok_or_else(|| {
            Unsupported::new(format!(
                "`super.{method}()` — no method `{method}` on `{}` or its ancestors",
                parent.name
            ))
        })?;
    let HirExprKind::Call { args, .. } = &e.kind else {
        unreachable!()
    };
    let mut new_args = vec![this_ident()];
    for a in args {
        let mut a = a.clone();
        descend_super_expr(&mut a, Some(parent))?;
        new_args.push(a);
    }
    e.kind = HirExprKind::Call {
        callee: Box::new(ident(&fn_name)),
        args: new_args,
    };
    Ok(())
}

/// Recurse into a non-super expression's sub-expressions so a nested `super` is
/// still rewritten (and so non-super calls keep working).
fn descend_super_expr(e: &mut HirExpr, parent: Option<&ClassDesc>) -> FrontResult<()> {
    match &mut e.kind {
        HirExprKind::Bin { lhs, rhs, .. } => {
            rewrite_super_expr(lhs, parent)?;
            rewrite_super_expr(rhs, parent)
        }
        HirExprKind::Unary { operand, .. } => rewrite_super_expr(operand, parent),
        HirExprKind::Assign { target, value } | HirExprKind::AssignOp { target, value, .. } => {
            rewrite_super_expr(target, parent)?;
            rewrite_super_expr(value, parent)
        }
        HirExprKind::Call { callee, args } => {
            rewrite_super_expr(callee, parent)?;
            for a in args {
                rewrite_super_expr(a, parent)?;
            }
            Ok(())
        }
        HirExprKind::MethodCall { object, args, .. } => {
            rewrite_super_expr(object, parent)?;
            for a in args {
                rewrite_super_expr(a, parent)?;
            }
            Ok(())
        }
        HirExprKind::Member { object, .. } => rewrite_super_expr(object, parent),
        HirExprKind::Index { object, index } => {
            rewrite_super_expr(object, parent)?;
            rewrite_super_expr(index, parent)
        }
        HirExprKind::Ternary { cond, then, else_ } => {
            rewrite_super_expr(cond, parent)?;
            rewrite_super_expr(then, parent)?;
            rewrite_super_expr(else_, parent)
        }
        HirExprKind::Array(elems) => {
            for el in elems {
                rewrite_super_expr(el, parent)?;
            }
            Ok(())
        }
        HirExprKind::New { args, .. } => {
            for a in args {
                rewrite_super_expr(a, parent)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Parse the method name out of a `Raw("SuperProp(SuperPropExpr { … sym: \"m\" … })")`
/// debug string. The prop ident's `sym: "<name>"` is the LAST `sym: "…"` in the
/// string (the obj is `Super` which carries no sym). A malformed string bails.
fn super_prop_method(raw: &str) -> FrontResult<String> {
    let needle = "sym: \"";
    let start = raw
        .rfind(needle)
        .ok_or_else(|| Unsupported::new("could not parse `super.<m>()` method name".to_string()))?
        + needle.len();
    let rest = &raw[start..];
    let end = rest
        .find('"')
        .ok_or_else(|| Unsupported::new("malformed `super.<m>()` method name".to_string()))?;
    Ok(rest[..end].to_string())
}

fn this_ident() -> HirExpr {
    ident(THIS)
}

fn ident(name: &str) -> HirExpr {
    HirExpr::new(HirExprKind::Ident(name.to_string()), HirType::Unknown)
}
