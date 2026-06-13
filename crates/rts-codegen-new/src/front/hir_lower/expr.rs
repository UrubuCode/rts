//! Expression lowering for the numeric subset.
//!
//! Each `HirExpr` lowers to a [`Val`] (an SSA value + its [`Repr`]). The repr is
//! chosen from the operands, not the HIR's annotated `ty` (which can be `Unknown`
//! for locals); the annotated type is only consulted at the param/return/`let`
//! boundary where it is a real annotation. Native ops only: `fadd`/`iadd`,
//! `fcmp`/`icmp`, `fneg`/`ineg`, no boxing.

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{types, InstBuilder, Value};

use rts_hir::ir::HirExprKind;
use rts_hir::{HirBinOp, HirExpr, HirLit, HirUnOp};

use crate::repr::Repr;

use super::super::error::{unsupported, FrontResult};
use super::{Lowerer, Val};

impl<'a, 'b> Lowerer<'a, 'b> {
    /// Lower an expression to its value + repr.
    pub(super) fn lower_expr(&mut self, e: &HirExpr) -> FrontResult<Val> {
        match &e.kind {
            HirExprKind::Lit(lit) => self.lower_lit(lit),
            HirExprKind::Ident(name) => self.lower_ident(name),
            HirExprKind::Bin { op, lhs, rhs } => self.lower_bin(*op, lhs, rhs),
            HirExprKind::Unary { op, operand } => self.lower_unary(*op, operand),
            HirExprKind::Ternary { cond, then, else_ } => self.lower_ternary(cond, then, else_),
            HirExprKind::Assign { target, value } => self.lower_assign(target, value),
            HirExprKind::AssignOp { op, target, value } => self.lower_assign_op(*op, target, value),
            HirExprKind::PreInc(t) => self.lower_incdec(t, true, true),
            HirExprKind::PreDec(t) => self.lower_incdec(t, false, true),
            HirExprKind::PostInc(t) => self.lower_incdec(t, true, false),
            HirExprKind::PostDec(t) => self.lower_incdec(t, false, false),
            HirExprKind::Call { callee, .. } => {
                let name = match &callee.kind {
                    HirExprKind::Ident(n) => n.clone(),
                    _ => "<expr>".into(),
                };
                unsupported!("call to `{name}` (cross-function calls are a later increment)")
            }
            other => unsupported!("expression {}", variant_name(other)),
        }
    }

    fn lower_lit(&mut self, lit: &HirLit) -> FrontResult<Val> {
        match lit {
            HirLit::Float(f) | HirLit::Number(f) => {
                let v = self.builder.ins().f64const(*f);
                Ok(Val { v, repr: Repr::Float64 })
            }
            HirLit::Int(n) => {
                // A JS integer literal's natural register width is i64 (and the
                // HIR types integer literals as `I64`). Carry it as `Int64`;
                // `coerce` narrows it to `Int32` where a local/param annotation
                // demands, or widens to `Float64` in float context. Both int
                // reprs share the i64 register, so this is bit-exact.
                let v = self.builder.ins().iconst(types::I64, *n);
                Ok(Val { v, repr: Repr::Int64 })
            }
            HirLit::Bool(b) => {
                let v = self.builder.ins().iconst(types::I64, *b as i64);
                Ok(Val { v, repr: Repr::Bool })
            }
            HirLit::Str(_) => unsupported!("string literal"),
            HirLit::Null => unsupported!("null literal"),
            HirLit::Undefined => unsupported!("undefined literal"),
        }
    }

    fn lower_ident(&mut self, name: &str) -> FrontResult<Val> {
        match self.local(name) {
            Some(local) => {
                let v = self.builder.use_var(local.var);
                Ok(Val { v, repr: local.repr })
            }
            None => unsupported!("unbound identifier `{name}` (globals/captures later)"),
        }
    }

    fn lower_bin(&mut self, op: HirBinOp, lhs: &HirExpr, rhs: &HirExpr) -> FrontResult<Val> {
        // Logical and/or short-circuit and produce a value — model via control
        // flow in stmt.rs is overkill for the numeric subset; here both arms are
        // proven-numeric/bool so we evaluate both and select. JS `&&`/`||` return
        // an operand, but for the bool-typed numeric subset (cond contexts) the
        // operands are booleans, so select(l, r, l-or-false) matches.
        if matches!(op, HirBinOp::LogAnd | HirBinOp::LogOr) {
            return self.lower_logical(op, lhs, rhs);
        }

        let l = self.lower_expr(lhs)?;
        let r = self.lower_expr(rhs)?;

        if op.is_comparison() {
            return self.lower_compare(op, l, r);
        }
        if op.is_arithmetic() {
            return self.lower_arith(op, l, r);
        }
        unsupported!("binary operator {op:?}")
    }

    /// Arithmetic `+ - * / %` on two numeric operands.
    ///
    /// - Both operands integer (`Int32`/`Int64`, all i64-carried) → native
    ///   `iadd`/`isub`/`imul`/`srem`, result the wider int repr. `/` is the
    ///   exception: JS division is always real (double), so `/` widens to f64.
    /// - Any float operand → widen both to `Float64` and use `fadd`/…. Float `%`
    ///   needs a runtime `fmod` (out of the pure-IR subset) → explicit bail.
    /// - Bool is not arithmetic.
    pub(super) fn lower_arith(&mut self, op: HirBinOp, l: Val, r: Val) -> FrontResult<Val> {
        if matches!(l.repr, Repr::Bool) || matches!(r.repr, Repr::Bool) {
            return unsupported!("arithmetic on a boolean operand");
        }
        let both_int = is_int_repr(l.repr) && is_int_repr(r.repr);

        match op {
            HirBinOp::Div => {
                // JS `/` is real division — always compute in f64.
                let lv = self.coerce(l, Repr::Float64)?;
                let rv = self.coerce(r, Repr::Float64)?;
                let v = self.builder.ins().fdiv(lv, rv);
                Ok(Val { v, repr: Repr::Float64 })
            }
            HirBinOp::Rem if !both_int => {
                unsupported!("float remainder `%` (needs runtime fmod)")
            }
            _ if both_int => {
                let v = match op {
                    HirBinOp::Add => self.builder.ins().iadd(l.v, r.v),
                    HirBinOp::Sub => self.builder.ins().isub(l.v, r.v),
                    HirBinOp::Mul => self.builder.ins().imul(l.v, r.v),
                    HirBinOp::Rem => self.builder.ins().srem(l.v, r.v),
                    _ => return unsupported!("arithmetic op {op:?}"),
                };
                Ok(Val { v, repr: wider_int(l.repr, r.repr) })
            }
            _ => {
                // Mixed int/float or both-float: widen to f64 and use float ops.
                let lv = self.coerce(l, Repr::Float64)?;
                let rv = self.coerce(r, Repr::Float64)?;
                let v = match op {
                    HirBinOp::Add => self.builder.ins().fadd(lv, rv),
                    HirBinOp::Sub => self.builder.ins().fsub(lv, rv),
                    HirBinOp::Mul => self.builder.ins().fmul(lv, rv),
                    _ => return unsupported!("arithmetic op {op:?}"),
                };
                Ok(Val { v, repr: Repr::Float64 })
            }
        }
    }

    /// Comparison `< <= > >= == !=` → a `Bool` (i64 0/1). Both numeric operands
    /// are widened to a common repr first.
    fn lower_compare(&mut self, op: HirBinOp, l: Val, r: Val) -> FrontResult<Val> {
        let use_float = matches!(l.repr, Repr::Float64) || matches!(r.repr, Repr::Float64);
        // Booleans only compare with == / != against each other.
        let bool_cmp = matches!(l.repr, Repr::Bool) || matches!(r.repr, Repr::Bool);
        if bool_cmp && !matches!(op, HirBinOp::Eq | HirBinOp::Ne) {
            return unsupported!("ordering comparison on a boolean");
        }

        let cmp = if use_float {
            let lv = self.coerce(l, Repr::Float64)?;
            let rv = self.coerce(r, Repr::Float64)?;
            let cc = match op {
                HirBinOp::Eq => FloatCC::Equal,
                HirBinOp::Ne => FloatCC::NotEqual,
                HirBinOp::Lt => FloatCC::LessThan,
                HirBinOp::Le => FloatCC::LessThanOrEqual,
                HirBinOp::Gt => FloatCC::GreaterThan,
                HirBinOp::Ge => FloatCC::GreaterThanOrEqual,
                _ => return unsupported!("comparison op {op:?}"),
            };
            self.builder.ins().fcmp(cc, lv, rv)
        } else {
            // Both i64-carried (Int32 or Bool); signed comparison.
            let cc = match op {
                HirBinOp::Eq => IntCC::Equal,
                HirBinOp::Ne => IntCC::NotEqual,
                HirBinOp::Lt => IntCC::SignedLessThan,
                HirBinOp::Le => IntCC::SignedLessThanOrEqual,
                HirBinOp::Gt => IntCC::SignedGreaterThan,
                HirBinOp::Ge => IntCC::SignedGreaterThanOrEqual,
                _ => return unsupported!("comparison op {op:?}"),
            };
            self.builder.ins().icmp(cc, l.v, r.v)
        };
        // icmp/fcmp yield an i8 0/1; widen to the i64 Bool carrier.
        let widened = self.builder.ins().uextend(types::I64, cmp);
        Ok(Val { v: widened, repr: Repr::Bool })
    }

    /// Logical `&&`/`||` on two boolean operands → boolean via `select` (both
    /// operands proven-bool; no short-circuit side effects in the numeric subset).
    fn lower_logical(&mut self, op: HirBinOp, lhs: &HirExpr, rhs: &HirExpr) -> FrontResult<Val> {
        let l = self.lower_expr(lhs)?;
        let r = self.lower_expr(rhs)?;
        if !matches!(l.repr, Repr::Bool) || !matches!(r.repr, Repr::Bool) {
            return unsupported!("logical {op:?} on non-boolean operands");
        }
        let v = match op {
            // a && b == a ? b : a   (a is false → a (0); else b)
            HirBinOp::LogAnd => self.builder.ins().select(l.v, r.v, l.v),
            // a || b == a ? a : b   (a is true → a (1); else b)
            HirBinOp::LogOr => self.builder.ins().select(l.v, l.v, r.v),
            _ => return unsupported!("logical op {op:?}"),
        };
        Ok(Val { v, repr: Repr::Bool })
    }

    fn lower_unary(&mut self, op: HirUnOp, operand: &HirExpr) -> FrontResult<Val> {
        let val = self.lower_expr(operand)?;
        match op {
            HirUnOp::Neg => match val.repr {
                Repr::Float64 => {
                    let v = self.builder.ins().fneg(val.v);
                    Ok(Val { v, repr: Repr::Float64 })
                }
                Repr::Int32 | Repr::Int64 => {
                    let v = self.builder.ins().ineg(val.v);
                    Ok(Val { v, repr: val.repr })
                }
                other => unsupported!("unary `-` on repr {other:?}"),
            },
            HirUnOp::Not => {
                // !b for a boolean (i64 0/1): xor 1.
                if !matches!(val.repr, Repr::Bool) {
                    return unsupported!("logical `!` on a non-boolean");
                }
                let one = self.builder.ins().iconst(types::I64, 1);
                let v = self.builder.ins().bxor(val.v, one);
                Ok(Val { v, repr: Repr::Bool })
            }
            other => unsupported!("unary operator {other:?}"),
        }
    }

    fn lower_ternary(
        &mut self,
        cond: &HirExpr,
        then: &HirExpr,
        else_: &HirExpr,
    ) -> FrontResult<Val> {
        let c = self.lower_expr(cond)?;
        let cond_v = self.as_bool_value(c)?;
        let t = self.lower_expr(then)?;
        let e = self.lower_expr(else_)?;
        // Join the two arms to a common repr (Int32 widens to Float64 if needed).
        let target = t.repr.join(e.repr);
        let target = if target == Repr::Tagged {
            // disagreeing arms: try widening both numerics to f64.
            if t.repr.is_unboxed() && e.repr.is_unboxed() {
                Repr::Float64
            } else {
                return unsupported!("ternary arms have incompatible reprs");
            }
        } else {
            target
        };
        let tv = self.coerce(t, target)?;
        let ev = self.coerce(e, target)?;
        let v = self.builder.ins().select(cond_v, tv, ev);
        Ok(Val { v, repr: target })
    }

    /// Reduce a value to an i8/i64 truthiness flag usable by `select`/`brif`.
    /// In the numeric subset a condition is a `Bool` (already 0/1). A bare number
    /// in a condition (`if (x)`) is rejected: JS `ToBoolean` of a double needs a
    /// NaN check + zero check; that is a small follow-up, kept explicit here.
    pub(super) fn as_bool_value(&mut self, v: Val) -> FrontResult<Value> {
        match v.repr {
            Repr::Bool => Ok(v.v),
            other => unsupported!(
                "condition of repr {other:?} (only `boolean` conditions in this increment)"
            ),
        }
    }
}

/// Whether `r` is an integer repr carried in an i64 register.
fn is_int_repr(r: Repr) -> bool {
    matches!(r, Repr::Int32 | Repr::Int64)
}

/// The wider of two integer reprs (`Int64` dominates `Int32`). Both are
/// i64-carried, so this only affects the *repr label* propagated forward — the
/// machine value is identical.
fn wider_int(a: Repr, b: Repr) -> Repr {
    if matches!(a, Repr::Int64) || matches!(b, Repr::Int64) {
        Repr::Int64
    } else {
        Repr::Int32
    }
}

/// A readable name for an unsupported expression variant (for the bail message).
fn variant_name(k: &HirExprKind) -> &'static str {
    match k {
        HirExprKind::Lit(_) => "literal",
        HirExprKind::Ident(_) => "identifier",
        HirExprKind::Bin { .. } => "binary",
        HirExprKind::Unary { .. } => "unary",
        HirExprKind::Assign { .. } => "assignment",
        HirExprKind::AssignOp { .. } => "compound-assignment",
        HirExprKind::Call { .. } => "call",
        HirExprKind::MethodCall { .. } => "method-call",
        HirExprKind::New { .. } => "new",
        HirExprKind::Member { .. } => "member-access",
        HirExprKind::Index { .. } => "index",
        HirExprKind::Array(_) => "array-literal",
        HirExprKind::Object(_) => "object-literal",
        HirExprKind::Ternary { .. } => "ternary",
        HirExprKind::Await(_) => "await",
        HirExprKind::Cast { .. } => "cast",
        HirExprKind::Arrow { .. } => "arrow",
        HirExprKind::PreInc(_) => "pre-increment",
        HirExprKind::PreDec(_) => "pre-decrement",
        HirExprKind::PostInc(_) => "post-increment",
        HirExprKind::PostDec(_) => "post-decrement",
        HirExprKind::Spread(_) => "spread",
        HirExprKind::Seq(_) => "sequence",
        HirExprKind::Raw(_) => "raw/unrecognized",
    }
}
