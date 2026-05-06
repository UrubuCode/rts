use crate::ir::{HirExpr, HirExprKind, HirLit, HirType, HirBinOp};
use crate::scope::Scope;

/// Infer the best concrete `HirType` for an expression from its initializer.
///
/// This is the main type-refinement pass: it promotes `Unknown` and `Any` to
/// specific integer/float types where the initializer makes the type
/// unambiguous — so the codegen can emit `iadd_imm` instead of ABI coercions.
pub fn refine_type_from_init(expr: &HirExpr, scope: &Scope) -> HirType {
    refine_expr(expr, scope)
}

fn refine_expr(expr: &HirExpr, scope: &Scope) -> HirType {
    match &expr.kind {
        // Integer literals without annotation → i64
        HirExprKind::Lit(HirLit::Int(_)) => HirType::I64,
        // Float literals without annotation → f64
        HirExprKind::Lit(HirLit::Float(_)) => HirType::F64,
        // JS number literal → Number (f64 semantics)
        HirExprKind::Lit(HirLit::Number(_)) => HirType::Number,
        HirExprKind::Lit(HirLit::Bool(_)) => HirType::Bool,
        HirExprKind::Lit(HirLit::Str(_)) => HirType::Str,
        HirExprKind::Lit(HirLit::Null | HirLit::Undefined) => HirType::Any,

        // Arithmetic: propagate type from both operands
        HirExprKind::Bin { op, lhs, rhs } if op.is_arithmetic() => {
            let lt = refine_expr(lhs, scope);
            let rt = refine_expr(rhs, scope);
            numeric_promotion(lt, rt)
        }

        // Comparison always returns Bool
        HirExprKind::Bin { op, .. } if op.is_comparison() => HirType::Bool,

        // Logical operators return Bool
        HirExprKind::Bin {
            op: HirBinOp::LogAnd | HirBinOp::LogOr,
            lhs,
            rhs,
        } => {
            // Short-circuit: if both sides are the same type, propagate it
            let lt = refine_expr(lhs, scope);
            let rt = refine_expr(rhs, scope);
            if lt == rt { lt } else { HirType::Any }
        }

        // Bitwise ops: integers stay integers
        HirExprKind::Bin {
            op:
                HirBinOp::BitAnd
                | HirBinOp::BitOr
                | HirBinOp::BitXor
                | HirBinOp::Shl
                | HirBinOp::Shr
                | HirBinOp::UShr,
            lhs,
            rhs,
        } => {
            let lt = refine_expr(lhs, scope);
            let rt = refine_expr(rhs, scope);
            if lt.is_integer() && rt.is_integer() {
                numeric_promotion(lt, rt)
            } else {
                HirType::I64 // JS bitwise always yields I32 semantically, we use I64
            }
        }

        // Identifiers: look up in scope
        HirExprKind::Ident(name) => scope
            .lookup(name)
            .cloned()
            .unwrap_or(HirType::Unknown),

        // Array literal
        HirExprKind::Array(_) => HirType::Array(Box::new(HirType::Any)),

        // Object literal
        HirExprKind::Object(_) => HirType::Object,

        // new C(...) → Class or Array
        HirExprKind::New { class, .. } => {
            if class == "Array" {
                HirType::Array(Box::new(HirType::Any))
            } else {
                scope
                    .resolve_class(class)
                    .map(HirType::Class)
                    .unwrap_or(HirType::Unknown)
            }
        }

        // Calls: use return type if known
        HirExprKind::Call { callee, .. } => {
            if let HirExprKind::Ident(name) = &callee.kind {
                scope.return_type_of(name).unwrap_or(HirType::Unknown)
            } else {
                HirType::Unknown
            }
        }

        // Method call: propagate from the expr's own type annotation
        HirExprKind::MethodCall { .. } => expr.ty.clone(),

        // Ternary: promote both branches
        HirExprKind::Ternary { then, else_, .. } => {
            let tt = refine_expr(then, scope);
            let et = refine_expr(else_, scope);
            if tt == et { tt } else { HirType::Any }
        }

        // Prefix unary
        HirExprKind::Unary { op: crate::ir::HirUnOp::Neg, operand } => {
            refine_expr(operand, scope)
        }
        HirExprKind::Unary { op: crate::ir::HirUnOp::Not, .. } => HirType::Bool,
        HirExprKind::Unary { op: crate::ir::HirUnOp::BitNot, operand } => {
            let t = refine_expr(operand, scope);
            if t.is_integer() { t } else { HirType::I64 }
        }

        // Explicit cast
        HirExprKind::Cast { target, .. } => target.clone(),

        // Pre/post inc/dec: same type as operand
        HirExprKind::PreInc(e)
        | HirExprKind::PreDec(e)
        | HirExprKind::PostInc(e)
        | HirExprKind::PostDec(e) => refine_expr(e, scope),

        // Fallback: use the type already on the node (may be Unknown)
        _ => expr.ty.clone(),
    }
}

/// Compute the widened type for a binary numeric operation.
///
/// Rules (from widest to narrowest):
///   F64 > F32 > Number > I128/U128 > I64/U64 > I32/U32 > I16/U16 > I8/U8
///
/// Any non-numeric operand (Str, Any, etc.) causes promotion to `Any` since
/// the result is not statically knowable.
pub fn numeric_promotion(a: HirType, b: HirType) -> HirType {
    use HirType::*;

    // If either side is unknown/any, we cannot do better
    if matches!(a, Unknown | Any) || matches!(b, Unknown | Any) {
        return Number; // JS default
    }

    match (&a, &b) {
        // Float wins over everything
        (F64, _) | (_, F64) => F64,
        (Number, _) | (_, Number) => Number,
        (F32, _) | (_, F32) => F32,

        // Wide integers
        (I128, _) | (_, I128) | (U128, _) | (_, U128) => I128,
        (I64, _) | (_, I64) | (U64, _) | (_, U64) => I64,
        (I32, _) | (_, I32) | (U32, _) | (_, U32) => I32,
        (I16, _) | (_, I16) | (U16, _) | (_, U16) => I16,
        (I8, _) | (_, I8) | (U8, _) | (_, U8) => I8,

        _ => Number, // default: JS number semantics
    }
}
