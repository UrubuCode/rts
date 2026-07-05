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
        let label = self.pending_label.take();
        self.loop_stack.push(LoopCtx {
            exit: exit_block,
            continue_target: update_block,
            label,
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
        // PER-ITERATION `let` binding (spec CreatePerIterationEnvironment): a
        // loop-header `let i` that is a CELL local (captured+mutated by a
        // closure in the body) gets a FRESH cell here — copied from the current
        // one — so each iteration's closures capture THEIR iteration's binding
        // (`fns.push(() => i)` reads 0,1,2, not 3,3,3). The update below then
        // mutates the NEW cell (the next iteration's binding), exactly the
        // spec's copy-then-update order.
        if let Some(HirStmt::Let { name, .. }) = init {
            if self.is_cell_local(name) {
                if let Some(local) = self.local(name) {
                    let var = local.var;
                    let cur_handle = self.builder.use_var(var);
                    let val = self.emit_cell_get(module, cur_handle);
                    let fresh = self.emit_cell_alloc(module, val.v);
                    self.builder.def_var(var, fresh);
                }
            }
        }
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
        // ITERABLE CLASS via `[Symbol.iterator]()` (parser-canonical method name
        // `"Symbol.iterator"`): a class instance whose class declares that method
        // (real JS — the `.ts` writes `*[Symbol.iterator]()`, NO engine hook). Call
        // it and walk the elements: a generator method DRAINs, an array-returning one
        // walks directly. The engine names ONLY the `Symbol.iterator` protocol key,
        // never the class.
        if let Some(arr_word) = self.try_class_iterator_source_word(module, iterable)? {
            return self.lower_index_walk(module, binding, arr_word, body, None, false);
        }
        // LAZY generator source (`for (const x of g())` where `g` is a generator
        // with loops/yield*): `g()` returns a GenState handle. DRAIN it to a real
        // array (runs the state machine to completion, collecting yields) and walk
        // that. Detected by the callee's `ret_lazy_gen` flag.
        if let Some(arr_word) = self.try_lazy_gen_source_word(module, iterable)? {
            return self.lower_index_walk(module, binding, arr_word, body, None, false);
        }
        // PROVEN array: the fast index walk, unchanged.
        if self.is_array_valued(iterable) {
            let arr = self.lower_expr(module, iterable)?;
            let arr_word = self.box_value(arr);
            return self.lower_index_walk(module, binding, arr_word, body, None, false);
        }
        let v = self.lower_expr(module, iterable)?;
        // PROVEN string: code-point char array (elements proven strings).
        if matches!(v.kind, JsKind::Str) {
            let word = self.box_value(v);
            let chars = self
                .call_runtime(module, "__rtsadp_str_chars", &[word])?
                .ok_or_else(|| {
                    crate::front::error::Unsupported::new("__rtsadp_str_chars returns a value")
                })?;
            return self.lower_index_walk(module, binding, chars, body, None, true);
        }
        // GENERIC (Tagged) source: the LAZY iterator protocol — `iter_open` picks
        // an array cursor (array/string/Set/Map by shape) or a LIVE custom
        // `[Symbol.iterator]` iterator; `break` runs IteratorClose (`return()`);
        // a non-iterable source THROWS TypeError.
        if matches!(v.repr, Repr::Tagged) {
            let word = self.box_value(v);
            return self.lower_iter_protocol_walk(module, binding, word, body);
        }
        unsupported!(
            "for-of over a non-iterable (a proven number/bool can never be iterable)"
        )
    }

    /// The LAZY protocol walk for an unproven for-of source: open a cursor,
    /// bind `iter_next` results until the EMPTY sentinel, and run
    /// IteratorClose on `break`.
    fn lower_iter_protocol_walk(
        &mut self,
        module: &mut dyn Module,
        binding: &str,
        src_word: Value,
        body: &[HirStmt],
    ) -> FrontResult<()> {
        let cursor = self
            .call_runtime(module, "__rtsadp_iter_open", &[src_word])?
            .expect("__rtsadp_iter_open returns a cursor");
        // A non-iterable source threw — route the unwind before looping.
        self.emit_post_call_error_check(module)?;
        let cursor_var = self.builder.declare_var(cl_type(Repr::Tagged));
        self.builder.def_var(cursor_var, cursor);

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
        let break_block = self.builder.create_block();
        let exit_block = self.builder.create_block();

        self.builder.ins().jump(header, &[]);

        // ---- header: v = iter_next(cursor); EMPTY → exit ----
        self.builder.switch_to_block(header);
        self.block_terminated = false;
        let cur = self.builder.use_var(cursor_var);
        let v = self
            .call_runtime(module, "__rtsadp_iter_next", &[cur])?
            .expect("__rtsadp_iter_next returns a value");
        // A user `next()` can THROW — route the unwind (no close; spec skips
        // IteratorClose when next itself throws).
        self.emit_post_call_error_check(module)?;
        let empty = self
            .builder
            .ins()
            .iconst(types::I64, crate::value::PolyValue::empty().raw() as i64);
        let is_done = self.builder.ins().icmp(IntCC::Equal, v, empty);
        self.builder
            .ins()
            .brif(is_done, exit_block, &[], body_block, &[]);

        // ---- body ----
        self.builder.switch_to_block(body_block);
        self.builder.seal_block(body_block);
        self.block_terminated = false;
        self.builder.def_var(bind_var, v);
        let label = self.pending_label.take();
        self.loop_stack.push(LoopCtx {
            exit: break_block,
            continue_target: header,
            label,
        });
        self.lower_block(module, body)?;
        self.loop_stack.pop();
        if !self.block_terminated {
            self.builder.ins().jump(header, &[]);
        }
        self.builder.seal_block(header);

        // ---- break: IteratorClose (`return()`), then exit ----
        self.builder.switch_to_block(break_block);
        self.builder.seal_block(break_block);
        self.block_terminated = false;
        let cur = self.builder.use_var(cursor_var);
        self.call_runtime(module, "__rtsadp_iter_close", &[cur])?;
        self.builder.ins().jump(exit_block, &[]);

        self.builder.switch_to_block(exit_block);
        self.builder.seal_block(exit_block);
        self.block_terminated = false;
        Ok(())
    }

    /// If `iterable` is an instance of a class declaring `[Symbol.iterator]()`
    /// (method name `"Symbol.iterator"`), synthesize the call and produce the
    /// element-array word to walk: a GENERATOR iterator method (`ret_lazy_gen`) is
    /// DRAINed; an array-returning one is used directly. `Ok(None)` when not such an
    /// iterable class. The engine names only the protocol key.
    pub(super) fn try_class_iterator_source_word(
        &mut self,
        module: &mut dyn Module,
        iterable: &HirExpr,
    ) -> FrontResult<Option<Value>> {
        let Some(class) = self
            .static_instance_class(iterable)
            .or_else(|| self.global_instance_class(iterable))
        else {
            return Ok(None);
        };
        // Both canonical spellings: the PARSER names a `[Symbol.iterator]` class
        // member `@@iterator` (well-known canonicalization); older snippet-derived
        // tables carry the textual `Symbol.iterator`.
        let Some((mname, synth)) = self.classes.get(&class).and_then(|d| {
            ["Symbol.iterator", "@@iterator"]
                .iter()
                .find_map(|k| d.methods.get(*k).map(|f| (k.to_string(), f.clone())))
        }) else {
            return Ok(None);
        };
        let sig = self.sigs.get(&synth);
        let is_lazy = sig.is_some_and(|s| s.ret_lazy_gen);
        let is_array = sig.is_some_and(|s| s.ret_array);
        if !is_lazy && !is_array {
            return Ok(None);
        }
        let iter_call = HirExpr::new(
            HirExprKind::MethodCall {
                object: Box::new(iterable.clone()),
                method: mname,
                args: Vec::new(),
            },
            HirType::Any,
        );
        let res = self.lower_expr(module, &iter_call)?;
        if is_lazy {
            // The method returned a GenState handle (raw i64) → DRAIN to an array.
            // DRAIN returns the RAW Vec handle; rebox it as a TAG_OBJECT array
            // word (the spread-append consumer reads a word — a raw id there
            // appended nothing).
            let handle = self.coerce(res, Repr::Int64)?;
            let arr = self
                .call_runtime(module, "__RTS_FN_NS_GC_GEN_SM_DRAIN", &[handle])?
                .expect("GEN_SM_DRAIN returns a Vec handle");
            let word = self
                .call_runtime(module, "__rtsadp_box_handle_auto", &[arr])?
                .expect("__rtsadp_box_handle_auto returns a word");
            Ok(Some(word))
        } else {
            // An array-returning iterator method → its result IS the element array.
            Ok(Some(self.box_value(res)))
        }
    }

    /// If `iterable` is a CALL to a lazy generator constructor (`ret_lazy_gen`),
    /// lower it to the GenState handle and `GEN_SM_DRAIN` it into an element array
    /// word to walk; `Ok(None)` otherwise.
    pub(super) fn try_lazy_gen_source_word(
        &mut self,
        module: &mut dyn Module,
        iterable: &HirExpr,
    ) -> FrontResult<Option<Value>> {
        let is_lazy = match &iterable.kind {
            HirExprKind::Call { callee, .. } => match &callee.kind {
                HirExprKind::Ident(f) => self.sigs.get(f).is_some_and(|s| s.ret_lazy_gen),
                _ => false,
            },
            // A LAZY generator LOCAL (`const gen = counter(..); for (const v
            // of gen)`): the local rides a raw Int64 GenState handle — DRAIN
            // it exactly like a direct-call source.
            HirExprKind::Ident(n) => self.generator_locals.get(n).copied() == Some(true),
            _ => false,
        };
        if !is_lazy {
            return Ok(None);
        }
        let h = self.lower_expr(module, iterable)?;
        // The GenState handle rides a raw `Int64`; pass it verbatim to DRAIN.
        // DRAIN returns the RAW Vec handle → rebox as a TAG_OBJECT array word
        // (spread-append / element reads consume a word).
        let handle = self.coerce(h, Repr::Int64)?;
        let arr = self
            .call_runtime(module, "__RTS_FN_NS_GC_GEN_SM_DRAIN", &[handle])?
            .expect("GEN_SM_DRAIN returns a Vec handle");
        let word = self
            .call_runtime(module, "__rtsadp_box_handle_auto", &[arr])?
            .expect("__rtsadp_box_handle_auto returns a word");
        Ok(Some(word))
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
        // POLYMORPHIC source: `__rtsadp_obj_keys` tag-dispatches the receiver
        // (keyed object → own enumerable keys; array → "0".."n-1"; string →
        // code-unit indices; anything else → `[]`, so the loop runs zero times —
        // exactly JS for-in semantics). No static receiver gate needed.
        let obj = self.lower_expr(module, object)?;
        let obj_word = self.box_value(obj);
        let keys_word = self
            .call_runtime(module, "__rtsadp_for_in_keys", &[obj_word])?
            .expect("__rtsadp_for_in_keys returns a value");
        // for-in keys are always PROPERTY-NAME strings → string-kinded binding.
        self.lower_index_walk(module, binding, keys_word, body, Some(obj_word), true)
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
        // for-in: the SOURCE object word — each key is re-checked with
        // `[[HasProperty]]` before the body runs (a key deleted mid-iteration is
        // SKIPPED, JS EnumerateObjectProperties). for-of passes `None`.
        present_guard: Option<Value>,
        // Every element is a PROVEN string (a string source's char array, a
        // for-in key array) — the binding is marked string-kinded so string
        // methods on it resolve the String rows.
        binding_is_string: bool,
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
        if binding_is_string {
            self.string_locals.insert(binding.to_string());
        } else {
            self.string_locals.remove(binding);
        }

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
        // for-of yields `undefined` for a sparse HOLE ([[Get]] semantics).
        let elem = crate::value::emit_marshal::emit_hole_to_undef(self.builder, elem);
        self.builder.def_var(bind_var, elem);
        // for-in mid-iteration guard: a key deleted since the snapshot is skipped.
        if let Some(obj_w) = present_guard {
            let has = self
                .call_runtime(module, "__rtsadp_obj_has", &[obj_w, elem])?
                .expect("__rtsadp_obj_has returns a flag");
            let run_block = self.builder.create_block();
            self.builder
                .ins()
                .brif(has, run_block, &[], advance_block, &[]);
            self.builder.switch_to_block(run_block);
            self.builder.seal_block(run_block);
            self.block_terminated = false;
        }
        let label = self.pending_label.take();
        self.loop_stack.push(LoopCtx {
            exit: exit_block,
            continue_target: advance_block,
            label,
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

    /// `break` — jump to a loop's exit. Unlabeled → the innermost loop/switch; a
    /// `break label` → the enclosing loop carrying that label (searched outward).
    /// A `break` outside any loop, or an unknown label, BAILS.
    pub(super) fn lower_break(&mut self, label: Option<&str>) -> FrontResult<()> {
        let exit = match label {
            Some(l) => self
                .loop_stack
                .iter()
                .rev()
                .find(|c| c.label.as_deref() == Some(l))
                .map(|c| c.exit)
                .ok_or_else(|| {
                    crate::front::error::Unsupported::new(format!(
                        "`break {l}`: no enclosing loop labeled `{l}`"
                    ))
                })?,
            None => self
                .loop_stack
                .last()
                .map(|c| c.exit)
                .ok_or_else(|| crate::front::error::Unsupported::new("`break` outside a loop"))?,
        };
        self.builder.ins().jump(exit, &[]);
        self.block_terminated = true;
        Ok(())
    }

    /// `continue` — jump to a loop's continue target (header / update / advance).
    /// Unlabeled → the innermost loop; a `continue label` → the enclosing loop with
    /// that label. A `continue` outside any loop, or an unknown label, BAILS.
    pub(super) fn lower_continue(&mut self, label: Option<&str>) -> FrontResult<()> {
        let target = match label {
            Some(l) => self
                .loop_stack
                .iter()
                .rev()
                .find(|c| c.label.as_deref() == Some(l))
                .map(|c| c.continue_target)
                .ok_or_else(|| {
                    crate::front::error::Unsupported::new(format!(
                        "`continue {l}`: no enclosing loop labeled `{l}`"
                    ))
                })?,
            None => self.loop_stack.last().map(|c| c.continue_target).ok_or_else(|| {
                crate::front::error::Unsupported::new("`continue` outside a loop")
            })?,
        };
        self.builder.ins().jump(target, &[]);
        self.block_terminated = true;
        Ok(())
    }
}
