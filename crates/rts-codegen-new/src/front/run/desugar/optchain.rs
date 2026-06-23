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
/// Reserved nullish-guarded METHOD call (`a?.b(args)`) the lowerer intercepts:
/// `nullish(a) ? undefined : a.b(args)` — a real method dispatch on `a`, NOT a
/// property-read-then-call (a class method is not a data slot). Emitted as
/// `recv.__rts_opt_method_call(<methodNameLit>, …realArgs)`.
pub(crate) const OPT_METHOD_CALL: &str = "__rts_opt_method_call";

/// Parse the leading `span: LO..HI` out of a `Raw("OptChain(OptChainExpr { span:
/// LO..HI, … })")` payload. `None` if not an optional-chain placeholder.
pub(super) fn parse_span(payload: &str) -> Option<(u32, u32)> {
    let rest = payload.strip_prefix("OptChain(")?;
    let idx = rest.find("span:")?;
    let after = rest[idx + "span:".len()..].trim_start();
    let dotdot = after.find("..")?;
    let lo: u32 = after[..dotdot].trim().parse().ok()?;
    let tail = &after[dotdot + 2..];
    let end = tail
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(tail.len());
    let hi: u32 = tail[..end].parse().ok()?;
    Some((lo, hi))
}

/// Desugar an `OptChainExpr` into reserved-method-call HIR, or `None` if it
/// contains a link we don't desugar soundly.
pub(super) fn build_opt_chain(oc: &swc_ecma_ast::OptChainExpr) -> Option<HirExpr> {
    let scope = Scope::new();
    let (base, steps) = flatten(oc, &scope)?;
    let mut cur = base;
    let mut i = 0;
    while i < steps.len() {
        match &steps[i] {
            Step::Get { key } => {
                // METHOD-CALL fusion: a `Get` of a string-literal key IMMEDIATELY
                // followed by a `Call` is `a?.b(args)` — a guarded METHOD CALL, not a
                // property read then a value-call (a class method is not a data slot,
                // so `opt_get` would read `undefined`). Fuse into `opt_method_call`.
                // The receiver `cur` is re-evaluated in the present branch, so it must
                // be side-effect-free (an ident / member / opt_get chain); an impure
                // receiver bails the whole chain (a clean `Raw`, never a double effect).
                if let (Some(name), Some(Step::Call { args, optional: false })) =
                    (str_lit_key(key), steps.get(i + 1))
                {
                    if !is_pure_recv(&cur) {
                        return None;
                    }
                    cur = opt_method_call(cur, name, args.clone());
                    i += 2;
                    continue;
                }
                cur = opt_get(cur, key.clone());
                i += 1;
            }
            Step::Call { args, .. } => {
                cur = opt_call(cur, args.clone());
                i += 1;
            }
        }
    }
    Some(cur)
}

/// The string value of a member key that is a plain string literal (`.name`), or
/// `None` for a computed/non-string key (which is never a method-call fusion).
fn str_lit_key(key: &HirExpr) -> Option<String> {
    match &key.kind {
        HirExprKind::Lit(HirLit::Str(s)) => Some(s.clone()),
        _ => None,
    }
}

/// Whether a receiver expr is safe to re-evaluate (no observable side effect): an
/// identifier, a literal, a plain/optional member read, or an `opt_get` chain over
/// one. A `Call`/`opt_call`/`new` is NOT pure (re-eval would repeat the effect).
fn is_pure_recv(e: &HirExpr) -> bool {
    match &e.kind {
        HirExprKind::Ident(_) | HirExprKind::Lit(_) => true,
        HirExprKind::Member { object, .. } => is_pure_recv(object),
        HirExprKind::MethodCall { method, object, .. } if method == OPT_GET => {
            is_pure_recv(object)
        }
        _ => false,
    }
}

/// A desugared step over the running receiver value.
enum Step {
    /// Nullish-tolerant property/index read (key is a string/computed HIR expr).
    Get { key: HirExpr },
    /// A call of the running value. `optional` is the call link's OWN `?.()` flag:
    /// `true` for `a?.()` (an optional value-call — guards the function value),
    /// `false` for the plain `()` after a member (`a?.b()` — a METHOD call, fused
    /// with the preceding `Get` in [`build_opt_chain`]).
    Call { args: Vec<HirExpr>, optional: bool },
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
            // A member link lowers to `opt_get`, which short-circuits a nullish
            // receiver to `undefined`. This is sound for BOTH an optional `?.b`
            // (optional=true) AND a plain `.c` AFTER an optional link (`a?.b.c`,
            // optional=false): if `a` is nullish, the preceding `opt_get` already
            // yielded `undefined`, and `opt_get(undefined, "c")` is `undefined` — the
            // JS whole-chain short-circuit. So a member link is accepted regardless of
            // its own `optional` flag. (`_optional` intentionally unused.)
            let _ = optional;
            let root = walk_expr(&m.obj, steps, scope)?;
            let key = member_key(&m.prop, scope)?;
            steps.push(Step::Get { key });
            Some(root)
        }
        swc_ecma_ast::OptChainBase::Call(call) => {
            // A call link is accepted regardless of its OWN `optional` flag: an
            // optional call `a?.()` (optional=true) AND a plain call after an optional
            // member `(a?.b)()` (optional=false) both short-circuit when the receiver
            // is nullish — `opt_call` / the fused `opt_method_call` (built in
            // `build_opt_chain`) guards it. `_optional` is intentionally unused.
            let root = walk_expr(&call.callee, steps, scope)?;
            let args = lower_args(&call.args, scope)?;
            steps.push(Step::Call { args, optional });
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

/// Build the guarded method-call op `recv.__rts_opt_method_call(<methodNameLit>,
/// …args)`: the method name rides as a leading string-literal arg the lowerer
/// strips, the rest are the real call args.
fn opt_method_call(recv: HirExpr, method: String, args: Vec<HirExpr>) -> HirExpr {
    let mut all_args = Vec::with_capacity(args.len() + 1);
    all_args.push(HirExpr::new(
        HirExprKind::Lit(HirLit::Str(method)),
        HirType::Str,
    ));
    all_args.extend(args);
    HirExpr::new(
        HirExprKind::MethodCall {
            object: Box::new(recv),
            method: OPT_METHOD_CALL.to_string(),
            args: all_args,
        },
        HirType::Unknown,
    )
}
