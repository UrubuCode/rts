//! VIRTUAL method dispatch — shape-keyed runtime method resolution.
//!
//! A class instance carries its CLASS IDENTITY in slot 0 (the class's UNIQUE
//! `global_shape`, baked by [`super::super::newexpr`]). When the receiver's runtime
//! class may differ from its static class — an override exists in the subtree, or
//! the receiver is a Tagged value of unproven class (a for-of binding, a cast, a
//! reassigned local, a function return) — the target method is resolved at RUNTIME
//! by comparing that shape-id, NOT at compile time. This is the design's data-IC
//! dispatch in its simplest form: a straight-line `icmp` chain on the shape word,
//! one arm per candidate class, merged through a block param.
//!
//! Two entry points:
//! - [`Lowerer::virtual_targets`] + [`Lowerer::emit_virtual_dispatch`] — the STATIC
//!   path: the receiver's static class `C` is known, and the dispatch covers `C`'s
//!   overriding descendants with `C`'s own method as the DEFAULT arm.
//! - [`Lowerer::try_user_virtual_dynamic`] — the DYNAMIC path: the receiver is a
//!   Tagged value of unproven class, so the dispatch covers EVERY user class that
//!   defines `method`, guarded by an is-object check, with `undefined` (a
//!   TypeError-class sentinel, never a wrong value) as the default.
//!
//! Both re-lower each argument per arm, so they accept only SIDE-EFFECT-FREE args
//! (idents/literals); an effectful arg bails (single-eval marshaling is a later
//! increment). This covers the common 0/1-arg virtual call.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;

use rts_hir::HirExpr;
use rts_hir::ir::HirExprKind;

use crate::repr::Repr;
use crate::value::{self, emit_marshal};

use crate::front::error::{FrontResult, unsupported};

use super::super::lower::{JsKind, Lowerer, Val};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// The polymorphic override set for `method` on `class`: every PROPER descendant
    /// `D` of `class` that resolves `method` to a DIFFERENT synthesized fn than
    /// `class` itself (an override), as `(D.global_shape, D's fn name)`. Returns
    /// `None` when no descendant overrides `method` (the site is MONOMORPHIC — every
    /// possible runtime class shares `class`'s fn, so a direct call is correct).
    pub(in crate::front::run) fn virtual_targets(
        &self,
        class: &str,
        method: &str,
    ) -> Option<Vec<(u32, String)>> {
        let base = self.classes.get(class)?.method_fn(method)?.to_string();
        let mut out: Vec<(u32, String)> = Vec::new();
        for d in self.classes.iter() {
            if d.name == class || !self.is_descendant(&d.name, class) {
                continue;
            }
            if let Some(f) = d.method_fn(method) {
                if f != base {
                    out.push((d.global_shape, f.to_string()));
                }
            }
        }
        (!out.is_empty()).then_some(out)
    }

    /// Like [`Self::virtual_targets`] but for an ABSTRACT-declared method: the
    /// base class has NO implementation, so EVERY descendant defining `method`
    /// is a target (no base-fn filter). `None` when no descendant defines it.
    pub(in crate::front::run) fn virtual_targets_abstract(
        &self,
        class: &str,
        method: &str,
    ) -> Option<Vec<(u32, String)>> {
        let mut out: Vec<(u32, String)> = Vec::new();
        for d in self.classes.iter() {
            if d.name == class || !self.is_descendant(&d.name, class) {
                continue;
            }
            if let Some(f) = d.method_fn(method) {
                out.push((d.global_shape, f.to_string()));
            }
        }
        (!out.is_empty()).then_some(out)
    }

    /// Whether the synthesized METHOD `fn_name` (its `params[0]` is `this`) is
    /// callable with `argc` explicit args: every user param beyond `argc` must be
    /// FILLABLE (optional/defaulted); a rest param packs any surplus. Extra args
    /// beyond the declared arity are fine (JS evaluates + ignores them).
    fn sig_accepts_argc(&self, fn_name: &str, argc: usize) -> bool {
        let Some(sig) = self.sigs.get(fn_name) else {
            return false;
        };
        if sig.rest_param.is_some() {
            return true;
        }
        let pi = 1usize; // methods carry `this` as params[0]
        let user = sig.params.len().saturating_sub(pi);
        (argc..user).all(|i| sig.fillable.get(pi + i).copied().unwrap_or(false))
    }

    /// Whether class `d` is a (transitive) descendant of `ancestor` — walks the
    /// `parent` chain. `d == ancestor` is NOT a descendant (returns false).
    fn is_descendant(&self, d: &str, ancestor: &str) -> bool {
        let mut cur = self.classes.get(d).and_then(|x| x.parent.clone());
        while let Some(p) = cur {
            if p == ancestor {
                return true;
            }
            cur = self.classes.get(&p).and_then(|x| x.parent.clone());
        }
        false
    }

    /// STATIC virtual dispatch: the receiver's static class is known, `targets` are
    /// its overriding descendants, and `base_fn` (the static class's method) is the
    /// DEFAULT arm (covers the class itself and any non-overriding descendant). The
    /// receiver is lowered ONCE; the result of every arm merges through a block param.
    pub(in crate::front::run) fn emit_virtual_dispatch(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        targets: &[(u32, String)],
        base_fn: &str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        if !args.iter().all(is_side_effect_free_arg) {
            return unsupported!(
                "virtual method dispatch with an effectful argument (single-eval \
                 marshaling is a later increment)"
            );
        }
        let recv = self.lower_expr(module, object)?;
        let recv_word = self.box_value(recv);
        let shape_word = self.read_shape_word(module, recv_word);

        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, types::I64);
        self.emit_shape_arms(module, recv_word, shape_word, targets, args, merge)?;
        // DEFAULT arm: the static class's own method.
        let v = self.call_synth_fn(module, base_fn, Some(recv_word), args)?;
        let w = self.box_value(v);
        self.builder.ins().jump(merge, &[w.into()]);

        let result = self.finish_merge(merge);
        let kind = ret_kind(self.sigs.get(base_fn).and_then(|s| s.ret));
        Ok(Val::new_with_kind(result, Repr::Tagged, kind))
    }

    /// DYNAMIC virtual dispatch on a TAGGED receiver of unproven class: dispatch to
    /// whichever USER class (by runtime shape-id) defines `method`. Guarded by an
    /// is-object check (a non-object receiver — a number/string/bool — yields the
    /// `undefined` sentinel, never reading a bogus slot); a shape matching no
    /// class-with-`method` also yields `undefined` (a TypeError in JS — the honest
    /// floor, never a wrong value).
    ///
    /// Returns `Ok(None)` when NO user class defines `method` (the caller falls
    /// through / bails) or an argument is effectful (single-eval is a later
    /// increment) — never a guess. `recv` is the ALREADY-lowered receiver `Val`.
    pub(in crate::front::run) fn try_user_virtual_dynamic(
        &mut self,
        module: &mut dyn Module,
        recv: Val,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Option<Val>> {
        // Candidate user classes defining `method`, including object-literal
        // synthesized "literal classes" (`__rtsl_*`). Post-cutover, an object
        // literal IS a shape-slot object and its method `this` is that shape
        // object, so dispatching its body on a Tagged receiver carrying the SAME
        // shape-id is sound (the per-shape arm in `emit_shape_arms` only runs the
        // body for an exactly-matching shape). This lets a free top-level
        // `const o = { m(){…} }` referenced inside another function's body
        // dispatch `o.m()` by runtime shape-id instead of bailing. (The old
        // exclusion guarded the deleted `collections.map_get(this, …)` model,
        // which no longer exists.)
        let mut targets: Vec<(u32, String)> = Vec::new();
        for d in self.classes.iter() {
            if let Some(f) = d.method_fn(method) {
                targets.push((d.global_shape, f.to_string()));
            }
        }
        // Drop ARITY-INCOMPATIBLE candidates (a 2-arg literal `has(target, key)`
        // — a Proxy handler — colliding with a 1-arg `Set.has(v)` call site):
        // emitting their arm would fail the WHOLE compile with "missing required
        // arg". A skipped candidate's receiver falls to the default arm (own-prop
        // read → `undefined` sentinel) — honest, never a wrong value.
        targets.retain(|(_, f)| self.sig_accepts_argc(f, args.len()));
        if targets.is_empty() || !args.iter().all(is_side_effect_free_arg) {
            return Ok(None);
        }

        let recv_word = self.box_value(recv);
        let is_obj = self.emit_is_object(recv_word);
        let undef = value::PolyValue::undefined().raw() as i64;

        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, types::I64);
        let guarded = self.builder.create_block();
        let nonobj = self.builder.create_block();
        self.builder.ins().brif(is_obj, guarded, &[], nonobj, &[]);

        // Non-object receiver → undefined.
        self.builder.switch_to_block(nonobj);
        self.builder.seal_block(nonobj);
        let u = self.builder.ins().iconst(types::I64, undef);
        self.builder.ins().jump(merge, &[u.into()]);

        // Object receiver → shape switch over every class-with-method; the
        // DEFAULT arm falls through below.
        self.builder.switch_to_block(guarded);
        self.builder.seal_block(guarded);
        let shape_word = self.read_shape_word(module, recv_word);
        self.emit_shape_arms(module, recv_word, shape_word, &targets, args, merge)?;
        // DEFAULT arm — no class shape matched: the receiver may still carry
        // `method` as an OWN function-valued property (`{ add: (a, b) => a + b }`
        // — an object literal whose fn-valued member shares a name with some
        // class's method, or an own override). Read the own slot; when it holds a
        // FUNCTION word, invoke it as a method (`__rtsadp_fn_invoke_method` binds
        // `this` for a this-first callee); anything else keeps the `undefined`
        // sentinel. Mirrors `try_user_getter_dynamic`'s data-slot default arm —
        // an own property beats "no method" (JS). Without this, a class defining
        // `method` ANYWHERE in the program (e.g. the prelude `Set.add`) stole the
        // call from every plain object carrying an own fn of that name.
        if args.len() <= 3 {
            use cranelift_codegen::ir::condcodes::IntCC;
            let key = crate::value::abi_adapter::intern_poly(method);
            let key_word = self.builder.ins().iconst(types::I64, key.raw() as i64);
            let own = self
                .call_runtime(module, "__rtsadp_obj_get", &[recv_word, key_word])?
                .expect("__rtsadp_obj_get returns a value");
            let boxed = value::emit_is_boxed(self.builder, own);
            let shifted = self.builder.ins().ushr_imm(own, value::TAG_SHIFT as i64);
            let tag = self.builder.ins().band_imm(shifted, value::TAG_MASK as i64);
            let is_fn_tag = self
                .builder
                .ins()
                .icmp_imm(IntCC::Equal, tag, value::TAG_FUNCTION as i64);
            let is_fn = self.builder.ins().band(boxed, is_fn_tag);
            let b_fn = self.builder.create_block();
            let b_undef = self.builder.create_block();
            self.builder.ins().brif(is_fn, b_fn, &[], b_undef, &[]);
            self.builder.seal_block(b_fn);
            self.builder.seal_block(b_undef);

            self.builder.switch_to_block(b_fn);
            let v = self.lower_value_call_word_with_this(module, own, recv_word, args)?;
            let w = self.box_value(v);
            self.builder.ins().jump(merge, &[w.into()]);

            self.builder.switch_to_block(b_undef);
            let u = self.builder.ins().iconst(types::I64, undef);
            self.builder.ins().jump(merge, &[u.into()]);
        } else {
            let u = self.builder.ins().iconst(types::I64, undef);
            self.builder.ins().jump(merge, &[u.into()]);
        }

        let result = self.finish_merge(merge);
        Ok(Some(Val::new_with_kind(result, Repr::Tagged, JsKind::Unknown)))
    }

    /// DYNAMIC GETTER dispatch on a TAGGED receiver of unproven class: an accessor
    /// READ `recv.prop` where `prop` is a `get prop()` on some USER class (e.g.
    /// `map.size`, `set.size`) resolved at RUNTIME by the instance's shape-id. The
    /// DEFAULT arm (a shape matching no getter-class) reads the property as a DATA
    /// slot (`__rtsadp_obj_get`), so a plain object with a real `prop` slot still
    /// reads its value — only a getter-class shape calls the getter. A non-object
    /// receiver yields `undefined`.
    ///
    /// Returns `Ok(None)` when NO user class declares a getter `prop` (the caller
    /// keeps its existing dynamic-data-read fallback) — never a guess.
    pub(in crate::front::run) fn try_user_getter_dynamic(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        prop: &str,
    ) -> FrontResult<Option<Val>> {
        // Candidate real user classes with a GETTER `prop` (literal classes excluded,
        // same reason as the method path).
        let mut targets: Vec<(u32, String)> = Vec::new();
        for d in self.classes.iter() {
            if d.name.starts_with("__rtsl_") {
                continue;
            }
            if let Some(getter) = d.accessor(prop).and_then(|a| a.getter.clone()) {
                targets.push((d.global_shape, getter));
            }
        }
        if targets.is_empty() {
            return Ok(None);
        }

        let recv = self.lower_expr(module, object)?;
        let recv_word = self.box_value(recv);
        let is_obj = self.emit_is_object(recv_word);
        let undef = value::PolyValue::undefined().raw() as i64;

        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, types::I64);
        let guarded = self.builder.create_block();
        let nonobj = self.builder.create_block();
        self.builder.ins().brif(is_obj, guarded, &[], nonobj, &[]);

        // Non-object receiver → undefined (`(5).size`).
        self.builder.switch_to_block(nonobj);
        self.builder.seal_block(nonobj);
        let u = self.builder.ins().iconst(types::I64, undef);
        self.builder.ins().jump(merge, &[u.into()]);

        // Object receiver → shape switch over getter-classes; DEFAULT = data-slot read
        // (`__rtsadp_obj_get`) so a plain object reads its real `prop` value.
        self.builder.switch_to_block(guarded);
        self.builder.seal_block(guarded);
        let shape_word = self.read_shape_word(module, recv_word);
        self.emit_shape_arms(module, recv_word, shape_word, &targets, &[], merge)?;
        let key_word = self.intern_key_word(prop);
        let data = self
            .call_runtime(module, "__rtsadp_obj_get", &[recv_word, key_word])?
            .expect("__rtsadp_obj_get returns a value");
        self.builder.ins().jump(merge, &[data.into()]);

        let result = self.finish_merge(merge);
        Ok(Some(Val::new_with_kind(result, Repr::Tagged, JsKind::Unknown)))
    }

    /// Read the instance's slot 0 (its class `global_shape`, a `from_i32` tagged-int
    /// word) — the runtime class identity compared by each dispatch arm.
    fn read_shape_word(&mut self, module: &mut dyn Module, recv_word: Value) -> Value {
        let idx0 = self.builder.ins().iconst(types::I64, 0);
        emit_marshal::emit_vec_get(module, self.builder, recv_word, idx0)
    }

    /// Emit one `if shape == D.global_shape { call D's fn }` arm per target, each
    /// jumping to `merge` with its boxed result word. Leaves the builder on the
    /// fall-through block so the caller emits the DEFAULT arm + jump to `merge`.
    fn emit_shape_arms(
        &mut self,
        module: &mut dyn Module,
        recv_word: Value,
        shape_word: Value,
        targets: &[(u32, String)],
        args: &[HirExpr],
        merge: cranelift_codegen::ir::Block,
    ) -> FrontResult<()> {
        for (gs, fname) in targets {
            let call_blk = self.builder.create_block();
            let next_blk = self.builder.create_block();
            let want = self.builder.ins().iconst(
                types::I64,
                value::PolyValue::from_i32(*gs as i32).raw() as i64,
            );
            let eq = self.builder.ins().icmp(IntCC::Equal, shape_word, want);
            self.builder.ins().brif(eq, call_blk, &[], next_blk, &[]);

            self.builder.switch_to_block(call_blk);
            self.builder.seal_block(call_blk);
            let v = self.call_synth_fn(module, fname, Some(recv_word), args)?;
            let w = self.box_value(v);
            self.builder.ins().jump(merge, &[w.into()]);

            self.builder.switch_to_block(next_blk);
            self.builder.seal_block(next_blk);
        }
        Ok(())
    }

    /// Switch to + seal `merge` and read its single block param (the merged result).
    fn finish_merge(&mut self, merge: cranelift_codegen::ir::Block) -> Value {
        self.builder.switch_to_block(merge);
        self.builder.seal_block(merge);
        self.builder.block_params(merge)[0]
    }

    /// `is_object(v)` as IR: `(v & BOX_BASE) == BOX_BASE && tag(v) == TAG_OBJECT`.
    /// A boxed-object word reads its slot 0; any other word does not (so a number /
    /// string / singleton receiver routes to the `undefined` default).
    fn emit_is_object(&mut self, v: Value) -> Value {
        let boxed = value::emit_is_boxed(self.builder, v);
        let shifted = self.builder.ins().ushr_imm(v, value::TAG_SHIFT as i64);
        let tag = self.builder.ins().band_imm(shifted, value::TAG_MASK as i64);
        let want = self
            .builder
            .ins()
            .iconst(types::I64, value::TAG_OBJECT as i64);
        let is_obj_tag = self.builder.ins().icmp(IntCC::Equal, tag, want);
        self.builder.ins().band(boxed, is_obj_tag)
    }
}

/// The merged-result `JsKind` for a method whose static return repr is `ret`:
/// numeric reprs → `Number` (so `+`/print read it right), everything else stays
/// `Unknown` (a boxed STR word still prints correctly via its runtime tag).
fn ret_kind(ret: Option<Repr>) -> JsKind {
    match ret {
        Some(Repr::Int32) | Some(Repr::Int64) | Some(Repr::Float64) => JsKind::Number,
        Some(Repr::Bool) => JsKind::Bool,
        _ => JsKind::Unknown,
    }
}

/// Whether `e` re-evaluates with NO observable side effect — a bare identifier or a
/// literal. A virtual-dispatch arm re-lowers each argument, so only such args are
/// safe (an effectful arg bails / declines).
fn is_side_effect_free_arg(e: &HirExpr) -> bool {
    match &e.kind {
        HirExprKind::Ident(_) | HirExprKind::Lit(_) => true,
        // Pure arithmetic/comparison over side-effect-free operands (`cur + 1`
        // as a call arg): re-evaluating per dispatch arm is harmless. Rejecting
        // it made the virtual dispatch DECLINE and the caller fall through to a
        // dynamic own-prop miss — `map.set(k, cur + 1)` silently no-opped.
        HirExprKind::Bin { lhs, rhs, .. } => {
            is_side_effect_free_arg(lhs) && is_side_effect_free_arg(rhs)
        }
        HirExprKind::Unary { operand, .. } => is_side_effect_free_arg(operand),
        // A MEMBER READ (`this.x`, `obj.k`) may hit a GETTER with effects in
        // principle, but the object-position operand is an Ident (no call), so
        // double-lowering only re-reads — accepted (matches the member reads the
        // dispatch arms themselves perform).
        HirExprKind::Member { object, .. } => is_side_effect_free_arg(object),
        // An INDEX READ (`arr[i]`, `this.#items[i]`) over side-effect-free
        // operands re-lowers as a plain re-read, same reasoning as Member.
        // Rejecting it made `other.has(this.#items[i])` (the stdlib Set ops)
        // decline the virtual dispatch and fall to a dynamic own-prop miss.
        HirExprKind::Index { object, index } => {
            is_side_effect_free_arg(object) && is_side_effect_free_arg(index)
        }
        _ => false,
    }
}
