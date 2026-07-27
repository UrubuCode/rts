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
        let recv = self.lower_expr(module, object)?;
        let recv_word = self.box_value(recv);
        // SINGLE-EVAL marshaling: lower + box every argument ONCE, before any
        // branching — the SSA values dominate every arm, so an effectful arg
        // (a call, `new`, an assignment) evaluates exactly once (JS order:
        // receiver first, then args). A Spread arg is not a plain value and
        // still bails.
        let arg_words = self.lower_arg_words(module, args)?;
        let shape_word = self.read_shape_word(module, recv_word);

        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, types::I64);
        self.emit_shape_arms(module, recv_word, shape_word, targets, &arg_words, merge)?;
        // DEFAULT arm: the static class's own method.
        let v = self.call_synth_fn_words(module, base_fn, Some(recv_word), &arg_words)?;
        let w = self.box_value(v);
        self.builder.ins().jump(merge, &[w.into()]);

        let result = self.finish_merge(merge);
        let kind = ret_kind(self.sigs.get(base_fn).and_then(|s| s.ret));
        Ok(Val::new_with_kind(result, Repr::Tagged, kind))
    }

    /// Lower + box each plain argument once (the single-eval prelude of the
    /// virtual dispatchers). A `Spread` arg has no single-value form → bail.
    fn lower_arg_words(
        &mut self,
        module: &mut dyn Module,
        args: &[HirExpr],
    ) -> FrontResult<Vec<Value>> {
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            if matches!(a.kind, HirExprKind::Spread(_)) {
                return unsupported!(
                    "virtual method dispatch with a spread argument (later increment)"
                );
            }
            let v = self.lower_expr(module, a)?;
            out.push(self.box_value(v));
        }
        Ok(out)
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
        if targets.is_empty() {
            return Ok(None);
        }

        let recv_word = self.box_value(recv);
        // SINGLE-EVAL marshaling: every argument lowers ONCE here; the words
        // dominate all arms (effectful args are sound — evaluated exactly once).
        let arg_words = self.lower_arg_words(module, args)?;
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
        self.emit_shape_arms(module, recv_word, shape_word, &targets, &arg_words, merge)?;
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
        if arg_words.len() <= 3 {
            use cranelift_codegen::ir::condcodes::IntCC;
            let key_word = self.emit_str_const_word(module, method)?;
            let own = self
                .call_runtime(module, "__rtsadp_obj_get", &[recv_word, key_word])?
                .expect("__rtsadp_obj_get returns a value");
            let boxed = value::emit_is_boxed(self.builder, own);
            let shifted = self.builder.ins().ushr_imm(own, value::TAG_SHIFT as i64);
            let tag = self.builder.ins().band_imm(shifted, value::TAG_MASK as i64);
            let is_fn_tag =
                self.builder
                    .ins()
                    .icmp_imm(IntCC::Equal, tag, value::TAG_FUNCTION as i64);
            let is_fn = self.builder.ins().band(boxed, is_fn_tag);
            let b_fn = self.builder.create_block();
            let b_undef = self.builder.create_block();
            self.builder.ins().brif(is_fn, b_fn, &[], b_undef, &[]);
            self.builder.seal_block(b_fn);
            self.builder.seal_block(b_undef);

            self.builder.switch_to_block(b_fn);
            // Same emission as `lower_value_call_word_with_this`, over the
            // PRE-LOWERED words (no re-eval of effectful args).
            let undef_c = self
                .builder
                .ins()
                .iconst(types::I64, value::PolyValue::undefined().raw() as i64);
            let mut slots: [Value; 3] = [undef_c; 3];
            for (i, &w) in arg_words.iter().take(3).enumerate() {
                slots[i] = w;
            }
            let zero = self.builder.ins().iconst(types::I64, 0);
            let res = self
                .call_runtime(
                    module,
                    "__rtsadp_fn_invoke_method",
                    &[own, recv_word, slots[0], slots[1], slots[2], zero],
                )?
                .expect("__rtsadp_fn_invoke_method returns a value");
            self.emit_post_call_error_check(module)?;
            self.builder.ins().jump(merge, &[res.into()]);

            // No matching user shape AND no own function-valued property: the
            // receiver may still be a native object-backed Registry instance
            // (`Hash`/`StringDecoder`/…) whose method NAME collides with some user
            // class's method (which is why this virtual path was entered at all).
            // Resolve it via the runtime-CI table by the receiver's `__rts_class`
            // tag; a true miss keeps the `undefined` sentinel (this trampoline
            // returns `undefined` rather than throwing).
            self.builder.switch_to_block(b_undef);
            // Fresh arg slots dominating THIS block (the `b_fn` ones do not).
            let u_undef = self.builder.ins().iconst(types::I64, undef);
            let mut ci_slots: [Value; 3] = [u_undef; 3];
            for (i, &w) in arg_words.iter().take(3).enumerate() {
                ci_slots[i] = w;
            }
            let ci = self
                .call_runtime(
                    module,
                    "__rtsadp_dyn_ci_or_undef",
                    &[recv_word, key_word, ci_slots[0], ci_slots[1], ci_slots[2]],
                )?
                .expect("__rtsadp_dyn_ci_or_undef returns a value");
            self.emit_post_call_error_check(module)?;
            self.builder.ins().jump(merge, &[ci.into()]);
        } else {
            let u = self.builder.ins().iconst(types::I64, undef);
            self.builder.ins().jump(merge, &[u.into()]);
        }

        let result = self.finish_merge(merge);
        Ok(Some(Val::new_with_kind(
            result,
            Repr::Tagged,
            JsKind::Unknown,
        )))
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
        let key_word = self.emit_str_const_word(module, prop)?;
        let data = self
            .call_runtime(module, "__rtsadp_obj_get", &[recv_word, key_word])?
            .expect("__rtsadp_obj_get returns a value");
        self.builder.ins().jump(merge, &[data.into()]);

        let result = self.finish_merge(merge);
        Ok(Some(Val::new_with_kind(
            result,
            Repr::Tagged,
            JsKind::Unknown,
        )))
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
    /// `arg_words` are the PRE-LOWERED, boxed argument words (single-eval — they
    /// dominate every arm; only the arm the runtime takes uses them).
    fn emit_shape_arms(
        &mut self,
        module: &mut dyn Module,
        recv_word: Value,
        shape_word: Value,
        targets: &[(u32, String)],
        arg_words: &[Value],
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
            let v = self.call_synth_fn_words(module, fname, Some(recv_word), arg_words)?;
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
    /// `true` quando o receptor é um valor de HEAP com propriedades — objeto **ou
    /// função**.
    ///
    /// Aceitar FUNCTION não é cosmético: sem isso, `new Function("a","return a").name`
    /// devolvia `undefined` onde o Node dá `"anonymous"`. O caminho é este — alguma
    /// classe do prelúdio declara um getter `name`, então `try_user_getter_dynamic`
    /// assume o acesso; uma função tem tag FUNCTION (5), não OBJECT (4), caía no ramo
    /// "não-objeto" e saía com `undefined` ANTES de alcançar o fall-through
    /// `__rtsadp_obj_get`, que lê o nome corretamente (`objops.rs` trata receptor-
    /// função explicitamente). O sintoma valia para QUALQUER propriedade cujo nome
    /// colida com um getter declarado (`name`, `size`, `flags`…) sobre um valor-função;
    /// o acesso dinâmico `f["na"+"me"]` acertava por ir direto ao runtime.
    ///
    /// Continua excluindo primitivos: `(5).size` segue lendo `undefined`, que é o
    /// comportamento JS correto e o motivo de o gate existir.
    fn emit_is_object(&mut self, v: Value) -> Value {
        let boxed = value::emit_is_boxed(self.builder, v);
        let shifted = self.builder.ins().ushr_imm(v, value::TAG_SHIFT as i64);
        let tag = self.builder.ins().band_imm(shifted, value::TAG_MASK as i64);
        let want_obj = self
            .builder
            .ins()
            .iconst(types::I64, value::TAG_OBJECT as i64);
        let want_fn = self
            .builder
            .ins()
            .iconst(types::I64, value::TAG_FUNCTION as i64);
        let is_obj_tag = self.builder.ins().icmp(IntCC::Equal, tag, want_obj);
        let is_fn_tag = self.builder.ins().icmp(IntCC::Equal, tag, want_fn);
        let is_heap = self.builder.ins().bor(is_obj_tag, is_fn_tag);
        self.builder.ins().band(boxed, is_heap)
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

// (The old `is_side_effect_free_arg` gate is GONE: the dispatchers now lower
// every argument ONCE before branching — single-eval marshaling — so effectful
// args are sound. Only a Spread arg still bails, in `lower_arg_words`.)
