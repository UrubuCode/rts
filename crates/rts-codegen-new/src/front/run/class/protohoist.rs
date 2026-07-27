//! Which user classes does a function body `new` — the prescan that lets the
//! per-class prototype wiring be hoisted out of loops.
//!
//! ## Why
//!
//! `new C(args)` emitted, per construction:
//!
//! ```text
//!   proto = __rtsadp_class_proto_init(<class name>, <parent name>)   // per CLASS
//!   for each method m: __rtsadp_obj_set(proto, "m", <reified m>)     // per CLASS
//!   __rtsadp_obj_set_proto(instance, proto)                          // per INSTANCE
//! ```
//!
//! Only the last line is per-instance. The first two depend solely on the class —
//! a compile-time constant — and `class_proto_init` is idempotent (it returns the
//! one shared prototype object for that name). Yet all of it ran on EVERY `new`,
//! so a loop allocating N instances paid N × (1 + method-count) extern calls that
//! compute the same thing every pass.
//!
//! `RTS_REPR_STATS=1` on `bench/objbench.ts` (see `docs/specs/FUTURE_OPTIMIZATION.md`)
//! measured this as 2 of the 17 in-loop calls for a class with NO methods; a class
//! with methods pays one more per method per construction.
//!
//! ## Why a prescan rather than a lazy cache
//!
//! The existing [`Lowerer::gcell_cache`](super::super::lower::Lowerer) pattern
//! memoizes at the FIRST use and reuses afterwards. That does not hoist: when the
//! first use is itself inside the loop body, the defining call still executes once
//! per iteration. Genuinely lifting the work out requires emitting it in the ENTRY
//! block, which dominates every use — and Cranelift's `FunctionBuilder` cannot
//! append to a block that is already filled and terminated. So the set of classes
//! has to be known BEFORE the body is lowered. Hence this walk.
//!
//! ## Soundness — why the hoist is GATED
//!
//! `__rtsadp_class_proto_init` is deliberately ORDER-DEPENDENT. Its chain-linking
//! step is guarded by `if proto_of(proto).is_none()`, whose comment states the
//! contract outright: *"NEVER overwrite a [[Prototype]] the user already wired
//! (`F.prototype = Object.create(Base.prototype)` runs before the first `new
//! F()`)"*. It defers that decision to the first construction precisely so a
//! prototype the program replaced beforehand survives.
//!
//! Hoisting to the entry block runs the init BEFORE such an assignment, so the
//! guard sees an unlinked proto, wires it to the default root, and the program's
//! later replacement no longer reaches the instances. Measured: it regressed
//! `tests/fn_prototype_set_explicit.test.ts` (`legs`/`barks` read `undefined`).
//!
//! So the hoist is gated on [`assigns_prototype`]: if ANY function in the program
//! writes through a `.prototype` member, NO function hoists, and every `new`
//! keeps wiring inline exactly as before. The gate is program-wide because the
//! assignment and the `new` can live in different functions. Conservative in the
//! only direction that is safe — a missed hoist costs speed, never correctness.
//!
//! What remains sound under the gate:
//!
//! - Emitting at entry runs the wiring even when the `new` sits behind a branch
//!   that is never taken. That is not observable: `class_proto_init` runs no user
//!   code — it materializes the shared prototype (already reachable as
//!   `C.prototype` from anywhere) and links the extends chain. With no prototype
//!   assignment anywhere in the program, the link it computes is the same one it
//!   would compute later, so only the moment of creation moves, not any value.
//! - A MISS is safe. Anything this walk fails to reach simply keeps the original
//!   per-`new` emission, so the walk being non-exhaustive costs a hoist, never
//!   correctness. (It is written exhaustively anyway, so a new HIR variant fails
//!   the build instead of silently going unhoisted.)
//! - `Arrow` bodies are deliberately NOT descended into: an arrow is lifted to its
//!   own function with its own entry block and its own prescan, so hoisting its
//!   `new` into the ENCLOSING function's entry would wire a class the enclosing
//!   body may never construct — still not wrong, but it would be work attributed
//!   to the wrong frame.

use std::collections::BTreeSet;

use rts_hir::ir::{HirArrowBody, HirExpr, HirExprKind};
use rts_hir::HirStmt;

/// What one walk of a body collects.
#[derive(Default)]
pub(in crate::front::run) struct Scan {
    /// Class names appearing in a `new C(...)`. `BTreeSet` so the emission order
    /// is deterministic across runs (the JIT cache and the resident-prelude bake
    /// both compare emitted code).
    pub classes: BTreeSet<String>,
    /// Whether anything is ASSIGNED through a `.prototype` member (`F.prototype =
    /// …`, `F.prototype.m = …`, `F.prototype[k] = …`). See the module doc: this
    /// disables the hoist program-wide.
    pub proto_assign: bool,
}

/// Walk `body`, excluding nested arrow bodies (each arrow is lowered as its own
/// function and prescanned there).
pub(in crate::front::run) fn scan(body: &[HirStmt]) -> Scan {
    let mut out = Scan::default();
    for s in body {
        walk_stmt(s, &mut out);
    }
    out
}

/// Every class name `body` constructs.
pub(in crate::front::run) fn constructed_classes(body: &[HirStmt]) -> BTreeSet<String> {
    scan(body).classes
}

/// Whether `body` writes through a `.prototype` member. Program-wide truth is the
/// OR over every function plus main — the assignment and the `new` need not share
/// a function.
pub(in crate::front::run) fn assigns_prototype(body: &[HirStmt]) -> bool {
    scan(body).proto_assign
}

/// Does this assignment TARGET reach through a `.prototype`? Recurses the
/// object/index spine so `F.prototype.m`, `F.prototype[k]`, and the bare
/// `F.prototype` all count.
fn target_touches_prototype(e: &HirExpr) -> bool {
    match &e.kind {
        HirExprKind::Member { object, prop } => {
            prop == "prototype" || target_touches_prototype(object)
        }
        HirExprKind::Index { object, .. } => target_touches_prototype(object),
        _ => false,
    }
}

fn walk_block(stmts: &[HirStmt], out: &mut Scan) {
    for s in stmts {
        walk_stmt(s, out);
    }
}

fn walk_stmt(s: &HirStmt, out: &mut Scan) {
    match s {
        HirStmt::Expr(e) | HirStmt::Throw(e) => walk_expr(e, out),
        HirStmt::Return(e) => {
            if let Some(e) = e {
                walk_expr(e, out);
            }
        }
        HirStmt::Let { init, .. } => {
            if let Some(e) = init {
                walk_expr(e, out);
            }
        }
        HirStmt::Const { init, .. } => walk_expr(init, out),
        HirStmt::If { cond, then, else_ } => {
            walk_expr(cond, out);
            walk_block(then, out);
            if let Some(e) = else_ {
                walk_block(e, out);
            }
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            walk_expr(cond, out);
            walk_block(body, out);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                walk_stmt(i, out);
            }
            if let Some(c) = cond {
                walk_expr(c, out);
            }
            if let Some(u) = update {
                walk_expr(u, out);
            }
            walk_block(body, out);
        }
        HirStmt::ForOf { iterable, body, .. } => {
            walk_expr(iterable, out);
            walk_block(body, out);
        }
        HirStmt::ForIn { object, body, .. } => {
            walk_expr(object, out);
            walk_block(body, out);
        }
        HirStmt::Try {
            body,
            catch,
            finally,
        } => {
            walk_block(body, out);
            if let Some(c) = catch {
                walk_block(&c.body, out);
            }
            if let Some(f) = finally {
                walk_block(f, out);
            }
        }
        HirStmt::Switch {
            discriminant,
            cases,
        } => {
            walk_expr(discriminant, out);
            for case in cases {
                if let Some(t) = &case.test {
                    walk_expr(t, out);
                }
                walk_block(&case.body, out);
            }
        }
        HirStmt::Block(b) => walk_block(b, out),
        HirStmt::Labeled { body, .. } => walk_stmt(body, out),
        HirStmt::Break(_) | HirStmt::Continue(_) | HirStmt::Raw(_) => {}
    }
}

fn walk_expr(e: &HirExpr, out: &mut Scan) {
    match &e.kind {
        HirExprKind::New { class, args } => {
            out.classes.insert(class.clone());
            for a in args {
                walk_expr(a, out);
            }
        }

        HirExprKind::Bin { lhs, rhs, .. } => {
            walk_expr(lhs, out);
            walk_expr(rhs, out);
        }
        HirExprKind::Assign { target, value } | HirExprKind::AssignOp { target, value, .. } => {
            if target_touches_prototype(target) {
                out.proto_assign = true;
            }
            walk_expr(target, out);
            walk_expr(value, out);
        }
        HirExprKind::Unary { operand, .. } => walk_expr(operand, out),
        HirExprKind::Call { callee, args } => {
            walk_expr(callee, out);
            for a in args {
                walk_expr(a, out);
            }
        }
        HirExprKind::MethodCall { object, args, .. } => {
            walk_expr(object, out);
            for a in args {
                walk_expr(a, out);
            }
        }
        HirExprKind::Member { object, .. } => walk_expr(object, out),
        HirExprKind::Index { object, index } => {
            walk_expr(object, out);
            walk_expr(index, out);
        }
        HirExprKind::Array(items) => {
            for i in items {
                walk_expr(i, out);
            }
        }
        HirExprKind::Object(entries) => {
            for (_, v) in entries {
                walk_expr(v, out);
            }
        }
        HirExprKind::Ternary { cond, then, else_ } => {
            walk_expr(cond, out);
            walk_expr(then, out);
            walk_expr(else_, out);
        }
        HirExprKind::Cast { expr, .. } => walk_expr(expr, out),
        HirExprKind::Seq(items) => {
            for i in items {
                walk_expr(i, out);
            }
        }
        HirExprKind::Await(inner)
        | HirExprKind::Spread(inner)
        | HirExprKind::PreInc(inner)
        | HirExprKind::PreDec(inner)
        | HirExprKind::PostInc(inner)
        | HirExprKind::PostDec(inner) => walk_expr(inner, out),

        // An arrow is lifted to its OWN function with its own entry block, and gets
        // its own prescan there — see the module doc.
        HirExprKind::Arrow { body, .. } => {
            let _ = body;
            debug_assert!(matches!(
                body,
                HirArrowBody::Expr(_) | HirArrowBody::Block(_)
            ));
        }

        HirExprKind::Lit(_) | HirExprKind::Ident(_) | HirExprKind::Raw(_) => {}
    }
}
