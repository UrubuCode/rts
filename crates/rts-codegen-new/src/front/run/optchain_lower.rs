//! Optional-chaining lowering (P5.8) — the reserved `__rts_opt_get` /
//! `__rts_opt_call` ops the desugar ([`super::desugar`]) emits, intercepted at the
//! top of [`super::call`]'s method-call lowering.
//!
//! - `recv.__rts_opt_get(key)` → `__rtsadp_obj_get(box(recv), box(key))`, which
//!   returns `undefined` for a nullish OR non-object receiver — the JS optional
//!   short-circuit, composable across links, with NO static shape required.
//! - `recv.__rts_opt_call(args)` → `nullish(recv) ? undefined : invoke(recv,args)`.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{types, InstBuilder};
use cranelift_module::Module;

use rts_hir::HirExpr;

use crate::repr::Repr;
use crate::value;

use crate::front::error::FrontResult;

use super::lower::{Lowerer, Val};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Lower an optional property/index read `recv.__rts_opt_get(key)` (P5.8) to a
    /// nullish-tolerant `__rtsadp_obj_get(box(recv), box(key))`: it returns
    /// `undefined` when `recv` is `null`/`undefined` or any non-object value — which
    /// is exactly the optional-chain short-circuit, and composes (a nullish at one
    /// link makes every later read see `undefined`). No static shape required.
    pub(super) fn lower_opt_get(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        key: &HirExpr,
    ) -> FrontResult<Val> {
        let recv = self.lower_expr(module, object)?;
        let recv_word = self.box_value(recv);
        let key_val = self.lower_expr(module, key)?;
        let key_word = self.box_value(key_val);
        let word = self
            .call_runtime(module, "__rtsadp_obj_get", &[recv_word, key_word])?
            .expect("__rtsadp_obj_get returns a value");
        Ok(Val::new(word, Repr::Tagged))
    }

    /// Lower an optional call `recv.__rts_opt_call(args)` (P5.8, the `?.()` link):
    /// `nullish(recv) ? undefined : invoke(recv, args)`. The receiver is the
    /// already-built preceding read (a `__rts_opt_get` result or a value); it is
    /// boxed once and branched on, then the function VALUE is invoked through the
    /// uniform-ABI indirect path. A receiver that is not a function VALUE at runtime
    /// would fault the invoke — but the guard only reaches the invoke for a
    /// non-nullish receiver, and the optional-call idiom (`o?.f?.()`) always reads a
    /// function or `undefined`, so the non-nullish branch is a function.
    pub(super) fn lower_opt_call(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        let recv = self.lower_expr(module, object)?;
        let recv_word = self.box_value(recv);

        // Nullish test on the boxed receiver word.
        let null_w = self
            .builder
            .ins()
            .iconst(types::I64, value::PolyValue::null().raw() as i64);
        let undef_w = self
            .builder
            .ins()
            .iconst(types::I64, value::PolyValue::undefined().raw() as i64);
        let is_null = self.builder.ins().icmp(IntCC::Equal, recv_word, null_w);
        let is_undef = self.builder.ins().icmp(IntCC::Equal, recv_word, undef_w);
        let nullish = self.builder.ins().bor(is_null, is_undef);

        let then_blk = self.builder.create_block();
        let else_blk = self.builder.create_block();
        let join_blk = self.builder.create_block();
        self.builder.append_block_param(join_blk, types::I64);

        self.builder.ins().brif(nullish, then_blk, &[], else_blk, &[]);

        // nullish → undefined.
        self.builder.switch_to_block(then_blk);
        self.builder.seal_block(then_blk);
        let undef = self
            .builder
            .ins()
            .iconst(types::I64, value::PolyValue::undefined().raw() as i64);
        self.builder.ins().jump(join_blk, &[undef.into()]);

        // present → invoke the function value.
        self.builder.switch_to_block(else_blk);
        self.builder.seal_block(else_blk);
        let called = self.lower_value_call_word(module, recv_word, args)?;
        let called_word = self.box_value(called);
        self.builder.ins().jump(join_blk, &[called_word.into()]);

        self.builder.switch_to_block(join_blk);
        self.builder.seal_block(join_blk);
        let result = self.builder.block_params(join_blk)[0];
        Ok(Val::new(result, Repr::Tagged))
    }
}
