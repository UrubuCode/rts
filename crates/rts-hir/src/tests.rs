//! Unit tests for HIR type refinement and promotion.
//!
//! Cover the public surface that rts-codegen consumes (refine_type_from_init
//! + numeric_promotion + Scope) without depending on SWC parsing.

use crate::ir::*;
use crate::scope::Scope;
use crate::type_refine::{numeric_promotion, refine_func_ret, refine_type_from_init};
use crate::type_map::CraneliftTypeHint;

fn lit_int(n: i64) -> HirExpr {
    HirExpr::new(HirExprKind::Lit(HirLit::Int(n)), HirType::Unknown)
}
fn lit_float(n: f64) -> HirExpr {
    HirExpr::new(HirExprKind::Lit(HirLit::Float(n)), HirType::Unknown)
}
fn lit_num(n: f64) -> HirExpr {
    HirExpr::new(HirExprKind::Lit(HirLit::Number(n)), HirType::Unknown)
}
fn lit_bool(b: bool) -> HirExpr {
    HirExpr::new(HirExprKind::Lit(HirLit::Bool(b)), HirType::Unknown)
}
fn ident(name: &str) -> HirExpr {
    HirExpr::new(HirExprKind::Ident(name.into()), HirType::Unknown)
}
fn bin(op: HirBinOp, lhs: HirExpr, rhs: HirExpr) -> HirExpr {
    HirExpr::new(
        HirExprKind::Bin {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        },
        HirType::Unknown,
    )
}

#[test]
fn refine_int_literal_to_i64() {
    let s = Scope::new();
    assert_eq!(refine_type_from_init(&lit_int(0), &s), HirType::I64);
    assert_eq!(refine_type_from_init(&lit_int(-42), &s), HirType::I64);
}

#[test]
fn refine_float_literal_to_f64() {
    let s = Scope::new();
    assert_eq!(refine_type_from_init(&lit_float(1.5), &s), HirType::F64);
}

#[test]
fn refine_js_number_to_number() {
    let s = Scope::new();
    assert_eq!(refine_type_from_init(&lit_num(3.0), &s), HirType::Number);
}

#[test]
fn refine_bool_literal() {
    let s = Scope::new();
    assert_eq!(refine_type_from_init(&lit_bool(true), &s), HirType::Bool);
}

#[test]
fn refine_arithmetic_int_int_to_i64() {
    let s = Scope::new();
    let e = bin(HirBinOp::Add, lit_int(1), lit_int(2));
    assert_eq!(refine_type_from_init(&e, &s), HirType::I64);
}

#[test]
fn refine_arithmetic_int_float_promotes_to_f64() {
    let s = Scope::new();
    let e = bin(HirBinOp::Mul, lit_int(2), lit_float(0.5));
    assert_eq!(refine_type_from_init(&e, &s), HirType::F64);
}

#[test]
fn refine_comparison_to_bool() {
    let s = Scope::new();
    let e = bin(HirBinOp::Lt, lit_int(1), lit_int(2));
    assert_eq!(refine_type_from_init(&e, &s), HirType::Bool);
}

#[test]
fn refine_bitwise_int_int_keeps_int() {
    let s = Scope::new();
    let e = bin(HirBinOp::BitAnd, lit_int(0xff), lit_int(0x0f));
    assert_eq!(refine_type_from_init(&e, &s), HirType::I64);
}

#[test]
fn refine_ident_uses_scope() {
    let mut s = Scope::new();
    s.define("counter", HirType::I32);
    assert_eq!(refine_type_from_init(&ident("counter"), &s), HirType::I32);
}

#[test]
fn refine_ident_unknown_when_undefined() {
    let s = Scope::new();
    assert_eq!(refine_type_from_init(&ident("nope"), &s), HirType::Unknown);
}

#[test]
fn refine_call_uses_return_type_from_scope() {
    let mut s = Scope::new();
    s.register_return_type("step", HirType::I64);
    let call = HirExpr::new(
        HirExprKind::Call {
            callee: Box::new(ident("step")),
            args: vec![lit_int(0)],
        },
        HirType::Unknown,
    );
    assert_eq!(refine_type_from_init(&call, &s), HirType::I64);
}

#[test]
fn refine_call_unknown_when_callee_not_registered() {
    let s = Scope::new();
    let call = HirExpr::new(
        HirExprKind::Call {
            callee: Box::new(ident("ghost")),
            args: vec![],
        },
        HirType::Unknown,
    );
    assert_eq!(refine_type_from_init(&call, &s), HirType::Unknown);
}

#[test]
fn refine_cast_uses_target() {
    let s = Scope::new();
    let cast = HirExpr::new(
        HirExprKind::Cast {
            expr: Box::new(lit_int(0)),
            target: HirType::I32,
        },
        HirType::Unknown,
    );
    assert_eq!(refine_type_from_init(&cast, &s), HirType::I32);
}

#[test]
fn promotion_f64_dominates() {
    assert_eq!(numeric_promotion(HirType::I64, HirType::F64), HirType::F64);
    assert_eq!(numeric_promotion(HirType::F64, HirType::I32), HirType::F64);
}

#[test]
fn promotion_widens_int_pairs() {
    assert_eq!(numeric_promotion(HirType::I32, HirType::I64), HirType::I64);
    assert_eq!(numeric_promotion(HirType::I8, HirType::I16), HirType::I16);
    assert_eq!(numeric_promotion(HirType::I64, HirType::U32), HirType::I64);
}

#[test]
fn promotion_unknown_falls_back_to_number() {
    assert_eq!(
        numeric_promotion(HirType::Unknown, HirType::I64),
        HirType::Number
    );
    assert_eq!(
        numeric_promotion(HirType::Any, HirType::F64),
        HirType::Number
    );
}

#[test]
fn cranelift_hint_maps_basic() {
    assert_eq!(HirType::I64.cranelift_hint(), Some(CraneliftTypeHint::I64));
    assert_eq!(HirType::I32.cranelift_hint(), Some(CraneliftTypeHint::I32));
    assert_eq!(HirType::I8.cranelift_hint(), Some(CraneliftTypeHint::I8));
    assert_eq!(HirType::U32.cranelift_hint(), Some(CraneliftTypeHint::I32));
    assert_eq!(HirType::F32.cranelift_hint(), Some(CraneliftTypeHint::F32));
    assert_eq!(HirType::F64.cranelift_hint(), Some(CraneliftTypeHint::F64));
    assert_eq!(
        HirType::Number.cranelift_hint(),
        Some(CraneliftTypeHint::F64)
    );
    assert_eq!(
        HirType::Bool.cranelift_hint(),
        Some(CraneliftTypeHint::I64)
    );
    assert_eq!(
        HirType::Handle(HandleKind::String).cranelift_hint(),
        Some(CraneliftTypeHint::I64)
    );
    assert_eq!(
        HirType::Str.cranelift_hint(),
        Some(CraneliftTypeHint::StrPtr)
    );
    assert_eq!(HirType::Void.cranelift_hint(), Some(CraneliftTypeHint::Void));
}

#[test]
fn cranelift_hint_none_for_compound_and_unknown() {
    assert_eq!(HirType::Any.cranelift_hint(), None);
    assert_eq!(HirType::Unknown.cranelift_hint(), None);
    assert_eq!(HirType::Object.cranelift_hint(), None);
    assert_eq!(
        HirType::Array(Box::new(HirType::I64)).cranelift_hint(),
        None
    );
    assert_eq!(HirType::Class(ClassId(0)).cranelift_hint(), None);
}

#[test]
fn scope_nesting_isolates_inner_definitions() {
    let mut s = Scope::new();
    s.define("x", HirType::I64);
    s.push();
    s.define("y", HirType::F64);
    assert_eq!(s.lookup("x"), Some(&HirType::I64));
    assert_eq!(s.lookup("y"), Some(&HirType::F64));
    s.pop();
    assert_eq!(s.lookup("x"), Some(&HirType::I64));
    assert_eq!(s.lookup("y"), None);
}

#[test]
fn scope_inner_shadows_outer() {
    let mut s = Scope::new();
    s.define("x", HirType::I64);
    s.push();
    s.define("x", HirType::F64);
    assert_eq!(s.lookup("x"), Some(&HirType::F64));
    s.pop();
    assert_eq!(s.lookup("x"), Some(&HirType::I64));
}

#[test]
fn parse_type_annotation_object_keyword_and_literal() {
    use crate::lower::parse_type_annotation;
    // Both the `object` keyword and the `{object}` sentinel emitted for an
    // object-type-literal map to the distinct `HirType::Object` (not `any`).
    assert_eq!(parse_type_annotation("object"), HirType::Object);
    assert_eq!(parse_type_annotation("{object}"), HirType::Object);
    assert_eq!(parse_type_annotation("any"), HirType::Any);
    assert_eq!(parse_type_annotation("number"), HirType::Number);
}

#[test]
fn binop_strict_and_loose_eq_are_distinct_comparisons() {
    // The new strict ops are distinct variants but still classify as comparisons
    // (so their result type refines to Bool), and remain != the loose ops.
    assert!(HirBinOp::StrictEq.is_comparison());
    assert!(HirBinOp::StrictNe.is_comparison());
    assert!(HirBinOp::Eq.is_comparison());
    assert_ne!(HirBinOp::Eq, HirBinOp::StrictEq);
    assert_ne!(HirBinOp::Ne, HirBinOp::StrictNe);
}

#[test]
fn unop_plus_distinct_from_not() {
    // Unary `+` (ToNumber) is no longer conflated with `!`.
    assert_ne!(HirUnOp::Plus, HirUnOp::Not);
}

#[test]
fn refine_ret_all_int_returns_to_i64() {
    let s = Scope::new();
    let body = vec![
        HirStmt::Return(Some(lit_int(0))),
    ];
    assert_eq!(refine_func_ret(&HirType::Unknown, &body, &s), HirType::I64);
}

#[test]
fn refine_ret_mixed_int_float_promotes_to_f64() {
    let s = Scope::new();
    let body = vec![
        HirStmt::If {
            cond: lit_bool(true),
            then: vec![HirStmt::Return(Some(lit_int(0)))],
            else_: Some(vec![HirStmt::Return(Some(lit_float(1.5)))]),
        },
    ];
    assert_eq!(refine_func_ret(&HirType::Number, &body, &s), HirType::F64);
}

#[test]
fn refine_ret_keeps_explicit_narrow_annotation() {
    let s = Scope::new();
    let body = vec![HirStmt::Return(Some(lit_float(1.5)))];
    // Anotação explícita I32 não pode ser sobrescrita por body float
    assert_eq!(refine_func_ret(&HirType::I32, &body, &s), HirType::I32);
}

#[test]
fn refine_ret_keeps_declared_when_body_empty() {
    let s = Scope::new();
    assert_eq!(refine_func_ret(&HirType::Number, &[], &s), HirType::Number);
}

#[test]
fn refine_ret_finds_returns_inside_loops_and_try() {
    let s = Scope::new();
    let body = vec![
        HirStmt::While {
            cond: lit_bool(true),
            body: vec![HirStmt::Return(Some(lit_int(7)))],
        },
        HirStmt::Try {
            body: vec![HirStmt::Return(Some(lit_int(8)))],
            catch: None,
            finally: None,
        },
    ];
    assert_eq!(refine_func_ret(&HirType::Unknown, &body, &s), HirType::I64);
}

#[test]
fn refine_ret_unknown_when_no_return_arg() {
    let s = Scope::new();
    let body = vec![HirStmt::Return(None)];
    // refine_func_ret deve manter `declared` se só achar Void
    assert_eq!(refine_func_ret(&HirType::Unknown, &body, &s), HirType::Unknown);
}

#[test]
fn scope_class_registry() {
    let mut s = Scope::new();
    let id_a = s.register_class("Foo");
    let id_b = s.register_class("Bar");
    assert_ne!(id_a, id_b);
    assert_eq!(s.resolve_class("Foo"), Some(id_a));
    assert_eq!(s.resolve_class("Bar"), Some(id_b));
    assert_eq!(s.resolve_class("Baz"), None);
}
