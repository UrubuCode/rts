//! The ONE generic marshal-from-`AbiType` path (Pilar 6) — calls a REAL runtime
//! `__RTS_FN_*` symbol resolved through the Registry, with NO per-class wrapper.
//!
//! Given a [`ResolvedCall`] (symbol + `AbiType` signature, straight from the real
//! [`rts_engine::Registry`]) plus the receiver value + the explicit arg values,
//! this:
//!
//! 1. marshals the receiver (always a `Handle` for an instance method) from its
//!    `TAG_OBJECT` PolyValue word to the real runtime handle (`POLY_TO_HANDLE`);
//! 2. marshals each explicit arg from its `Val` to the parameter's `AbiType`
//!    (`Handle` → real handle; `StrPtr` → `(ptr,len)` two slots; `F64`/`I64`/`I32`/
//!    `U64`/`Bool` → the unboxed scalar via the coercion authority);
//! 3. builds the Cranelift signature DIRECTLY from those `AbiType`s
//!    ([`abi_sig::cranelift_sig_from_abis`]) and emits the `call`;
//! 4. reboxes the result per the return `AbiType` (+ a caller kind hint for the
//!    `Handle`-returns-string-vs-object ambiguity) back into a PolyValue [`Val`].
//!
//! This single function REPLACES the per-class `__rtsadp_*` trampolines for every
//! method whose ABI is scalar/handle/string (Date getters & string methods, Error
//! props & toString, the wrapper ctors, RegExp scalar getters, …). The trampolines
//! that genuinely OWN a representation the runtime ABI can't express (Map/Set
//! string-key marshaling, array-result building) keep their codegen-native path.

use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;

use rts_engine::abi::AbiType;

use crate::repr::Repr;
use crate::value;

use crate::front::error::{FrontResult, unsupported};

use super::lower::{JsKind, Lowerer, Val};
use super::registry::ResolvedCall;

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Marshal one already-lowered [`Val`] to the Cranelift slot(s) the parameter
    /// `AbiType` expects, pushing onto `out`. `Handle` and `StrPtr` both require a
    /// real string/object handle behind the value; an arg that is not the proven
    /// kind those need BAILS (never a silent mis-marshal).
    fn marshal_reg_arg(
        &mut self,
        module: &mut dyn Module,
        ty: AbiType,
        val: Val,
        out: &mut Vec<Value>,
    ) -> FrontResult<()> {
        match ty {
            AbiType::Handle => {
                // A spec-DEFAULTED omitted Handle arg (DefaultArg::Undefined on a
                // Handle slot — an optional options-bag like `new TextDecoder(l,
                // opts?)`): the ABSENT handle `0`, never a table-load of the
                // `undefined` word (whose payload 0 would read a bogus slot).
                if matches!(val.kind, JsKind::Undefined) {
                    out.push(self.builder.ins().iconst(types::I64, 0));
                    return Ok(());
                }
                // Two kinds of value reach a `Handle` param:
                //  * an OPAQUE RESOURCE handle — a raw `u64` id riding as an INTEGER
                //    PolyValue (`audio`/`buffer`/`net` results, TS `number`). It is
                //    NOT a GC handle, so pass the integer VERBATIM (a table-load on
                //    it would dereference a bogus slot and SIGILL).
                //  * a HEAP value (string/object) — table-load its real GC handle.
                if matches!(val.kind, JsKind::Number)
                    || matches!(val.repr, Repr::Int64 | Repr::Int32)
                {
                    out.push(self.coerce(val, Repr::Int64)?);
                } else {
                    let word = self.box_value(val);
                    out.push(value::emit_marshal::emit_table_load(
                        module,
                        self.builder,
                        word,
                    ));
                }
            }
            AbiType::StrPtr => {
                // A spec-DEFAULTED omitted string arg (DefaultArg::Undefined on a
                // StrPtr slot - `new URLSearchParams()`): the EMPTY string, not
                // the "undefined" text ToString would render.
                if matches!(val.kind, JsKind::Undefined) {
                    let w = self.emit_str_const_word(module, "")?;
                    let handle = value::emit_marshal::emit_table_load(module, self.builder, w);
                    let (ptr, len) =
                        value::emit_marshal::emit_string_ptr_len(module, self.builder, handle);
                    out.push(ptr);
                    out.push(len);
                    return Ok(());
                }
                // A StrPtr param needs a real string handle behind the value. A
                // PROVEN string rides its word directly; a Tagged/typed value whose
                // kind is lost at a function boundary (a `string`-typed param, an
                // `any`) is an UNTRUSTED boundary — coerce it through the ToString
                // authority (Pilar 3) so it marshals correctly (ToString of a string
                // is itself; of a number/bool/etc. the JS string form). This is what
                // lets the `rts:test` bundle pass `name`/`expected` params (Tagged) to
                // `test_core.*`/`string.*` without a mis-marshal or a false bail.
                // Even a `JsKind::Str` value is NOT trusted here: a `string`-typed
                // field/param can hold a non-string smuggled through `any` (the TS
                // annotation is an untrusted boundary, Pilar 3). `__rtsadp_to_string`
                // is the identity for a real string word (no re-intern), so routing
                // EVERY value through it is sound and cheap — trusting the kind was
                // the bug where `expect(bool)` printed a garbage interned string.
                let w = self.box_value(val);
                let word = self
                    .call_runtime(module, "__rtsadp_to_string", &[w])?
                    .expect("__rtsadp_to_string returns a string word");
                let handle = value::emit_marshal::emit_table_load(module, self.builder, word);
                let (ptr, len) =
                    value::emit_marshal::emit_string_ptr_len(module, self.builder, handle);
                out.push(ptr);
                out.push(len);
            }
            AbiType::F64 => out.push(self.coerce(val, Repr::Float64)?),
            AbiType::I64 | AbiType::U64 | AbiType::I32 => {
                // Marshal numeric args through the int coercion (ToInt-style); the
                // Cranelift slot is i64 for I64/U64/Handle and i32 for I32.
                //
                // A Tagged arg NOT proven number dispatches on its runtime tag
                // instead (`__rtsadp_word_to_abi_i64`): a heap value yields its real
                // handle id (the collections/promise namespaces take handles in
                // `U64` params — e.g. `vec_len(await Promise.all(v))`), a number its
                // truncation. The blind Tagged→Int saturating convert read garbage
                // from an OBJECT word.
                let i = if matches!(ty, AbiType::U64) && matches!(val.repr, Repr::Float64) {
                    // An f64 arg into an OPAQUE `U64` slot rides as its BITS —
                    // the historical `thread.spawn(fp, 3.14)` convention (#247):
                    // the typed invoke reads the bits back via the FN_KINDS
                    // registry. Truncating (fcvt) here destroyed the fraction
                    // before the worker ever ran.
                    self.builder.ins().bitcast(
                        types::I64,
                        cranelift_codegen::ir::MemFlags::new(),
                        val.v,
                    )
                } else if matches!(val.repr, Repr::Tagged) && !matches!(val.kind, JsKind::Number) {
                    let w = self.box_value(val);
                    self.call_runtime(module, "__rtsadp_word_to_abi_i64", &[w])?
                        .expect("__rtsadp_word_to_abi_i64 returns a value")
                } else {
                    self.coerce(val, Repr::Int64)?
                };
                if matches!(ty, AbiType::I32) {
                    out.push(self.builder.ins().ireduce(types::I32, i));
                } else {
                    out.push(i);
                }
            }
            AbiType::Bool => {
                // A proven bool rides its i64 0/1; anything else BAILS (ToBoolean
                // of an arbitrary value at a typed boundary is a later increment).
                out.push(self.coerce(val, Repr::Bool)?);
            }
            AbiType::PolyValue => {
                // Raw NaN-boxed word, verbatim — a function-VALUE listener / any
                // value the backend stores + invokes via the callback bridge keeps
                // its tag (no coerce, no table-load).
                out.push(self.box_value(val));
            }
            AbiType::Void => return unsupported!("Void is not a valid argument type"),
        }
        Ok(())
    }

    /// Rebox a runtime call's raw Cranelift result into a PolyValue [`Val`], per
    /// the return `AbiType`. For a `Handle` return the JS kind is ambiguous
    /// (string vs object), so the caller passes the expected `kind`:
    /// `JsKind::Str` → box as `TAG_STR`; otherwise box as `TAG_OBJECT`.
    fn rebox_result(
        &mut self,
        module: &mut dyn Module,
        ret: AbiType,
        kind: JsKind,
        res: Option<Value>,
    ) -> Val {
        match ret {
            AbiType::Void => {
                let undef = value::PolyValue::undefined().raw() as i64;
                let v = self.builder.ins().iconst(types::I64, undef);
                Val::tagged_kind(v, JsKind::Undefined)
            }
            AbiType::Handle => {
                let h = res.expect("Handle return has a value");
                match kind {
                    // A HEAP STRING handle (`gc.string_*`, `string.*`): intern as
                    // a `TAG_STR` PolyValue.
                    JsKind::Str => {
                        let w = value::emit_marshal::emit_box_real_string(module, self.builder, h);
                        Val::tagged_kind(w, JsKind::Str)
                    }
                    // An OPAQUE RESOURCE handle (`audio.*`, `buffer.alloc`,
                    // `net.*` — a raw `u64` id from the namespace's own table, TS
                    // type `number`): it is NOT a GC handle, so it rides as a plain
                    // INTEGER PolyValue. Boxing it as an object word would NaN-box a
                    // raw id as a heap pointer and SIGILL on the next table-load.
                    JsKind::Number => Val::new(h, Repr::Int64),
                    // A FRESH JS ARRAY handle (`ts_signature` return `T[]`, e.g.
                    // `fs.readdir(): string[]`): the namespace built this Vec for
                    // this call, so the heterogeneous authority may normalize its
                    // legacy raw string-handle elements to words in place.
                    JsKind::Array => {
                        let w = self
                            .call_runtime(module, "__rtsadp_box_handle_auto", &[h])
                            .expect("box_handle_auto is registered")
                            .expect("__rtsadp_box_handle_auto returns a word");
                        // `Val::new` derives its kind from `Repr` alone (Tagged ⇒
                        // Unknown) — it would silently drop the very `Array`
                        // provenness this arm exists to establish. Every downstream
                        // consumer of this call's result (a chained `.length`/
                        // `Buffer.from(result)`/`is_array_valued` on a bound local)
                        // needs the `Val` itself to carry the proven kind, not just
                        // the box to look array-shaped at runtime.
                        Val::tagged_kind(w, JsKind::Array)
                    }
                    // Any other expectation (the construct/Date path) is a real
                    // object handle. NOT routed through `__rtsadp_box_handle_auto`:
                    // a Registry-class INSTANCE may itself be an `Entry::Vec` whose
                    // raw-handle layout belongs to its Rust backend (URLSearchParams
                    // stores raw pair handles) — normalizing it in place corrupts
                    // the backend's own reads. Only the row paths whose results are
                    // FRESH JS arrays (`match`/`matchAll`) normalize.
                    _ => {
                        let w = self.box_object_handle(module, h);
                        Val::tagged_kind(w, JsKind::Object)
                    }
                }
            }
            AbiType::F64 => Val::new(res.expect("F64 return"), Repr::Float64),
            AbiType::I64 | AbiType::U64 => Val::new(res.expect("int return"), Repr::Int64),
            AbiType::I32 => {
                let v = self
                    .builder
                    .ins()
                    .sextend(types::I64, res.expect("i32 return"));
                Val::new(v, Repr::Int64)
            }
            AbiType::Bool => Val::new(res.expect("bool return"), Repr::Bool),
            AbiType::PolyValue => {
                // The runtime returned a raw PolyValue word verbatim — keep it Tagged.
                Val::new(res.expect("PolyValue return"), Repr::Tagged)
            }
            AbiType::StrPtr => {
                // No runtime symbol RETURNS a StrPtr (it is not `is_returnable`);
                // a string result always comes back as a `Handle`.
                unreachable!("StrPtr is not a valid return type");
            }
        }
    }

    /// Box a real runtime object handle as a `TAG_OBJECT` PolyValue word (the
    /// instance representation: `POLY_FROM_HANDLE` to the bare 48-bit slot, then
    /// `BOX_BASE | (TAG_OBJECT<<48) | slot`). Mirrors `emit_box_real_string` for
    /// objects.
    fn box_object_handle(&mut self, module: &mut dyn Module, real_handle: Value) -> Value {
        let payload48 = value::emit_marshal::emit_call(
            module,
            self.builder,
            "__RTS_FN_NS_GC_POLY_FROM_HANDLE",
            &[real_handle],
        )
        .expect("POLY_FROM_HANDLE returns a value");
        let header = value::encode(value::TAG_OBJECT, 0) as i64;
        let mask = self
            .builder
            .ins()
            .iconst(types::I64, value::PAYLOAD_MASK as i64);
        let payload = self.builder.ins().band(payload48, mask);
        let header_v = self.builder.ins().iconst(types::I64, header);
        let obj = self.builder.ins().bor(payload, header_v);
        // A `0` real handle is ABSENT → `null` (a nullable `): object` return such
        // as `Dir.readSync()` at end-of-directory), never a bogus payload-0 object.
        let is_absent = self.builder.ins().icmp_imm(
            cranelift_codegen::ir::condcodes::IntCC::Equal,
            real_handle,
            0,
        );
        let null_w = self
            .builder
            .ins()
            .iconst(types::I64, value::PolyValue::null().raw() as i64);
        self.builder.ins().select(is_absent, null_w, obj)
    }

    /// Emit `lhs instanceof <class>` via the Registry's real `instanceof_predicate`
    /// symbol (`fn(handle)->i64`). The predicate dereferences a real handle, so it
    /// is only called when `word` is a `TAG_OBJECT`; a non-object operand yields
    /// `false` without touching the handle table. Result: a `Bool` [`Val`].
    pub(super) fn emit_registry_instanceof(
        &mut self,
        module: &mut dyn Module,
        word: Value,
        predicate: &str,
    ) -> FrontResult<Val> {
        // is_object = (word & BOX_BASE)==BOX_BASE && tag(word)==TAG_OBJECT.
        let boxed = value::emit_is_boxed(self.builder, word);
        let tag_shift = value::TAG_SHIFT as i64;
        let tag_mask = value::TAG_MASK as i64;
        let shifted = self.builder.ins().ushr_imm(word, tag_shift);
        let tag = self.builder.ins().band_imm(shifted, tag_mask);
        let want = self
            .builder
            .ins()
            .iconst(types::I64, value::TAG_OBJECT as i64);
        let is_obj_tag =
            self.builder
                .ins()
                .icmp(cranelift_codegen::ir::condcodes::IntCC::Equal, tag, want);
        let is_object = self.builder.ins().band(boxed, is_obj_tag);

        // handle = POLY_TO_HANDLE(word & PAYLOAD_MASK). Computing this for a
        // non-object word is harmless (it is gated by the select below — the
        // predicate is only meaningful when is_object, and POLY_TO_HANDLE just
        // reconstructs a slot index; we still guard the RESULT, not the call, but
        // the runtime predicates tolerate a bogus handle by returning 0).
        let handle = value::emit_marshal::emit_table_load(module, self.builder, word);
        let pred = value::emit_marshal::emit_call_sig(
            module,
            self.builder,
            predicate,
            &[handle],
            &[AbiType::Handle],
            AbiType::Bool,
        )
        .expect("instanceof predicate returns a value");
        // false (0) when not an object; else the predicate's i64 0/1.
        let zero = self.builder.ins().iconst(types::I64, 0);
        let result = self.builder.ins().select(is_object, pred, zero);
        Ok(Val::new(result, Repr::Bool))
    }

    /// Emit a call to a Registry-resolved runtime symbol, given the already-lowered
    /// receiver `Val` (or `None` for a ctor/static) and the explicit arg `Val`s.
    /// `result_kind` disambiguates a `Handle` return (string vs object). This is
    /// the SINGLE generic path; the lowering hands it a [`ResolvedCall`] from the
    /// Registry and never names a per-method symbol.
    pub(super) fn emit_registry_call(
        &mut self,
        module: &mut dyn Module,
        call: &ResolvedCall,
        recv: Option<Val>,
        args: &[Val],
        result_kind: JsKind,
    ) -> FrontResult<Val> {
        if args.len() != call.arg_abis.len() {
            return unsupported!(
                "registry call `{}` expects {} args, got {}",
                call.symbol,
                call.arg_abis.len(),
                args.len()
            );
        }
        // Build the full param AbiType list (receiver first, if any) for the
        // Cranelift signature; marshal the matching values in the same order.
        let mut params: Vec<AbiType> = Vec::with_capacity(args.len() + 1);
        let mut slots: Vec<Value> = Vec::new();
        if let Some(rabi) = call.recv_abi {
            let rv = recv.expect("instance call needs a receiver");
            params.push(rabi);
            self.marshal_reg_arg(module, rabi, rv, &mut slots)?;
        }
        for (&abi, &val) in call.arg_abis.iter().zip(args.iter()) {
            params.push(abi);
            self.marshal_reg_arg(module, abi, val, &mut slots)?;
        }

        let res = value::emit_marshal::emit_call_sig(
            module,
            self.builder,
            call.symbol,
            &slots,
            &params,
            call.ret,
        );
        // A THROWS-flagged member may have set the pending-error slot: emit the
        // same post-call check user calls get, so `try/catch` routes the unwind
        // (data on the spec — the engine names no class here).
        if call.flags.contains(rts_engine::MemberFlags::THROWS) {
            self.emit_post_call_error_check(module)?;
        }
        // NULLABLE string return (`: string | null`): a `0` handle is ABSENT → rebox
        // as `null`, not the empty string (`URLSearchParams.get(missing)` etc.). An
        // empty `""` interns to a NON-zero handle, so `0` is unambiguously absent.
        if call.ret == AbiType::Handle
            && matches!(result_kind, JsKind::Str)
            && call.ret_is_nullable_string
        {
            let h = res.expect("Handle return has a value");
            let is_absent =
                self.builder
                    .ins()
                    .icmp_imm(cranelift_codegen::ir::condcodes::IntCC::Equal, h, 0);
            let null_w = self
                .builder
                .ins()
                .iconst(types::I64, value::PolyValue::null().raw() as i64);
            let str_w = value::emit_marshal::emit_box_real_string(module, self.builder, h);
            let w = self.builder.ins().select(is_absent, null_w, str_w);
            return Ok(Val::new(w, crate::repr::Repr::Tagged));
        }
        Ok(self.rebox_result(module, call.ret, result_kind, res))
    }
}
