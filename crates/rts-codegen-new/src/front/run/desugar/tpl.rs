//! Template-literal recovery + the swc collector shared with optional chaining.
//!
//! The collector ([`walk_stmt`]) does a canonical depth-first swc walk recording
//! every `Tpl` (by source position) and `OptChainExpr` (by byte span), treating
//! each as a LEAF — it never descends into a recorded node's children, because the
//! HIR flattened that whole construct to a single `Raw` placeholder and the
//! builders ([`build_template`] / [`super::optchain::build_opt_chain`]) recover the
//! entire subtree directly from swc (including any NESTED template/chain). Stopping
//! at the leaf keeps the collector's node set in exact 1:1 correspondence with the
//! HIR placeholders.
//!
//! [`build_template`] desugars `` `q0${e0}q1${e1}…` `` to a left-associative `+`
//! chain `q0 + e0 + q1 + e1 + …` seeded with the first quasi as a STRING literal,
//! so the whole chain is string-typed and every interpolation coerces through the
//! one `__rtsadp_add` ToString path (no divergent coercion).

use swc_ecma_visit::{Visit, VisitWith};

use rts_hir::ir::{HirBinOp, HirExprKind, HirLit};
use rts_hir::scope::Scope;
use rts_hir::{HirExpr, HirType};

use super::Recovered;

/// Collect every `Tpl` / `OptChainExpr` reachable from `stmt` into `acc`.
pub(super) fn walk_stmt(stmt: &swc_ecma_ast::Stmt, acc: &mut Recovered) {
    let mut c = Collector { acc };
    stmt.visit_with(&mut c);
    // Templates are matched positionally in document order; the visitor records
    // them in source order already, but sorting by span.lo is a cheap guarantee
    // that the order is canonical regardless of visitor field order.
    acc.templates.sort_by_key(|t| t.span.lo.0);
}

struct Collector<'a> {
    acc: &'a mut Recovered,
}

impl Visit for Collector<'_> {
    fn visit_tpl(&mut self, node: &swc_ecma_ast::Tpl) {
        // Record + STOP: do not descend into `node.exprs` (a nested template/chain
        // is recovered by `build_template` straight from swc).
        self.acc.templates.push(node.clone());
    }

    fn visit_opt_chain_expr(&mut self, node: &swc_ecma_ast::OptChainExpr) {
        // Record by exact byte span + STOP (nested chains are recovered by
        // `build_opt_chain`).
        let span = (node.span.lo.0, node.span.hi.0);
        self.acc.opt_chains.insert(span, node.clone());
    }

    fn visit_tagged_tpl(&mut self, _node: &swc_ecma_ast::TaggedTpl) {
        // A TAGGED template (`tag\`…\``) is a SEPARATE feature — do NOT record it as
        // a plain template (its HIR is `Raw("TaggedTpl(…)")`, which has no
        // `template_literal` placeholder anyway). Stop here so an inner plain
        // template inside the tag args is not mis-attributed.
    }
}

/// Build the HIR `+`-chain for a template literal.
///
/// `quasis` has exactly `exprs.len() + 1` elements; the result is
/// `q0 + e0 + q1 + e1 + … + qN`. The first operand is the first quasi as a string
/// literal, which forces the whole left-associative chain to string concatenation
/// (so `${5}` coerces to "5" via the generic `+`). An empty template `` `` `` (one
/// empty quasi, no exprs) builds the `""` literal.
pub(super) fn build_template(tpl: &swc_ecma_ast::Tpl) -> HirExpr {
    let scope = Scope::new();
    let quasi_str = |i: usize| -> String {
        tpl.quasis
            .get(i)
            .map(|q| cooked_quasi(q))
            .unwrap_or_default()
    };

    // Seed with the first quasi as a string literal.
    let mut acc = str_lit(quasi_str(0));
    for (i, expr) in tpl.exprs.iter().enumerate() {
        // Interpolation: lower the swc expr to HIR (nested templates/chains inside
        // it stay as their own `Raw` placeholders — the caller's unit walk does not
        // re-reach them, so a nested template inside an interpolation that we DID
        // record as a leaf is rebuilt right here).
        let interp = rebuild_interp(expr, &scope);
        acc = add(acc, interp);
        // Following quasi.
        acc = add(acc, str_lit(quasi_str(i + 1)));
    }
    acc
}

/// Lower an interpolation expression to HIR. A nested `Tpl` / `OptChainExpr` is
/// rebuilt directly (rts-hir would only give a `Raw` placeholder); everything else
/// goes through rts-hir's real lowering.
///
/// A template interpolation uses the JS STRING hint for `ToPrimitive` — which DIFFERS
/// from the `+` operator's DEFAULT hint for an object with a `valueOf` (`` `${o}` ``
/// runs `toString`, `"" + o` runs `valueOf`). The string-seeded `+` chain this
/// builder produces would otherwise apply the default hint to an OBJECT interpolation.
/// So an object-producing interpolation (an object literal or a `new C()`) is wrapped
/// in `String(<expr>)`, which the lowerer resolves with the correct STRING hint
/// (P5.15). Primitive interpolations keep the `+`-ToString path (byte-identical).
fn rebuild_interp(expr: &swc_ecma_ast::Expr, scope: &Scope) -> HirExpr {
    match expr {
        swc_ecma_ast::Expr::Tpl(t) => build_template(t),
        swc_ecma_ast::Expr::OptChain(oc) => {
            super::optchain::build_opt_chain(oc).unwrap_or_else(|| rts_hir::lower::lower_swc_expr(expr, scope))
        }
        swc_ecma_ast::Expr::Paren(p) => rebuild_interp(&p.expr, scope),
        // An OBJECT interpolation (object literal, `new C()`, or a bare identifier
        // that may be bound to a ToPrimitive object) needs the STRING hint → route
        // through `String(...)`. For a primitive bound to the identifier `String()`
        // ToStrings byte-identically to the `+` path, so this is safe for all idents.
        swc_ecma_ast::Expr::Object(_)
        | swc_ecma_ast::Expr::New(_)
        | swc_ecma_ast::Expr::Ident(_) => {
            wrap_string_call(rts_hir::lower::lower_swc_expr(expr, scope))
        }
        _ => rts_hir::lower::lower_swc_expr(expr, scope),
    }
}

/// Wrap `arg` in a `String(arg)` HIR call (the global `String` conversion), so a
/// template interpolation of an object runs the STRING-hint `ToPrimitive`.
fn wrap_string_call(arg: HirExpr) -> HirExpr {
    let callee = HirExpr::new(HirExprKind::Ident("String".to_string()), HirType::Unknown);
    HirExpr::new(
        HirExprKind::Call { callee: Box::new(callee), args: vec![arg] },
        HirType::Str,
    )
}

/// The cooked text of a quasi (the literal segment), with the standard CR
/// normalization swc documents on `raw`. Prefer `cooked` (escape-resolved); fall
/// back to `raw` only if cooked is absent (an invalid escape — rare).
fn cooked_quasi(q: &swc_ecma_ast::TplElement) -> String {
    if let Some(cooked) = &q.cooked {
        return cooked.to_string_lossy().into_owned();
    }
    q.raw.replace("\r\n", "\n").replace('\r', "\n")
}

/// A string-literal HIR expr.
fn str_lit(s: String) -> HirExpr {
    HirExpr::new(HirExprKind::Lit(HirLit::Str(s)), HirType::Str)
}

/// `lhs + rhs` as an HIR `Add` (string-typed: the chain is seeded by a string).
fn add(lhs: HirExpr, rhs: HirExpr) -> HirExpr {
    HirExpr::new(
        HirExprKind::Bin { op: HirBinOp::Add, lhs: Box::new(lhs), rhs: Box::new(rhs) },
        HirType::Str,
    )
}
