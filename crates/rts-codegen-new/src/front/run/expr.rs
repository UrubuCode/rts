//! Expression lowering for the whole-program (Tagged-capable) path.
//!
//! Numeric operands keep the native fast path (`iadd`/`fadd`/`fcmp`/…); the
//! Tagged additions over [`crate::front::hir_lower`] are: string literals,
//! `+`/`===`/`!==`/`typeof` on Tagged/mixed operands (the ONE generic runtime
//! path), `console.log(...)`, and cross-function calls with per-param box/unbox.

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{types, InstBuilder};
use cranelift_module::Module;

use rts_hir::ir::HirExprKind;
use rts_hir::{HirBinOp, HirExpr, HirLit, HirUnOp};

use crate::repr::Repr;
use crate::value;
use crate::value::abi_adapter;

use crate::front::error::{unsupported, FrontResult};

use super::lower::{JsKind, Lowerer, Val};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Lower an expression to its value + repr.
    pub(super) fn lower_expr(
        &mut self,
        module: &mut dyn Module,
        e: &HirExpr,
    ) -> FrontResult<Val> {
        match &e.kind {
            HirExprKind::Lit(lit) => self.lower_lit(lit),
            HirExprKind::Ident(name) => self.lower_ident(name),
            HirExprKind::Bin { op, lhs, rhs } => self.lower_bin(module, *op, lhs, rhs),
            HirExprKind::Unary { op, operand } => self.lower_unary(module, *op, operand),
            HirExprKind::Ternary { cond, then, else_ } => {
                self.lower_ternary(module, cond, then, else_)
            }
            HirExprKind::Assign { target, value } => self.lower_assign(module, target, value),
            HirExprKind::AssignOp { op, target, value } => {
                self.lower_assign_op(module, *op, target, value)
            }
            HirExprKind::PreInc(t) => self.lower_incdec(t, true, true),
            HirExprKind::PreDec(t) => self.lower_incdec(t, false, true),
            HirExprKind::PostInc(t) => self.lower_incdec(t, true, false),
            HirExprKind::PostDec(t) => self.lower_incdec(t, false, false),
            HirExprKind::Call { callee, args } => self.lower_call(module, callee, args),
            HirExprKind::MethodCall { object, method, args } => {
                self.lower_method_call(module, object, method, args)
            }
            other => unsupported!("expression {}", super::stmt::expr_variant_name(other)),
        }
    }

    fn lower_lit(&mut self, lit: &HirLit) -> FrontResult<Val> {
        match lit {
            HirLit::Float(f) | HirLit::Number(f) => {
                let v = self.builder.ins().f64const(*f);
                Ok(Val::new(v, Repr::Float64))
            }
            HirLit::Int(n) => {
                let v = self.builder.ins().iconst(types::I64, *n);
                Ok(Val::new(v, Repr::Int64))
            }
            HirLit::Bool(b) => {
                let v = self.builder.ins().iconst(types::I64, *b as i64);
                Ok(Val::new(v, Repr::Bool))
            }
            HirLit::Str(s) => {
                // Intern the literal in the REAL string pool at lowering time and
                // splice the boxed string PolyValue word (whose 48-bit payload is
                // the real handle's slot+shard) in as a constant (Tagged, kind
                // Str). At run time `__RTS_FN_NS_GC_POLY_TO_HANDLE(payload)`
                // reconstructs the full handle (generation read from the live
                // slot); the slot is a normal GC reference, no side table.
                let pv = abi_adapter::intern_poly(s);
                let v = self.builder.ins().iconst(types::I64, pv.raw() as i64);
                Ok(Val::tagged_kind(v, JsKind::Str))
            }
            HirLit::Null => {
                let v = self
                    .builder
                    .ins()
                    .iconst(types::I64, value::PolyValue::null().raw() as i64);
                Ok(Val::tagged_kind(v, JsKind::Null))
            }
            HirLit::Undefined => {
                let v = self
                    .builder
                    .ins()
                    .iconst(types::I64, value::PolyValue::undefined().raw() as i64);
                Ok(Val::tagged_kind(v, JsKind::Undefined))
            }
        }
    }

    fn lower_ident(&mut self, name: &str) -> FrontResult<Val> {
        match self.local(name) {
            Some(local) => {
                let v = self.builder.use_var(local.var);
                // A local's static kind is its repr-implied kind; a Tagged local
                // carries `Unknown` (we do not flow string-ness through vars yet),
                // which makes strict-eq over it conservatively bail.
                Ok(Val::new(v, local.repr))
            }
            None => unsupported!("unbound identifier `{name}`"),
        }
    }

    fn lower_bin(
        &mut self,
        module: &mut dyn Module,
        op: HirBinOp,
        lhs: &HirExpr,
        rhs: &HirExpr,
    ) -> FrontResult<Val> {
        if matches!(op, HirBinOp::LogAnd | HirBinOp::LogOr) {
            return self.lower_logical(module, op, lhs, rhs);
        }

        let l = self.lower_expr(module, lhs)?;
        let r = self.lower_expr(module, rhs)?;

        // Equality. swc collapses BOTH `==` and `===` onto `HirBinOp::Eq` (and
        // `!=`/`!==` onto `Ne`), so the engine cannot tell loose from strict at
        // the HIR. `==` and `===` AGREE iff the operands are the same JS kind
        // (both numbers, both strings, both booleans, …) — cross-kind is exactly
        // where loose coercion diverges (`0 == ""` is `true` loose, `false`
        // strict). So: lower equality only when both operand kinds are the SAME
        // proven kind; a different/unknown kind pairing BAILS (never a wrong
        // value). Numeric-vs-numeric stays on the native compare below.
        if matches!(op, HirBinOp::Eq | HirBinOp::Ne) {
            if !same_proven_kind(l, r) {
                return unsupported!(
                    "equality on operands of differing/unknown kind ({:?} vs {:?}) — \
                     `==`/`===` are indistinguishable in HIR and diverge here",
                    l.kind,
                    r.kind
                );
            }
            if is_tagged(l) || is_tagged(r) {
                return self.lower_strict_eq(module, op, l, r);
            }
            // same-kind numeric/bool falls through to the native compare.
        }

        if op.is_comparison() {
            return self.lower_compare(op, l, r);
        }
        if op.is_arithmetic() {
            return self.lower_arith(module, op, l, r);
        }
        unsupported!("binary operator {op:?}")
    }

    /// Arithmetic. Both-numeric uses the native fast path; a Tagged/string/mixed
    /// `+` boxes both and calls the generic `__rtsadp_add` (the ONE `+` path). The
    /// other arithmetic ops require proven-numeric operands (Tagged `-`/`*`/`/`/
    /// `%` are a later increment — bail, never a wrong value).
    pub(super) fn lower_arith(
        &mut self,
        module: &mut dyn Module,
        op: HirBinOp,
        l: Val,
        r: Val,
    ) -> FrontResult<Val> {
        let tagged = is_tagged(l) || is_tagged(r);
        if tagged {
            if matches!(op, HirBinOp::Add) {
                let ba = self.box_value(l);
                let bb = self.box_value(r);
                let res = self
                    .call_runtime(module, "__rtsadp_add", &[ba, bb])?
                    .expect("__rtsadp_add returns a value");
                // The result is a string when concatenating, a number when both
                // sides coerced numeric — not statically known, so kind Unknown.
                return Ok(Val::new(res, Repr::Tagged));
            }
            return unsupported!("`{op:?}` on a tagged/string operand (only `+` generic in this increment)");
        }

        if matches!(l.repr, Repr::Bool) || matches!(r.repr, Repr::Bool) {
            return unsupported!("arithmetic on a boolean operand");
        }
        let both_int = is_int_repr(l.repr) && is_int_repr(r.repr);
        match op {
            HirBinOp::Div => {
                let lv = self.coerce(l, Repr::Float64)?;
                let rv = self.coerce(r, Repr::Float64)?;
                let v = self.builder.ins().fdiv(lv, rv);
                Ok(Val::new(v, Repr::Float64))
            }
            HirBinOp::Rem if !both_int => unsupported!("float remainder `%` (needs runtime fmod)"),
            _ if both_int => {
                let v = match op {
                    HirBinOp::Add => self.builder.ins().iadd(l.v, r.v),
                    HirBinOp::Sub => self.builder.ins().isub(l.v, r.v),
                    HirBinOp::Mul => self.builder.ins().imul(l.v, r.v),
                    HirBinOp::Rem => self.builder.ins().srem(l.v, r.v),
                    _ => return unsupported!("arithmetic op {op:?}"),
                };
                Ok(Val::new(v, wider_int(l.repr, r.repr)))
            }
            _ => {
                let lv = self.coerce(l, Repr::Float64)?;
                let rv = self.coerce(r, Repr::Float64)?;
                let v = match op {
                    HirBinOp::Add => self.builder.ins().fadd(lv, rv),
                    HirBinOp::Sub => self.builder.ins().fsub(lv, rv),
                    HirBinOp::Mul => self.builder.ins().fmul(lv, rv),
                    _ => return unsupported!("arithmetic op {op:?}"),
                };
                Ok(Val::new(v, Repr::Float64))
            }
        }
    }

    /// `===` / `!==` over a tag-dispatched runtime compare → a `Bool` (i64 0/1).
    fn lower_strict_eq(
        &mut self,
        module: &mut dyn Module,
        op: HirBinOp,
        l: Val,
        r: Val,
    ) -> FrontResult<Val> {
        let ba = self.box_value(l);
        let bb = self.box_value(r);
        let sym = match op {
            HirBinOp::Eq => "__rtsadp_strict_eq",
            HirBinOp::Ne => "__rtsadp_strict_neq",
            _ => return unsupported!("strict-eq op {op:?}"),
        };
        let res = self
            .call_runtime(module, sym, &[ba, bb])?
            .expect("strict-eq returns a value");
        // The runtime returns a boolean PolyValue word; reduce it to an i64 0/1
        // Bool carrier by comparing against the `true` singleton.
        let true_word = self
            .builder
            .ins()
            .iconst(types::I64, value::PolyValue::bool(true).raw() as i64);
        let b = self.builder.ins().icmp(IntCC::Equal, res, true_word);
        let widened = self.builder.ins().uextend(types::I64, b);
        Ok(Val::new(widened, Repr::Bool))
    }

    /// Numeric comparison `< <= > >= == !=` → a `Bool`. Operands proven numeric
    /// (the Tagged `==`/`!=` case was already split off in `lower_bin`).
    fn lower_compare(&mut self, op: HirBinOp, l: Val, r: Val) -> FrontResult<Val> {
        let use_float = matches!(l.repr, Repr::Float64) || matches!(r.repr, Repr::Float64);
        let bool_cmp = matches!(l.repr, Repr::Bool) || matches!(r.repr, Repr::Bool);
        if bool_cmp && !matches!(op, HirBinOp::Eq | HirBinOp::Ne) {
            return unsupported!("ordering comparison on a boolean");
        }
        let cmp = if use_float {
            let lv = self.coerce(l, Repr::Float64)?;
            let rv = self.coerce(r, Repr::Float64)?;
            let cc = float_cc(op)?;
            self.builder.ins().fcmp(cc, lv, rv)
        } else {
            let cc = int_cc(op)?;
            self.builder.ins().icmp(cc, l.v, r.v)
        };
        let widened = self.builder.ins().uextend(types::I64, cmp);
        Ok(Val::new(widened, Repr::Bool))
    }

    /// Logical `&&`/`||` on two boolean operands → boolean via `select`.
    fn lower_logical(
        &mut self,
        module: &mut dyn Module,
        op: HirBinOp,
        lhs: &HirExpr,
        rhs: &HirExpr,
    ) -> FrontResult<Val> {
        let l = self.lower_expr(module, lhs)?;
        let r = self.lower_expr(module, rhs)?;
        if !matches!(l.repr, Repr::Bool) || !matches!(r.repr, Repr::Bool) {
            return unsupported!("logical {op:?} on non-boolean operands");
        }
        let v = match op {
            HirBinOp::LogAnd => self.builder.ins().select(l.v, r.v, l.v),
            HirBinOp::LogOr => self.builder.ins().select(l.v, l.v, r.v),
            _ => return unsupported!("logical op {op:?}"),
        };
        Ok(Val::new(v, Repr::Bool))
    }

    fn lower_unary(
        &mut self,
        module: &mut dyn Module,
        op: HirUnOp,
        operand: &HirExpr,
    ) -> FrontResult<Val> {
        // `typeof e`: if the operand is a literal we know statically, fold to a
        // constant string; else box + `__rtsadp_typeof`. The result is a string.
        if matches!(op, HirUnOp::TypeOf) {
            if let Some(s) = static_typeof(operand) {
                let pv = abi_adapter::intern_poly(s);
                let v = self.builder.ins().iconst(types::I64, pv.raw() as i64);
                return Ok(Val::tagged_kind(v, JsKind::Str));
            }
            let val = self.lower_expr(module, operand)?;
            let boxed = self.box_value(val);
            let res = self
                .call_runtime(module, "__rtsadp_typeof", &[boxed])?
                .expect("__rtsadp_typeof returns a value");
            return Ok(Val::tagged_kind(res, JsKind::Str));
        }

        let val = self.lower_expr(module, operand)?;
        match op {
            HirUnOp::Neg => match val.repr {
                Repr::Float64 => {
                    let v = self.builder.ins().fneg(val.v);
                    Ok(Val::new(v, Repr::Float64))
                }
                Repr::Int32 | Repr::Int64 => {
                    let v = self.builder.ins().ineg(val.v);
                    Ok(Val::new(v, val.repr))
                }
                other => unsupported!("unary `-` on repr {other:?}"),
            },
            // CRITICAL soundness bail: swc lowers BOTH unary `!` and unary `+` to
            // `HirUnOp::Not`, so the engine cannot tell them apart from the HIR —
            // and they disagree (`!5` is `false`, `+5` is `5`). Emitting either
            // would silently miscompile the other, so we REFUSE the whole `Not`
            // family. (This is the exact honesty-floor case the redesign exists to
            // hold: refuse, never guess.)
            HirUnOp::Not => unsupported!(
                "unary `!`/`+` (HIR conflates them; cannot lower soundly without the AST)"
            ),
            other => unsupported!("unary operator {other:?}"),
        }
    }

    fn lower_ternary(
        &mut self,
        module: &mut dyn Module,
        cond: &HirExpr,
        then: &HirExpr,
        else_: &HirExpr,
    ) -> FrontResult<Val> {
        let c = self.lower_expr(module, cond)?;
        let cond_v = self.as_bool_value(module, c)?;
        let t = self.lower_expr(module, then)?;
        let e = self.lower_expr(module, else_)?;
        let target = ternary_target(t.repr, e.repr)?;
        let tv = self.coerce(t, target)?;
        let ev = self.coerce(e, target)?;
        let v = self.builder.ins().select(cond_v, tv, ev);
        // Kind is provable only when both arms share it; otherwise fall to the
        // repr-implied kind (Unknown for Tagged).
        let kind = if t.kind == e.kind { t.kind } else { JsKind::Unknown };
        Ok(Val { v, repr: target, kind })
    }
}

// ---------------------------------------------------------------------------
// Free helpers.
// ---------------------------------------------------------------------------

fn is_tagged(v: Val) -> bool {
    matches!(v.repr, Repr::Tagged)
}

/// Whether two operands have the SAME statically-proven JS kind — the condition
/// under which `==` and `===` agree (so equality is sound to lower despite the
/// HIR conflating the two operators). `Unknown` kinds never qualify: when we
/// cannot prove both sides share a kind, equality bails rather than risk the
/// loose-vs-strict divergence.
fn same_proven_kind(l: Val, r: Val) -> bool {
    l.kind != JsKind::Unknown && l.kind == r.kind
}

fn is_int_repr(r: Repr) -> bool {
    matches!(r, Repr::Int32 | Repr::Int64)
}

fn wider_int(a: Repr, b: Repr) -> Repr {
    if matches!(a, Repr::Int64) || matches!(b, Repr::Int64) {
        Repr::Int64
    } else {
        Repr::Int32
    }
}

/// The join repr for two ternary arms; widen disagreeing numerics to f64, or
/// fall to `Tagged` when one arm is already Tagged.
fn ternary_target(t: Repr, e: Repr) -> FrontResult<Repr> {
    if t == e {
        return Ok(t);
    }
    if matches!(t, Repr::Tagged) || matches!(e, Repr::Tagged) {
        return Ok(Repr::Tagged);
    }
    if t.is_unboxed() && e.is_unboxed() && !matches!(t, Repr::Bool) && !matches!(e, Repr::Bool) {
        return Ok(Repr::Float64);
    }
    unsupported!("ternary arms have incompatible reprs {t:?} / {e:?}")
}

fn float_cc(op: HirBinOp) -> FrontResult<FloatCC> {
    Ok(match op {
        HirBinOp::Eq => FloatCC::Equal,
        HirBinOp::Ne => FloatCC::NotEqual,
        HirBinOp::Lt => FloatCC::LessThan,
        HirBinOp::Le => FloatCC::LessThanOrEqual,
        HirBinOp::Gt => FloatCC::GreaterThan,
        HirBinOp::Ge => FloatCC::GreaterThanOrEqual,
        _ => return unsupported!("comparison op {op:?}"),
    })
}

fn int_cc(op: HirBinOp) -> FrontResult<IntCC> {
    Ok(match op {
        HirBinOp::Eq => IntCC::Equal,
        HirBinOp::Ne => IntCC::NotEqual,
        HirBinOp::Lt => IntCC::SignedLessThan,
        HirBinOp::Le => IntCC::SignedLessThanOrEqual,
        HirBinOp::Gt => IntCC::SignedGreaterThan,
        HirBinOp::Ge => IntCC::SignedGreaterThanOrEqual,
        _ => return unsupported!("comparison op {op:?}"),
    })
}

/// The compile-time `typeof` string for a literal operand, when statically known.
/// (Only literals fold; an identifier's runtime tag is inspected via the runtime
/// op, which is always correct.)
fn static_typeof(e: &HirExpr) -> Option<&'static str> {
    match &e.kind {
        HirExprKind::Lit(HirLit::Int(_))
        | HirExprKind::Lit(HirLit::Float(_))
        | HirExprKind::Lit(HirLit::Number(_)) => Some("number"),
        HirExprKind::Lit(HirLit::Str(_)) => Some("string"),
        HirExprKind::Lit(HirLit::Bool(_)) => Some("boolean"),
        HirExprKind::Lit(HirLit::Undefined) => Some("undefined"),
        // `typeof null` is the famous `"object"`.
        HirExprKind::Lit(HirLit::Null) => Some("object"),
        _ => None,
    }
}
