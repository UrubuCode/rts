//! `switch` statement lowering.
//!
//! JS `switch` semantics: evaluate the discriminant once, compare it with each
//! `case` test via STRICT equality (`===`) in source order, and begin executing at
//! the FIRST matching case — then FALL THROUGH into subsequent case bodies until a
//! `break` (or the switch ends). `default` runs when no case matches; per spec its
//! position in the fall-through chain is its SOURCE position (a `default` in the
//! middle is only entered after the no-match dispatch, but still falls through to
//! whatever case follows it textually).
//!
//! Lowering shape (one body block per case + a shared exit):
//!   - The discriminant is evaluated ONCE into a Tagged Variable.
//!   - A dispatch chain of test blocks compares the discriminant `=== case.test`
//!     (via the runtime `__rtsadp_strict_eq`); a match jumps to that case's body
//!     block. If no case matches, control jumps to the `default` body block (or the
//!     exit when there is no default).
//!   - Body blocks are emitted in source order and FALL THROUGH to the next body
//!     block (the JS fall-through) unless the body `break`s — `break` is wired to
//!     the exit via a `LoopCtx` pushed for the switch. `continue` inside the switch
//!     still targets the ENCLOSING loop (its `continue_target` is inherited).

use cranelift_codegen::ir::condcodes::FloatCC;
use cranelift_codegen::ir::{Block, InstBuilder, Value, types};
use cranelift_frontend::{Switch, Variable};

use rts_hir::ir::{HirExpr, HirExprKind, HirLit, HirSwitchCase};

use super::lower::{LoopCtx, Lowerer, cl_type};
use crate::front::error::FrontResult;
use crate::repr::Repr;

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    pub(super) fn lower_switch(
        &mut self,
        module: &mut dyn cranelift_module::Module,
        discriminant: &HirExpr,
        cases: &[HirSwitchCase],
    ) -> FrontResult<()> {
        // Evaluate the discriminant ONCE and stash it in a Tagged Variable so each
        // strict-equality test reads the same value (no re-evaluation of side
        // effects).
        let disc = self.lower_expr(module, discriminant)?;
        let disc_word = self.box_value(disc);
        let disc_var = self.builder.declare_var(cl_type(Repr::Tagged));
        self.builder.def_var(disc_var, disc_word);

        // One body block per case + the shared exit. The default's body block (if
        // any) is the dispatch fall-back target.
        let body_blocks: Vec<Block> = cases.iter().map(|_| self.builder.create_block()).collect();
        let exit_block = self.builder.create_block();
        let default_idx = cases.iter().position(|c| c.test.is_none());
        let no_match_target = default_idx.map(|i| body_blocks[i]).unwrap_or(exit_block);

        // ── fast dispatch: a jump table when every case tests an integer ─────────
        // The generic chain below costs ONE `__rtsadp_strict_eq` CALL per case, in
        // source order — a 20-case switch can pay 20 extern calls to reach the last
        // one. When the tests are integer literals, Cranelift's `Switch` builder
        // picks a `br_table` (or a binary search) instead: no calls, O(1)-ish.
        let jump_table = self.try_emit_switch_table(
            module,
            disc,
            disc_var,
            cases,
            &body_blocks,
            no_match_target,
        )?;

        // ── dispatch chain: test each case (in source order) for `disc === test` ──
        for (i, case) in cases.iter().enumerate() {
            if jump_table {
                break;
            }
            let Some(test) = &case.test else {
                continue; // `default` is not part of the equality dispatch
            };
            let test_block = self.builder.create_block();
            // Compare disc === test. `lower_bin` re-reads the discriminant from its
            // Variable via a synthetic ident is overkill; instead compute the
            // equality through the runtime helper directly.
            let eq = self.switch_case_matches(module, disc_var, test)?;
            self.builder
                .ins()
                .brif(eq, body_blocks[i], &[], test_block, &[]);
            self.builder.seal_block(test_block);
            self.builder.switch_to_block(test_block);
        }
        // No case matched → default (or exit). The jump-table path already emitted
        // its own terminator, so only the chain needs this.
        if !jump_table {
            self.builder.ins().jump(no_match_target, &[]);
        }

        // ── body blocks: emit in source order, FALL THROUGH to the next ──────────
        // `continue` inside a switch belongs to the enclosing loop; inherit its
        // target (or the exit when there is no enclosing loop — a `continue` there
        // bails elsewhere anyway).
        let continue_target = self
            .loop_stack
            .last()
            .map(|c| c.continue_target)
            .unwrap_or(exit_block);
        let label = self.pending_label.take();
        self.loop_stack.push(LoopCtx {
            exit: exit_block,
            continue_target,
            label,
            finally_depth: self.finally_stack.len(),
        });
        for (i, case) in cases.iter().enumerate() {
            self.builder.seal_block(body_blocks[i]);
            self.builder.switch_to_block(body_blocks[i]);
            self.block_terminated = false;
            self.lower_block(module, &case.body)?;
            // Fall through to the next case body (JS fall-through) unless this body
            // already terminated (a `break`/`return`/`throw`).
            if !self.block_terminated {
                let next = body_blocks.get(i + 1).copied().unwrap_or(exit_block);
                self.builder.ins().jump(next, &[]);
            }
        }
        self.loop_stack.pop();

        self.builder.seal_block(exit_block);
        self.builder.switch_to_block(exit_block);
        self.block_terminated = false;
        Ok(())
    }

    /// Emit an integer JUMP TABLE for this switch, or return `false` to let the
    /// caller fall back to the generic `===` chain.
    ///
    /// Taken only when the discriminant is a PROVEN number and every non-`default`
    /// case tests an integral literal. Both conditions matter:
    ///
    /// * A `Tagged` discriminant could be a string/object at runtime, where `===`
    ///   is not integer comparison at all.
    /// * A non-integral test (`case 1.5:`) cannot be a table key.
    ///
    /// ## Why the exactness guard exists
    ///
    /// TS `number` is an f64, so the discriminant usually arrives as `Float64` and
    /// has to be narrowed to an integer key. Truncating alone would be WRONG:
    /// `switch (1.5)` would truncate to `1` and enter `case 1:`. So the emitted
    /// code converts to i64, converts BACK to f64, and takes the table only if the
    /// round-trip is bit-exact; anything else jumps to the no-match target. That
    /// single `fcmp` also gives the right answer for the awkward values:
    ///
    /// * `NaN` — every comparison is false → no-match, matching JS (`NaN === NaN`
    ///   is false, so no case can match).
    /// * `±Infinity` and out-of-i64-range — the saturating convert clamps, so the
    ///   round-trip differs → no-match.
    /// * `-0.0` — round-trips to `0.0` and `-0.0 == 0.0` is true, so it enters
    ///   `case 0:`, which is correct (`-0 === 0` in JS).
    ///
    /// Duplicate case values keep the FIRST body (JS dispatches to the first
    /// match); `Switch` would otherwise reject or silently prefer the last.
    fn try_emit_switch_table(
        &mut self,
        module: &mut dyn cranelift_module::Module,
        disc: super::lower::Val,
        disc_var: Variable,
        cases: &[HirSwitchCase],
        body_blocks: &[Block],
        no_match_target: Block,
    ) -> FrontResult<bool> {
        // A Tagged discriminant is not provably numeric — the chain handles it.
        if !matches!(disc.repr, Repr::Int32 | Repr::Int64 | Repr::Float64) {
            return Ok(false);
        }

        // Collect (key, body) for every non-default case; bail on the first test
        // that is not an integral literal.
        let mut entries: Vec<(i64, Block)> = Vec::new();
        for (i, case) in cases.iter().enumerate() {
            let Some(test) = &case.test else { continue };
            let HirExprKind::Lit(lit) = &test.kind else {
                return Ok(false);
            };
            let key = match lit {
                HirLit::Int(v) => *v,
                HirLit::Number(f) | HirLit::Float(f) => {
                    // Integral and representable as i64 — `case 1.5:` and
                    // `case 1e300:` are not table keys.
                    if f.fract() != 0.0 || !f.is_finite() || *f < i64::MIN as f64 || *f > i64::MAX as f64
                    {
                        return Ok(false);
                    }
                    *f as i64
                }
                _ => return Ok(false),
            };
            entries.push((key, body_blocks[i]));
        }
        // Nothing to dispatch on (`switch (x) { default: … }`) — the chain's single
        // jump is already optimal.
        if entries.is_empty() {
            return Ok(false);
        }

        // The key, narrowed to i64, plus the exactness guard described above.
        let disc_word = self.builder.use_var(disc_var);
        let _ = disc_word; // the Variable exists for the chain path; the table reads `disc` directly
        let key = self.coerce(disc, Repr::Int64)?;
        if matches!(disc.repr, Repr::Float64) {
            let back = self.builder.ins().fcvt_from_sint(types::F64, key);
            let exact = self.builder.ins().fcmp(FloatCC::Equal, back, disc.v);
            let table_block = self.builder.create_block();
            self.builder
                .ins()
                .brif(exact, table_block, &[], no_match_target, &[]);
            self.builder.seal_block(table_block);
            self.builder.switch_to_block(table_block);
        }

        let mut sw = Switch::new();
        let mut seen = std::collections::HashSet::new();
        for (key_value, block) in entries {
            if seen.insert(key_value) {
                sw.set_entry(key_value as u64 as u128, block);
            }
        }
        sw.emit(self.builder, key, no_match_target);
        Ok(true)
    }

    /// `disc === test` for a switch case: read the discriminant from its Variable,
    /// lower the case test, and compute strict equality through the runtime helper.
    /// Returns an i64 0/1 the dispatch `brif` consumes.
    fn switch_case_matches(
        &mut self,
        module: &mut dyn cranelift_module::Module,
        disc_var: Variable,
        test: &HirExpr,
    ) -> FrontResult<Value> {
        let disc_word = self.builder.use_var(disc_var);
        let t = self.lower_expr(module, test)?;
        let t_word = self.box_value(t);
        let res = self
            .call_runtime(module, "__rtsadp_strict_eq", &[disc_word, t_word])?
            .expect("__rtsadp_strict_eq returns a value");
        // The helper returns a boolean PolyValue; `as_bool_value` turns it into the
        // i64 0/1 the dispatch `brif` consumes.
        let bool_val = super::lower::Val::tagged_kind(res, super::lower::JsKind::Bool);
        self.as_bool_value(module, bool_val)
    }
}
