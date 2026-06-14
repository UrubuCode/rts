//! Binary-operator lowering for the whole-program path.
//!
//! Split out of [`super::expr`] (the 500-line module rule). Holds the
//! native-vs-generic decision for every binary operator + the helpers the
//! decision needs:
//!
//! - **Equality `==`/`===`** — swc conflates the two onto one HIR op, so we lower
//!   it only when the operand KINDS prove `==`/`===` agree (same proven kind);
//!   cross/unknown kind BAILS. Same-kind Tagged → the runtime `strict_eq`;
//!   same-kind native → the native compare.
//! - **Relational `< <= > >=`** — native when both proven numeric; the generic
//!   `__rtsadp_{lt,le,gt,ge}` PolyValue path when any operand is Tagged.
//! - **Arithmetic `+ - * / % **`** — native fast path UNCHANGED for proven
//!   numeric operands; any Tagged/string/mixed operand routes to the matching
//!   `__rtsadp_*` (`+` is the one generic concat/add path). `%` on proven floats
//!   and `**` (no native op) route generic for correctness.
//! - **Bitwise/shifts `& | ^ << >> >>>`** — ALWAYS generic: JS bitwise semantics
//!   (ToInt32/ToUint32, 5-bit shift-count mask, unsigned `>>>` result) are not a
//!   native i64 op, so a naive `ishl`/`sshr` would be WRONG (`1 << 32`, `-1 >>>
//!   0`); the trampoline implements the exact rule.

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{types, InstBuilder, Value};
use cranelift_module::Module;

use rts_hir::{HirBinOp, HirExpr};

use crate::repr::Repr;
use crate::value;

use crate::front::error::{unsupported, FrontResult};

use super::lower::{JsKind, Lowerer, Val};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    pub(super) fn lower_bin(
        &mut self,
        module: &mut dyn Module,
        op: HirBinOp,
        lhs: &HirExpr,
        rhs: &HirExpr,
    ) -> FrontResult<Val> {
        if matches!(op, HirBinOp::LogAnd | HirBinOp::LogOr) {
            return self.lower_logical(module, op, lhs, rhs);
        }

        // `x instanceof C` (P5.3). swc collapses `instanceof`/`in`/etc onto
        // `HirBinOp::Unsupported`; we only treat it as instanceof when the RHS is a
        // bare identifier naming a class the engine can check (a user class, or a
        // runtime/Registry class Map/Set/Error-family/Array). That keeps `"k" in o`
        // (rhs not a class ident) safely bailed — never a wrong instanceof.
        if matches!(op, HirBinOp::Unsupported) {
            if let rts_hir::ir::HirExprKind::Ident(class) = &rhs.kind {
                if let Some(val) = self.try_instanceof(module, lhs, class)? {
                    return Ok(val);
                }
            }
            return unsupported!(
                "binary operator (other) — `instanceof`/`in`/unmapped op (rhs is not an \
                 engine-checkable class)"
            );
        }

        // A WHOLE object/array operand needs JS ToPrimitive (`[1]+[2]` → `"12"`,
        // `[]+{}` → `"[object Object]"`, with array `.join(",")` coercion) — a
        // later increment. Bail rather than emit the runtime ToString, which
        // diverges from Bun/Node for these.
        //
        // EXCEPTION (P5.8): a `+` where the OTHER operand is a PROVEN STRING and the
        // heap operand is an ARRAY is pure string concatenation — `"x" + [1,2,3]` is
        // `"x1,2,3"`, well-defined by `String(array)` = `.join(",")`, which
        // `__rtsadp_add`'s string path does exactly (and identically for
        // `${[1,2,3]}` in a template). A whole OBJECT operand is NOT relaxed: an
        // object may override `toString`/`valueOf`/`Symbol.toPrimitive` (the engine
        // would render `[object Object]` and diverge), so it keeps bailing.
        // ToPrimitive (issue #304): a `+` where an operand is a STATICALLY-KNOWN-
        // CLASS object that defines `toString`/`valueOf` coerces that object via its
        // method AT LOWERING TIME (where the class is in scope), then concatenates/
        // adds the resulting primitives — never the default `[object Object]`. Plain
        // objects, arrays, and dynamic-class objects keep the gate below.
        if matches!(op, HirBinOp::Add)
            && (self.has_object_toprimitive(lhs) || self.has_object_toprimitive(rhs))
        {
            return self.lower_add_with_toprimitive(module, lhs, rhs);
        }

        let obj_operand = self.is_whole_object_value(lhs) || self.is_whole_object_value(rhs);
        if self.is_whole_heap_value(lhs) || self.is_whole_heap_value(rhs) {
            let array_string_concat = matches!(op, HirBinOp::Add)
                && !obj_operand
                && (is_proven_string_expr(lhs) || is_proven_string_expr(rhs));
            if !array_string_concat {
                return unsupported!(
                    "binary `{op:?}` on a whole object/array operand (ToPrimitive coercion is a later increment)"
                );
            }
        }

        let l = self.lower_expr(module, lhs)?;
        let r = self.lower_expr(module, rhs)?;

        // Strict equality `===`/`!==`. swc now lowers these to distinct
        // `StrictEq`/`StrictNe` ops, so the engine knows it is strict and can
        // lower soundly for ANY operand kinds (no coercion). Tagged → the runtime
        // `strict_eq`; native (proven-numeric/bool) → the native compare.
        if matches!(op, HirBinOp::StrictEq | HirBinOp::StrictNe) {
            if is_tagged(l) || is_tagged(r) {
                return self.lower_strict_eq(module, op, l, r);
            }
            return self.lower_compare(op, l, r);
        }
        // Loose equality `==`/`!=`. swc now lowers these to DISTINCT `Eq`/`Ne` ops
        // (no longer conflated with `===`/`!==`), so the engine can run the REAL JS
        // Abstract Equality algorithm (`__rtsadp_loose_eq`). The proven-same-kind
        // native path stays the fast `iadd`/`fcmp`-style compare (`0 == ""` etc.
        // need coercion, but two proven numbers don't); everything else routes to
        // the generic loose-eq, which ToPrimitive/ToNumber-coerces per spec.
        if matches!(op, HirBinOp::Eq | HirBinOp::Ne) {
            if !is_tagged(l) && !is_tagged(r) && same_proven_kind(l, r) {
                // Both proven, same kind (number==number, bool==bool): native.
                return self.lower_compare(op, l, r);
            }
            return self.lower_loose_eq(module, op, l, r);
        }
        // Relational `< <= > >=`: native when both proven numeric; else the
        // generic PolyValue path (mixed/string operands compared per JS rules).
        if matches!(op, HirBinOp::Lt | HirBinOp::Le | HirBinOp::Gt | HirBinOp::Ge) {
            if is_tagged(l) || is_tagged(r) {
                return self.lower_generic_relational(module, op, l, r);
            }
            return self.lower_compare(op, l, r);
        }
        // Arithmetic `+ - * / %` (and `**`): native fast path when both proven
        // numeric; the generic `__rtsadp_*` path when any operand is Tagged.
        if op.is_arithmetic() || matches!(op, HirBinOp::Exp) {
            return self.lower_arith(module, op, l, r);
        }
        // Bitwise/shifts `& | ^ << >> >>>`: always the generic PolyValue path.
        if matches!(
            op,
            HirBinOp::BitAnd
                | HirBinOp::BitOr
                | HirBinOp::BitXor
                | HirBinOp::Shl
                | HirBinOp::Shr
                | HirBinOp::UShr
        ) {
            return self.lower_bitwise(module, op, l, r);
        }
        unsupported!("binary operator {op:?}")
    }

    /// Generic relational `< <= > >=` over a tag-dispatched runtime compare →
    /// a `Bool` (i64 0/1). Used when either operand is Tagged.
    fn lower_generic_relational(
        &mut self,
        module: &mut dyn Module,
        op: HirBinOp,
        l: Val,
        r: Val,
    ) -> FrontResult<Val> {
        let sym = match op {
            HirBinOp::Lt => "__rtsadp_lt",
            HirBinOp::Le => "__rtsadp_le",
            HirBinOp::Gt => "__rtsadp_gt",
            HirBinOp::Ge => "__rtsadp_ge",
            _ => return unsupported!("generic relational op {op:?}"),
        };
        let ba = self.box_value(l);
        let bb = self.box_value(r);
        let res = self
            .call_runtime(module, sym, &[ba, bb])?
            .expect("relational returns a value");
        Ok(self.poly_bool_to_bool(res))
    }

    /// Bitwise/shift ops — always the generic `__rtsadp_*` trampoline (JS bitwise
    /// semantics are ToInt32/ToUint32 + 5-bit shift-count mask, NOT a native i64
    /// op). Result is a JS number (int32, or a double for a large `>>>`).
    fn lower_bitwise(
        &mut self,
        module: &mut dyn Module,
        op: HirBinOp,
        l: Val,
        r: Val,
    ) -> FrontResult<Val> {
        let sym = match op {
            HirBinOp::BitAnd => "__rtsadp_band",
            HirBinOp::BitOr => "__rtsadp_bor",
            HirBinOp::BitXor => "__rtsadp_bxor",
            HirBinOp::Shl => "__rtsadp_shl",
            HirBinOp::Shr => "__rtsadp_shr",
            HirBinOp::UShr => "__rtsadp_ushr",
            _ => return unsupported!("bitwise op {op:?}"),
        };
        let ba = self.box_value(l);
        let bb = self.box_value(r);
        let res = self
            .call_runtime(module, sym, &[ba, bb])?
            .expect("bitwise returns a value");
        Ok(Val { v: res, repr: Repr::Tagged, kind: JsKind::Number })
    }

    /// Reduce a boolean PolyValue word to an i64 0/1 `Bool` carrier by comparing
    /// against the `true` singleton (the shared tail of every generic predicate).
    pub(super) fn poly_bool_to_bool(&mut self, res: Value) -> Val {
        let true_word = self
            .builder
            .ins()
            .iconst(types::I64, value::PolyValue::bool(true).raw() as i64);
        let b = self.builder.ins().icmp(IntCC::Equal, res, true_word);
        let widened = self.builder.ins().uextend(types::I64, b);
        Val::new(widened, Repr::Bool)
    }

    /// Arithmetic `+ - * / % **`. Both-numeric uses the native fast path
    /// (UNCHANGED — the proven-numeric benchmarks must NOT route through the
    /// generic trampolines); any Tagged/string/mixed operand boxes both and calls
    /// the matching generic `__rtsadp_*` (`+` → `__rtsadp_add`, the ONE `+` path).
    pub(super) fn lower_arith(
        &mut self,
        module: &mut dyn Module,
        op: HirBinOp,
        l: Val,
        r: Val,
    ) -> FrontResult<Val> {
        if is_tagged(l) || is_tagged(r) {
            return self.lower_generic_arith(module, op, l, r);
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
            // Float `%` is fmod-style (sign of dividend); `**` has no native op.
            // Route both to the generic numeric trampolines (correct, rare).
            HirBinOp::Rem if !both_int => self.lower_generic_arith(module, op, l, r),
            HirBinOp::Exp => self.lower_generic_arith(module, op, l, r),
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

    /// The generic arithmetic path: box both operands to PolyValue and call the
    /// matching `__rtsadp_*` trampoline (the one tag-dispatched arithmetic path).
    fn lower_generic_arith(
        &mut self,
        module: &mut dyn Module,
        op: HirBinOp,
        l: Val,
        r: Val,
    ) -> FrontResult<Val> {
        let sym = match op {
            HirBinOp::Add => "__rtsadp_add",
            HirBinOp::Sub => "__rtsadp_sub",
            HirBinOp::Mul => "__rtsadp_mul",
            HirBinOp::Div => "__rtsadp_div",
            HirBinOp::Rem => "__rtsadp_mod",
            HirBinOp::Exp => "__rtsadp_pow",
            _ => return unsupported!("generic arithmetic op {op:?}"),
        };
        let ba = self.box_value(l);
        let bb = self.box_value(r);
        let res = self
            .call_runtime(module, sym, &[ba, bb])?
            .expect("generic arithmetic returns a value");
        // Every arithmetic op EXCEPT `+` produces a JS number unconditionally
        // (only `+` can yield a string via concatenation). Recording the proven
        // `Number` kind lets a following `x%2 === 0` see same-kind operands and
        // lower the strict-eq soundly instead of bailing on Unknown.
        let kind = if matches!(op, HirBinOp::Add) {
            JsKind::Unknown
        } else {
            JsKind::Number
        };
        Ok(Val { v: res, repr: Repr::Tagged, kind })
    }

    /// `==` / `!=` over the JS Abstract Equality algorithm (`__rtsadp_loose_eq`),
    /// → a `Bool` (i64 0/1). Used when an operand is Tagged or the kinds differ.
    fn lower_loose_eq(
        &mut self,
        module: &mut dyn Module,
        op: HirBinOp,
        l: Val,
        r: Val,
    ) -> FrontResult<Val> {
        let ba = self.box_value(l);
        let bb = self.box_value(r);
        let sym = match op {
            HirBinOp::Eq => "__rtsadp_loose_eq",
            HirBinOp::Ne => "__rtsadp_loose_neq",
            _ => return unsupported!("loose-eq op {op:?}"),
        };
        let res = self
            .call_runtime(module, sym, &[ba, bb])?
            .expect("loose-eq returns a value");
        Ok(self.poly_bool_to_bool(res))
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
            HirBinOp::StrictEq => "__rtsadp_strict_eq",
            HirBinOp::StrictNe => "__rtsadp_strict_neq",
            _ => return unsupported!("strict-eq op {op:?}"),
        };
        let res = self
            .call_runtime(module, sym, &[ba, bb])?
            .expect("strict-eq returns a value");
        Ok(self.poly_bool_to_bool(res))
    }

    /// Numeric comparison `< <= > >= == !=` → a `Bool`. Operands proven numeric
    /// (the Tagged `==`/`!=` case was already split off in `lower_bin`).
    fn lower_compare(&mut self, op: HirBinOp, l: Val, r: Val) -> FrontResult<Val> {
        let use_float = matches!(l.repr, Repr::Float64) || matches!(r.repr, Repr::Float64);
        let bool_cmp = matches!(l.repr, Repr::Bool) || matches!(r.repr, Repr::Bool);
        if bool_cmp
            && !matches!(
                op,
                HirBinOp::Eq | HirBinOp::Ne | HirBinOp::StrictEq | HirBinOp::StrictNe
            )
        {
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
}

// ---------------------------------------------------------------------------
// Free helpers shared by the binary-op lowering (and `expr.rs`).
// ---------------------------------------------------------------------------

pub(super) fn is_tagged(v: Val) -> bool {
    matches!(v.repr, Repr::Tagged)
}

/// Whether two operands have the SAME statically-proven JS kind — the condition
/// under which `==` and `===` agree (so equality is sound to lower despite the
/// HIR conflating the two operators). `Unknown` kinds never qualify.
fn same_proven_kind(l: Val, r: Val) -> bool {
    l.kind != JsKind::Unknown && l.kind == r.kind
}

pub(super) fn is_int_repr(r: Repr) -> bool {
    matches!(r, Repr::Int32 | Repr::Int64)
}

/// Whether `e` is a STATICALLY-PROVEN string expression (P5.8): a string literal,
/// or a `+` chain whose HIR result type is `Str` (what the template desugar emits —
/// the seed quasi is a string literal, forcing the whole chain to string
/// concatenation). Used to allow `string + array/object` (pure concatenation via
/// `String(...)`), which is well-defined, while still bailing the true ToPrimitive
/// `array + array` case.
fn is_proven_string_expr(e: &HirExpr) -> bool {
    use rts_hir::ir::{HirExprKind, HirLit};
    if matches!(e.ty, rts_hir::HirType::Str) {
        return true;
    }
    match &e.kind {
        HirExprKind::Lit(HirLit::Str(_)) => true,
        HirExprKind::Bin { op: HirBinOp::Add, lhs, rhs } => {
            is_proven_string_expr(lhs) || is_proven_string_expr(rhs)
        }
        _ => false,
    }
}

pub(super) fn wider_int(a: Repr, b: Repr) -> Repr {
    if matches!(a, Repr::Int64) || matches!(b, Repr::Int64) {
        Repr::Int64
    } else {
        Repr::Int32
    }
}

fn float_cc(op: HirBinOp) -> FrontResult<FloatCC> {
    Ok(match op {
        HirBinOp::Eq | HirBinOp::StrictEq => FloatCC::Equal,
        HirBinOp::Ne | HirBinOp::StrictNe => FloatCC::NotEqual,
        HirBinOp::Lt => FloatCC::LessThan,
        HirBinOp::Le => FloatCC::LessThanOrEqual,
        HirBinOp::Gt => FloatCC::GreaterThan,
        HirBinOp::Ge => FloatCC::GreaterThanOrEqual,
        _ => return unsupported!("comparison op {op:?}"),
    })
}

fn int_cc(op: HirBinOp) -> FrontResult<IntCC> {
    Ok(match op {
        HirBinOp::Eq | HirBinOp::StrictEq => IntCC::Equal,
        HirBinOp::Ne | HirBinOp::StrictNe => IntCC::NotEqual,
        HirBinOp::Lt => IntCC::SignedLessThan,
        HirBinOp::Le => IntCC::SignedLessThanOrEqual,
        HirBinOp::Gt => IntCC::SignedGreaterThan,
        HirBinOp::Ge => IntCC::SignedGreaterThanOrEqual,
        _ => return unsupported!("comparison op {op:?}"),
    })
}
