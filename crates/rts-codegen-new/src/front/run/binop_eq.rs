//! Equality / comparison / logical / nullish-coalesce lowering.
//!
//! Split out of [`super::binop`] (the 500-line module rule). Holds the operators
//! whose lowering is a runtime tag-dispatch (`==`/`===`) or a short-circuiting
//! control-flow shape (`&&`/`||`/`??`), keeping `binop.rs` focused on the
//! native-vs-generic arithmetic/relational decision.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, types};
use cranelift_module::Module;

use rts_hir::{HirBinOp, HirExpr};

use crate::repr::Repr;
use crate::value;

use crate::front::error::{FrontResult, unsupported};

use super::binop::{float_cc, int_cc, is_effect_free};
use super::lower::{JsKind, Lowerer, Val};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// `==` / `!=` over the JS Abstract Equality algorithm (`__rtsadp_loose_eq`),
    /// → a `Bool` (i64 0/1). Used when an operand is Tagged or the kinds differ.
    pub(super) fn lower_loose_eq(
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
    pub(super) fn lower_strict_eq(
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
    pub(super) fn lower_compare(&mut self, op: HirBinOp, l: Val, r: Val) -> FrontResult<Val> {
        // STRICT equality across a Bool and a NUMBER is ALWAYS unequal — `===` never
        // coerces, and `boolean` and `number` are different TYPES, so `0 === false` /
        // `1 === true` are `false` (NOT the value-equal `0 == false` of loose `==`,
        // which routes to the runtime loose-eq, never here). Without this the raw
        // `icmp(0, 0)` below would treat `false` (repr 0) as the number 0. Only fires
        // when EXACTLY one side is Bool (both-Bool / both-numeric compare normally).
        if matches!(op, HirBinOp::StrictEq | HirBinOp::StrictNe) {
            let l_bool = matches!(l.repr, Repr::Bool);
            let r_bool = matches!(r.repr, Repr::Bool);
            if l_bool != r_bool {
                let val = matches!(op, HirBinOp::StrictNe); // `===` → false, `!==` → true
                let c = self.builder.ins().iconst(types::I64, val as i64);
                return Ok(Val::new(c, Repr::Bool));
            }
        }
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
    pub(super) fn lower_logical(
        &mut self,
        module: &mut dyn Module,
        op: HirBinOp,
        lhs: &HirExpr,
        rhs: &HirExpr,
    ) -> FrontResult<Val> {
        let l = self.lower_expr(module, lhs)?;
        // Both operands PROVEN bool AND the rhs is side-effect-free: the branchless
        // select fast path (evaluating rhs eagerly is observationally identical, and
        // the result stays a proven Bool). A rhs with side effects (a CALL) must NOT
        // take this path — `&&`/`||` short-circuit, so `false && f()` must NOT run
        // `f()`; it falls through to the general short-circuit form below.
        if matches!(l.repr, Repr::Bool) && is_effect_free(rhs) {
            let r = self.lower_expr(module, rhs)?;
            if matches!(r.repr, Repr::Bool) {
                let v = match op {
                    HirBinOp::LogAnd => self.builder.ins().select(l.v, r.v, l.v),
                    HirBinOp::LogOr => self.builder.ins().select(l.v, l.v, r.v),
                    _ => return unsupported!("logical op {op:?}"),
                };
                return Ok(Val::new(v, Repr::Bool));
            }
            // Mixed (bool && non-bool effect-free): fall through to the general form.
        }

        // GENERAL JS `&&`/`||`: the result is one of the OPERANDS (not a bool), with
        // true short-circuit — `b` is evaluated only when needed. `a && b` yields `b`
        // when `a` is truthy else `a`; `a || b` yields `a` when truthy else `b`. The
        // ToBoolean test is on `a`'s value; the carried result is the raw operand
        // word (Tagged, since the two operands may differ in type).
        let a_word = self.box_value(l);
        // ToBoolean of `a` (Tagged) — `as_bool_value` routes a Tagged through
        // `__rtsadp_to_boolean` (handles "", 0, null, undefined, NaN as falsy).
        let a_tagged = Val::new(a_word, Repr::Tagged);
        let a_truthy = self.as_bool_value(module, a_tagged)?;

        let rhs_blk = self.builder.create_block();
        let carry_blk = self.builder.create_block();
        let join_blk = self.builder.create_block();
        self.builder.append_block_param(join_blk, types::I64);

        // `&&`: truthy → evaluate `b`; falsy → carry `a`. `||`: the reverse.
        let (true_blk, false_blk) = match op {
            HirBinOp::LogAnd => (rhs_blk, carry_blk),
            HirBinOp::LogOr => (carry_blk, rhs_blk),
            _ => return unsupported!("logical op {op:?}"),
        };
        self.builder
            .ins()
            .brif(a_truthy, true_blk, &[], false_blk, &[]);

        // rhs branch: evaluate `b`, yield its word.
        self.builder.switch_to_block(rhs_blk);
        self.builder.seal_block(rhs_blk);
        let b = self.lower_expr(module, rhs)?;
        let b_word = self.box_value(b);
        self.builder.ins().jump(join_blk, &[b_word.into()]);

        // carry branch: yield `a`'s word unchanged.
        self.builder.switch_to_block(carry_blk);
        self.builder.seal_block(carry_blk);
        self.builder.ins().jump(join_blk, &[a_word.into()]);

        self.builder.switch_to_block(join_blk);
        self.builder.seal_block(join_blk);
        let res = self.builder.block_params(join_blk)[0];
        Ok(Val::new(res, Repr::Tagged))
    }

    /// Nullish coalescing `a ?? b` — short-circuit: lower `a`; if its boxed word
    /// is the `null` OR `undefined` singleton, lower and yield `b`, otherwise
    /// carry `a`. `b` is evaluated ONLY on the nullish branch (true JS
    /// short-circuit). Mirrors the optional-chain block structure
    /// ([`super::optchain_lower`]): a `brif` to a `b`-block vs an `a`-carry block,
    /// merged through a continuation block param holding the result word. The
    /// result is `Tagged`/`Unknown` since `a` and `b` may differ in type.
    pub(super) fn lower_nullish_coalesce(
        &mut self,
        module: &mut dyn Module,
        lhs: &HirExpr,
        rhs: &HirExpr,
    ) -> FrontResult<Val> {
        let a = self.lower_expr(module, lhs)?;
        let a_word = self.box_value(a);

        // `a` is nullish iff its word equals the `null` OR `undefined` singleton.
        let null_w = self
            .builder
            .ins()
            .iconst(types::I64, value::PolyValue::null().raw() as i64);
        let undef_w = self
            .builder
            .ins()
            .iconst(types::I64, value::PolyValue::undefined().raw() as i64);
        let is_null = self.builder.ins().icmp(IntCC::Equal, a_word, null_w);
        let is_undef = self.builder.ins().icmp(IntCC::Equal, a_word, undef_w);
        let nullish = self.builder.ins().bor(is_null, is_undef);

        let rhs_blk = self.builder.create_block();
        let carry_blk = self.builder.create_block();
        let join_blk = self.builder.create_block();
        self.builder.append_block_param(join_blk, types::I64);

        self.builder
            .ins()
            .brif(nullish, rhs_blk, &[], carry_blk, &[]);

        // nullish → evaluate `b` and yield it.
        self.builder.switch_to_block(rhs_blk);
        self.builder.seal_block(rhs_blk);
        let b = self.lower_expr(module, rhs)?;
        let b_word = self.box_value(b);
        self.builder.ins().jump(join_blk, &[b_word.into()]);

        // present → carry `a` (`a_word` dominates this block; `b` is never reached).
        self.builder.switch_to_block(carry_blk);
        self.builder.seal_block(carry_blk);
        self.builder.ins().jump(join_blk, &[a_word.into()]);

        self.builder.switch_to_block(join_blk);
        self.builder.seal_block(join_blk);
        let result = self.builder.block_params(join_blk)[0];
        Ok(Val {
            v: result,
            repr: Repr::Tagged,
            kind: JsKind::Unknown,
        })
    }
}
