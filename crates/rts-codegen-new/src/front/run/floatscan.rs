//! Pre-scan: numeric locals that must bind as `Float64`.
//!
//! A `let` initialized with an integer-valued literal proves `Int64` (swc lowers
//! `0`/`0.0`/`20.0` — no fractional part — to an integer literal, see
//! `rts-hir::lower::lower_lit`). If that same local is later ASSIGNED a value the
//! front cannot prove is integral, binding it `Int64` makes the assign's coerce
//! TRUNCATE the fraction (`fcvt_to_sint_sat`), silently losing it. Two shapes hit
//! this:
//!
//!   1. accumulation of a heap-read number —
//!      `let s = 0; for (const p of ps) { s = s + p.age; }` (`p.age` is a
//!      `number` field, F64 at runtime; `72.7` became `72`);
//!   2. assignment of a fractional value —
//!      `let vy = 0.0; vy = vy - 0.272;` (the literal `0.272` is fractional; `vy`
//!      printed `0` instead of `-0.272` — issue #1869).
//!
//! This scan walks a function body BEFORE lowering and collects every local that
//! is the target of an assignment (`=` or `op=`) whose value tree REQUIRES float
//! — it contains a heap read (member/index the front cannot prove integral) OR a
//! genuinely fractional numeric literal. The `let` numeric tail then binds such a
//! local as `Float64` from the start. JS numbers are f64, so widening an
//! int-inferred local to `Float64` is ALWAYS semantically sound; only reprs that
//! WOULD have been `Int*` are widened (a string/object/Tagged binding is
//! untouched). A local only ever assigned integer-valued literals/locals is NOT
//! promoted — a Fibonacci-style `let a = 0.0; a = a + 1.0;` loop keeps its native
//! unboxed `Int64` fast path.

use std::collections::HashSet;

use rts_hir::HirExpr;
use rts_hir::ir::{HirBinOp, HirExprKind, HirLit, HirStmt};

/// Collect the names needing a `Float64` binding (see module doc).
pub(super) fn float_promoted_locals(body: &[HirStmt]) -> HashSet<String> {
    // A value can reach an assignment through a BINDING instead of appearing in
    // its tree, and the first version of this scan could not see that:
    //
    //   let s = 0;
    //   const x = vals[i];   // the heap read is HERE
    //   s += x as number;    // the RHS is a bare Ident — `tree_requires_float`
    //                        // answered false, `s` stayed Int64, and the coerce
    //                        // in `assign.rs` truncated every fraction
    //
    // Measured before this fix: summing 0, 0.5, 1, 1.5 gave 2 instead of 3, and
    // `s += vals[i]` written INLINE gave the right answer — so the same program
    // returned different numbers depending on whether an intermediate `const`
    // existed. TWO things hid the read: the binding, and the `as number` CAST
    // wrapping the ident (a `Cast` node the scan also walked past).
    // The annotation that tells a reader "this is a number" was part of what hid
    // that it might not be an integer one.
    //
    // So float-ness propagates through bindings: a name bound to a
    // float-requiring tree is itself float-requiring wherever it is READ. The
    // closure is iterated because a binding can name another binding
    // (`const a = vals[0]; const b = a; s += b;`).
    let mut float_bound = HashSet::new();
    loop {
        let before = float_bound.len();
        collect_float_bindings(body, &mut float_bound);
        if float_bound.len() == before {
            break;
        }
    }
    let mut out = HashSet::new();
    walk_stmts(body, &float_bound, &mut out);
    out
}

/// Names bound to a float-requiring tree — `const x = obj.f`, `let y = 0.5`, and
/// (transitively) `const z = x`. Additive: called until it stops growing.
fn collect_float_bindings(stmts: &[HirStmt], bound: &mut HashSet<String>) {
    for s in stmts {
        match s {
            HirStmt::Let {
                name,
                init: Some(e),
                ..
            }
            | HirStmt::Const { name, init: e, .. } => {
                if tree_requires_float(e, bound) {
                    bound.insert(name.clone());
                }
            }
            _ => {}
        }
        // A binding inside a nested block/loop/branch counts the same — this scan
        // is name-based rather than scope-aware, exactly like the promotion it
        // feeds (see the module doc). Widening a local to `Float64` is always
        // semantically sound, so a name collision can only cost the unboxed int
        // path, never correctness.
        for_each_child_block(s, &mut |body| collect_float_bindings(body, bound));
    }
}

/// Run `f` over every statement list nested inside `stmt`.
fn for_each_child_block(stmt: &HirStmt, f: &mut impl FnMut(&[HirStmt])) {
    match stmt {
        HirStmt::If { then, else_, .. } => {
            f(then);
            if let Some(e) = else_ {
                f(e);
            }
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => f(body),
        HirStmt::For { body, .. } | HirStmt::ForOf { body, .. } | HirStmt::ForIn { body, .. } => {
            f(body)
        }
        HirStmt::Block(body) => f(body),
        HirStmt::Try {
            body,
            catch,
            finally,
        } => {
            f(body);
            if let Some(c) = catch {
                f(&c.body);
            }
            if let Some(fin) = finally {
                f(fin);
            }
        }
        HirStmt::Switch { cases, .. } => {
            for c in cases {
                f(&c.body);
            }
        }
        _ => {}
    }
}

fn walk_stmts(stmts: &[HirStmt], bound: &HashSet<String>, out: &mut HashSet<String>) {
    for s in stmts {
        walk_stmt(s, bound, out);
    }
}

fn walk_stmt(stmt: &HirStmt, bound: &HashSet<String>, out: &mut HashSet<String>) {
    match stmt {
        HirStmt::Expr(e) => walk_expr(e, bound, out),
        HirStmt::Let { init: Some(e), .. } => walk_expr(e, bound, out),
        HirStmt::Const { init, .. } => walk_expr(init, bound, out),
        HirStmt::Return(Some(e)) | HirStmt::Throw(e) => walk_expr(e, bound, out),
        HirStmt::If { cond, then, else_ } => {
            walk_expr(cond, bound, out);
            walk_stmts(then, bound, out);
            if let Some(e) = else_ {
                walk_stmts(e, bound, out);
            }
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { cond, body } => {
            walk_expr(cond, bound, out);
            walk_stmts(body, bound, out);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                walk_stmt(i, bound, out);
            }
            if let Some(c) = cond {
                walk_expr(c, bound, out);
            }
            if let Some(u) = update {
                walk_expr(u, bound, out);
            }
            walk_stmts(body, bound, out);
        }
        HirStmt::ForOf { iterable, body, .. } => {
            walk_expr(iterable, bound, out);
            walk_stmts(body, bound, out);
        }
        HirStmt::ForIn { object, body, .. } => {
            walk_expr(object, bound, out);
            walk_stmts(body, bound, out);
        }
        HirStmt::Block(stmts) => walk_stmts(stmts, bound, out),
        HirStmt::Try {
            body,
            catch,
            finally,
        } => {
            walk_stmts(body, bound, out);
            if let Some(c) = catch {
                walk_stmts(&c.body, bound, out);
            }
            if let Some(f) = finally {
                walk_stmts(f, bound, out);
            }
        }
        HirStmt::Switch {
            discriminant,
            cases,
        } => {
            walk_expr(discriminant, bound, out);
            for c in cases {
                if let Some(t) = &c.test {
                    walk_expr(t, bound, out);
                }
                walk_stmts(&c.body, bound, out);
            }
        }
        _ => {}
    }
}

fn walk_expr(e: &HirExpr, bound: &HashSet<String>, out: &mut HashSet<String>) {
    match &e.kind {
        // `x = …` — promote `x` when the assigned value REQUIRES float: it folds
        // in a heap read (`s = s + p.age`, an F64 field) OR a genuinely fractional
        // literal (`vy = vy - 0.272`). Either would truncate against an `Int*`
        // slot. A pure integer-valued tree (`a = b + 1`) leaves `x` unpromoted.
        HirExprKind::Assign { target, value } => {
            if let HirExprKind::Ident(name) = &target.kind {
                if tree_requires_float(value, bound) {
                    out.insert(name.clone());
                }
            }
            walk_expr(value, bound, out);
        }
        // `x op= …` — same rule; `op=` desugars to `x = x op value`, so a
        // fractional/heap-read RHS forces `x` to `Float64` (`vy -= 0.272`).
        HirExprKind::AssignOp { op, target, value } => {
            if let HirExprKind::Ident(name) = &target.kind {
                if is_arith_op(*op) && tree_requires_float(value, bound) {
                    out.insert(name.clone());
                }
            }
            walk_expr(value, bound, out);
        }
        HirExprKind::Bin { lhs, rhs, .. } => {
            walk_expr(lhs, bound, out);
            walk_expr(rhs, bound, out);
        }
        HirExprKind::Unary { operand, .. } | HirExprKind::Cast { expr: operand, .. } => {
            walk_expr(operand, bound, out)
        }
        HirExprKind::Call { callee, args } => {
            walk_expr(callee, bound, out);
            for a in args {
                walk_expr(a, bound, out);
            }
        }
        HirExprKind::MethodCall { object, args, .. } => {
            walk_expr(object, bound, out);
            for a in args {
                walk_expr(a, bound, out);
            }
        }
        HirExprKind::Ternary { cond, then, else_ } => {
            walk_expr(cond, bound, out);
            walk_expr(then, bound, out);
            walk_expr(else_, bound, out);
        }
        _ => {}
    }
}

/// The value tree REQUIRES a float slot — the front cannot prove it integral, so
/// truncating it into an `Int*` slot would drop a fraction. True when the tree
/// contains EITHER:
///   - a HEAP READ (member/index — a `number` field is F64 at runtime), or
///   - a genuinely FRACTIONAL numeric literal (`2.7`, `0.272`; a `.fract() != 0`
///     `Number`/`Float`). Integer-valued literals (`1.0`, which swc already
///     lowered to `Int`; or a `Number(3.0)`) do NOT force a float — a pure
///     integer tree keeps its unboxed `Int64` fast path.
fn tree_requires_float(e: &HirExpr, bound: &HashSet<String>) -> bool {
    match &e.kind {
        HirExprKind::Member { .. } | HirExprKind::Index { .. } => true,
        // A name BOUND to a float-requiring tree carries the requirement to every
        // READ of it. Without this arm the scan could only see a heap read spelled
        // inline, so `const x = vals[i]; s += x;` truncated where `s += vals[i];`
        // did not — the same program, two answers.
        HirExprKind::Ident(n) => bound.contains(n),
        // `x as number` is a `Cast` WRAPPING the ident, so the scan has to see
        // through it. It is a type ASSERTION, not a conversion — the runtime word
        // is unchanged, so whether a fraction can arrive is decided entirely by
        // the operand. Missing this arm was half of the truncation bug: the
        // annotation that tells the reader "this is a number" was the thing
        // hiding that it might not be an integer one.
        HirExprKind::Cast { expr, .. } => tree_requires_float(expr, bound),
        HirExprKind::Lit(HirLit::Number(v) | HirLit::Float(v)) => v.fract() != 0.0,
        HirExprKind::Bin { lhs, rhs, .. } => tree_requires_float(lhs, bound) || tree_requires_float(rhs, bound),
        HirExprKind::Unary { operand, .. } => tree_requires_float(operand, bound),
        _ => false,
    }
}

fn is_arith_op(op: HirBinOp) -> bool {
    matches!(
        op,
        HirBinOp::Add | HirBinOp::Sub | HirBinOp::Mul | HirBinOp::Div
    )
}
