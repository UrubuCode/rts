//! Tail-call optimization (TCO) — the SELECTIVE tail set + the `return_call`
//! emission for `return f(args)` in tail position.
//!
//! ## Design (mirrors the preserved old-engine optimization, made selective)
//!
//! Cranelift's `return_call` requires BOTH caller and callee to use
//! `CallConv::Tail`. Blanket-switching every user fn to `Tail` is unsound here:
//! some user fns have their RAW code address handed to the runtime and are
//! called back as `extern "C"` (`getPointer(f)` → `thread`/`parallel`;
//! `__RTS_GEN_SM_NEW(state_fn, …)`; the async-inner fns invoked by
//! `promise.create`) — a callconv mismatch there is the historical #206 crash.
//!
//! So the tail set is computed per program: a fn enters it only when it
//! participates in a DIRECT tail edge (`return g(args)` with a bare-Ident
//! callee — never a member/method callee, per the `tco_method_chain_not_tail`
//! regression) AND both endpoints are safe to re-conv:
//!
//! - not `__rts_startup` (the host calls it as `extern "C" fn()`);
//! - not `async` (its inner fn's ptr crosses to the runtime);
//! - not a generator constructor (lazy or eager);
//! - never named as an argument to `getPointer(..)` / `__RTS_GEN_SM_NEW(..)`
//!   anywhere in the program (raw-address takers).
//!
//! Function VALUES are unaffected: a reified fn value carries its uniform-ABI
//! THUNK address (the thunk keeps the default callconv and `call`s the real fn
//! by its declared signature — a normal cross-callconv direct call).
//!
//! At the return site, `return_call` is emitted only when it is ALSO locally
//! sound: no enclosing `try` (a throw in the callee must reach OUR catch — with
//! a tail call our frame is gone) and no enclosing `finally` (JS runs the
//! finalizer AFTER the return expression evaluates — a tail call would skip
//! it), exact positional arity (no spread / rest / defaulted tail / `this`),
//! and the callee's return repr EXACTLY matches ours (Cranelift validates
//! `return_call` return types against the caller's signature).

use std::collections::{HashMap, HashSet};

use cranelift_codegen::ir::InstBuilder;
use cranelift_module::Module;

use rts_hir::ir::{HirExprKind, HirStmt};
use rts_hir::{HirExpr, HirFunc};

use crate::front::error::{FrontResult, Unsupported};

use super::lower::Lowerer;
use super::sig::FnSig;

/// Compute the program's TAIL SET (fn names to compile under `CallConv::Tail`).
/// See the module doc for the rules.
pub(crate) fn compute_tail_set(
    funcs: &[HirFunc],
    sigs: &HashMap<String, FnSig>,
) -> HashSet<String> {
    // Raw-address takers: any fn named as a bare-Ident ARG of `getPointer(..)`
    // or `__RTS_GEN_SM_NEW(..)` anywhere (body of any fn) must keep the default
    // callconv — the runtime calls that address as `extern "C"`.
    let mut addr_taken: HashSet<String> = HashSet::new();
    for f in funcs {
        collect_addr_taken_stmts(&f.body, &mut addr_taken);
    }

    let eligible = |name: &str| -> bool {
        let Some(sig) = sigs.get(name) else {
            return false;
        };
        name != "__rts_startup"
            && !sig.is_async
            && !sig.ret_lazy_gen
            && !sig.ret_eager_gen
            && !addr_taken.contains(name)
    };

    // Tail edges: `return g(args)` with a bare-Ident callee and no spread arg.
    let mut set = HashSet::new();
    for f in funcs {
        if !eligible(&f.name) {
            continue;
        }
        let mut callees: HashSet<String> = HashSet::new();
        collect_tail_callees(&f.body, &mut callees);
        for g in callees {
            if eligible(&g) {
                set.insert(f.name.clone());
                set.insert(g);
            }
        }
    }
    set
}

/// Record every bare-Ident fn name used as an argument of `getPointer(..)` /
/// `__RTS_GEN_SM_NEW(..)` in `stmts` (recursing through every statement and
/// expression that can contain a call).
fn collect_addr_taken_stmts(stmts: &[HirStmt], out: &mut HashSet<String>) {
    for s in stmts {
        walk_stmt_exprs(s, &mut |e| {
            if let HirExprKind::Call { callee, args } = &e.kind {
                if let HirExprKind::Ident(n) = &callee.kind {
                    if n == "getPointer" || n == "__RTS_GEN_SM_NEW" {
                        for a in args {
                            if let HirExprKind::Ident(fn_name) = &a.kind {
                                out.insert(fn_name.clone());
                            }
                        }
                    }
                }
            }
        });
    }
}

/// Collect the callee names of every `return <Ident>(args…)` (no spread) in
/// `stmts`, recursing into nested statement bodies (if/loops/blocks — a tail
/// return anywhere in the body is a tail return when executed). `try`/`finally`
/// bodies are DELIBERATELY included in the walk: membership in the tail SET
/// only switches the callconv; the per-site `try_stack`/`finally_stack` guard
/// in [`Lowerer::try_tail_return_call`] keeps those sites on the plain call.
fn collect_tail_callees(stmts: &[HirStmt], out: &mut HashSet<String>) {
    for s in stmts {
        match s {
            HirStmt::Return(Some(e)) => {
                if let Some(name) = direct_call_callee(e) {
                    out.insert(name);
                }
            }
            other => {
                for b in stmt_child_blocks(other) {
                    collect_tail_callees(b, out);
                }
            }
        }
    }
}

/// The callee name of a DIRECT bare-Ident call with purely positional args
/// (`f(a, b)` — no spread), or `None`. A member/method callee is NEVER a tail
/// call (`return g(n-1).concat([n])`'s tail is `.concat`, not `g` — the
/// `tco_method_chain_not_tail` regression).
fn direct_call_callee(e: &HirExpr) -> Option<String> {
    let HirExprKind::Call { callee, args } = &e.kind else {
        return None;
    };
    let HirExprKind::Ident(name) = &callee.kind else {
        return None;
    };
    if args
        .iter()
        .any(|a| matches!(a.kind, HirExprKind::Spread(_)))
    {
        return None;
    }
    Some(name.clone())
}

/// The nested statement BLOCKS of `s` (bodies the walk recurses into).
fn stmt_child_blocks(s: &HirStmt) -> Vec<&[HirStmt]> {
    match s {
        HirStmt::If { then, else_, .. } => {
            let mut v = vec![then.as_slice()];
            if let Some(el) = else_ {
                v.push(el.as_slice());
            }
            v
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => vec![body.as_slice()],
        HirStmt::For { body, .. } => vec![body.as_slice()],
        HirStmt::ForOf { body, .. } | HirStmt::ForIn { body, .. } => vec![body.as_slice()],
        HirStmt::Block(b) => vec![b.as_slice()],
        HirStmt::Try {
            body,
            catch,
            finally,
        } => {
            let mut v = vec![body.as_slice()];
            if let Some(c) = catch {
                v.push(c.body.as_slice());
            }
            if let Some(f) = finally {
                v.push(f.as_slice());
            }
            v
        }
        HirStmt::Switch { cases, .. } => cases.iter().map(|c| c.body.as_slice()).collect(),
        HirStmt::Labeled { body, .. } => vec![std::slice::from_ref(body.as_ref())],
        _ => Vec::new(),
    }
}

/// Walk every expression contained in `s` (this visits the stmt's OWN exprs,
/// then recurses through [`stmt_child_blocks`]).
fn walk_stmt_exprs(s: &HirStmt, f: &mut impl FnMut(&HirExpr)) {
    match s {
        HirStmt::Expr(e) | HirStmt::Return(Some(e)) | HirStmt::Throw(e) => walk_expr(e, f),
        HirStmt::Let { init, .. } => {
            if let Some(v) = init {
                walk_expr(v, f);
            }
        }
        HirStmt::Const { init, .. } => walk_expr(init, f),
        HirStmt::If { cond, .. } => walk_expr(cond, f),
        HirStmt::While { cond, .. } | HirStmt::DoWhile { cond, .. } => walk_expr(cond, f),
        HirStmt::For {
            init, cond, update, ..
        } => {
            if let Some(i) = init {
                walk_stmt_exprs(i, f);
            }
            if let Some(c) = cond {
                walk_expr(c, f);
            }
            if let Some(u) = update {
                walk_expr(u, f);
            }
        }
        HirStmt::ForOf { iterable, .. } => walk_expr(iterable, f),
        HirStmt::ForIn { object, .. } => walk_expr(object, f),
        HirStmt::Switch {
            discriminant,
            cases,
        } => {
            walk_expr(discriminant, f);
            for c in cases {
                if let Some(t) = &c.test {
                    walk_expr(t, f);
                }
            }
        }
        _ => {}
    }
    for b in stmt_child_blocks(s) {
        for inner in b {
            walk_stmt_exprs(inner, f);
        }
    }
}

/// Walk `e` and every sub-expression, calling `f` on each node.
fn walk_expr(e: &HirExpr, f: &mut impl FnMut(&HirExpr)) {
    f(e);
    match &e.kind {
        HirExprKind::Call { callee, args } => {
            walk_expr(callee, f);
            for a in args {
                walk_expr(a, f);
            }
        }
        HirExprKind::MethodCall { object, args, .. } => {
            walk_expr(object, f);
            for a in args {
                walk_expr(a, f);
            }
        }
        HirExprKind::Bin { lhs, rhs, .. } => {
            walk_expr(lhs, f);
            walk_expr(rhs, f);
        }
        HirExprKind::Unary { operand, .. } => walk_expr(operand, f),
        HirExprKind::Ternary { cond, then, else_ } => {
            walk_expr(cond, f);
            walk_expr(then, f);
            walk_expr(else_, f);
        }
        HirExprKind::Member { object, .. } => walk_expr(object, f),
        HirExprKind::Index { object, index } => {
            walk_expr(object, f);
            walk_expr(index, f);
        }
        HirExprKind::Array(elems) => {
            for el in elems {
                walk_expr(el, f);
            }
        }
        HirExprKind::New { args, .. } => {
            for a in args {
                walk_expr(a, f);
            }
        }
        HirExprKind::Assign { target, value } => {
            walk_expr(target, f);
            walk_expr(value, f);
        }
        HirExprKind::AssignOp { target, value, .. } => {
            walk_expr(target, f);
            walk_expr(value, f);
        }
        HirExprKind::Object(entries) => {
            for (_, v) in entries {
                walk_expr(v, f);
            }
        }
        HirExprKind::Cast { expr, .. } => walk_expr(expr, f),
        HirExprKind::Await(inner) => walk_expr(inner, f),
        HirExprKind::Spread(inner) => walk_expr(inner, f),
        _ => {}
    }
}

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Try to lower `return e` as a Cranelift `return_call`. Returns `Ok(true)`
    /// when the tail call was emitted (the block is terminated), `Ok(false)`
    /// when the site does not qualify (the caller lowers the plain
    /// call-then-return). NOTHING is emitted on the `false` path — every check
    /// runs before any arg lowers, so the fallback never double-evaluates.
    pub(super) fn try_tail_return_call(
        &mut self,
        module: &mut dyn Module,
        e: &HirExpr,
    ) -> FrontResult<bool> {
        // Local soundness: no enclosing try (the callee's throw must reach OUR
        // catch) and no enclosing finally (it must run AFTER the call returns).
        if !self.try_stack.is_empty() || !self.finally_stack.is_empty() {
            return Ok(false);
        }
        let Some(callee_name) = direct_call_callee(e) else {
            return Ok(false);
        };
        // The name must resolve to the top-level user fn — not a shadowing
        // local/closure/gcell/builtin.
        if self.local(&callee_name).is_some()
            || self.captures.contains_key(&callee_name)
            || self.gcell_id(&callee_name).is_some()
            || self.builtins.contains_key(&callee_name)
        {
            return Ok(false);
        }
        let (Some(callee_sig), Some(cur_sig)) = (
            self.sigs.get(&callee_name).cloned(),
            self.sigs.get(&self.current_fn).cloned(),
        ) else {
            return Ok(false);
        };
        // Both endpoints must be in the tail set (CallConv::Tail on both).
        if !callee_sig.tail || !cur_sig.tail {
            return Ok(false);
        }
        // Exact positional arity only (no rest/`this`/defaulted tail) and an
        // EXACTLY matching return repr — `return_call` validates the callee's
        // returns against OUR signature.
        let HirExprKind::Call { args, .. } = &e.kind else {
            return Ok(false);
        };
        if callee_sig.has_this
            || callee_sig.rest_param.is_some()
            || args.len() != callee_sig.params.len()
            || callee_sig.ret != self.ret
        {
            return Ok(false);
        }
        // Marshal the args exactly like a plain user call, then `return_call`.
        let lowered = self.marshal_call_args(module, &callee_sig, None, args)?;
        let cl_sig = callee_sig.to_cranelift(module);
        let callee_id = module
            .declare_function(&callee_name, cranelift_module::Linkage::Local, &cl_sig)
            .map_err(|e| Unsupported::new(format!("declare tail callee `{callee_name}`: {e}")))?;
        let func_ref = self.func_ref(module, callee_id);
        self.builder.ins().return_call(func_ref, &lowered);
        self.block_terminated = true;
        Ok(true)
    }
}
