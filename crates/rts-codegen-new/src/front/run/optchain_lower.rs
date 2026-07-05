//! Optional-chaining lowering (P5.8) — the reserved `__rts_opt_get` /
//! `__rts_opt_call` ops the desugar ([`super::desugar`]) emits, intercepted at the
//! top of [`super::call`]'s method-call lowering.
//!
//! - `recv.__rts_opt_get(key)` → `__rtsadp_obj_get(box(recv), box(key))`, which
//!   returns `undefined` for a nullish OR non-object receiver — the JS optional
//!   short-circuit, composable across links, with NO static shape required.
//! - `recv.__rts_opt_call(args)` → `nullish(recv) ? undefined : invoke(recv,args)`.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, types};
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

    /// Lower an optional COMPUTED index `recv.__rts_opt_index(key)` (`a?.[k]`) to the
    /// generic `__rtsadp_idx_get(box(recv), box(key))`: an array element (VEC_GET), a
    /// string char, or an object key — and `undefined` for a nullish/foreign receiver
    /// (the optional short-circuit). Unlike `opt_get` (a keyed-object slot read), this
    /// handles `arr?.[1]` correctly.
    pub(super) fn lower_opt_index(
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
            .call_runtime(module, "__rtsadp_idx_get", &[recv_word, key_word])?
            .expect("__rtsadp_idx_get returns a value");
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

        self.builder
            .ins()
            .brif(nullish, then_blk, &[], else_blk, &[]);

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

    /// Lower a guarded METHOD call `a?.b(args)` (P5.8) emitted as
    /// `recv.__rts_opt_method_call(<methodNameLit>, …realArgs)`:
    /// `nullish(recv) ? undefined : recv.b(realArgs)`. The present branch re-lowers a
    /// REAL method call (`lower_method_call`) so it reuses the full dispatch (user
    /// class / String / Number / Map / dynamic-virtual), unlike `opt_get`+`opt_call`
    /// which cannot reach a non-slot class method. `object` (the receiver) is proven
    /// side-effect-free by the desugar, so re-evaluating it here is sound.
    pub(super) fn lower_opt_method_call(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        // args[0] = the method name (a string literal the desugar prepended); the
        // remaining args are the real call arguments.
        let method = match args.first().map(|a| &a.kind) {
            Some(rts_hir::ir::HirExprKind::Lit(rts_hir::ir::HirLit::Str(s))) => s.clone(),
            _ => {
                return crate::front::error::unsupported!(
                    "optional method call without a literal method name"
                );
            }
        };
        let real_args = &args[1..];

        let recv = self.lower_expr(module, object)?;
        let recv_word = self.box_value(recv);

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

        self.builder
            .ins()
            .brif(nullish, then_blk, &[], else_blk, &[]);

        // nullish → undefined (short-circuit, no call).
        self.builder.switch_to_block(then_blk);
        self.builder.seal_block(then_blk);
        let undef = self
            .builder
            .ins()
            .iconst(types::I64, value::PolyValue::undefined().raw() as i64);
        self.builder.ins().jump(join_blk, &[undef.into()]);

        // present → the REAL method dispatch on the (re-evaluated, pure) receiver,
        // BUT only when the method is resolvable: the receiver has a statically-known
        // class, or some user/ambient class declares `method` (so the dynamic path can
        // dispatch it). When NO class declares it (a pure `any`/`null` receiver whose
        // method is unknown, e.g. `deep1?.getValue()` with `deep1: any = null`), the
        // dispatch would bail — but in this GUARDED context an unresolvable method is a
        // runtime `undefined` (the common null receiver never reaches this branch, and
        // a real object missing the method TypeErrors → `undefined` is the honest
        // sentinel). Emit `undefined` instead of bailing the whole program.
        self.builder.switch_to_block(else_blk);
        self.builder.seal_block(else_blk);
        let resolvable = self.static_instance_class(object).is_some()
            || self.classes.iter().any(|d| {
                !d.name.starts_with("__rtsl_")
                    && (d.method_fn(&method).is_some() || d.accessor(&method).is_some())
            });
        let called_word = if resolvable {
            // Dispatch over a HIDDEN TEMP bound to the ALREADY-EVALUATED
            // receiver word — the method lowering re-lowers only the Ident
            // (a pure read), so an IMPURE receiver (`deps.get(k)?.forEach(..)`)
            // is evaluated exactly once.
            let tmp = format!("__rtsn_optrecv_{}", self.builder.func.dfg.num_values());
            self.bind_tagged_local(&tmp, Val::new(recv_word, Repr::Tagged));
            let tmp_expr = rts_hir::HirExpr::new(
                rts_hir::ir::HirExprKind::Ident(tmp),
                rts_hir::HirType::Unknown,
            );
            let called = self.lower_method_call(module, &tmp_expr, &method, real_args)?;
            self.box_value(called)
        } else {
            self.builder
                .ins()
                .iconst(types::I64, value::PolyValue::undefined().raw() as i64)
        };
        self.builder.ins().jump(join_blk, &[called_word.into()]);

        self.builder.switch_to_block(join_blk);
        self.builder.seal_block(join_blk);
        let result = self.builder.block_params(join_blk)[0];
        Ok(Val::new(result, Repr::Tagged))
    }
}
