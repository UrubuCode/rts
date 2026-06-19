//! Loop lowering (P5.10): C-style `for`, `for-of`, `for-in`, plus `break`/
//! `continue`.
//!
//! Every loop reuses the SAME real Cranelift block machinery the `while` lowering
//! ([`super::stmt`]) already uses — a header (test), a body, a per-iteration
//! CONTINUE target, and an exit — with the break/continue jump targets recorded on
//! [`Lowerer::loop_stack`](super::lower::Lowerer). The three forms differ only in
//! how they drive the header:
//!
//! - **C-`for`** desugars to `{ init; while (test) { body; update; } }`, but with
//!   the update split into its OWN block so a `continue` runs the update before
//!   re-testing (JS semantics) instead of skipping it.
//! - **`for-of`** / **`for-in`** desugar to an index walk `for (i in 0..len) { x =
//!   arr[i]; body }` over a real `Entry::Vec` of boxed PolyValue words: an array
//!   iterable IS such a Vec; a string (for-of) / object (for-in) is first
//!   materialized into one by the iteration-source trampolines
//!   ([`crate::value::iterops`]). The per-iteration binding `x`/`k` is a fresh
//!   `Tagged` local re-`def`ed each pass.
//!
//! Anything outside the modeled subset BAILS explicitly (a labeled break/continue,
//! a non-array/non-string for-of source, a for-in over a non-object): the soundness
//! floor — never a silently-wrong iteration.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;

use rts_hir::ir::HirExprKind;
use rts_hir::{HirExpr, HirStmt, HirType};

use crate::repr::Repr;

use crate::front::error::{FrontResult, unsupported};

use super::lower::{JsKind, Local, LoopCtx, Lowerer, cl_type};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// C-style `for (init; test; update) body`.
    ///
    /// Desugars to `init` followed by a loop with a separate UPDATE block, so a
    /// `continue` (which jumps to the update block) still runs the update before
    /// re-testing — JS semantics. An omitted `test` is an infinite `true` (the
    /// only way out is a `break`/`return`); an omitted `init`/`update` is a no-op.
    pub(super) fn lower_for(
        &mut self,
        module: &mut dyn Module,
        init: Option<&HirStmt>,
        cond: Option<&HirExpr>,
        update: Option<&HirExpr>,
        body: &[HirStmt],
    ) -> FrontResult<()> {
        // `init` runs once before the loop, in the enclosing scope (the `let i` it
        // declares stays visible to the test/update/body — the engine has a flat
        // per-function local map, so this is automatic).
        if let Some(init) = init {
            self.lower_stmt(module, init)?;
        }

        let header = self.builder.create_block();
        let body_block = self.builder.create_block();
        let update_block = self.builder.create_block();
        let exit_block = self.builder.create_block();

        self.builder.ins().jump(header, &[]);

        // ---- header: evaluate the test (absent ⇒ always-true) ----
        self.builder.switch_to_block(header);
        let cond_v = match cond {
            Some(c) => {
                let v = self.lower_expr(module, c)?;
                self.as_bool_value(module, v)?
            }
            None => self.builder.ins().iconst(types::I64, 1),
        };
        self.builder
            .ins()
            .brif(cond_v, body_block, &[], exit_block, &[]);

        // ---- body: `continue` → update_block, `break` → exit_block ----
        self.builder.switch_to_block(body_block);
        self.builder.seal_block(body_block);
        self.block_terminated = false;
        self.loop_stack.push(LoopCtx {
            exit: exit_block,
            continue_target: update_block,
        });
        self.lower_block(module, body)?;
        self.loop_stack.pop();
        if !self.block_terminated {
            self.builder.ins().jump(update_block, &[]);
        }

        // ---- update: run the step, then re-test ----
        self.builder.seal_block(update_block);
        self.builder.switch_to_block(update_block);
        self.block_terminated = false;
        if let Some(u) = update {
            self.lower_expr(module, u)?;
        }
        self.builder.ins().jump(header, &[]);

        // All predecessors of `header` (the pre-loop jump + the update jump) are now
        // emitted; seal it. Likewise the exit (only `break`s + the header's false
        // edge target it).
        self.builder.seal_block(header);
        self.builder.seal_block(exit_block);
        self.builder.switch_to_block(exit_block);
        self.block_terminated = false;
        Ok(())
    }

    /// `for (const x of iterable) body` over an ARRAY or a STRING.
    ///
    /// Both reduce to an index walk over a real `Entry::Vec` of boxed PolyValue
    /// words: an array feeds its own Vec; a string is materialized to a char array
    /// (`__rtsadp_str_chars`). Each iteration binds `x` to `VEC_GET(arr, i)` (a
    /// fresh `Tagged` local). `break`/`continue` use the shared loop machinery.
    /// A non-array / non-string source BAILS (JS would throw; the engine has no
    /// throw, so refusing is the only sound option).
    pub(super) fn lower_for_of(
        &mut self,
        module: &mut dyn Module,
        binding: &str,
        _binding_ty: &HirType,
        iterable: &HirExpr,
        body: &[HirStmt],
    ) -> FrontResult<()> {
        // LAZY generator source (`for (const x of g())` where `g` is a generator
        // with loops/yield*): `g()` returns a GenState handle. DRAIN it to a real
        // array (runs the state machine to completion, collecting yields) and walk
        // that. Detected by the callee's `ret_lazy_gen` flag.
        if let Some(arr_word) = self.try_lazy_gen_source_word(module, iterable)? {
            return self.lower_index_walk(module, binding, arr_word, body);
        }
        let arr_word = self.for_of_source_word(module, iterable)?;
        self.lower_index_walk(module, binding, arr_word, body)
    }

    /// If `iterable` is a CALL to a lazy generator constructor (`ret_lazy_gen`),
    /// lower it to the GenState handle and `GEN_SM_DRAIN` it into an element array
    /// word to walk; `Ok(None)` otherwise.
    fn try_lazy_gen_source_word(
        &mut self,
        module: &mut dyn Module,
        iterable: &HirExpr,
    ) -> FrontResult<Option<Value>> {
        let is_lazy = match &iterable.kind {
            HirExprKind::Call { callee, .. } => match &callee.kind {
                HirExprKind::Ident(f) => self.sigs.get(f).is_some_and(|s| s.ret_lazy_gen),
                _ => false,
            },
            _ => false,
        };
        if !is_lazy {
            return Ok(None);
        }
        let h = self.lower_expr(module, iterable)?;
        // The GenState handle rides a raw `Int64`; pass it verbatim to DRAIN.
        let handle = self.coerce(h, Repr::Int64)?;
        let arr = self
            .call_runtime(module, "__RTS_FN_NS_GC_GEN_SM_DRAIN", &[handle])?
            .expect("GEN_SM_DRAIN returns an array word");
        Ok(Some(arr))
    }

    /// `for (const k in object) body` over a keyed OBJECT.
    ///
    /// The object's OWN enumerable keys are materialized to an array of key strings
    /// (`__rtsadp_obj_keys`, recovered from the slot-0 shape id), then walked: each
    /// iteration binds `k` to the next key string. A non-object source BAILS (array
    /// index-key enumeration / primitive for-in is a later increment).
    pub(super) fn lower_for_in(
        &mut self,
        module: &mut dyn Module,
        binding: &str,
        object: &HirExpr,
        body: &[HirStmt],
    ) -> FrontResult<()> {
        // Only a proven keyed OBJECT (a known-shape object literal local or a
        // reassigned object local) can enumerate keys; anything else bails.
        let is_object = match &object.kind {
            HirExprKind::Ident(name) => {
                self.object_locals.contains(name)
                    || matches!(
                        self.local_shapes.get(name),
                        Some(super::lower::HeapShape::Object(_))
                    )
            }
            HirExprKind::Object(_) => true,
            _ => false,
        };
        if !is_object {
            return unsupported!(
                "for-in over a non-object receiver (only a proven keyed object is supported)"
            );
        }
        let obj = self.lower_expr(module, object)?;
        let obj_word = self.box_value(obj);
        let keys_word = self
            .call_runtime(module, "__rtsadp_obj_keys", &[obj_word])?
            .expect("__rtsadp_obj_keys returns a value");
        self.lower_index_walk(module, binding, keys_word, body)
    }

    /// Resolve the for-of `iterable` to an array PolyValue WORD to walk: a proven
    /// ARRAY feeds its own boxed Vec; a proven STRING is materialized to a char
    /// array. Anything else bails.
    fn for_of_source_word(
        &mut self,
        module: &mut dyn Module,
        iterable: &HirExpr,
    ) -> FrontResult<Value> {
        if self.is_array_valued(iterable) {
            let arr = self.lower_expr(module, iterable)?;
            return Ok(self.box_value(arr));
        }
        // A STRING source (a literal, a string-kinded local/expression): iterate
        // code points. We lower it and require a proven `Str` kind — a Tagged value
        // of Unknown kind could be a number/object and would silently mis-iterate,
        // so refuse it.
        let v = self.lower_expr(module, iterable)?;
        if matches!(v.kind, JsKind::Str) {
            let word = self.box_value(v);
            return self
                .call_runtime(module, "__rtsadp_str_chars", &[word])?
                .ok_or_else(|| {
                    crate::front::error::Unsupported::new("__rtsadp_str_chars returns a value")
                });
        }
        // GENERIC fallback for an UNPROVEN source: a `string` PARAM (`for (const ch
        // of s)`), a nested-array for-of binding (`for (const x of row)`), an `any`,
        // a call return. Coerce at RUNTIME via `__rtsadp_to_iter_array` (array→self /
        // string→chars / else→empty). GATED OUT for a known RUNTIME-class instance
        // (a `new Set()`/`new Map()`/`new Date()` local): those iterate via a
        // protocol we don't model yet, so they keep bailing HONESTLY rather than
        // silently iterating nothing.
        // Only a TAGGED value (a `string`/`any` param, a for-of binding, a call
        // return — kind genuinely unknown) takes the generic runtime path. A PROVEN
        // numeric/bool source (`for (const x of 42)`) is NOT iterable → keep bailing
        // (a JS TypeError; honest, never a silent empty walk). A known runtime-class
        // instance (Set/Map/Date) also keeps bailing (iteration protocol unmodeled).
        if matches!(v.repr, Repr::Tagged)
            && self.static_instance_class(iterable).is_none()
            && self.global_instance_class(iterable).is_none()
        {
            let word = self.box_value(v);
            return self
                .call_runtime(module, "__rtsadp_to_iter_array", &[word])?
                .ok_or_else(|| {
                    crate::front::error::Unsupported::new("__rtsadp_to_iter_array returns a value")
                });
        }
        unsupported!(
            "for-of over a non-iterable (only a proven array or string is supported in this increment)"
        )
    }

    /// The shared index walk both for-of and for-in compile to: iterate
    /// `i in 0..VEC_LEN(arr_word)`, binding `binding` to `VEC_GET(arr_word, i)` (a
    /// fresh `Tagged` local) each pass, then lower `body`. `break` exits; `continue`
    /// jumps to the index-advance step (so the loop always makes progress).
    ///
    /// `arr_word` is a freshly-built array (the iterable itself, a char array, or a
    /// key array) — its words are valid for the whole loop. The binding rides a
    /// `Tagged` local; its element/key word is a real PolyValue, so reads of it in
    /// the body go through the generic (tag-dispatched) operators correctly without
    /// the lowering having to prove a static kind.
    fn lower_index_walk(
        &mut self,
        module: &mut dyn Module,
        binding: &str,
        arr_word: Value,
        body: &[HirStmt],
    ) -> FrontResult<()> {
        // Hold the source array + the live length in fresh Tagged/Int64 locals so
        // they survive across the loop's blocks (SSA φ is the builder's job).
        let arr_var = self.builder.declare_var(cl_type(Repr::Tagged));
        self.builder.def_var(arr_var, arr_word);
        let len = crate::value::emit_marshal::emit_vec_len(module, self.builder, arr_word);
        let len_var = self.builder.declare_var(types::I64);
        self.builder.def_var(len_var, len);
        // The index counter, initialized to 0.
        let idx_var = self.builder.declare_var(types::I64);
        let zero = self.builder.ins().iconst(types::I64, 0);
        self.builder.def_var(idx_var, zero);
        // The per-iteration binding local (fresh Tagged var). Record its shape/class
        // bookkeeping as cleared — it holds an opaque element word.
        let bind_var = self.builder.declare_var(cl_type(Repr::Tagged));
        self.locals.insert(
            binding.to_string(),
            Local {
                var: bind_var,
                repr: Repr::Tagged,
            },
        );
        self.local_shapes.remove(binding);
        self.local_classes.remove(binding);
        self.global_instance_classes.remove(binding);
        self.object_locals.remove(binding);

        let header = self.builder.create_block();
        let body_block = self.builder.create_block();
        let advance_block = self.builder.create_block();
        let exit_block = self.builder.create_block();

        self.builder.ins().jump(header, &[]);

        // ---- header: `i < len` ? body : exit ----
        self.builder.switch_to_block(header);
        let i = self.builder.use_var(idx_var);
        let n = self.builder.use_var(len_var);
        let lt = self.builder.ins().icmp(IntCC::SignedLessThan, i, n);
        let cond_v = self.builder.ins().uextend(types::I64, lt);
        self.builder
            .ins()
            .brif(cond_v, body_block, &[], exit_block, &[]);

        // ---- body: bind `x = arr[i]`, then run the body ----
        self.builder.switch_to_block(body_block);
        self.builder.seal_block(body_block);
        self.block_terminated = false;
        let arr = self.builder.use_var(arr_var);
        let i = self.builder.use_var(idx_var);
        let elem = crate::value::emit_marshal::emit_vec_get(module, self.builder, arr, i);
        self.builder.def_var(bind_var, elem);
        self.loop_stack.push(LoopCtx {
            exit: exit_block,
            continue_target: advance_block,
        });
        self.lower_block(module, body)?;
        self.loop_stack.pop();
        if !self.block_terminated {
            self.builder.ins().jump(advance_block, &[]);
        }

        // ---- advance: `i += 1`, re-test ----
        self.builder.seal_block(advance_block);
        self.builder.switch_to_block(advance_block);
        self.block_terminated = false;
        let i = self.builder.use_var(idx_var);
        let one = self.builder.ins().iconst(types::I64, 1);
        let next = self.builder.ins().iadd(i, one);
        self.builder.def_var(idx_var, next);
        self.builder.ins().jump(header, &[]);

        self.builder.seal_block(header);
        self.builder.seal_block(exit_block);
        self.builder.switch_to_block(exit_block);
        self.block_terminated = false;
        Ok(())
    }

    /// `break` — jump to the innermost loop's exit. A labeled break BAILS (labeled
    /// control flow is out of this increment); a `break` outside any loop BAILS.
    pub(super) fn lower_break(&mut self, label: Option<&str>) -> FrontResult<()> {
        if label.is_some() {
            return unsupported!("labeled `break` (labeled control flow is a later increment)");
        }
        let ctx = self
            .loop_stack
            .last()
            .copied()
            .ok_or_else(|| crate::front::error::Unsupported::new("`break` outside a loop"))?;
        self.builder.ins().jump(ctx.exit, &[]);
        self.block_terminated = true;
        Ok(())
    }

    /// `continue` — jump to the innermost loop's continue target (the header for a
    /// `while`/for-of/for-in advance, the update step for a C-`for`). A labeled
    /// continue BAILS; a `continue` outside any loop BAILS.
    pub(super) fn lower_continue(&mut self, label: Option<&str>) -> FrontResult<()> {
        if label.is_some() {
            return unsupported!("labeled `continue` (labeled control flow is a later increment)");
        }
        let ctx =
            self.loop_stack.last().copied().ok_or_else(|| {
                crate::front::error::Unsupported::new("`continue` outside a loop")
            })?;
        self.builder.ins().jump(ctx.continue_target, &[]);
        self.block_terminated = true;
        Ok(())
    }
}
