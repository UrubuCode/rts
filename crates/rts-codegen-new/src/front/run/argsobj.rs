//! The `arguments` object (#450/#299) — a pre-pass over the lowered HirFuncs.
//!
//! A NON-arrow user function whose body references the bare ident `arguments`
//! gets it materialized as an ordinary ARRAY local, riding the existing array
//! machinery (`.length`, `arguments[i]`, iteration):
//!
//! - **No declared params** (`function variadic() { … arguments … }`): a
//!   synthetic VARIADIC rest param named `arguments` is appended — the call
//!   marshal packs EVERY passed arg into a fresh array bound to it (the real
//!   variadic surface, #299).
//! - **Declared params** (`function f(a, b) { … arguments … }`): a prologue
//!   `let arguments = [a, b]` array literal of the declared params (the
//!   old-engine #450 surface — extra args beyond the declared arity are not
//!   observed; a fixed-arity native signature cannot receive them).
//!
//! Arrow functions are skipped (JS: an arrow has no own `arguments`; it reads
//! the enclosing function's — unsupported, so the free ident keeps its honest
//! "unbound identifier" bail). A function already declaring a rest param or an
//! own binding named `arguments` is left untouched.

use rts_hir::ir::{HirExprKind, HirStmt};
use rts_hir::{HirExpr, HirFunc, HirParam, HirType};

/// Materialize `arguments` in every qualifying function of `funcs` (in place).
pub(crate) fn expand_arguments_object(funcs: &mut [HirFunc]) {
    for f in funcs.iter_mut() {
        if f.is_arrow || f.is_async {
            continue;
        }
        // An own param named `arguments` (incl. an already-synthesized rest)
        // shadows the object — nothing to do.
        if f.params.iter().any(|p| p.name == "arguments") {
            continue;
        }
        if !body_references_arguments(&f.body) {
            continue;
        }
        let user_params: Vec<(String, HirType)> = f
            .params
            .iter()
            .filter(|p| p.name != "this")
            .map(|p| (p.name.clone(), p.ty.clone()))
            .collect();
        if user_params.is_empty() && !f.params.iter().any(|p| p.variadic) {
            // Zero-arity fn → real variadic: a rest param sees every call arg.
            f.params.push(HirParam {
                name: "arguments".to_string(),
                ty: HirType::Array(Box::new(HirType::Unknown)),
                variadic: true,
                has_default: false,
                optional: false,
                default_expr: None,
            });
        } else {
            // Declared params → `let arguments = [p1, …, pN]` prologue.
            let elems: Vec<HirExpr> = user_params
                .into_iter()
                .map(|(name, ty)| HirExpr::new(HirExprKind::Ident(name), ty))
                .collect();
            f.body.insert(
                0,
                HirStmt::Let {
                    name: "arguments".to_string(),
                    ty: HirType::Array(Box::new(HirType::Unknown)),
                    init: Some(HirExpr::new(
                        HirExprKind::Array(elems),
                        HirType::Array(Box::new(HirType::Unknown)),
                    )),
                },
            );
        }
    }
}

/// Whether any statement in `body` references the bare ident `arguments`.
/// (Shadowing inner `let arguments` is not tracked — the injected binding is
/// then simply shadowed, matching JS.)
fn body_references_arguments(body: &[HirStmt]) -> bool {
    let mut found = false;
    for s in body {
        walk_stmt(s, &mut found);
        if found {
            return true;
        }
    }
    false
}

fn walk_stmt(s: &HirStmt, found: &mut bool) {
    if *found {
        return;
    }
    match s {
        HirStmt::Expr(e) | HirStmt::Return(Some(e)) | HirStmt::Throw(e) => walk(e, found),
        HirStmt::Let { init, .. } => {
            if let Some(v) = init {
                walk(v, found);
            }
        }
        HirStmt::Const { init, .. } => walk(init, found),
        HirStmt::If { cond, then, else_ } => {
            walk(cond, found);
            for st in then {
                walk_stmt(st, found);
            }
            if let Some(el) = else_ {
                for st in el {
                    walk_stmt(st, found);
                }
            }
        }
        HirStmt::While { cond, body } => {
            walk(cond, found);
            for st in body {
                walk_stmt(st, found);
            }
        }
        HirStmt::DoWhile { body, cond } => {
            for st in body {
                walk_stmt(st, found);
            }
            walk(cond, found);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                walk_stmt(i, found);
            }
            if let Some(c) = cond {
                walk(c, found);
            }
            if let Some(u) = update {
                walk(u, found);
            }
            for st in body {
                walk_stmt(st, found);
            }
        }
        HirStmt::ForOf { iterable, body, .. } => {
            walk(iterable, found);
            for st in body {
                walk_stmt(st, found);
            }
        }
        HirStmt::ForIn { object, body, .. } => {
            walk(object, found);
            for st in body {
                walk_stmt(st, found);
            }
        }
        HirStmt::Try {
            body,
            catch,
            finally,
        } => {
            for st in body {
                walk_stmt(st, found);
            }
            if let Some(c) = catch {
                for st in &c.body {
                    walk_stmt(st, found);
                }
            }
            if let Some(fin) = finally {
                for st in fin {
                    walk_stmt(st, found);
                }
            }
        }
        HirStmt::Switch {
            discriminant,
            cases,
        } => {
            walk(discriminant, found);
            for c in cases {
                if let Some(t) = &c.test {
                    walk(t, found);
                }
                for st in &c.body {
                    walk_stmt(st, found);
                }
            }
        }
        HirStmt::Block(b) => {
            for st in b {
                walk_stmt(st, found);
            }
        }
        HirStmt::Labeled { body, .. } => walk_stmt(body, found),
        _ => {}
    }
}

fn walk(e: &HirExpr, found: &mut bool) {
    if *found {
        return;
    }
    match &e.kind {
        HirExprKind::Ident(n) if n == "arguments" => *found = true,
        HirExprKind::Call { callee, args } => {
            walk(callee, found);
            for a in args {
                walk(a, found);
            }
        }
        HirExprKind::MethodCall { object, args, .. } => {
            walk(object, found);
            for a in args {
                walk(a, found);
            }
        }
        HirExprKind::Bin { lhs, rhs, .. } => {
            walk(lhs, found);
            walk(rhs, found);
        }
        HirExprKind::Unary { operand, .. } => walk(operand, found),
        HirExprKind::Ternary { cond, then, else_ } => {
            walk(cond, found);
            walk(then, found);
            walk(else_, found);
        }
        HirExprKind::Member { object, .. } => walk(object, found),
        HirExprKind::Index { object, index } => {
            walk(object, found);
            walk(index, found);
        }
        HirExprKind::Array(elems) => {
            for el in elems {
                walk(el, found);
            }
        }
        HirExprKind::Object(entries) => {
            for (_, v) in entries {
                walk(v, found);
            }
        }
        HirExprKind::New { args, .. } => {
            for a in args {
                walk(a, found);
            }
        }
        HirExprKind::Assign { target, value } => {
            walk(target, found);
            walk(value, found);
        }
        HirExprKind::AssignOp { target, value, .. } => {
            walk(target, found);
            walk(value, found);
        }
        HirExprKind::Cast { expr, .. } => walk(expr, found),
        HirExprKind::Await(inner) => walk(inner, found),
        HirExprKind::Spread(inner) => walk(inner, found),
        _ => {}
    }
}
