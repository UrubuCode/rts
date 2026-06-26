//! swc binding `Pat` → a list of `const name = <read>` HIR statements.
//!
//! The recursion takes the pattern and the NAME of an already-bound source local
//! (a side-effect-free identifier whose proven heap shape the lowerer resolves
//! accesses against), and produces one `const` binding per leaf identifier reading
//! the corresponding element (array) / property (object) off the source.
//!
//! Returns `None` (a total bail — the caller keeps the original statement, which
//! itself bails at lowering) for any form outside the modeled subset:
//! - a NESTED pattern element/property (`[[a], b]`, `{a: {b}}`): the intermediate
//!   element read yields a shapeless `Tagged` value, so a further destructure off it
//!   cannot resolve a static shape — bailing is sound (never a wrong value);
//! - an OBJECT rest `{a, ...rest}` (needs a new object of the remaining keys — the
//!   shape-minus-a transition is a later increment);
//! - a COMPUTED object key `{[k]: v}` (the runtime-key read path is a later
//!   increment);
//! - an array rest that is not the LAST element (JS forbids it anyway).

use rts_hir::HirStmt;

use super::Gen;
use super::builders::{const_bind, default_ternary, elem_at, ident, prop_get, slice_from};

/// Expand a binding `Pat` reading off source local `src`. The single recursion
/// entry used for both top-level and NESTED patterns: an array/object element
/// that is itself a pattern is bound to a fresh temp (`const __rtsd_n_K = <read>`)
/// and re-expanded off that temp — the intermediate read is a `Tagged` value, but
/// element/property access against it lowers fine (index/prop reads do not require
/// a statically-proven shape), so nesting is sound.
pub(super) fn expand_pat(src: &str, pat: &swc_ecma_ast::Pat, g: &mut Gen) -> Option<Vec<HirStmt>> {
    match pat {
        swc_ecma_ast::Pat::Array(a) => expand_array(src, a, g),
        swc_ecma_ast::Pat::Object(o) => expand_object(src, o, g),
        _ => None,
    }
}

/// Bind nested pattern `pat` off the read `access`: introduce a fresh temp holding
/// `access`, then expand `pat` off that temp.
fn expand_nested(
    access: rts_hir::HirExpr,
    pat: &swc_ecma_ast::Pat,
    g: &mut Gen,
    out: &mut Vec<HirStmt>,
) -> Option<()> {
    let tmp = g.fresh("n");
    out.push(const_bind(&tmp, access));
    out.extend(expand_pat(&tmp, pat, g)?);
    Some(())
}

/// Expand an ARRAY pattern `[p0, p1, ..., ...rest]` reading off source local
/// `src`. A hole (`,`) skips an index; a default applies `=== undefined` ternary;
/// a rest binds `src.slice(i)`; a nested element is bound to a temp and recursed.
pub(super) fn expand_array(
    src: &str,
    pat: &swc_ecma_ast::ArrayPat,
    g: &mut Gen,
) -> Option<Vec<HirStmt>> {
    let mut out = Vec::new();
    for (i, slot) in pat.elems.iter().enumerate() {
        // A hole (elision) — `[, b]` — is `None`; skip the index, bind nothing.
        let Some(elem) = slot else { continue };
        match elem {
            // `...rest` — must be a bare identifier rest, and reads the array tail.
            swc_ecma_ast::Pat::Rest(rest) => {
                let name = leaf_name(&rest.arg)?;
                out.push(const_bind(&name, slice_from(ident(src), i as i64)));
            }
            // `a = default`
            swc_ecma_ast::Pat::Assign(assign) => {
                let name = leaf_name(&assign.left)?;
                let default = rebuild_default(&assign.right)?;
                let access = elem_at(ident(src), i as i64);
                out.push(const_bind(&name, default_ternary(access, default)));
            }
            // `a`
            swc_ecma_ast::Pat::Ident(id) => {
                let name = id.id.sym.to_string();
                out.push(const_bind(&name, elem_at(ident(src), i as i64)));
            }
            // Nested `[...]` / `{...}` element → temp + recurse.
            nested @ (swc_ecma_ast::Pat::Array(_) | swc_ecma_ast::Pat::Object(_)) => {
                expand_nested(elem_at(ident(src), i as i64), nested, g, &mut out)?;
            }
            // Anything else → bail.
            _ => return None,
        }
    }
    Some(out)
}

/// Expand an OBJECT pattern `{a, b: c, d = 5}` reading off source local `src`.
/// Returns `None` for a rest property or a computed key; a nested value pattern is
/// bound to a temp and recursed.
pub(super) fn expand_object(
    src: &str,
    pat: &swc_ecma_ast::ObjectPat,
    g: &mut Gen,
) -> Option<Vec<HirStmt>> {
    let mut out = Vec::new();
    for prop in &pat.props {
        match prop {
            // `{ ...rest }` — needs a new object of the remaining keys → bail.
            swc_ecma_ast::ObjectPatProp::Rest(_) => return None,
            // `{ key: value_pat }` — including `{ a: b }` rename. The VALUE pat may be
            // a plain ident, `ident = default`, or a nested pattern (temp + recurse).
            swc_ecma_ast::ObjectPatProp::KeyValue(kv) => {
                let key = static_key(&kv.key)?;
                match kv.value.as_ref() {
                    swc_ecma_ast::Pat::Ident(id) => {
                        let name = id.id.sym.to_string();
                        out.push(const_bind(&name, prop_get(ident(src), &key)));
                    }
                    swc_ecma_ast::Pat::Assign(assign) => {
                        let name = leaf_name(&assign.left)?;
                        let default = rebuild_default(&assign.right)?;
                        let access = prop_get(ident(src), &key);
                        out.push(const_bind(&name, default_ternary(access, default)));
                    }
                    nested @ (swc_ecma_ast::Pat::Array(_) | swc_ecma_ast::Pat::Object(_)) => {
                        expand_nested(prop_get(ident(src), &key), nested, g, &mut out)?;
                    }
                    _ => return None,
                }
            }
            // `{ a }` shorthand, or `{ a = 5 }` shorthand-with-default.
            swc_ecma_ast::ObjectPatProp::Assign(a) => {
                let name = a.key.sym.to_string();
                let access = prop_get(ident(src), &name);
                match &a.value {
                    Some(default_expr) => {
                        let default = rebuild_default(default_expr)?;
                        out.push(const_bind(&name, default_ternary(access, default)));
                    }
                    None => out.push(const_bind(&name, access)),
                }
            }
        }
    }
    Some(out)
}

/// A binding `Pat` that must be a bare identifier (a leaf). `None` for a nested
/// pattern (which the caller bails on).
fn leaf_name(pat: &swc_ecma_ast::Pat) -> Option<String> {
    match pat {
        swc_ecma_ast::Pat::Ident(id) => Some(id.id.sym.to_string()),
        _ => None,
    }
}

/// A STATIC object-pattern key (`a`, `"a"`, `0`). A computed key `[k]` bails.
fn static_key(key: &swc_ecma_ast::PropName) -> Option<String> {
    match key {
        swc_ecma_ast::PropName::Ident(id) => Some(id.sym.to_string()),
        swc_ecma_ast::PropName::Str(s) => Some(s.value.to_string_lossy().into_owned()),
        swc_ecma_ast::PropName::Num(n) => Some(format!("{}", n.value)),
        // Computed / BigInt keys need the runtime-key path → bail.
        _ => None,
    }
}

/// Lower a default expression (the RHS of `= default`) to HIR via rts-hir's real
/// lowering, accepting it ONLY when it is side-effect-free.
///
/// SOUNDNESS: the desugar applies a default with a ternary
/// `(access === undefined) ? default : access`, and the engine's ternary lowering
/// evaluates BOTH arms eagerly (it is a `select`, not a branch). So a default with
/// an observable SIDE EFFECT (a call, an assignment, `++`) would run even when the
/// element/property is present — diverging from JS, which evaluates a default ONLY
/// when the value is missing. We therefore accept only a PURE default (literal /
/// ident / pure read / pure operator tree); any impure default makes the whole
/// pattern bail (the original `"_"` binding stays and bails at lowering) — never a
/// wrong side-effect order. A `Raw` placeholder (template/optional-chain/regex the
/// desugar would have to recover separately) is likewise rejected.
fn rebuild_default(expr: &swc_ecma_ast::Expr) -> Option<rts_hir::HirExpr> {
    let scope = rts_hir::scope::Scope::new();
    let e = rts_hir::lower::lower_swc_expr(expr, &scope);
    if !is_pure(&e) {
        return None;
    }
    Some(e)
}

/// Whether an HIR expression is side-effect-free AND fully modeled (no `Raw`): no
/// call/new/assignment/increment anywhere, and no unmodeled placeholder. Reads
/// (ident/member/index) and pure operator trees over pure operands are allowed.
fn is_pure(e: &rts_hir::HirExpr) -> bool {
    use rts_hir::ir::HirExprKind;
    match &e.kind {
        // Observable side effects, or an unmodeled placeholder → not pure.
        HirExprKind::Raw(_)
        | HirExprKind::Call { .. }
        | HirExprKind::MethodCall { .. }
        | HirExprKind::New { .. }
        | HirExprKind::Assign { .. }
        | HirExprKind::AssignOp { .. }
        | HirExprKind::PreInc(_)
        | HirExprKind::PreDec(_)
        | HirExprKind::PostInc(_)
        | HirExprKind::PostDec(_)
        | HirExprKind::Await(_)
        | HirExprKind::Arrow { .. }
        | HirExprKind::Spread(_) => false,
        HirExprKind::Lit(_) | HirExprKind::Ident(_) => true,
        HirExprKind::Bin { lhs, rhs, .. }
        | HirExprKind::Index {
            object: lhs,
            index: rhs,
        } => is_pure(lhs) && is_pure(rhs),
        HirExprKind::Unary { operand, .. }
        | HirExprKind::Cast { expr: operand, .. }
        | HirExprKind::Member {
            object: operand, ..
        } => is_pure(operand),
        HirExprKind::Array(els) => els.iter().all(is_pure),
        HirExprKind::Object(fields) => fields.iter().all(|(_, v)| is_pure(v)),
        HirExprKind::Ternary { cond, then, else_ } => {
            is_pure(cond) && is_pure(then) && is_pure(else_)
        }
        HirExprKind::Seq(es) => es.iter().all(is_pure),
    }
}
