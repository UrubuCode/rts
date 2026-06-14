//! Optional-chaining recovery + desugar.
//!
//! We desugar to reserved method calls the lowerer owns, NOT to a static member
//! access — because an optional chain's receiver type is dynamic (it may be
//! `null`/`undefined`/an object), so a static-shape member read would bail.
//!
//! - an OPTIONAL property/index read `a?.b` / `a?.[k]` → `a.__rts_opt_get(key)`,
//!   lowered to `__rtsadp_obj_get(box(a), key)`, which returns `undefined` for a
//!   nullish OR non-object receiver — exactly the JS short-circuit, with NO static
//!   shape needed. Chains compose: `a?.b?.c` → `opt_get(opt_get(a,"b"),"c")`, and a
//!   nullish at any link makes every later `opt_get` see `undefined` and yield
//!   `undefined` — the correct whole-chain short-circuit.
//! - an OPTIONAL call `a?.()` (typically `o?.f?.()`) → `recv.__rts_opt_call(args)`,
//!   lowered to `nullish(recv) ? undefined : invoke(recv, args)` (eval-once: the
//!   receiver is the already-built preceding read, re-evaluated only as the boxed
//!   word, which is pure).
//!
//! SOUNDNESS: we only desugar a chain in which every member/index link is OPTIONAL
//! (`?.`) and every call link is optional. A NON-optional `.c` after an optional
//! link has a dynamic receiver that JS would throw on if nullish — and this engine
//! has no real `throw` — so we BAIL (return `None`) rather than substitute the
//! wrong `undefined`. The required all-optional chains (`o?.a?.b`, `o?.f?.()`, …)
//! are fully covered; a mixed chain stays a `Raw` bail.

use rts_hir::ir::{HirExprKind, HirLit};
use rts_hir::scope::Scope;
use rts_hir::{HirExpr, HirType};

/// Reserved nullish-tolerant property read the lowerer intercepts.
pub(crate) const OPT_GET: &str = "__rts_opt_get";
/// Reserved nullish-guarded value-call the lowerer intercepts.
pub(crate) const OPT_CALL: &str = "__rts_opt_call";

/// Parse the leading `span: LO..HI` out of a `Raw("OptChain(OptChainExpr { span:
/// LO..HI, … })")` payload. `None` if not an optional-chain placeholder.
pub(super) fn parse_span(payload: &str) -> Option<(u32, u32)> {
    let rest = payload.strip_prefix("OptChain(")?;
    let idx = rest.find("span:")?;
    let after = rest[idx + "span:".len()..].trim_start();
    let dotdot = after.find("..")?;
    let lo: u32 = after[..dotdot].trim().parse().ok()?;
    let tail = &after[dotdot + 2..];
    let end = tail.find(|c: char| !c.is_ascii_digit()).unwrap_or(tail.len());
    let hi: u32 = tail[..end].parse().ok()?;
    Some((lo, hi))
}

/// Desugar an `OptChainExpr` into reserved-method-call HIR, or `None` if it
/// contains a link we don't desugar soundly.
pub(super) fn build_opt_chain(oc: &swc_ecma_ast::OptChainExpr) -> Option<HirExpr> {
    let scope = Scope::new();
    let (base, steps) = flatten(oc, &scope)?;
    let mut cur = base;
    for step in steps {
        cur = match step {
            Step::Get { key } => opt_get(cur, key),
            Step::Call { args } => opt_call(cur, args),
        };
    }
    Some(cur)
}

/// A desugared step over the running receiver value.
enum Step {
    /// Nullish-tolerant property/index read (key is a string/computed HIR expr).
    Get { key: HirExpr },
    /// Nullish-guarded call of the running value.
    Call { args: Vec<HirExpr> },
}

/// Flatten a (possibly nested) optional chain into a base root + ordered steps,
/// rejecting any non-optional member/index link (dynamic receiver that JS would
/// throw on) or unsupported form.
fn flatten(oc: &swc_ecma_ast::OptChainExpr, scope: &Scope) -> Option<(HirExpr, Vec<Step>)> {
    let mut steps = Vec::new();
    let base = walk_base(&oc.base, oc.optional, &mut steps, scope)?;
    Some((base, steps))
}

fn walk_base(
    base: &swc_ecma_ast::OptChainBase,
    optional: bool,
    steps: &mut Vec<Step>,
    scope: &Scope,
) -> Option<HirExpr> {
    match base {
        swc_ecma_ast::OptChainBase::Member(m) => {
            let root = walk_expr(&m.obj, steps, scope)?;
            // A member link must be OPTIONAL to short-circuit soundly via opt_get.
            if !optional {
                return None;
            }
            let key = member_key(&m.prop, scope)?;
            steps.push(Step::Get { key });
            Some(root)
        }
        swc_ecma_ast::OptChainBase::Call(call) => {
            let root = walk_expr(&call.callee, steps, scope)?;
            if !optional {
                return None;
            }
            let args = lower_args(&call.args, scope)?;
            steps.push(Step::Call { args });
            Some(root)
        }
    }
}

/// Walk the receiver side of a link. A nested optional chain recurses; a plain
/// member/call inside the spine is only sound when it is NOT after an optional link
/// — but since `flatten` already requires every chain link to be optional, the
/// only plain forms reachable here are the PURE ROOT prefix (e.g. `obj` in
/// `obj.inner?.x`, where `obj.inner` is a plain member evaluated once as the root).
fn walk_expr(e: &swc_ecma_ast::Expr, steps: &mut Vec<Step>, scope: &Scope) -> Option<HirExpr> {
    match e {
        swc_ecma_ast::Expr::OptChain(oc) => walk_base(&oc.base, oc.optional, steps, scope),
        swc_ecma_ast::Expr::Paren(p) => walk_expr(&p.expr, steps, scope),
        // The pure root: lower it via rts-hir directly. A plain `a.b` root or an
        // identifier becomes the first receiver value; opt_get/opt_call tolerate
        // whatever it is at runtime.
        _ => Some(rts_hir::lower::lower_swc_expr(e, scope)),
    }
}

/// The key HIR for a member access: a string literal for `.name`, the lowered
/// index expr for `[k]`. A private name bails.
fn member_key(prop: &swc_ecma_ast::MemberProp, scope: &Scope) -> Option<HirExpr> {
    match prop {
        swc_ecma_ast::MemberProp::Ident(id) => Some(HirExpr::new(
            HirExprKind::Lit(HirLit::Str(id.sym.to_string())),
            HirType::Str,
        )),
        swc_ecma_ast::MemberProp::Computed(c) => {
            Some(rts_hir::lower::lower_swc_expr(&c.expr, scope))
        }
        swc_ecma_ast::MemberProp::PrivateName(_) => None,
    }
}

fn lower_args(args: &[swc_ecma_ast::ExprOrSpread], scope: &Scope) -> Option<Vec<HirExpr>> {
    let mut out = Vec::with_capacity(args.len());
    for a in args {
        if a.spread.is_some() {
            return None;
        }
        out.push(rts_hir::lower::lower_swc_expr(&a.expr, scope));
    }
    Some(out)
}

fn opt_get(recv: HirExpr, key: HirExpr) -> HirExpr {
    HirExpr::new(
        HirExprKind::MethodCall {
            object: Box::new(recv),
            method: OPT_GET.to_string(),
            args: vec![key],
        },
        HirType::Unknown,
    )
}

fn opt_call(recv: HirExpr, args: Vec<HirExpr>) -> HirExpr {
    HirExpr::new(
        HirExprKind::MethodCall {
            object: Box::new(recv),
            method: OPT_CALL.to_string(),
            args,
        },
        HirType::Unknown,
    )
}
