//! Small HIR-node constructors used by the destructuring desugar.
//!
//! Every synthesized binding/access node is typed `HirType::Any` so it rides the
//! `Tagged` PolyValue path (the destructured names are not in the numeric subset);
//! the existing lowerer then resolves each access (`src[0]` / `src.k`) against the
//! source local's proven heap shape, exactly as a hand-written `const a = src[0]`
//! would.

use rts_hir::ir::{HirBinOp, HirExprKind, HirLit};
use rts_hir::{HirExpr, HirStmt, HirType};

/// `ident` reference.
pub(super) fn ident(name: &str) -> HirExpr {
    HirExpr::new(HirExprKind::Ident(name.to_string()), HirType::Any)
}

/// `undefined` literal (the JS default sentinel for a missing element/property).
pub(super) fn undefined() -> HirExpr {
    HirExpr::new(HirExprKind::Lit(HirLit::Undefined), HirType::Any)
}

/// `obj.at(i)` — the element at index `i` via the DYNAMIC array dispatch
/// (`__rtsadp_dyn_at`), which works on ANY Tagged array word (no proven shape
/// required) and yields `undefined` for an out-of-range index — exactly the JS
/// element-read semantics destructuring needs (so a missing element triggers its
/// default).
pub(super) fn elem_at(obj: HirExpr, i: i64) -> HirExpr {
    let idx = HirExpr::new(HirExprKind::Lit(HirLit::Int(i)), HirType::I64);
    HirExpr::new(
        HirExprKind::MethodCall {
            object: Box::new(obj),
            method: "at".to_string(),
            args: vec![idx],
        },
        HirType::Any,
    )
}

/// `obj.__rts_opt_get("prop")` — a property read via the reserved nullish-tolerant
/// getter (lowered to `__rtsadp_obj_get`), which resolves the key at RUNTIME on any
/// Tagged object word (no proven shape required) and yields `undefined` for a
/// missing key — exactly the JS property-read semantics destructuring needs.
pub(super) fn prop_get(obj: HirExpr, prop: &str) -> HirExpr {
    let key = HirExpr::new(
        HirExprKind::Lit(HirLit::Str(prop.to_string())),
        HirType::Str,
    );
    HirExpr::new(
        HirExprKind::MethodCall {
            object: Box::new(obj),
            method: super::super::OPT_GET.to_string(),
            args: vec![key],
        },
        HirType::Any,
    )
}

/// `recv.slice(from, BIG)` — the real array slice (a fresh array) used for a rest
/// element `...rest`. The explicit large end bound (`i32::MAX`, clamped to length by
/// both the static `__rtsadp_arr_slice` and the dynamic `__rtsadp_dyn_slice`) lets
/// the call dispatch through the 2-arg `Array.slice` row regardless of whether the
/// source is a PROVEN array (static dispatch) or an unproven Tagged array (dynamic
/// dispatch) — the 1-arg form is not registered on the static array path.
pub(super) fn slice_from(recv: HirExpr, from: i64) -> HirExpr {
    let start = HirExpr::new(HirExprKind::Lit(HirLit::Int(from)), HirType::I64);
    let end = HirExpr::new(HirExprKind::Lit(HirLit::Int(i32::MAX as i64)), HirType::I64);
    HirExpr::new(
        HirExprKind::MethodCall {
            object: Box::new(recv),
            method: "slice".to_string(),
            args: vec![start, end],
        },
        HirType::Any,
    )
}

/// `(access === undefined) ? default : access` — the JS default-application
/// ternary. The `access` is evaluated twice; the desugar only emits this when the
/// access is a pure read off a side-effect-free source local, so the double read is
/// observationally identical to JS's single read-then-default.
pub(super) fn default_ternary(access: HirExpr, default: HirExpr) -> HirExpr {
    let is_undef = HirExpr::new(
        HirExprKind::Bin {
            op: HirBinOp::StrictEq,
            lhs: Box::new(access.clone()),
            rhs: Box::new(undefined()),
        },
        HirType::Bool,
    );
    HirExpr::new(
        HirExprKind::Ternary {
            cond: Box::new(is_undef),
            then: Box::new(default),
            else_: Box::new(access),
        },
        HirType::Any,
    )
}

/// `const name = init;` — a binding statement (the destructuring leaf).
pub(super) fn const_bind(name: &str, init: HirExpr) -> HirStmt {
    HirStmt::Const {
        name: name.to_string(),
        ty: HirType::Any,
        init,
    }
}
