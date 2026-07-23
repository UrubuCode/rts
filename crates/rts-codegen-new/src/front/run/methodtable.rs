//! PROTO-METHOD prologue emission (main only) — reify every class method as a
//! this-first function WORD and store it as an own property of the class's
//! SHARED prototype object. The dynamic property read (`__rtsadp_obj_get`)
//! already walks the prototype chain on an own-slot miss, so a class-instance
//! method becomes readable as a VALUE from a Tagged receiver (`typeof v.m ===
//! "function"`, borrowed methods, the prelude's JSON `toJSON` hook) — exactly
//! the JS prototypal model, with zero risk of cross-class leakage (the proto is
//! per-class; a plain `{x: 1}` literal sharing a class's field SHAPE never sees
//! its methods).

use cranelift_codegen::ir::{InstBuilder, types};
use cranelift_module::Module;

use crate::value;

use crate::front::error::FrontResult;

use super::lower::Lowerer;

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// For every user class: `proto = __rtsadp_class_proto_init(name, parent)`
    /// (idempotent — the same call `new` makes), then one
    /// `__rtsadp_obj_set(proto, name, reified-method-word)` per method. Runs
    /// once, at the `__rtsn_main` prologue (JIT and AOT alike — `func_addr` is
    /// relocatable). An async method (no reifiable thunk) is skipped: the
    /// dynamic read keeps its honest `undefined` for those.
    pub(super) fn emit_method_table_registrations(
        &mut self,
        module: &mut dyn Module,
    ) -> FrontResult<()> {
        // Collect first — the emission below re-borrows `self` mutably.
        let mut rows: Vec<(String, Option<String>, Vec<(String, String, i64)>)> = Vec::new();
        for desc in self.classes.descs() {
            let mut methods: Vec<(String, String, i64)> = Vec::new();
            for (mname, fname) in &desc.methods {
                let Some(sig) = self.sigs.get(fname.as_str()) else {
                    continue;
                };
                if sig.is_async {
                    continue;
                }
                if !self.thunks.contains_key(fname) {
                    continue;
                }
                methods.push((mname.clone(), fname.clone(), sig.params.len() as i64));
            }
            // ACCESSORS on the proto too, under the `__get_<key>`/`__set_<key>`
            // convention the dynamic paths already dispatch (`__rtsadp_obj_get`
            // invokes a chain `__get_x` on own-slot miss; `__rtsadp_obj_set`
            // mirrors with `__set_x`) — so a TAGGED receiver (an `any` local, a
            // `new Function` body, a union-typed return) reaches class getters/
            // setters exactly like literal/defineProperty accessors. The STATIC
            // known-class dispatch is untouched (it resolves at compile time and
            // never consults the proto).
            for (aname, acc) in &desc.accessors {
                for (fname, key) in [
                    (acc.getter.as_ref(), format!("__get_{aname}")),
                    (acc.setter.as_ref(), format!("__set_{aname}")),
                ] {
                    let Some(fname) = fname else { continue };
                    let Some(sig) = self.sigs.get(fname.as_str()) else {
                        continue;
                    };
                    if sig.is_async || !self.thunks.contains_key(fname) {
                        continue;
                    }
                    methods.push((key, fname.clone(), sig.params.len() as i64));
                }
            }
            if !methods.is_empty() {
                rows.push((desc.name.clone(), desc.parent.clone(), methods));
            }
        }
        for (class, parent, methods) in rows {
            let k_word = self.emit_str_const_word(module, &class)?;
            let parent_word = match &parent {
                Some(p) => self.emit_str_const_word(module, p)?,
                None => self
                    .builder
                    .ins()
                    .iconst(types::I64, value::PolyValue::undefined().raw() as i64),
            };
            let proto = self
                .call_runtime(module, "__rtsadp_class_proto_init", &[k_word, parent_word])?
                .expect("__rtsadp_class_proto_init returns a word");
            for (mname, fname, nparams) in methods {
                let thunk_id = *self.thunks.get(&fname).expect("thunk presence checked");
                let func_ref = self.func_ref(module, thunk_id);
                let addr = self.builder.ins().func_addr(types::I64, func_ref);
                let np_v = self.builder.ins().iconst(types::I64, nparams);
                let zero = self.builder.ins().iconst(types::I64, 0);
                let payload = self
                    .call_runtime(module, "__rtsadp_fn_reify_this", &[addr, np_v, zero, zero])?
                    .expect("__rtsadp_fn_reify_this returns a payload");
                // Box the bare 48-bit payload as a TAG_FUNCTION PolyValue word.
                let header = value::encode(value::TAG_FUNCTION, 0) as i64;
                let mask = self
                    .builder
                    .ins()
                    .iconst(types::I64, value::PAYLOAD_MASK as i64);
                let masked = self.builder.ins().band(payload, mask);
                let header_v = self.builder.ins().iconst(types::I64, header);
                let word = self.builder.ins().bor(masked, header_v);
                let name_w = self.emit_str_const_word(module, &mname)?;
                self.call_runtime(module, "__rtsadp_proto_set_method", &[proto, name_w, word])?;
            }
        }
        Ok(())
    }
}
