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
        // works); the logical ops (`&&=`/`||=`/`??=`) ride the binary `&&`/`||`/`??`
        // expression lowering — `obj.x ||= v` → `obj.x = (obj.x || v)`. Restricted to
        // a SIDE-EFFECT-FREE object (a bare identifier — incl. `this`) so re-reading
        // the object for the load and the store is harmless; a complex object expr
        // (`f().p += x`) would double-evaluate, so it bails. NOTE: the logical desugar
        // does NOT short-circuit the RHS (the binary form always evaluates it before
        // the store) — exact for a side-effect-free RHS on a plain data property,
        // which is the covered surface.
        if Self::target_is_replayable(target) {
            // Logical assign on a MEMBER short-circuits the whole STORE (spec
            // AssignmentExpression : LeftHandSideExpression &&= ...): the target
            // is read once, and the RHS + the setter run ONLY on the taken
            // branch (`obj.x ??= v` with `x` present must not fire the setter —
            // the `obj.x = (obj.x ?? v)` desugar fired it unconditionally).
            if matches!(
                op,
                HirBinOp::LogAnd | HirBinOp::LogOr | HirBinOp::NullCoalesce
            ) {
                return self.lower_logical_assign_member(module, op, target, value);
            }
            if op.is_arithmetic() || op.is_bitwise() || matches!(op, HirBinOp::Exp) {
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
        }
        // A member/index target whose base is NOT replayable (`f().p += x`,
        // `a[g()].n |= m`): evaluate each effectful part ONCE into a hidden
        // local and retry against those. The desugar then replays only bare
        // identifiers, so nothing runs twice — which is the whole reason the
        // replay restriction existed. Same technique the destructuring path uses.
        match &target.kind {
            HirExprKind::Member { object, prop } if !Self::is_replayable_base(object) => {
                let base = self.hidden_local_for(module, object, "obj")?;
                let t = HirExpr::new(
                    HirExprKind::Member {
                        object: Box::new(base),
                        prop: prop.clone(),
                    },
                    HirType::Unknown,
                );
                return self.lower_assign_op(module, op, &t, value);
            }
            HirExprKind::Index { object, index } => {
                let base_ok = Self::is_replayable_base(object);
                let idx_ok = matches!(index.kind, HirExprKind::Ident(_) | HirExprKind::Lit(_));
                if !base_ok || !idx_ok {
                    let base = if base_ok {
                        (**object).clone()
                    } else {
                        self.hidden_local_for(module, object, "obj")?
                    };
                    let idx = if idx_ok {
                        (**index).clone()
                    } else {
                        self.hidden_local_for(module, index, "idx")?
                    };
                    let t = HirExpr::new(
                        HirExprKind::Index {
                            object: Box::new(base),
                            index: Box::new(idx),
                        },
                        HirType::Unknown,
                    );
                    return self.lower_assign_op(module, op, &t, value);
                }
            }
            _ => {}
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
            } else if op.is_bitwise() {
                // Same ToInt32/ToUint32 generic path a plain local uses — the cell
                // read is already Tagged, exactly what `lower_bitwise` boxes.
                self.lower_bitwise(module, op, cur, rhs)?
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
            } else if op.is_bitwise() {
                self.lower_bitwise(module, op, cur, rhs)?
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
        // Tier 2.2: `n += 1` must read the same DECLARED-native-int provenance the
        // equivalent `n = n + 1` gets from `lower_ident` — otherwise the two
        // spellings of one operation would disagree on overflow semantics.
        let cur_val = Val::new(cur, local.repr).as_native_int(self.native_int_locals.contains(&name));
        let rhs = self.lower_expr(module, value)?;
        let result = if op.is_arithmetic() || matches!(op, HirBinOp::Exp) {
            self.lower_arith(module, op, cur_val, rhs)?
        } else if op.is_bitwise() {
            // Bitwise compound-assign (`&= |= ^= <<= >>= >>>=`) on a plain local —
            // JS ToInt32/ToUint32 semantics live in the generic `__rtsadp_*` path.
            self.lower_bitwise(module, op, cur_val, rhs)?
        } else {
            return unsupported!("compound-assign operator {op:?}");
        };
        let coerced = self.coerce(result, local.repr)?;
        self.builder.def_var(local.var, coerced);
        Ok(Val::new(coerced, local.repr))
    }

    /// Whether the WHOLE member/index target can be replayed: the base, and —
    /// for an index — the KEY too. Checking only the base was a real hole:
    /// `arr[f()] *= 3` passed the base test, took the desugar path, and ran
    /// `f()` TWICE (medido: 2 chamadas onde o Node faz 1).
    pub(super) fn target_is_replayable(target: &HirExpr) -> bool {
        match &target.kind {
            HirExprKind::Member { object, .. } => Self::is_replayable_base(object),
            HirExprKind::Index { object, index } => {
                Self::is_replayable_base(object)
                    && matches!(index.kind, HirExprKind::Ident(_) | HirExprKind::Lit(_))
            }
            _ => false,
        }
    }

    /// Evaluate `e` ONCE, bind the word to a fresh hidden local, and return an
    /// `Ident` expression naming it. Lets a target with an effectful part be
    /// rewritten into one whose parts are all bare identifiers — replayable by
    /// construction, so the load/store desugar cannot re-run the effect.
    pub(super) fn hidden_local_for(
        &mut self,
        module: &mut dyn Module,
        e: &HirExpr,
        what: &str,
    ) -> FrontResult<HirExpr> {
        let v = self.lower_expr(module, e)?;
        let word = self.box_value(v);
        // Nome único por SÍTIO: o contador de blocos do builder já é único e
        // monotônico dentro da função, e o `what` separa base de índice no
        // mesmo alvo. Um nome reusado sobrescreveria o temporário de um
        // compound-assign aninhado.
        let name = format!(
            "__rtsn_ca_{what}_{}",
            self.builder.create_block().as_u32()
        );
        self.bind_tagged_local(&name, Val::new(word, Repr::Tagged));
        Ok(HirExpr::new(HirExprKind::Ident(name), HirType::Unknown))
    }

    /// Whether the OBJECT of a compound-assign target can be evaluated TWICE
    /// (once for the load, once for the store) with the same result and no extra
    /// effect — which is what lets `o.p += v` desugar to `o.p = o.p + v`.
    ///
    /// A bare identifier qualifies (`this` reaches here as one), and so does a CHAIN of
    /// plain property reads over one: `this.$1.count += 1` and `a.b[k] |= m` are
    /// routine in minified code, and requiring the base to be a single ident
    /// rejected the whole file for them. An INDEX is only replayable when the key
    /// itself is — a literal or an identifier; anything computed (`a[f()].n += 1`)
    /// would run twice, so it keeps bailing.
    ///
    /// The assumption is the same one the single-ident case already made: a plain
    /// property read has no observable effect. A getter with side effects breaks
    /// it, exactly as it did before — this widens the shape, not the risk.
    pub(super) fn is_replayable_base(e: &HirExpr) -> bool {
        match &e.kind {
            HirExprKind::Ident(_) => true,
            HirExprKind::Member { object, .. } => Self::is_replayable_base(object),
            HirExprKind::Index { object, index } => {
                Self::is_replayable_base(object)
                    && matches!(
                        index.kind,
                        HirExprKind::Ident(_) | HirExprKind::Lit(_)
                    )
            }
            _ => false,
        }
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

    /// Logical-assign on a MEMBER/INDEX target (`obj.x ||= v`, `arr[i] ??= v`).
    /// The target is READ once (its getter runs once); the RHS and the STORE run
    /// only on the taken branch — the expression yields the old value otherwise.
    /// The object is a bare identifier (the caller's gate), so the store path
    /// re-reading it is harmless.
    fn lower_logical_assign_member(
        &mut self,
        module: &mut dyn Module,
        op: HirBinOp,
        target: &HirExpr,
        value: &HirExpr,
    ) -> FrontResult<Val> {
        let cur = self.lower_expr(module, target)?;
        let cur_word = self.box_value(cur);
        let do_assign = match op {
            HirBinOp::LogAnd => self.as_bool_value(module, cur)?,
            HirBinOp::LogOr => {
                let b = self.as_bool_value(module, cur)?;
                let zero = self.builder.ins().iconst(types::I64, 0);
                let neg = self.builder.ins().icmp(IntCC::Equal, b, zero);
                self.builder.ins().uextend(types::I64, neg)
            }
            HirBinOp::NullCoalesce => {
                let null_w = self
                    .builder
                    .ins()
                    .iconst(types::I64, crate::value::PolyValue::null().raw() as i64);
                let undef_w = self.builder.ins().iconst(
                    types::I64,
                    crate::value::PolyValue::undefined().raw() as i64,
                );
                let is_null = self.builder.ins().icmp(IntCC::Equal, cur_word, null_w);
                let is_undef = self.builder.ins().icmp(IntCC::Equal, cur_word, undef_w);
                let either = self.builder.ins().bor(is_null, is_undef);
                self.builder.ins().uextend(types::I64, either)
            }
            _ => return unsupported!("logical-assign op {op:?}"),
        };

        let assign_block = self.builder.create_block();
        let cont_block = self.builder.create_block();
        self.builder.append_block_param(cont_block, types::I64);
        self.builder
            .ins()
            .brif(do_assign, assign_block, &[], cont_block, &[cur_word.into()]);

        self.builder.switch_to_block(assign_block);
        self.builder.seal_block(assign_block);
        // The plain `=` path: evaluates the RHS and runs the setter — only here.
        let new_v = self.lower_assign(module, target, value)?;
        let new_word = self.box_value(new_v);
        self.builder.ins().jump(cont_block, &[new_word.into()]);

        self.builder.switch_to_block(cont_block);
        self.builder.seal_block(cont_block);
        let merged = self.builder.block_params(cont_block)[0];
        Ok(Val::new(merged, Repr::Tagged))
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
