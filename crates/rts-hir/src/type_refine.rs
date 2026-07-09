use crate::ir::{HirExpr, HirExprKind, HirLit, HirStmt, HirType, HirBinOp};
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

        // Arithmetic: propagate type from both operands. JS `+` with EITHER operand a
        // string is CONCATENATION → `Str` (`"a" + "b"`, `"a" + 1`), not numeric — so a
        // `const u = "a" + "b"` infers `Str` and a fn returning `u` marshals a string,
        // not an f64 (`NaN`).
        HirExprKind::Bin { op, lhs, rhs } if op.is_arithmetic() => {
            let lt = refine_expr(lhs, scope);
            let rt = refine_expr(rhs, scope);
            if matches!(op, HirBinOp::Add)
                && (matches!(lt, HirType::Str) || matches!(rt, HirType::Str))
            {
                HirType::Str
            } else {
                numeric_promotion(lt, rt)
            }
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

/// Refine the declared return type of a function from the bodies of its
/// `return` statements.
///
/// Use this only when the declared annotation is ambiguous (`Unknown`,
/// `Number`, or `Any`) — explicit narrow annotations (`I32`, `F32`, etc.)
/// are authoritative and never overridden.
///
/// Returns the refined type, or `declared` unchanged when the body has no
/// returns or the returns are ambiguous (Unknown/Any).
pub fn refine_func_ret(declared: &HirType, body: &[HirStmt], scope: &Scope) -> HirType {
    // Only refine ambiguous annotations; trust explicit narrow types.
    let allow_refine = matches!(
        declared,
        HirType::Unknown | HirType::Number | HirType::Any
    );
    if !allow_refine {
        return declared.clone();
    }

    // A `return null` / `return undefined` makes the function genuinely able to
    // yield a nullish value. Its result MUST stay Tagged (`Any`) — never collapse
    // to a numeric repr, which would coerce `null` to `NaN` on the return path
    // (e.g. `function f(x): number|null { if (x>0) return x; return null; }`).
    // This wins over a numeric return in the same body (`numeric_promotion`
    // optimistically absorbs `Any`, which is wrong for a definite nullish return).
    if body_has_nullish_return(body) {
        return HirType::Any;
    }

    let mut acc: Option<HirType> = None;
    collect_return_types(body, scope, &mut acc);

    match acc {
        Some(ty) if !matches!(ty, HirType::Unknown | HirType::Any) => ty,
        _ => declared.clone(),
    }
}

/// Whether any `return` in `body` (recursively) returns a `null`/`undefined`
/// LITERAL — the signal that the function is genuinely nullish and its return
/// repr must stay Tagged.
fn body_has_nullish_return(stmts: &[HirStmt]) -> bool {
    stmts.iter().any(stmt_has_nullish_return)
}

fn stmt_has_nullish_return(stmt: &HirStmt) -> bool {
    match stmt {
        HirStmt::Return(Some(expr)) => {
            matches!(&expr.kind, HirExprKind::Lit(HirLit::Null | HirLit::Undefined))
        }
        HirStmt::If { then, else_, .. } => {
            body_has_nullish_return(then)
                || else_.as_ref().is_some_and(|e| body_has_nullish_return(e))
        }
        HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::ForOf { body, .. }
        | HirStmt::ForIn { body, .. }
        | HirStmt::For { body, .. }
        | HirStmt::Block(body) => body_has_nullish_return(body),
        HirStmt::Try { body, catch, finally } => {
            body_has_nullish_return(body)
                || catch.as_ref().is_some_and(|c| body_has_nullish_return(&c.body))
                || finally.as_ref().is_some_and(|f| body_has_nullish_return(f))
        }
        HirStmt::Switch { cases, .. } => {
            cases.iter().any(|case| body_has_nullish_return(&case.body))
        }
        HirStmt::Labeled { body, .. } => stmt_has_nullish_return(body),
        _ => false,
    }
}

/// Merge two `return`-statement types for return-type inference. Two NUMERIC
/// types promote through [`numeric_promotion`] (the winning unboxed numeric
/// path). The surgical fix over calling `numeric_promotion` DIRECTLY: two EQUAL
/// non-numeric types (`Str + Str`, `Bool + Bool`) keep that type, and a non-numeric
/// MISMATCH (`Str` vs `Bool`, `Str` vs `I64`) is `Any` (genuinely polymorphic).
/// Without this, `numeric_promotion` collapsed `Str + Str` to `Number` (its
/// `_ => Number` default), turning a string-returning
/// `try { return "a" } finally { return "b" }` into an f64 fn that coerced the
/// strings to `NaN`.
///
/// A side involving `Unknown`/`Any` keeps the ORIGINAL `numeric_promotion`
/// behaviour (deliberately — `refine_func_ret` treats a resulting `Number` no
/// differently from a resulting `Any`/`Unknown` when the OTHER returns are
/// un-inferable, so preserving it avoids perturbing any established numeric path).
fn merge_return_types(a: HirType, b: HirType) -> HirType {
    use HirType::*;
    let is_numeric = |t: &HirType| {
        matches!(
            t,
            I8 | I16 | I32 | I64 | I128 | U8 | U16 | U32 | U64 | U128 | F32 | F64 | Number
        )
    };
    // Both sides a CONCRETE (non-Unknown/Any) type: apply the corrected merge.
    if !matches!(a, Unknown | Any) && !matches!(b, Unknown | Any) {
        if a == b {
            return a;
        }
        if is_numeric(&a) && is_numeric(&b) {
            return numeric_promotion(a, b);
        }
        // A non-numeric mismatch is genuinely polymorphic — stay Tagged, never a
        // bogus numeric collapse.
        return Any;
    }
    // An un-inferable side: unchanged from the historical behaviour.
    numeric_promotion(a, b)
}

/// Walk a stmt block and merge the type of every `return` expression found.
fn collect_return_types(stmts: &[HirStmt], scope: &Scope, acc: &mut Option<HirType>) {
    for stmt in stmts {
        collect_return_types_in_stmt(stmt, scope, acc);
    }
}

fn collect_return_types_in_stmt(stmt: &HirStmt, scope: &Scope, acc: &mut Option<HirType>) {
    match stmt {
        HirStmt::Return(Some(expr)) => {
            let ty = refine_type_from_init(expr, scope);
            *acc = Some(match acc.take() {
                Some(prev) => merge_return_types(prev, ty),
                None => ty,
            });
        }
        HirStmt::Return(None) => {
            // `return;` carries no value — leave the numeric accumulator
            // alone so the declared annotation wins.
        }
        HirStmt::If { then, else_, .. } => {
            collect_return_types(then, scope, acc);
            if let Some(e) = else_ {
                collect_return_types(e, scope, acc);
            }
        }
        HirStmt::While { body, .. }
        | HirStmt::DoWhile { body, .. }
        | HirStmt::ForOf { body, .. }
        | HirStmt::ForIn { body, .. } => {
            collect_return_types(body, scope, acc);
        }
        HirStmt::For { body, .. } => {
            collect_return_types(body, scope, acc);
        }
        HirStmt::Try { body, catch, finally } => {
            collect_return_types(body, scope, acc);
            if let Some(c) = catch {
                collect_return_types(&c.body, scope, acc);
            }
            if let Some(f) = finally {
                collect_return_types(f, scope, acc);
            }
        }
        HirStmt::Switch { cases, .. } => {
            for case in cases {
                collect_return_types(&case.body, scope, acc);
            }
        }
        HirStmt::Block(body) => collect_return_types(body, scope, acc),
        HirStmt::Labeled { body, .. } => {
            collect_return_types_in_stmt(body, scope, acc);
        }
        _ => {}
    }
}
