//! `try`/`catch`/`finally` + `throw` lowering — the manual-unwind exception
//! model (P5.13).
//!
//! Mirrors the old engine's phase-1 mechanism: a thread-local pending-error slot
//! (`crate::value::errslot`) + MANUAL unwinding. There is NO real stack unwind.
//!
//! - `throw e`: lower `e` to a PolyValue word, `__rtsadp_throw_set(word)`, then
//!   route control: to the innermost active `catch` block if inside a `try`, else
//!   RETURN a sentinel from the current function (propagation). The throwing block
//!   is terminated either way — code after the `throw` never runs.
//! - After every user-function call / function-value invoke, the call lowering
//!   emits [`Lowerer::emit_post_call_error_check`]: `if __rtsadp_err_pending()` →
//!   the same route (innermost catch, or sentinel-return to propagate).
//! - `try { B } catch (e) { H } finally { F }`: `B` lowers with a `catch`/`finally`
//!   block pushed on `try_stack`; a pending error after `B` routes to `H` (which
//!   `__rtsadp_err_take()`s the word + binds `e`) ; `F` always runs on both the
//!   normal and the caught path. A `throw` inside `H`/`F` re-sets the slot, and a
//!   `try`/`finally` with no `catch` re-checks the flag in `F` and lets a still-
//!   pending error propagate.

use cranelift_codegen::ir::{types, Block, InstBuilder, Value};
use cranelift_module::Module;

use rts_hir::ir::HirCatch;
use rts_hir::{HirExpr, HirStmt};

use crate::repr::Repr;
use crate::value;

use crate::front::error::FrontResult;

use super::lower::{Lowerer, TryCtx};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Lower `throw e`: set the pending error, then unwind manually. Terminates the
    /// current block (sets `block_terminated`), so no statement after it lowers.
    pub(super) fn lower_throw(
        &mut self,
        module: &mut dyn Module,
        arg: &HirExpr,
    ) -> FrontResult<()> {
        let v = self.lower_expr(module, arg)?;
        let word = self.box_value(v);
        self.call_runtime(module, "__rtsadp_throw_set", &[word])?;
        self.unwind(module)?;
        Ok(())
    }

    /// Route an in-progress unwind: jump to the innermost active `catch`/`finally`
    /// block (if inside a `try`), else RETURN a sentinel from the current function
    /// (propagate to the caller). Terminates the current block.
    fn unwind(&mut self, module: &mut dyn Module) -> FrontResult<()> {
        if let Some(ctx) = self.try_stack.last().copied() {
            self.builder.ins().jump(ctx.on_error, &[]);
        } else {
            self.emit_propagate_return(module)?;
        }
        self.block_terminated = true;
        Ok(())
    }

    /// Emit the sentinel `return` that propagates an unwind out of the current
    /// function. A void function returns nothing; a value-returning one returns a
    /// type-correct dummy (0 / 0.0 / the `undefined` word) — the caller's post-call
    /// error-check sees the pending flag and discards this value, so it is never
    /// observed. Does NOT itself set `block_terminated` (callers do).
    fn emit_propagate_return(&mut self, _module: &mut dyn Module) -> FrontResult<()> {
        match self.ret {
            None => {
                self.builder.ins().return_(&[]);
            }
            Some(Repr::Float64) => {
                let z = self.builder.ins().f64const(0.0);
                self.builder.ins().return_(&[z]);
            }
            Some(_) => {
                // Int*/Bool/Tagged all ride an i64 register; the undefined word (0
                // for ints) is a sound type-correct sentinel that is never read.
                let z = self
                    .builder
                    .ins()
                    .iconst(types::I64, value::PolyValue::undefined().raw() as i64);
                self.builder.ins().return_(&[z]);
            }
        }
        Ok(())
    }

    /// Emit the post-call error check the call lowering runs after EVERY
    /// user-function call / function-value invoke (P5.13): branch on
    /// `__rtsadp_err_pending()` → route the unwind (innermost catch / propagate) on
    /// the error edge, continue on the OK edge. After this returns the builder is on
    /// the OK (no-error) continuation block; `block_terminated` is `false`.
    pub(super) fn emit_post_call_error_check(
        &mut self,
        module: &mut dyn Module,
    ) -> FrontResult<()> {
        let pending = self
            .call_runtime(module, "__rtsadp_err_pending", &[])?
            .expect("__rtsadp_err_pending returns a value");
        let err_block = self.builder.create_block();
        let ok_block = self.builder.create_block();
        self.builder
            .ins()
            .brif(pending, err_block, &[], ok_block, &[]);

        // Error edge: route the unwind.
        self.builder.switch_to_block(err_block);
        self.builder.seal_block(err_block);
        self.block_terminated = false;
        self.unwind(module)?;

        // OK edge: the caller continues here with no pending error.
        self.builder.switch_to_block(ok_block);
        self.builder.seal_block(ok_block);
        self.block_terminated = false;
        Ok(())
    }

    /// Lower `try { body } [catch (e) { handler }] [finally { fin }]`.
    pub(super) fn lower_try(
        &mut self,
        module: &mut dyn Module,
        body: &[HirStmt],
        catch: Option<&HirCatch>,
        finally: Option<&[HirStmt]>,
    ) -> FrontResult<()> {
        // Labeled/empty edge: a try must have a catch or a finally (swc guarantees
        // it; defend anyway). A try with neither is just its body.
        if catch.is_none() && finally.is_none() {
            return self.lower_block(module, body);
        }

        let after_block = self.builder.create_block();
        let finally_block = finally.map(|_| self.builder.create_block());
        let catch_block = catch.map(|_| self.builder.create_block());
        // The block a pending error routes to: the catch if present, else finally.
        let on_error = catch_block.or(finally_block).expect("catch or finally exists");
        let ok_after_body = finally_block.unwrap_or(after_block);

        // Belt-and-braces: clear any stale pending error so the body's fall-through
        // check (and an inner try) cannot observe an outer error.
        self.call_runtime(module, "__rtsadp_err_clear", &[])?;

        // ---- body ----
        self.try_stack.push(TryCtx { on_error });
        self.block_terminated = false;
        self.lower_block(module, body)?;
        let body_falls_through = !self.block_terminated;
        self.try_stack.pop();
        if body_falls_through {
            // Body finished with no lexical throw. A propagated error from a call
            // would already have branched to `on_error`; on the normal edge the slot
            // is clear, so jump straight to finally/after.
            self.builder.ins().jump(ok_after_body, &[]);
        }

        // ---- catch ----
        if let Some(cb) = catch_block {
            let handler = catch.expect("catch_block ⇒ catch exists");
            self.builder.switch_to_block(cb);
            self.builder.seal_block(cb);
            self.block_terminated = false;
            // A `throw` INSIDE the catch handler must still run THIS try's `finally`
            // before propagating. When a finally exists, push it as the catch's
            // error route; the finally re-checks the pending flag and propagates a
            // still-set error after running. With no finally, the catch's throw
            // routes to the enclosing try (`try_stack.last()`) — the natural default.
            if let Some(fb) = finally_block {
                self.try_stack.push(TryCtx { on_error: fb });
            }
            self.lower_catch(module, handler, finally_block.unwrap_or(after_block))?;
            if finally_block.is_some() {
                self.try_stack.pop();
            }
        }

        // ---- finally ----
        if let Some(fb) = finally_block {
            let fin = finally.expect("finally_block ⇒ finally exists");
            self.builder.switch_to_block(fb);
            self.builder.seal_block(fb);
            self.block_terminated = false;
            // `finally` runs INSIDE any enclosing try: a throw in `fin` itself must
            // route to the OUTER catch (`try_stack.last()` — this try's entry was
            // already popped). So lower the finalizer plainly, then re-check the
            // pending flag: a still-pending error (the body or catch threw and was
            // not handled) keeps propagating AFTER the finalizer ran.
            self.lower_block(module, fin)?;
            if !self.block_terminated {
                self.emit_finally_propagate(module, after_block)?;
            }
        }

        // ---- after ----
        self.builder.seal_block(after_block);
        self.builder.switch_to_block(after_block);
        self.block_terminated = false;
        Ok(())
    }

    /// Lower the `catch (e) { handler }` block: take the thrown word (clearing the
    /// slot), bind it to the catch param `e` (when present) as a Tagged local
    /// recorded as an `Error` instance (so `e.message`/`e.name` dispatch), then
    /// lower the handler. Falls through to `next` (finally / after).
    fn lower_catch(
        &mut self,
        module: &mut dyn Module,
        handler: &HirCatch,
        next: Block,
    ) -> FrontResult<()> {
        let word = self
            .call_runtime(module, "__rtsadp_err_take", &[])?
            .expect("__rtsadp_err_take returns a value");
        if let Some(name) = &handler.binding {
            // Bind `e` to a fresh Tagged local holding the thrown word.
            self.bind_catch_local(name, word);
            // Record `e` as an `Error` instance so `e.message`/`e.name`/`e.stack`
            // resolve through the global-class error props. A thrown non-Error
            // (string/number) ignores this — those props are only read in code that
            // knows `e` is an Error, and the runtime props no-op a non-Error handle.
            self.global_instance_classes
                .insert(name.clone(), "Error".to_string());
        }
        self.lower_block(module, &handler.body)?;
        if !self.block_terminated {
            self.builder.ins().jump(next, &[]);
        }
        // Drop the catch binding's recorded class so it does not leak past the block.
        if let Some(name) = &handler.binding {
            self.global_instance_classes.remove(name);
        }
        Ok(())
    }

    /// In a `try`/`finally` with no `catch`: after the finalizer ran, re-check the
    /// pending flag and route a still-pending error (the body threw and nothing
    /// caught it) — to the enclosing catch, or a sentinel-return — else fall to
    /// `after`.
    fn emit_finally_propagate(
        &mut self,
        module: &mut dyn Module,
        after: Block,
    ) -> FrontResult<()> {
        let pending = self
            .call_runtime(module, "__rtsadp_err_pending", &[])?
            .expect("__rtsadp_err_pending returns a value");
        let err_block = self.builder.create_block();
        self.builder
            .ins()
            .brif(pending, err_block, &[], after, &[]);
        self.builder.switch_to_block(err_block);
        self.builder.seal_block(err_block);
        self.block_terminated = false;
        self.unwind(module)?;
        Ok(())
    }

    /// Bind a catch param to a fresh Tagged local holding the thrown PolyValue
    /// `word` (an `i64` register). Re-declares the name (a catch param shadows).
    fn bind_catch_local(&mut self, name: &str, word: Value) {
        let var = self.builder.declare_var(types::I64);
        self.builder.def_var(var, word);
        self.locals
            .insert(name.to_string(), super::lower::Local { var, repr: Repr::Tagged });
        // The thrown value is opaque w.r.t. object/array shape — drop any stale
        // shape/class so an access on the bound name does not use a wrong layout.
        self.local_shapes.remove(name);
        self.local_classes.remove(name);
        self.object_locals.remove(name);
    }
}
