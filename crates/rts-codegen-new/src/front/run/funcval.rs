//! First-class function VALUES — arrow extraction + capture analysis (P4.6).
//!
//! Top-level `const f = (x) => …` is ALREADY hoisted by the parser into a named
//! top-level `HirFunc` (`f`), so `f` lives in `sigs` and a direct call `f(x)`
//! takes the native fast path unchanged. What is NOT hoisted is an INLINE arrow
//! used as a value — an argument (`apply((x) => x + 1, 9)`) or a returned arrow
//! (`return (x) => x * x`). Those stay `HirExprKind::Arrow` nodes.
//!
//! This module runs ONE pre-pass over the program (every top-level function body
//! + the synthesized main body) that, for each NON-CAPTURING inline arrow,
//! synthesizes a fresh top-level `HirFunc` (`__rtsn_arrow_N`) and rewrites the
//! `Arrow` node in place to an `Ident("__rtsn_arrow_N")`. The expr lowering then
//! sees an identifier that resolves to a user function and REIFIES it into a
//! `TAG_FUNCTION` PolyValue (see [`super::expr`] / [`super::call`]).
//!
//! ## Capture rule (conservative — bail, never wrong)
//!
//! An arrow is NON-CAPTURING iff every free identifier in its body is one of its
//! own params, a global (`console`), or the name of a top-level function. Any
//! other free identifier means it captures an outer local — a CLOSURE, a later
//! increment — and the arrow is LEFT as-is so the lowering bails explicitly
//! (`expression arrow`). `this`/async/generator arrows are likewise left to bail.

use std::collections::HashSet;

use rts_hir::ir::{HirArrowBody, HirExprKind};
use rts_hir::{HirExpr, HirFunc, HirStmt, HirType};

/// Names always considered "not a capture" (engine globals the lowering knows).
const GLOBALS: &[&str] = &["console", "undefined", "Infinity", "NaN"];

/// Extract every non-capturing inline arrow from `funcs` + `main` into fresh
/// top-level `HirFunc`s, rewriting each in place to an `Ident` of the synthesized
/// name. Returns the synthesized functions (to append to the program's `funcs`).
///
/// `top_level` is the set of names that resolve to a function (the top-level
/// function names) — a free reference to one of these is NOT a capture.
pub fn extract_arrows(funcs: &mut Vec<HirFunc>, main: &mut HirFunc) -> Vec<HirFunc> {
    let mut top_level: HashSet<String> = funcs.iter().map(|f| f.name.clone()).collect();
    top_level.insert(main.name.clone());

    let mut ctx = Ctx { top_level, synthesized: Vec::new(), counter: 0 };

    // Rewrite arrows inside every existing function body and the main body. Each
    // function's params are in-scope for its body.
    for f in funcs.iter_mut() {
        let params: HashSet<String> = f.params.iter().map(|p| p.name.clone()).collect();
        ctx.rewrite_block(&mut f.body, &params);
    }
    let main_params: HashSet<String> = main.params.iter().map(|p| p.name.clone()).collect();
    ctx.rewrite_block(&mut main.body, &main_params);

    // The synthesized arrow bodies may THEMSELVES contain arrows; rewrite those
    // too (a fixpoint over a growing list — each new function's own params are in
    // scope for its body).
    let mut i = 0;
    while i < ctx.synthesized.len() {
        let mut f = ctx.synthesized[i].clone();
        let params: HashSet<String> = f.params.iter().map(|p| p.name.clone()).collect();
        ctx.rewrite_block(&mut f.body, &params);
        ctx.synthesized[i] = f;
        i += 1;
    }

    ctx.synthesized
}

struct Ctx {
    /// Names resolving to a top-level function (free refs to these are not captures).
    top_level: HashSet<String>,
    /// Functions synthesized from extracted arrows.
    synthesized: Vec<HirFunc>,
    /// Fresh-name counter.
    counter: usize,
}

impl Ctx {
    fn rewrite_block(&mut self, stmts: &mut [HirStmt], scope: &HashSet<String>) {
        // A statement list introduces locals (let/const); accumulate them so a
        // later arrow referencing an earlier local is correctly seen as a capture.
        let mut scope = scope.clone();
        for s in stmts.iter_mut() {
            self.rewrite_stmt(s, &mut scope);
        }
    }

    fn rewrite_stmt(&mut self, s: &mut HirStmt, scope: &mut HashSet<String>) {
        match s {
            HirStmt::Expr(e) => self.rewrite_expr(e, scope),
            HirStmt::Return(Some(e)) => self.rewrite_expr(e, scope),
            HirStmt::Return(None) => {}
            HirStmt::Let { name, init, .. } => {
                if let Some(e) = init {
                    self.rewrite_expr(e, scope);
                }
                scope.insert(name.clone());
            }
            HirStmt::Const { name, init, .. } => {
                self.rewrite_expr(init, scope);
                scope.insert(name.clone());
            }
            HirStmt::If { cond, then, else_ } => {
                self.rewrite_expr(cond, scope);
                self.rewrite_block(then, scope);
                if let Some(e) = else_ {
                    self.rewrite_block(e, scope);
                }
            }
            HirStmt::While { cond, body } => {
                self.rewrite_expr(cond, scope);
                self.rewrite_block(body, scope);
            }
            HirStmt::Block(b) => self.rewrite_block(b, scope),
            // Other statement kinds are outside the lowering subset; any arrow in
            // them will reach the lowering and bail. Leave untouched.
            _ => {}
        }
    }

    fn rewrite_expr(&mut self, e: &mut HirExpr, scope: &HashSet<String>) {
        // Depth-first: rewrite children first, then this node.
        match &mut e.kind {
            HirExprKind::Bin { lhs, rhs, .. } => {
                self.rewrite_expr(lhs, scope);
                self.rewrite_expr(rhs, scope);
            }
            HirExprKind::Unary { operand, .. } => self.rewrite_expr(operand, scope),
            HirExprKind::Assign { target, value } | HirExprKind::AssignOp { target, value, .. } => {
                self.rewrite_expr(target, scope);
                self.rewrite_expr(value, scope);
            }
            HirExprKind::Call { callee, args } => {
                self.rewrite_expr(callee, scope);
                for a in args.iter_mut() {
                    self.rewrite_expr(a, scope);
                }
            }
            HirExprKind::MethodCall { object, args, .. } => {
                self.rewrite_expr(object, scope);
                for a in args.iter_mut() {
                    self.rewrite_expr(a, scope);
                }
            }
            HirExprKind::Member { object, .. } => self.rewrite_expr(object, scope),
            HirExprKind::Index { object, index } => {
                self.rewrite_expr(object, scope);
                self.rewrite_expr(index, scope);
            }
            HirExprKind::Ternary { cond, then, else_ } => {
                self.rewrite_expr(cond, scope);
                self.rewrite_expr(then, scope);
                self.rewrite_expr(else_, scope);
            }
            HirExprKind::Array(elems) => {
                for el in elems.iter_mut() {
                    self.rewrite_expr(el, scope);
                }
            }
            HirExprKind::Object(fields) => {
                for (_, v) in fields.iter_mut() {
                    self.rewrite_expr(v, scope);
                }
            }
            HirExprKind::Arrow { .. } => {
                // Try to extract this arrow into a top-level function. On success,
                // replace the node with an Ident of the synthesized name. (The
                // outer `scope` is not needed: an arrow that references any name
                // outside {own params, globals, top-level fns} is rejected — the
                // capture check below treats every such free ident as a capture.)
                let _ = scope;
                if let Some(name) = self.try_extract(e) {
                    e.kind = HirExprKind::Ident(name);
                    e.ty = HirType::Function { params: Vec::new(), ret: Box::new(HirType::Any) };
                }
                // On failure (capturing/unsupported) leave the Arrow → lowering bails.
            }
            _ => {}
        }
    }

    /// Try to turn `e` (an `Arrow`) into a synthesized non-capturing top-level
    /// function. Returns its name on success, `None` if it captures an outer local
    /// or is otherwise out of scope (async/generator/`this`/block-with-locals are
    /// left to the lowering to bail).
    fn try_extract(&mut self, e: &mut HirExpr) -> Option<String> {
        let HirExprKind::Arrow { params, ret, body } = &e.kind else {
            return None;
        };
        // A variadic / defaulted param is out of this increment's arrow subset.
        if params.iter().any(|p| p.variadic || p.has_default) {
            return None;
        }
        let param_names: HashSet<String> = params.iter().map(|p| p.name.clone()).collect();

        // Build the function body and gather its free identifiers.
        let body_stmts: Vec<HirStmt> = match body {
            HirArrowBody::Expr(inner) => vec![HirStmt::Return(Some((**inner).clone()))],
            HirArrowBody::Block(stmts) => stmts.clone(),
        };
        let mut free = HashSet::new();
        let mut bound = param_names.clone();
        for s in &body_stmts {
            collect_free_stmt(s, &mut bound, &mut free);
        }
        // Capture check: every free ident must be a param, a global, or a
        // top-level function name. `scope` here is the OUTER scope; a free ident
        // that is in the outer local scope (and not top-level) is a CAPTURE.
        for id in &free {
            let ok = param_names.contains(id)
                || GLOBALS.contains(&id.as_str())
                || self.top_level.contains(id);
            if !ok {
                // A free identifier that is none of {own param, global, top-level
                // function} references an OUTER local (a capture) or an unknown
                // name. Either way we cannot soundly lift it this increment → bail
                // (the arrow stays an `Arrow` node and the lowering reports
                // `expression arrow`).
                return None;
            }
        }

        let name = format!("__rtsn_arrow_{}", self.counter);
        self.counter += 1;
        self.top_level.insert(name.clone());
        self.synthesized.push(HirFunc {
            name: name.clone(),
            params: params.clone(),
            ret: ret.clone(),
            body: body_stmts,
            is_async: false,
            is_arrow: true,
        });
        Some(name)
    }
}

// ---------------------------------------------------------------------------
// Free-variable collection over the lowering subset.
// ---------------------------------------------------------------------------

fn collect_free_stmt(s: &HirStmt, bound: &mut HashSet<String>, free: &mut HashSet<String>) {
    match s {
        HirStmt::Expr(e) => collect_free_expr(e, bound, free),
        HirStmt::Return(Some(e)) => collect_free_expr(e, bound, free),
        HirStmt::Return(None) => {}
        HirStmt::Let { name, init, .. } => {
            if let Some(e) = init {
                collect_free_expr(e, bound, free);
            }
            bound.insert(name.clone());
        }
        HirStmt::Const { name, init, .. } => {
            collect_free_expr(init, bound, free);
            bound.insert(name.clone());
        }
        HirStmt::If { cond, then, else_ } => {
            collect_free_expr(cond, bound, free);
            for st in then {
                collect_free_stmt(st, bound, free);
            }
            if let Some(e) = else_ {
                for st in e {
                    collect_free_stmt(st, bound, free);
                }
            }
        }
        HirStmt::While { cond, body } => {
            collect_free_expr(cond, bound, free);
            for st in body {
                collect_free_stmt(st, bound, free);
            }
        }
        HirStmt::Block(b) => {
            for st in b {
                collect_free_stmt(st, bound, free);
            }
        }
        // A statement kind outside the subset: be conservative and treat any
        // identifier it would reference as free (forces a bail). We approximate by
        // not descending — the lowering will bail on the construct itself anyway.
        _ => {}
    }
}

fn collect_free_expr(e: &HirExpr, bound: &HashSet<String>, free: &mut HashSet<String>) {
    match &e.kind {
        HirExprKind::Ident(name) => {
            if !bound.contains(name) {
                free.insert(name.clone());
            }
        }
        HirExprKind::Lit(_) => {}
        HirExprKind::Bin { lhs, rhs, .. } => {
            collect_free_expr(lhs, bound, free);
            collect_free_expr(rhs, bound, free);
        }
        HirExprKind::Unary { operand, .. } => collect_free_expr(operand, bound, free),
        HirExprKind::Assign { target, value } | HirExprKind::AssignOp { target, value, .. } => {
            collect_free_expr(target, bound, free);
            collect_free_expr(value, bound, free);
        }
        HirExprKind::Call { callee, args } => {
            collect_free_expr(callee, bound, free);
            for a in args {
                collect_free_expr(a, bound, free);
            }
        }
        HirExprKind::MethodCall { object, args, .. } => {
            collect_free_expr(object, bound, free);
            for a in args {
                collect_free_expr(a, bound, free);
            }
        }
        HirExprKind::Member { object, .. } => collect_free_expr(object, bound, free),
        HirExprKind::Index { object, index } => {
            collect_free_expr(object, bound, free);
            collect_free_expr(index, bound, free);
        }
        HirExprKind::Ternary { cond, then, else_ } => {
            collect_free_expr(cond, bound, free);
            collect_free_expr(then, bound, free);
            collect_free_expr(else_, bound, free);
        }
        HirExprKind::Array(elems) => {
            for el in elems {
                collect_free_expr(el, bound, free);
            }
        }
        HirExprKind::Object(fields) => {
            for (_, v) in fields {
                collect_free_expr(v, bound, free);
            }
        }
        HirExprKind::PreInc(t)
        | HirExprKind::PreDec(t)
        | HirExprKind::PostInc(t)
        | HirExprKind::PostDec(t) => collect_free_expr(t, bound, free),
        // A nested arrow's OWN params shadow; collect its free vars minus its params.
        HirExprKind::Arrow { params, body, .. } => {
            let mut inner_bound = bound.clone();
            for p in params {
                inner_bound.insert(p.name.clone());
            }
            match body {
                HirArrowBody::Expr(inner) => collect_free_expr(inner, &inner_bound, free),
                HirArrowBody::Block(stmts) => {
                    for st in stmts {
                        collect_free_stmt(st, &mut inner_bound.clone(), free);
                    }
                }
            }
        }
        // Anything else (await/spread/cast/seq/new/raw): conservatively descend
        // where there is an obvious child; otherwise ignore (lowering will bail).
        HirExprKind::Cast { expr, .. } => collect_free_expr(expr, bound, free),
        HirExprKind::Await(inner) | HirExprKind::Spread(inner) => {
            collect_free_expr(inner, bound, free)
        }
        HirExprKind::Seq(items) => {
            for it in items {
                collect_free_expr(it, bound, free);
            }
        }
        HirExprKind::New { args, .. } => {
            for a in args {
                collect_free_expr(a, bound, free);
            }
        }
        HirExprKind::Raw(_) => {}
    }
}
