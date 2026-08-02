//! Assignment + `++`/`--` lowering for a LOCAL (`x = e`, `x++`) — split out of
//! `stmt.rs` (the <500-line module rule). Property/index writes route on through
//! `lower_member_assign`/`lower_index_assign` (still in their own modules).

use cranelift_codegen::ir::{InstBuilder, types};
use cranelift_module::Module;

use rts_hir::HirExpr;
use rts_hir::ir::HirExprKind;

use crate::repr::Repr;

use crate::front::error::{FrontResult, Unsupported, unsupported};

use super::lower::{Lowerer, Val};
use super::stmt::ident_target;

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Plain assignment `x = e`. Only to an existing local; the value coerces to
    /// the local's repr.
    pub(super) fn lower_assign(
        &mut self,
        module: &mut dyn Module,
        target: &HirExpr,
        value: &HirExpr,
    ) -> FrontResult<Val> {
        // Property/index writes (`obj.k = v`, `arr[i] = v`) on a proven shape.
        match &target.kind {
            HirExprKind::Member { object, prop } => {
                return self.lower_member_assign(module, object, prop, value);
            }
            HirExprKind::Index { object, index } => {
                return self.lower_index_assign(module, object, index, value);
            }
            // DESTRUCTURING assignment `[x, arr[i], o.k] = rhs` (the swap
            // pattern `[a[i], a[j]] = [a[j], a[i]]`): evaluate the RHS to a
            // temp ARRAY word, then assign each target element from `tmp[k]`
            // recursively (each element may itself be an Ident/Index/Member).
            // The RHS evaluates FULLY before any write — JS order, which is
            // what makes the swap correct.
            HirExprKind::Array(targets) => {
                let rhs = self.lower_expr(module, value)?;
                let rhs_word = self.box_value(rhs);
                for (k, t) in targets.iter().enumerate() {
                    if matches!(t.kind, HirExprKind::Lit(rts_hir::ir::HirLit::Hole)) {
                        continue; // elision skips the slot
                    }
                    let idx = self
                        .builder
                        .ins()
                        .iconst(cranelift_codegen::ir::types::I64, k as i64);
                    let elem = crate::value::emit_marshal::emit_vec_get(
                        module,
                        self.builder,
                        rhs_word,
                        idx,
                    );
                    let elem = crate::value::emit_marshal::emit_hole_to_undef(self.builder, elem);
                    // Recurse through the normal assignment lowering with a
                    // synthetic pre-lowered value: bind the word to a fresh
                    // hidden local and assign `t = <hidden>`.
                    let tmp_name = format!("__rtsn_destr_tmp_{k}");
                    self.bind_tagged_local(&tmp_name, Val::new(elem, crate::repr::Repr::Tagged));
                    let tmp_expr =
                        HirExpr::new(HirExprKind::Ident(tmp_name), rts_hir::HirType::Unknown);
                    self.lower_assign(module, t, &tmp_expr)?;
                }
                return Ok(Val::new(rhs_word, crate::repr::Repr::Tagged));
            }
            _ => {}
        }
        let name = ident_target(target)?;
        // MODULE-LEVEL MUTABLE GLOBAL (epic #195): a top-level `let` written from a
        // function (or from main after promotion). Store through the runtime cell.
        if let Some(id) = self.gcell_id(&name) {
            let val = self.lower_expr(module, value)?;
            let word = self.box_value(val);
            self.emit_gcell_set(module, id, word)?;
            return Ok(Val::new(word, Repr::Tagged));
        }
        // FUNCTION-LOCAL CELL (#195): write the value through the cell so a
        // capturing closure (and later reads) see it live.
        if self.is_cell_local(&name) {
            let handle = {
                let local = self.local(&name).expect("cell-local is a bound local");
                self.builder.use_var(local.var)
            };
            let val = self.lower_expr(module, value)?;
            let word = self.box_value(val);
            self.emit_cell_set(module, handle, word);
            return Ok(Val::new(word, Repr::Tagged));
        }
        // Re-`x = {…}` to a DIFFERENT object literal: the local stays a proven
        // keyed OBJECT, but its exact shape may now differ from the old one. Lower
        // the new literal (recording its global shape-id in slot 0), bind it, and
        // mark `name` an `object_local` (dynamic-access) — so `x.k` after the
        // reassignment resolves the key at RUNTIME instead of bailing. If the new
        // literal happens to share the prior shape, the dynamic path still reads
        // correctly; we conservatively use the dynamic path rather than prove it.
        if matches!(&value.kind, HirExprKind::Object(_)) && self.local(&name).is_some() {
            if let HirExprKind::Object(fields) = &value.kind {
                let local = self.local(&name).expect("checked above");
                if matches!(local.repr, Repr::Tagged) {
                    let (val, _shape, lit_class) = self.lower_object_literal(module, fields)?;
                    self.builder.def_var(local.var, val.v);
                    self.local_shapes.remove(&name);
                    self.global_instance_classes.remove(&name);
                    self.object_locals.insert(name.clone());
                    // P5.15: re-assigning to a method-bearing literal records its
                    // literal-class (so a later `obj.method()` dispatches); a plain
                    // literal clears any stale class.
                    match lit_class {
                        Some(class) => {
                            self.local_classes.insert(name.clone(), class);
                        }
                        None => {
                            self.local_classes.remove(&name);
                        }
                    }
                    return Ok(Val::new(val.v, Repr::Tagged));
                }
            }
        }
        // `RTS_DIAG_UNBOUND=<name>`: what the failing scope actually holds. Pairs
        // with the lifter-side report in `funcval` — together they say whether a
        // name is missing because the closure never captured it, and why.
        if self.local(&name).is_none() && std::env::var("RTS_DIAG_UNBOUND").as_deref() == Ok(&*name)
        {
            let mut locals: Vec<&String> = self.locals.keys().collect();
            locals.sort();
            eprintln!(
                "[diag] escrita a `{name}` sem binding em `{}` | capturas={:?} | cells={:?} \
                 | locais={locals:?}",
                self.current_fn,
                self.captures.get(&self.current_fn),
                self.cell_locals,
            );
        }
        // Nome que não casa com binding léxico nenhum: atribuir a ele CRIA um
        // global (o "global implícito" do modo sloppy, que é o modo de todo
        // `<script>` de página). Simétrico ao `__rtsadp_global_ref` da LEITURA,
        // e pelo mesmo motivo — recusar derrubava o arquivo inteiro.
        if self.local(&name).is_none() {
            let name_w = self.emit_str_const_word(module, &name)?;
            let v = self.lower_expr(module, value)?;
            let word = self.box_value(v);
            let sym = rts_runtime::adapters::value::globalthis::GLOBAL_SET_ABI.name;
            let res = self
                .call_runtime(module, sym, &[name_w, word])?
                .expect("__rtsadp_global_set returns the assigned value");
            return Ok(Val::new(res, Repr::Tagged));
        }
        let local = self
            .local(&name)
            .ok_or_else(|| Unsupported::new(format!("assignment to unbound `{name}`")))?;
        let val = self.lower_expr(module, value)?;
        let coerced = self.coerce(val, local.repr)?;
        self.builder.def_var(local.var, coerced);
        // The local now holds an opaque value: drop any stale proven shape/class so
        // a later `name.key`/`name.method()` does NOT resolve against the old layout.
        self.local_shapes.remove(&name);
        self.local_classes.remove(&name);
        self.global_instance_classes.remove(&name);
        self.object_locals.remove(&name);
        // …unless the assigned value is a PROVEN ARRAY, in which case record the
        // shape rather than dropping to "opaque" — same reasoning as the
        // object-literal re-assignment branch above, one step MORE precise than
        // the clear. This is what makes `var xs = [..]` work: `var` hoisting
        // splits the declaration (`let xs = undefined`) from the initializer,
        // which then arrives here as an ASSIGNMENT, so the array-ness recorded at
        // a `let xs = [..]` site was simply never recorded — and every minified
        // file declares with `var`. Without it `xs.slice()` fell to the dynamic
        // path, whose hand-written array table lacks most methods
        // (`TypeError: slice is not a function` on a plain array).
        if (matches!(val.kind, super::lower::JsKind::Array)
            || matches!(&value.kind, HirExprKind::Array(_)))
            && matches!(local.repr, Repr::Tagged)
        {
            self.local_shapes
                .insert(name.clone(), super::lower::HeapShape::Array);
        }
        Ok(Val::new(coerced, local.repr))
    }

    /// `++x` / `x++` / `--x` / `x--` on a local.
    ///
    /// For a proven numeric repr (`Int*`/`Float64`) this is a native add/sub.
    /// For a `Tagged` local (e.g. a counter assigned from an `any` value) JS
    /// semantics apply: `ToNumber(old)`, then `± 1`, store back, and produce
    /// the OLD number (postfix) or the NEW number (prefix). The ToNumber and the
    /// arithmetic reuse the same generic `__rtsadp_*` trampolines as `+`/`-`.
    pub(super) fn lower_incdec(
        &mut self,
        module: &mut dyn Module,
        target: &HirExpr,
        inc: bool,
        prefix: bool,
    ) -> FrontResult<Val> {
        // MEMBER / INDEX `++`/`--` (`this.n++`, `obj.count--`, `arr[i]++`): read the
        // OLD value, write `target = target ± 1` (reusing the member/index store via a
        // synthesized `=`), and produce the NEW value (prefix) or the OLD (postfix).
        // Restricted to a SIDE-EFFECT-FREE object (a bare identifier, incl. `this`) so
        // the load + the store's re-read of the object are harmless; an ACCESSOR
        // property is skipped (a getter could have side effects → the double read
        // would diverge). A complex object expr bails (double-evaluation).
        if let HirExprKind::Member { object, prop } = &target.kind {
            // Mesma regra de replay do compound-assign: uma CADEIA de leituras
            // de propriedade (`this.a.b++`) é tão replayável quanto um
            // identificador único, e exigir o identificador recusava o arquivo.
            if Self::is_replayable_base(object) {
                if let Some(class) = self.static_instance_class(object) {
                    if self
                        .classes
                        .get(&class)
                        .and_then(|d| d.accessor(prop))
                        .is_some()
                    {
                        return unsupported!(
                            "`++`/`--` on accessor property `.{prop}` (later increment)"
                        );
                    }
                }
                return self.lower_member_index_incdec(module, target, inc, prefix);
            }
        }
        if let HirExprKind::Index { object, index } = &target.kind {
            if Self::is_replayable_base(object)
                && matches!(index.kind, HirExprKind::Ident(_) | HirExprKind::Lit(_))
            {
                return self.lower_member_index_incdec(module, target, inc, prefix);
            }
        }
        // Alvo com parte EFETIVA (`f().n++`, `a[g()]--`): avalia cada parte uma
        // vez num local oculto e refaz sobre elas — o mesmo que o compound-assign
        // faz, pelo mesmo motivo (o desugar lê e escreve o alvo, então uma
        // chamada na base rodaria duas vezes).
        match &target.kind {
            HirExprKind::Member { object, prop } if !Self::is_replayable_base(object) => {
                let base = self.hidden_local_for(module, object, "obj")?;
                let t = HirExpr::new(
                    HirExprKind::Member { object: Box::new(base), prop: prop.clone() },
                    rts_hir::HirType::Unknown,
                );
                return self.lower_incdec(module, &t, inc, prefix);
            }
            HirExprKind::Index { object, index } => {
                let base_ok = Self::is_replayable_base(object);
                let idx_ok = matches!(index.kind, HirExprKind::Ident(_) | HirExprKind::Lit(_));
                if !base_ok || !idx_ok {
                    let base = if base_ok { (**object).clone() }
                               else { self.hidden_local_for(module, object, "obj")? };
                    let idx = if idx_ok { (**index).clone() }
                              else { self.hidden_local_for(module, index, "idx")? };
                    let t = HirExpr::new(
                        HirExprKind::Index { object: Box::new(base), index: Box::new(idx) },
                        rts_hir::HirType::Unknown,
                    );
                    return self.lower_incdec(module, &t, inc, prefix);
                }
            }
            _ => {}
        }
        let name = ident_target(target)?;
        // MODULE-LEVEL MUTABLE GLOBAL (epic #195): `++`/`--` on a cell var. Read the
        // cell, ToNumber, ±1 generically, store back; produce old (postfix) or new
        // (prefix). Mirrors the Tagged-local path below but through gcell get/set.
        if let Some(id) = self.gcell_id(&name) {
            let old = self.emit_gcell_get(module, id)?;
            let old_num = self
                .call_runtime(module, "__rtsadp_pos", &[old.v])?
                .expect("__rtsadp_pos returns a value");
            let one_f64 = self.builder.ins().f64const(1.0);
            let one_word = self.box_value(Val::new(one_f64, Repr::Float64));
            let sym = if inc { "__rtsadp_add" } else { "__rtsadp_sub" };
            let new_num = self
                .call_runtime(module, sym, &[old_num, one_word])?
                .expect("generic arithmetic returns a value");
            self.emit_gcell_set(module, id, new_num)?;
            let produced = if prefix { new_num } else { old_num };
            return Ok(Val::new(produced, Repr::Tagged));
        }
        // FUNCTION-LOCAL CELL (#195): `++`/`--` through the cell (mirrors the gcell
        // path: read live, ToNumber, ±1 generically, store back).
        if self.is_cell_local(&name) {
            let handle = {
                let local = self.local(&name).expect("cell-local is a bound local");
                self.builder.use_var(local.var)
            };
            let old = self.emit_cell_get(module, handle);
            let old_num = self
                .call_runtime(module, "__rtsadp_pos", &[old.v])?
                .expect("__rtsadp_pos returns a value");
            let one_f64 = self.builder.ins().f64const(1.0);
            let one_word = self.box_value(Val::new(one_f64, Repr::Float64));
            let sym = if inc { "__rtsadp_add" } else { "__rtsadp_sub" };
            let new_num = self
                .call_runtime(module, sym, &[old_num, one_word])?
                .expect("generic arithmetic returns a value");
            self.emit_cell_set(module, handle, new_num);
            let produced = if prefix { new_num } else { old_num };
            return Ok(Val::new(produced, Repr::Tagged));
        }
        // Nome livre: `x++` lê e escreve o OBJETO GLOBAL, compondo os dois
        // trampolins que a cadeia de escopo já usa (`global_ref` na leitura,
        // `global_set` na escrita) — a mesma forma do caminho de célula logo
        // acima. Recusar derrubava o arquivo inteiro; um nome ausente de
        // verdade lança ReferenceError na LEITURA, que é a resposta do JS.
        if self.local(&name).is_none() {
            let name_w = self.emit_str_const_word(module, &name)?;
            let get = rts_runtime::adapters::value::globalthis::GLOBAL_REF_ABI.name;
            let set = rts_runtime::adapters::value::globalthis::GLOBAL_SET_ABI.name;
            let cur = self
                .call_runtime(module, get, &[name_w])?
                .expect("__rtsadp_global_ref returns a word");
            self.emit_post_call_error_check(module)?;
            let old_num = self
                .call_runtime(module, "__rtsadp_pos", &[cur])?
                .expect("__rtsadp_pos returns a value");
            let one_f64 = self.builder.ins().f64const(1.0);
            let one_word = self.box_value(Val::new(one_f64, Repr::Float64));
            let op = if inc { "__rtsadp_add" } else { "__rtsadp_sub" };
            let new_num = self
                .call_runtime(module, op, &[old_num, one_word])?
                .expect("generic arithmetic returns a value");
            self.call_runtime(module, set, &[name_w, new_num])?;
            let produced = if prefix { new_num } else { old_num };
            return Ok(Val::new(produced, Repr::Tagged));
        }
        let local = self
            .local(&name)
            .ok_or_else(|| Unsupported::new(format!("`++`/`--` on unbound `{name}`")))?;
        let old = self.builder.use_var(local.var);
        if matches!(local.repr, Repr::Tagged) {
            // ToNumber(old): a number PolyValue (boxed int32 or inline double).
            let old_num = self
                .call_runtime(module, "__rtsadp_pos", &[old])?
                .expect("__rtsadp_pos returns a value");
            // Box the literal `1` as a double PolyValue and add/sub generically.
            let one_f64 = self.builder.ins().f64const(1.0);
            let one_word = self.box_value(Val::new(one_f64, Repr::Float64));
            let sym = if inc { "__rtsadp_add" } else { "__rtsadp_sub" };
            let new_num = self
                .call_runtime(module, sym, &[old_num, one_word])?
                .expect("generic arithmetic returns a value");
            self.builder.def_var(local.var, new_num);
            let produced = if prefix { new_num } else { old_num };
            return Ok(Val::new(produced, Repr::Tagged));
        }
        let new = match local.repr {
            Repr::Int32 | Repr::Int64 => {
                let one = self.builder.ins().iconst(types::I64, 1);
                if inc {
                    self.builder.ins().iadd(old, one)
                } else {
                    self.builder.ins().isub(old, one)
                }
            }
            Repr::Float64 => {
                let one = self.builder.ins().f64const(1.0);
                if inc {
                    self.builder.ins().fadd(old, one)
                } else {
                    self.builder.ins().fsub(old, one)
                }
            }
            other => return unsupported!("`++`/`--` on repr {other:?}"),
        };
        self.builder.def_var(local.var, new);
        let produced = if prefix { new } else { old };
        Ok(Val::new(produced, local.repr))
    }
}
