//! `break` / `continue` lowering, including the FINALIZER-running on an abrupt
//! loop exit (split out of [`super::loops`] to keep it under the <500-line rule).
//!
//! A `break`/`continue` that ESCAPES a `try/finally` between it and its target
//! loop must run every escaped `finally` first (innermost-first) — JS runs
//! `finally` on abrupt loop completion too. Each loop's `LoopCtx` records the
//! `finally_stack` depth at its entry (`finally_depth`); the jump runs every
//! finalizer pushed after that depth.

use cranelift_codegen::ir::InstBuilder;
use cranelift_module::Module;

use rts_hir::HirStmt;

use crate::front::error::FrontResult;

use super::lower::Lowerer;

/// Whether a labeled statement's body is a construct that CONSUMES the pending
/// label into its own `LoopCtx` (a loop or a switch — `break`/`continue label`
/// targets). A body that does NOT consume the label (a block, an `if`, any other
/// statement) is a `break`-only label target handled by [`Lowerer::lower_labeled_block`].
pub(super) fn stmt_consumes_label(s: &HirStmt) -> bool {
    matches!(
        s,
        HirStmt::For { .. }
            | HirStmt::ForOf { .. }
            | HirStmt::ForIn { .. }
            | HirStmt::While { .. }
            | HirStmt::DoWhile { .. }
            | HirStmt::Switch { .. }
    )
}

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Lower a labeled NON-loop statement (`label: { … break label; … }`). This is
    /// a valid JS `break`-only target: `break label` jumps PAST the labeled
    /// statement. Give it its own `LoopCtx` (exit = a fresh continuation block;
    /// `continue_target` reuses `exit` — a `continue label` into a non-loop is a
    /// syntax error tsc rejects, never reached). Both the natural end of the body
    /// and every `break label` land at the continuation block.
    pub(super) fn lower_labeled_block(
        &mut self,
        module: &mut dyn Module,
        label: &str,
        body: &HirStmt,
    ) -> FrontResult<()> {
        let exit_block = self.builder.create_block();
        self.loop_stack.push(super::lower::LoopCtx {
            exit: exit_block,
            continue_target: exit_block,
            label: Some(label.to_string()),
            finally_depth: self.finally_stack.len(),
        });
        let r = self.lower_stmt(module, body);
        self.loop_stack.pop();
        r?;
        if !self.block_terminated {
            self.builder.ins().jump(exit_block, &[]);
        }
        self.builder.seal_block(exit_block);
        self.builder.switch_to_block(exit_block);
        self.block_terminated = false;
        Ok(())
    }

    /// `break` — jump to a loop's exit. Unlabeled → the innermost loop/switch; a
    /// `break label` → the enclosing loop carrying that label (searched outward).
    /// A `break` outside any loop, or an unknown label, BAILS. Every `finally` the
    /// jump ESCAPES (pushed after the target loop's `finally_depth`) runs first,
    /// innermost-first — JS runs `finally` on an abrupt loop exit.
    pub(super) fn lower_break(
        &mut self,
        module: &mut dyn Module,
        label: Option<&str>,
    ) -> FrontResult<()> {
        let ctx = match label {
            Some(l) => self
                .loop_stack
                .iter()
                .rev()
                .find(|c| c.label.as_deref() == Some(l))
                .cloned()
                .ok_or_else(|| {
                    crate::front::error::Unsupported::new(format!(
                        "`break {l}`: no enclosing loop labeled `{l}`"
                    ))
                })?,
            None => self
                .loop_stack
                .last()
                .cloned()
                .ok_or_else(|| crate::front::error::Unsupported::new("`break` outside a loop"))?,
        };
        self.run_escaping_finalizers(module, ctx.finally_depth)?;
        if self.block_terminated {
            // A finalizer's own `return`/`throw` overrode the break — done.
            return Ok(());
        }
        self.builder.ins().jump(ctx.exit, &[]);
        self.block_terminated = true;
        Ok(())
    }

    /// `continue` — jump to a loop's continue target (header / update / advance).
    /// Unlabeled → the innermost loop; a `continue label` → the enclosing loop with
    /// that label. A `continue` outside any loop, or an unknown label, BAILS. As
    /// with `break`, every escaped `finally` runs first.
    pub(super) fn lower_continue(
        &mut self,
        module: &mut dyn Module,
        label: Option<&str>,
    ) -> FrontResult<()> {
        let ctx = match label {
            Some(l) => self
                .loop_stack
                .iter()
                .rev()
                .find(|c| c.label.as_deref() == Some(l))
                .cloned()
                .ok_or_else(|| {
                    crate::front::error::Unsupported::new(format!(
                        "`continue {l}`: no enclosing loop labeled `{l}`"
                    ))
                })?,
            None => self.loop_stack.last().cloned().ok_or_else(|| {
                crate::front::error::Unsupported::new("`continue` outside a loop")
            })?,
        };
        self.run_escaping_finalizers(module, ctx.finally_depth)?;
        if self.block_terminated {
            return Ok(());
        }
        self.builder.ins().jump(ctx.continue_target, &[]);
        self.block_terminated = true;
        Ok(())
    }

    /// Run every `finally` block pushed AFTER `keep_depth` (the finalizers a
    /// `break`/`continue` escapes on its way out of a loop), innermost-first,
    /// leaving `finally_stack` unchanged for the enclosing lowering. Each finalizer
    /// runs with a cleared error slot (save/restore), exactly like the
    /// return-edge finalizers in `lower_return`. If one terminates the block (its
    /// own `return`/`throw`), it OVERRIDES the abrupt completion — the caller sees
    /// `block_terminated` and stops.
    fn run_escaping_finalizers(
        &mut self,
        module: &mut dyn Module,
        keep_depth: usize,
    ) -> FrontResult<()> {
        if self.finally_stack.len() <= keep_depth {
            return Ok(());
        }
        let saved = std::mem::take(&mut self.finally_stack);
        for i in (keep_depth..saved.len()).rev() {
            // Only the finalizers OUTER to `i` stay active while `i` lowers, so a
            // `return`/`break` inside it runs them but not itself.
            self.finally_stack = saved[..i].to_vec();
            self.lower_block(module, &saved[i])?;
            if self.block_terminated {
                self.finally_stack = saved;
                return Ok(());
            }
        }
        self.finally_stack = saved;
        Ok(())
    }
}
