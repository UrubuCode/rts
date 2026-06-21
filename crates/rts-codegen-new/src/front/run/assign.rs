//! Compound + logical assignment lowering (P5.6) — split out of [`super::stmt`]
//! (the <500-line module rule).
//!
//! - Compound assign `x <op>= e` → `x = x <op> e`: arithmetic + `**` reuse
//!   [`super::lower::Lowerer::lower_arith`].
//! - Logical-assign `&&=`/`||=`/`??=`: short-circuit — the RHS is evaluated and
//!   stored ONLY on the taken branch, via a Cranelift `if`/merge with the local's
//!   value flowing as the φ at the join. rts-hir now carries the distinct
//!   `LogAnd`/`LogOr`/`NullCoalesce` ops on the compound-assign node.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;

use rts_hir::ir::HirExprKind;
use rts_hir::{HirBinOp, HirExpr, HirType};

use crate::repr::Repr;

use crate::front::error::{FrontResult, Unsupported, unsupported};

use super::lower::{Local, Lowerer, Val};
use super::stmt::ident_target;

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Compound assignment `x <op>= e` → `x = x <op> e`. Arithmetic + `**` reuse
    /// `lower_arith`; the logical-assign ops (`&&=`/`||=`/`??=`) short-circuit.
    pub(super) fn lower_assign_op(
        &mut self,
        module: &mut dyn Module,
        op: HirBinOp,
        target: &HirExpr,
        value: &HirExpr,
    ) -> FrontResult<Val> {
        // MEMBER / INDEX compound-assign (`this.n += x`, `obj.prop *= 2`,
        // `arr[i] += 1`): desugar to `target = (target <op> value)` and reuse the
        // member/index write path. A compound assignment evaluates to the NEW value,
        // which is exactly what the synthesized `=` returns — so the desugar is
        // semantically exact (unlike `++`, there is no old-vs-new ambiguity). The
        // arithmetic/`+`-concat op rides the normal binary lowering (string `+=`
        // works). Restricted to arithmetic/`**` (logical-assign on a member is a
        // later increment) and a SIDE-EFFECT-FREE object (a bare identifier — incl.
        // `this`) so re-reading the object for the load and the store is harmless;
        // a complex object expr (`f().p += x`) would double-evaluate, so it bails.
        if matches!(
            &target.kind,
            HirExprKind::Member { object, .. } | HirExprKind::Index { object, .. }
                if matches!(object.kind, HirExprKind::Ident(_))
        ) && (op.is_arithmetic() || matches!(op, HirBinOp::Exp))
        {
            let new_value = HirExpr::new(
                HirExprKind::Bin {
                    op,
                    lhs: Box::new(target.clone()),
                    rhs: Box::new(value.clone()),
                },
                HirType::Unknown,
            );
            return self.lower_assign(module, target, &new_value);
        }
        let name = ident_target(target)?;
        // Logical-assign ops short-circuit: `a &&= b` only evaluates/assigns `b`
        // when `a` is truthy (resp. `||=` when falsy, `??=` when nullish). Handled
        // by a dedicated path that builds the conditional store.
        if matches!(
            op,
            HirBinOp::LogAnd | HirBinOp::LogOr | HirBinOp::NullCoalesce
        ) {
            return self.lower_logical_assign(module, op, &name, value);
        }
        // MODULE-LEVEL MUTABLE GLOBAL (epic #195): `x <op>= e` on a cell var. Read
        // the cell, apply the arithmetic generically (Tagged current + rhs), store
        // back. This is the rts:test harness's `__rtsCapturedOutput += v + "\n"`.
        if let Some(id) = self.gcell_id(&name) {
            let cur = self.emit_gcell_get(module, id)?;
            let rhs = self.lower_expr(module, value)?;
            let result = if op.is_arithmetic() || matches!(op, HirBinOp::Exp) {
                self.lower_arith(module, op, cur, rhs)?
            } else {
                return unsupported!("compound-assign operator {op:?} on a global cell");
            };
            let word = self.box_value(result);
            self.emit_gcell_set(module, id, word)?;
            return Ok(Val::new(word, Repr::Tagged));
        }
        // FUNCTION-LOCAL CELL (#195): `x <op>= e` through the cell — read live,
        // apply the arithmetic generically (Tagged), store back.
        if self.is_cell_local(&name) {
            let handle = {
                let local = self.local(&name).expect("cell-local is a bound local");
                self.builder.use_var(local.var)
            };
            let cur = self.emit_cell_get(module, handle);
            let rhs = self.lower_expr(module, value)?;
            let result = if op.is_arithmetic() || matches!(op, HirBinOp::Exp) {
                self.lower_arith(module, op, cur, rhs)?
            } else {
                return unsupported!("compound-assign operator {op:?} on a local cell");
            };
            let word = self.box_value(result);
            self.emit_cell_set(module, handle, word);
            return Ok(Val::new(word, Repr::Tagged));
        }
        let local = self
            .local(&name)
            .ok_or_else(|| Unsupported::new(format!("compound-assign to unbound `{name}`")))?;
        let cur = self.builder.use_var(local.var);
        let cur_val = Val::new(cur, local.repr);
        let rhs = self.lower_expr(module, value)?;
        let result = if op.is_arithmetic() || matches!(op, HirBinOp::Exp) {
            self.lower_arith(module, op, cur_val, rhs)?
        } else {
            return unsupported!("compound-assign operator {op:?}");
        };
        let coerced = self.coerce(result, local.repr)?;
        self.builder.def_var(local.var, coerced);
        Ok(Val::new(coerced, local.repr))
    }

    /// Logical-assign `a &&= b` / `a ||= b` / `a ??= b` on a simple ident LHS.
    /// `a &&= b` ≡ `if (toBoolean(a)) a = b` (returns the final `a`); `||=` is the
    /// negated condition; `??=` tests `a == null`. The RHS is evaluated ONLY on the
    /// taken branch (short-circuit), via a Cranelift `if`/merge with the local's
    /// value flowing as the φ at the join.
    fn lower_logical_assign(
        &mut self,
        module: &mut dyn Module,
        op: HirBinOp,
        name: &str,
        value: &HirExpr,
    ) -> FrontResult<Val> {
        // A FUNCTION-LOCAL CELL (#195) holds its value behind a handle, so the
        // def_var-based conditional store below would clobber the handle. Bail
        // honestly — logical-assign on a captured-mutated local is a later increment.
        if self.is_cell_local(name) {
            return unsupported!("logical-assign on a captured-mutated local `{name}`");
        }
        let local = self
            .local(name)
            .ok_or_else(|| Unsupported::new(format!("logical-assign to unbound `{name}`")))?;
        let cur = self.builder.use_var(local.var);
        // The branch condition: `&&=` assigns when current is truthy; `||=` when
        // falsy; `??=` when nullish (null/undefined). Build an i64 0/1 `do_assign`.
        let do_assign = match op {
            HirBinOp::LogAnd => self.as_bool_value(module, Val::new(cur, local.repr))?,
            HirBinOp::LogOr => {
                let b = self.as_bool_value(module, Val::new(cur, local.repr))?;
                let zero = self.builder.ins().iconst(types::I64, 0);
                let neg = self.builder.ins().icmp(IntCC::Equal, b, zero);
                self.builder.ins().uextend(types::I64, neg)
            }
            HirBinOp::NullCoalesce => self.is_nullish_cond(local, cur),
            _ => return unsupported!("logical-assign op {op:?}"),
        };

        let assign_block = self.builder.create_block();
        let cont_block = self.builder.create_block();
        self.builder
            .ins()
            .brif(do_assign, assign_block, &[], cont_block, &[]);

        self.builder.switch_to_block(assign_block);
        self.builder.seal_block(assign_block);
        let rhs = self.lower_expr(module, value)?;
        let coerced = self.coerce(rhs, local.repr)?;
        self.builder.def_var(local.var, coerced);
        self.builder.ins().jump(cont_block, &[]);

        self.builder.seal_block(cont_block);
        self.builder.switch_to_block(cont_block);
        // After the merge the local holds either the old or the new value (the
        // builder reconstructs the φ); re-read it as the expression's result.
        let merged = self.builder.use_var(local.var);
        Ok(Val::new(merged, local.repr))
    }

    /// An i64 0/1 condition that is `1` iff the local `cur` is JS-nullish
    /// (null/undefined). A native-repr local (number/bool) is never nullish → `0`;
    /// a Tagged local compares against the null AND undefined singleton words.
    fn is_nullish_cond(&mut self, local: Local, cur: Value) -> Value {
        if !matches!(local.repr, Repr::Tagged) {
            return self.builder.ins().iconst(types::I64, 0);
        }
        let null_w = self
            .builder
            .ins()
            .iconst(types::I64, crate::value::PolyValue::null().raw() as i64);
        let undef_w = self.builder.ins().iconst(
            types::I64,
            crate::value::PolyValue::undefined().raw() as i64,
        );
        let is_null = self.builder.ins().icmp(IntCC::Equal, cur, null_w);
        let is_undef = self.builder.ins().icmp(IntCC::Equal, cur, undef_w);
        let either = self.builder.ins().bor(is_null, is_undef);
        self.builder.ins().uextend(types::I64, either)
    }
}
