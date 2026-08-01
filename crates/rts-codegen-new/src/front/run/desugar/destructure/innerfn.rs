//! INLINE function bodies — pairing an `HirExprKind::Arrow` with the swc
//! `function`/arrow it was lowered from.
//!
//! The destructuring desugar (and the `var` hoist that rides with it) needs the
//! swc source PAIRED with the HIR it produced. `rts-ast` only carries the
//! MODULE's statements and each top-level `function` declaration, so a body
//! written INLINE — `xs.map(function (o) { … })`, `() => { … }`, a nested
//! `function inner(){…}` — had no swc counterpart and was never visited. Two
//! defects followed, both on the shapes every minified bundle is built from:
//!
//! - `var` was not hoisted there, so an assignment textually BEFORE its `var`
//!   (`R = k + 2; var R;`) bailed "assignment to unbound `R`";
//! - a destructuring binding (`var { R } = o`) kept rts-hir's flattened `"_"`
//!   name, so `R` was never bound at all ("R is not defined" at runtime).
//!
//! ## Correlation
//! Per STATEMENT, both sides are walked depth-first over the statement's OWN
//! expressions (never into a nested statement list — the caller recurses into
//! those, and never into a function body — that is the pairing's own recursion),
//! recording each function node as a LEAF. rts-hir lowers sub-expressions in
//! source order, so the two lists correspond index-for-index.
//!
//! The pairing is accepted only when the lists AGREE — equal length, and equal
//! param count / `async`-ness / function-vs-arrow kind at every index. On any
//! mismatch (a generator, an object-literal method, a construct rts-hir flattened
//! to `Raw`) the whole statement is skipped and its inline bodies keep today's
//! behaviour: an honest bail, never a silently wrong binding.

use rts_hir::ir::{HirArrowBody, HirExprKind, HirStmt};
use rts_hir::HirExpr;

use swc_ecma_ast as swc;

/// What an swc function node exposes for pairing: its body plus the shape fields
/// the guard compares against the HIR arrow.
struct SwcFn<'a> {
    body: SwcBody<'a>,
    params: usize,
    is_async: bool,
    is_fn_expr: bool,
}

enum SwcBody<'a> {
    Block(&'a swc::BlockStmt),
    Expr(&'a swc::Expr),
}

/// Visit every inline function body inside `stmt`'s own expressions, calling
/// `f(hir_body, swc_body)` for each matched pair whose bodies are both BLOCKS.
/// A concise arrow (`x => expr`) has no statement list of its own, so the walk
/// descends through its expression body looking for further nested functions.
pub(super) fn for_each_inner_fn_body(
    stmt: &mut HirStmt,
    swc_stmt: Option<&swc::Stmt>,
    f: &mut impl FnMut(&mut Vec<HirStmt>, &[&swc::Stmt]),
) {
    let Some(swc_stmt) = swc_stmt else { return };
    let mut hir: Vec<&mut HirExpr> = Vec::new();
    hir_stmt_arrows(stmt, &mut hir);
    if hir.is_empty() {
        return;
    }
    let swc_fns = swc_stmt_fns(swc_stmt);
    pair(hir, swc_fns, f);
}

/// Zip a matched HIR-arrow / swc-function list, recursing through concise-arrow
/// expression bodies. Bails as a whole on any disagreement.
fn pair(
    hir: Vec<&mut HirExpr>,
    swc_fns: Vec<SwcFn<'_>>,
    f: &mut impl FnMut(&mut Vec<HirStmt>, &[&swc::Stmt]),
) {
    if hir.len() != swc_fns.len() {
        return;
    }
    // Shape check across the WHOLE list first: a single disagreement means the two
    // walks diverged somewhere, so no pair in this statement can be trusted.
    for (h, s) in hir.iter().zip(swc_fns.iter()) {
        let HirExprKind::Arrow {
            params,
            is_async,
            is_fn_expr,
            ..
        } = &h.kind
        else {
            return;
        };
        if params.len() != s.params || *is_async != s.is_async || *is_fn_expr != s.is_fn_expr {
            return;
        }
    }
    for (h, s) in hir.into_iter().zip(swc_fns.into_iter()) {
        let HirExprKind::Arrow { body, .. } = &mut h.kind else {
            continue;
        };
        match (body, s.body) {
            (HirArrowBody::Block(stmts), SwcBody::Block(b)) => {
                let swc_stmts: Vec<&swc::Stmt> = b.stmts.iter().collect();
                f(stmts, &swc_stmts);
            }
            // A concise arrow body carries no `var`/binding of its own, but may
            // contain further inline functions — keep walking it in lockstep.
            (HirArrowBody::Expr(inner), SwcBody::Expr(e)) => {
                let mut nested: Vec<&mut HirExpr> = Vec::new();
                hir_expr_arrows(inner, &mut nested);
                if !nested.is_empty() {
                    pair(nested, swc_expr_fns(e), f);
                }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// swc side
// ---------------------------------------------------------------------------

/// The function nodes reachable from `stmt`'s OWN expressions (plus a direct
/// nested `function f(){…}` declaration, which rts-hir lowers to an arrow-valued
/// `let`), in depth-first source order.
fn swc_stmt_fns(stmt: &swc::Stmt) -> Vec<SwcFn<'_>> {
    use swc::{Decl, ForHead, Stmt, VarDeclOrExpr};
    let mut out: Vec<SwcFn<'_>> = Vec::new();
    match stmt {
        Stmt::Expr(e) => collect_expr(&e.expr, &mut out),
        Stmt::Return(r) => {
            if let Some(a) = &r.arg {
                collect_expr(a, &mut out);
            }
        }
        Stmt::Throw(t) => collect_expr(&t.arg, &mut out),
        Stmt::Decl(Decl::Var(vd)) => collect_var_decl(vd, &mut out),
        // A nested `function f(){…}` — rts-hir lowers it to `let f = <arrow>`, so
        // the HIR side yields exactly one arrow here too. Generators lower to
        // `Raw` (no arrow): skipped, which forces the guard to reject the
        // statement rather than mispair it.
        Stmt::Decl(Decl::Fn(fd)) if !fd.function.is_generator => {
            if let Some(f) = from_function(&fd.function) {
                out.push(f);
            }
        }
        Stmt::If(i) => collect_expr(&i.test, &mut out),
        Stmt::While(w) => collect_expr(&w.test, &mut out),
        Stmt::DoWhile(w) => collect_expr(&w.test, &mut out),
        Stmt::For(fo) => {
            match &fo.init {
                Some(VarDeclOrExpr::VarDecl(vd)) => collect_var_decl(vd, &mut out),
                Some(VarDeclOrExpr::Expr(e)) => collect_expr(e, &mut out),
                None => {}
            }
            if let Some(t) = &fo.test {
                collect_expr(t, &mut out);
            }
            if let Some(u) = &fo.update {
                collect_expr(u, &mut out);
            }
        }
        Stmt::ForIn(fi) => {
            if let ForHead::VarDecl(vd) = &fi.left {
                collect_var_decl(vd, &mut out);
            }
            collect_expr(&fi.right, &mut out);
        }
        Stmt::ForOf(fo) => {
            if let ForHead::VarDecl(vd) = &fo.left {
                collect_var_decl(vd, &mut out);
            }
            collect_expr(&fo.right, &mut out);
        }
        Stmt::Switch(sw) => collect_expr(&sw.discriminant, &mut out),
        _ => {}
    }
    out
}

fn collect_var_decl<'a>(vd: &'a swc::VarDecl, out: &mut Vec<SwcFn<'a>>) {
    for d in &vd.decls {
        if let Some(init) = &d.init {
            collect_expr(init, out);
        }
    }
}

fn swc_expr_fns(e: &swc::Expr) -> Vec<SwcFn<'_>> {
    let mut out = Vec::new();
    collect_expr(e, &mut out);
    out
}

/// Depth-first walk of `e` recording each arrow / function EXPRESSION as a LEAF
/// (never descending into its body — that is the pairing's own recursion).
///
/// Written by hand rather than with `swc_ecma_visit` because the visitor's node
/// lifetime is anonymous: it cannot hand out borrows into the tree, and cloning
/// every inline body would be quadratic on nested minified code.
///
/// The walk mirrors [`hir_expr_arrows`] operand-for-operand. A form NOT modeled
/// here (an optional chain, a class expression, an object-literal method) simply
/// records nothing — the count guard then rejects the statement, which is the
/// same honest bail as before.
fn collect_expr<'a>(e: &'a swc::Expr, out: &mut Vec<SwcFn<'a>>) {
    use swc::{Callee, Expr, MemberProp, Prop, PropOrSpread};
    match e {
        Expr::Arrow(a) => {
            if !a.is_generator {
                out.push(SwcFn {
                    body: match a.body.as_ref() {
                        swc::BlockStmtOrExpr::BlockStmt(b) => SwcBody::Block(b),
                        swc::BlockStmtOrExpr::Expr(inner) => SwcBody::Expr(inner),
                    },
                    params: a.params.len(),
                    is_async: a.is_async,
                    is_fn_expr: false,
                });
            }
        }
        Expr::Fn(f) => {
            if !f.function.is_generator {
                if let Some(sf) = from_function(&f.function) {
                    out.push(sf);
                }
            }
        }
        Expr::Paren(p) => collect_expr(&p.expr, out),
        Expr::Unary(u) => collect_expr(&u.arg, out),
        Expr::Update(u) => collect_expr(&u.arg, out),
        Expr::Await(a) => collect_expr(&a.arg, out),
        Expr::TsAs(t) => collect_expr(&t.expr, out),
        Expr::TsNonNull(t) => collect_expr(&t.expr, out),
        Expr::TsTypeAssertion(t) => collect_expr(&t.expr, out),
        Expr::TsConstAssertion(t) => collect_expr(&t.expr, out),
        Expr::TsSatisfies(t) => collect_expr(&t.expr, out),
        Expr::Bin(b) => {
            collect_expr(&b.left, out);
            collect_expr(&b.right, out);
        }
        Expr::Assign(a) => {
            // Mirrors the HIR `Assign { target, value }` order. Only a MEMBER
            // target can hold a sub-expression the HIR side also walks.
            if let swc::AssignTarget::Simple(swc::SimpleAssignTarget::Member(m)) = &a.left {
                collect_expr(&m.obj, out);
                if let MemberProp::Computed(c) = &m.prop {
                    collect_expr(&c.expr, out);
                }
            }
            collect_expr(&a.right, out);
        }
        Expr::Member(m) => {
            collect_expr(&m.obj, out);
            if let MemberProp::Computed(c) = &m.prop {
                collect_expr(&c.expr, out);
            }
        }
        Expr::Cond(c) => {
            collect_expr(&c.test, out);
            collect_expr(&c.cons, out);
            collect_expr(&c.alt, out);
        }
        Expr::Call(c) => {
            if let Callee::Expr(callee) = &c.callee {
                collect_expr(callee, out);
            }
            for a in &c.args {
                collect_expr(&a.expr, out);
            }
        }
        // `New { args }` on the HIR side carries no callee expression.
        Expr::New(n) => {
            for a in n.args.iter().flatten() {
                collect_expr(&a.expr, out);
            }
        }
        Expr::Seq(s) => {
            for x in &s.exprs {
                collect_expr(x, out);
            }
        }
        Expr::Array(a) => {
            for el in a.elems.iter().flatten() {
                collect_expr(&el.expr, out);
            }
        }
        Expr::Object(o) => {
            // Only plain `key: value` properties survive into the HIR object
            // (methods/accessors are recovered by `objmethod`, computed KEYS are
            // not modeled), so only their values are walked.
            for p in &o.props {
                if let PropOrSpread::Prop(p) = p {
                    if let Prop::KeyValue(kv) = &**p {
                        collect_expr(&kv.value, out);
                    }
                }
            }
        }
        // A template's interpolations are real HIR by the time this pass runs
        // (the P5.8 desugar rewrote them into a left-associative `+` chain, which
        // walks its operands in the same source order).
        Expr::Tpl(t) => {
            for x in &t.exprs {
                collect_expr(x, out);
            }
        }
        Expr::TaggedTpl(t) => {
            collect_expr(&t.tag, out);
            for x in &t.tpl.exprs {
                collect_expr(x, out);
            }
        }
        _ => {}
    }
}

/// A body-less `function` (an ambient/overload signature) is NOT recorded: there
/// is no `BlockStmt` to pair against. rts-hir still emits an arrow for it, so the
/// count guard rejects the statement — the sound outcome.
fn from_function(f: &swc::Function) -> Option<SwcFn<'_>> {
    Some(SwcFn {
        body: SwcBody::Block(f.body.as_ref()?),
        params: f.params.len(),
        is_async: f.is_async,
        is_fn_expr: true,
    })
}

// ---------------------------------------------------------------------------
// HIR side
// ---------------------------------------------------------------------------

/// The `Arrow` nodes in `stmt`'s OWN expressions, in the same depth-first source
/// order [`swc_stmt_fns`] uses.
fn hir_stmt_arrows<'a>(stmt: &'a mut HirStmt, out: &mut Vec<&'a mut HirExpr>) {
    match stmt {
        HirStmt::Expr(e) | HirStmt::Throw(e) | HirStmt::Return(Some(e)) => {
            hir_expr_arrows(e, out)
        }
        HirStmt::Let { init: Some(e), .. } | HirStmt::Const { init: e, .. } => {
            hir_expr_arrows(e, out)
        }
        HirStmt::If { cond, .. } => hir_expr_arrows(cond, out),
        HirStmt::While { cond, .. } | HirStmt::DoWhile { cond, .. } => hir_expr_arrows(cond, out),
        HirStmt::For {
            init,
            cond,
            update,
            ..
        } => {
            if let Some(i) = init {
                hir_stmt_arrows(i, out);
            }
            if let Some(c) = cond {
                hir_expr_arrows(c, out);
            }
            if let Some(u) = update {
                hir_expr_arrows(u, out);
            }
        }
        HirStmt::ForIn { object: e, .. } | HirStmt::ForOf { iterable: e, .. } => {
            hir_expr_arrows(e, out)
        }
        HirStmt::Switch { discriminant, .. } => hir_expr_arrows(discriminant, out),
        _ => {}
    }
}

fn hir_expr_arrows<'a>(e: &'a mut HirExpr, out: &mut Vec<&'a mut HirExpr>) {
    if matches!(e.kind, HirExprKind::Arrow { .. }) {
        out.push(e);
        return;
    }
    use HirExprKind::*;
    match &mut e.kind {
        Ident(_) | Lit(_) | Raw(_) | Arrow { .. } => {}
        Bin { lhs, rhs, .. }
        | Index {
            object: lhs,
            index: rhs,
        }
        | Assign {
            target: lhs,
            value: rhs,
        }
        | AssignOp {
            target: lhs,
            value: rhs,
            ..
        } => {
            hir_expr_arrows(lhs, out);
            hir_expr_arrows(rhs, out);
        }
        Unary { operand, .. }
        | Cast { expr: operand, .. }
        | Member {
            object: operand, ..
        }
        | Spread(operand)
        | PreInc(operand)
        | PreDec(operand)
        | PostInc(operand)
        | PostDec(operand)
        | Await(operand) => hir_expr_arrows(operand, out),
        Call { callee, args } => {
            hir_expr_arrows(callee, out);
            for a in args {
                hir_expr_arrows(a, out);
            }
        }
        MethodCall { object, args, .. } => {
            hir_expr_arrows(object, out);
            for a in args {
                hir_expr_arrows(a, out);
            }
        }
        New { args, .. } => {
            for a in args {
                hir_expr_arrows(a, out);
            }
        }
        Array(els) => {
            for a in els {
                hir_expr_arrows(a, out);
            }
        }
        Object(fields) => {
            for (_, v) in fields {
                hir_expr_arrows(v, out);
            }
        }
        Ternary { cond, then, else_ } => {
            hir_expr_arrows(cond, out);
            hir_expr_arrows(then, out);
            hir_expr_arrows(else_, out);
        }
        Seq(es) => {
            for a in es {
                hir_expr_arrows(a, out);
            }
        }
    }
}
