//! Function-DECLARATION hoisting — a pre-pass over a function/module body.
//!
//! JS hoists a `function f(){…}` DECLARATION fully — both the NAME and its
//! VALUE — to the top of the enclosing scope, so a call `f()` textually BEFORE
//! the declaration resolves. This differs from `var` hoisting (name only,
//! value `undefined` until the assignment) and from `const f = () => …` /
//! `const f = function g(){}` (a plain binding, NOT hoisted).
//!
//! rts-hir lowers a nested `function f(){…}` declaration to
//! `HirStmt::Let { name: f, init: Arrow { self_name: Some(f), … } }` (see
//! `lower_decl`'s `Decl::Fn` arm) — the marker `self_name == name` identifies a
//! DECLARATION (a named fn-EXPRESSION assigned to a `const`/`let` lowers to a
//! `Const`, or to a `Let` whose `self_name != name`). This pass moves every such
//! direct-body statement to the FRONT of the body, preserving their relative
//! order, so the value is bound before any earlier statement runs.
//!
//! Scope: only DIRECT body statements are hoisted (a function declared inside a
//! nested `{ }` block is block-scoped in a strict/module context — TS is always
//! strict — so it must NOT leak to the function top). Nested fn/arrow bodies are
//! separate `HirFunc`s handled on their own.

use rts_hir::ir::{HirExprKind, HirStmt};

/// Whether `s` is a direct function DECLARATION (`function f(){…}`), identified
/// by the `self_name == name` marker rts-hir stamps on the lowered arrow.
fn is_fn_decl(s: &HirStmt) -> bool {
    if let HirStmt::Let {
        name,
        init: Some(init),
        ..
    } = s
    {
        if let HirExprKind::Arrow {
            self_name: Some(sn),
            ..
        } = &init.kind
        {
            return sn == name;
        }
    }
    false
}

/// Move every direct function-declaration statement in `body` to the FRONT,
/// preserving the relative order of both the declarations and the remaining
/// statements (a stable partition). A no-op when the body has no declarations or
/// they are already all leading.
pub(crate) fn hoist_fn_decls(body: &mut Vec<HirStmt>) {
    if !body.iter().any(is_fn_decl) {
        return;
    }
    // Stable partition: declarations first (in source order), then the rest.
    let taken = std::mem::take(body);
    let mut decls = Vec::new();
    let mut rest = Vec::new();
    for s in taken {
        if is_fn_decl(&s) {
            decls.push(s);
        } else {
            rest.push(s);
        }
    }
    decls.extend(rest);
    *body = decls;
}
