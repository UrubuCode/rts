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

use cranelift_codegen::ir::{Block, InstBuilder, Value};
use cranelift_frontend::Variable;

use rts_hir::ir::{HirExpr, HirSwitchCase};

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

        // ── dispatch chain: test each case (in source order) for `disc === test` ──
        for (i, case) in cases.iter().enumerate() {
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
        // No case matched → default (or exit).
        self.builder.ins().jump(no_match_target, &[]);

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
