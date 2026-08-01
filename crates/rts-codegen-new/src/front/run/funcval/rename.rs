//! Ident-renaming over the HIR statement/expression surface — extracted from
//! [`super`] to keep `mod.rs` shrinking and to give the switch/labeled coverage
//! a focused test home (issue #2071).
//!
//! `rename_ident_stmt`/`_expr` rewrite `Ident(from)` → `to` across a synthesized
//! closure body: a named recursive fn-expression's self-name and a captured
//! `this` param that must not literally be `this`. The match is EXHAUSTIVE (no
//! `_ => {}`): a new `HirStmt`/`HirExprKind` variant is a compile error, not a
//! silently-unrenamed node — which is exactly how `switch`/`labeled` were missed
//! and a lifted React `mapIntoArray` bailed "call to unknown function `z`".

use rts_hir::ir::{HirArrowBody, HirExprKind};
use rts_hir::{HirExpr, HirStmt};

/// Rename every `Ident(from)` to `to` across `stmts` (a synthesized closure body
/// whose captured `this` param must not literally be named `this`). Walks the
/// same expr/stmt surface the extraction handles; an unvisited exotic node keeps
/// the old name and lowers to an unbound-ident bail (honest, never a wrong bind).
pub(super) fn rename_ident_stmts(stmts: &mut [HirStmt], from: &str, to: &str) {
    for s in stmts {
        rename_ident_stmt(s, from, to);
    }
}

fn rename_ident_stmt(s: &mut HirStmt, from: &str, to: &str) {
    match s {
        HirStmt::Expr(e) | HirStmt::Throw(e) => rename_ident_expr(e, from, to),
        HirStmt::Return(Some(e)) => rename_ident_expr(e, from, to),
        HirStmt::Let { init: Some(e), .. } => rename_ident_expr(e, from, to),
        HirStmt::Const { init, .. } => rename_ident_expr(init, from, to),
        HirStmt::If { cond, then, else_ } => {
            rename_ident_expr(cond, from, to);
            rename_ident_stmts(then, from, to);
            if let Some(e) = else_ {
                rename_ident_stmts(e, from, to);
            }
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { cond, body } => {
            rename_ident_expr(cond, from, to);
            rename_ident_stmts(body, from, to);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                rename_ident_stmt(i, from, to);
            }
            if let Some(c) = cond {
                rename_ident_expr(c, from, to);
            }
            if let Some(u) = update {
                rename_ident_expr(u, from, to);
            }
            rename_ident_stmts(body, from, to);
        }
        HirStmt::ForOf { iterable, body, .. } => {
            rename_ident_expr(iterable, from, to);
            rename_ident_stmts(body, from, to);
        }
        HirStmt::ForIn { object, body, .. } => {
            rename_ident_expr(object, from, to);
            rename_ident_stmts(body, from, to);
        }
        HirStmt::Block(b) => rename_ident_stmts(b, from, to),
        HirStmt::Try {
            body,
            catch,
            finally,
        } => {
            rename_ident_stmts(body, from, to);
            if let Some(c) = catch {
                rename_ident_stmts(&mut c.body, from, to);
            }
            if let Some(f) = finally {
                rename_ident_stmts(f, from, to);
            }
        }
        // A `switch` — the discriminant plus every case test AND body. Omitting
        // this left a `z(...)` inside `switch(x){ case a: return z(..) }`
        // UNRENAMED when a recursive `function z` was lifted, so the self-call
        // bailed "call to unknown function `z`" (React's `mapIntoArray`). The
        // `_ => {}` below silently swallowed it — the same match-not-visitor trap
        // that hid a `yield` detector gap.
        HirStmt::Switch {
            discriminant,
            cases,
        } => {
            rename_ident_expr(discriminant, from, to);
            for case in cases {
                if let Some(t) = &mut case.test {
                    rename_ident_expr(t, from, to);
                }
                rename_ident_stmts(&mut case.body, from, to);
            }
        }
        // A labeled statement (`L: for(..) { .. z(..) .. }`) — descend into its body.
        HirStmt::Labeled { body, .. } => rename_ident_stmt(body, from, to),
        HirStmt::Break { .. }
        | HirStmt::Continue { .. }
        | HirStmt::Raw(_)
        | HirStmt::Return(None)
        | HirStmt::Let { init: None, .. } => {}
    }
}

fn rename_ident_expr(e: &mut HirExpr, from: &str, to: &str) {
    if let HirExprKind::Ident(n) = &mut e.kind {
        if n == from {
            *n = to.to_string();
        }
        return;
    }
    match &mut e.kind {
        HirExprKind::Bin { lhs, rhs, .. } => {
            rename_ident_expr(lhs, from, to);
            rename_ident_expr(rhs, from, to);
        }
        HirExprKind::Unary { operand, .. } => rename_ident_expr(operand, from, to),
        HirExprKind::Assign { target, value } | HirExprKind::AssignOp { target, value, .. } => {
            rename_ident_expr(target, from, to);
            rename_ident_expr(value, from, to);
        }
        HirExprKind::Call { callee, args } => {
            rename_ident_expr(callee, from, to);
            args.iter_mut().for_each(|a| rename_ident_expr(a, from, to));
        }
        HirExprKind::MethodCall { object, args, .. } => {
            rename_ident_expr(object, from, to);
            args.iter_mut().for_each(|a| rename_ident_expr(a, from, to));
        }
        HirExprKind::Member { object, .. } => rename_ident_expr(object, from, to),
        HirExprKind::Index { object, index } => {
            rename_ident_expr(object, from, to);
            rename_ident_expr(index, from, to);
        }
        HirExprKind::Ternary { cond, then, else_ } => {
            rename_ident_expr(cond, from, to);
            rename_ident_expr(then, from, to);
            rename_ident_expr(else_, from, to);
        }
        HirExprKind::Array(elems) => elems
            .iter_mut()
            .for_each(|x| rename_ident_expr(x, from, to)),
        HirExprKind::Object(fields) => fields
            .iter_mut()
            .for_each(|(_, v)| rename_ident_expr(v, from, to)),
        HirExprKind::New { args, .. } => {
            args.iter_mut().for_each(|a| rename_ident_expr(a, from, to));
        }
        HirExprKind::Await(inner) | HirExprKind::Spread(inner) => {
            rename_ident_expr(inner, from, to);
        }
        HirExprKind::Seq(items) => items
            .iter_mut()
            .for_each(|x| rename_ident_expr(x, from, to)),
        // A NESTED arrow/fn-expression: the renamed name (a named fn-expr's
        // self-name) is visible inside its body too — descend, unless one of the
        // inner params SHADOWS it. Without this, `function w(n) { return () =>
        // w(n-1); }` kept the inner `w` unrenamed → an unknown free ident → the
        // outer extraction bailed ("expression arrow").
        HirExprKind::Arrow {
            params,
            body,
            self_name,
            ..
        } => {
            let shadowed =
                params.iter().any(|p| p.name == from) || self_name.as_deref() == Some(from);
            if !shadowed {
                match body {
                    rts_hir::ir::HirArrowBody::Expr(inner) => rename_ident_expr(inner, from, to),
                    rts_hir::ir::HirArrowBody::Block(stmts) => rename_ident_stmts(stmts, from, to),
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    //! Cobre o gap de RENAME (issue #2071): ao levantar uma fn recursiva que se
    //! chama dentro de `switch`/`labeled`, a chamada recursiva precisa ser
    //! renomeada junto, senão vira nome não-ligado.
    use super::*;
    use rts_hir::ir::{HirLit, HirSwitchCase, HirType};
    use rts_hir::{HirExpr, HirStmt};

    fn ident(name: &str) -> HirExpr {
        HirExpr::new(HirExprKind::Ident(name.into()), HirType::Any)
    }
    fn call(name: &str) -> HirExpr {
        HirExpr::new(
            HirExprKind::Call { callee: Box::new(ident(name)), args: vec![] },
            HirType::Any,
        )
    }
    fn num(n: f64) -> HirExpr {
        HirExpr::new(HirExprKind::Lit(HirLit::Number(n)), HirType::Number)
    }
    /// Nomes de ident lidos num único statement, para asserção.
    fn idents_of_stmt(s: &HirStmt) -> std::collections::HashSet<String> {
        let mut bound = std::collections::HashSet::new();
        let mut free = std::collections::HashSet::new();
        super::super::scan::collect_free_stmt(s, &mut bound, &mut free);
        free
    }

    #[test]
    fn renomeia_chamada_dentro_de_switch_case() {
        // switch (0) { case 1: return z(); } — a `z` no case deve virar `Z`.
        let mut s = HirStmt::Switch {
            discriminant: num(0.0),
            cases: vec![HirSwitchCase {
                test: Some(num(1.0)),
                body: vec![HirStmt::Return(Some(call("z")))],
            }],
        };
        rename_ident_stmts(std::slice::from_mut(&mut s), "z", "Z");
        let names = idents_of_stmt(&s);
        assert!(names.contains("Z"), "a chamada deveria ter sido renomeada para Z");
        assert!(!names.contains("z"), "nenhuma `z` deveria sobrar");
    }

    #[test]
    fn renomeia_no_test_e_no_discriminante_do_switch() {
        // switch (z) { case z: ; } — ambos os `z` renomeados.
        let mut s = HirStmt::Switch {
            discriminant: ident("z"),
            cases: vec![HirSwitchCase { test: Some(ident("z")), body: vec![] }],
        };
        rename_ident_stmts(std::slice::from_mut(&mut s), "z", "Z");
        let names = idents_of_stmt(&s);
        assert!(names.contains("Z"));
        assert!(!names.contains("z"));
    }

    #[test]
    fn renomeia_dentro_de_switch_ANINHADO() {
        let interno = HirStmt::Switch {
            discriminant: num(0.0),
            cases: vec![HirSwitchCase { test: Some(num(9.0)), body: vec![HirStmt::Return(Some(call("z")))] }],
        };
        let mut externo = HirStmt::Switch {
            discriminant: num(0.0),
            cases: vec![HirSwitchCase { test: Some(num(0.0)), body: vec![interno] }],
        };
        rename_ident_stmts(std::slice::from_mut(&mut externo), "z", "Z");
        let names = idents_of_stmt(&externo);
        assert!(names.contains("Z"));
        assert!(!names.contains("z"), "a `z` no switch aninhado deve ser renomeada");
    }

    #[test]
    fn renomeia_dentro_de_labeled() {
        // L: while (0) { z(); }
        let mut s = HirStmt::Labeled {
            label: "L".into(),
            body: Box::new(HirStmt::While {
                cond: num(0.0),
                body: vec![HirStmt::Expr(call("z"))],
            }),
        };
        rename_ident_stmts(std::slice::from_mut(&mut s), "z", "Z");
        let names = idents_of_stmt(&s);
        assert!(names.contains("Z"));
        assert!(!names.contains("z"));
    }

    #[test]
    fn nao_renomeia_nome_diferente() {
        // switch (0) { case 1: return w(); } — renomear `z` não toca `w`.
        let mut s = HirStmt::Switch {
            discriminant: num(0.0),
            cases: vec![HirSwitchCase { test: Some(num(1.0)), body: vec![HirStmt::Return(Some(call("w")))] }],
        };
        rename_ident_stmts(std::slice::from_mut(&mut s), "z", "Z");
        assert!(idents_of_stmt(&s).contains("w"), "w não deveria mudar");
    }
}
