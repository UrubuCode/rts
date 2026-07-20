//! `engine.*` — the PRIVATE engine-internal ambient global (arch/time/trace).
//!
//! `engine` re-exposes already-existing runtime functionality to the engine's OWN
//! embedded TS prelude (future `Error.ts`/`Date.ts`): `engine.arch()`,
//! `engine.now_ms()`/`now_ns()`/`unix_ms()`/`unix_ns()`, and the trace passthrough
//! (`trace_capture()`/`trace_print()`/`trace_push(...)`/`trace_pop()`) error
//! stacks need. It is NOT a value (like `Math`/`Number`), so a bare `engine`
//! identifier still bails (no value model).
//!
//! ## Privacy gate
//!
//! The engine has no import resolver — ambient globals are recognized BY NAME at
//! lowering time. `engine` is PRIVATE: only a PRELUDE-origin function (a function
//! that came from the engine's embedded TS includes — [`Lowerer::is_prelude`]) may
//! resolve it. A USER function (including the synthesized `__rtsn_main`) that names
//! `engine.*` gets an EXPLICIT `Unsupported` bail (the honest deny — never a
//! silent allow, never a wrong value). A user local/class named `engine` shadows
//! the global and is handled by the normal local/class paths before this is
//! reached.
//!
//! Each call lowers to a `call_runtime` of the matching `__RTS_FN_NS_ENGINE_*`
//! symbol, marshaled through the same Registry-style ABI path the other runtime
//! calls use: a `Handle` (string) return is boxed as a `TAG_STR` PolyValue; the
//! `I64` timestamps return a native `Int64`; the `Void` trace ops return
//! `undefined`.

use cranelift_codegen::ir::InstBuilder;
use cranelift_codegen::ir::types;
use cranelift_module::Module;

use rts_hir::HirExpr;
use rts_hir::ir::HirExprKind;

use crate::repr::Repr;
use crate::value::emit_marshal;

use crate::front::error::{FrontResult, unsupported};

use super::lower::{JsKind, Lowerer, Val};

/// How an `engine.method` lowers: the real `__RTS_FN_NS_ENGINE_*` symbol + a tag
/// describing how to rebox its result.
enum EngRet {
    /// A GC string handle → box as a `TAG_STR` PolyValue (`arch`, `trace_capture`).
    Str,
    /// A native `i64` timestamp (`now_ms`/`now_ns`/`unix_ms`/`unix_ns`).
    I64,
    /// A `void` op → `undefined` (`trace_pop`, `trace_print`).
    Void,
}

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Try to lower an `engine.m(args)` static call. Returns `Ok(None)` when
    /// `object` is not the bare `engine` global (so the caller falls through to its
    /// next handler), the explicit PRIVACY bail when a USER function names it, or
    /// the lowered call when a PRELUDE function names it.
    pub(super) fn try_engine_call(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Option<Val>> {
        let HirExprKind::Ident(name) = &object.kind else {
            return Ok(None);
        };
        if name != "engine" {
            return Ok(None);
        }
        // A local/user-class named `engine` shadows the private global — let the
        // normal paths own it (this handler only claims the bare global).
        if self.local(name).is_some() || self.classes.get(name).is_some() {
            return Ok(None);
        }
        // PRIVACY GATE: only prelude-origin code may reach the private
        // namespace. Exception: `tsa_raw` — the tagged-template DESUGAR itself
        // emits it into user-origin units (pairing cooked/raw string arrays);
        // it is a benign data-registration bridge, not a capability.
        if !self.is_prelude && method != "tsa_raw" {
            return unsupported!(
                "`engine.{method}()` is a PRIVATE engine-internal API (usable only from the engine's embedded prelude, not user code)"
            );
        }
        self.lower_engine_call(module, method, args).map(Some)
    }

    /// Lower `engine.method(args)` (already gated to a prelude-origin caller).
    fn lower_engine_call(
        &mut self,
        module: &mut dyn Module,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        // `trace_push(file, fn, line, col)` is the one member taking args; it
        // marshals two strings + two numbers. Everything else is a 0-arg op.
        if method == "trace_push" {
            return self.lower_engine_trace_push(module, args);
        }
        // `num_from_str(s)` — the string→number bridge (1 string arg → f64), used
        // by the `.ts` `NumberFactory` for the ToNumber string case. Shape differs
        // from the `num_*` formatters (1 arg in, F64 out), so it has its own arm.
        if method == "num_from_str" {
            return self.lower_engine_num_from_str(module, args);
        }
        // `fs_read_bytes(path)` / `fs_write_bytes(path, data)` / `fs_append_bytes(
        // path, data)` — the PRIVATE file-IO bridge for the `.ts` ReadStream/
        // WriteStream prelude. All-words in, a word out (a `Uint8Array` for read, a
        // byte-count number for write/append); the real `std::fs` lives in rts-node
        // `fs/streambridge.rs`. Tagged result — the `.ts` uses it as a value.
        if method == "fs_read_bytes" {
            return self.lower_engine_descriptor(
                module,
                args,
                "__RTS_FN_NS_ENGINE_FS_READ_BYTES",
                1,
                Repr::Tagged,
            );
        }
        if method == "fs_write_bytes" {
            return self.lower_engine_descriptor(
                module,
                args,
                "__RTS_FN_NS_ENGINE_FS_WRITE_BYTES",
                2,
                Repr::Tagged,
            );
        }
        if method == "fs_append_bytes" {
            return self.lower_engine_descriptor(
                module,
                args,
                "__RTS_FN_NS_ENGINE_FS_APPEND_BYTES",
                2,
                Repr::Tagged,
            );
        }
        if method == "fs_open_handle" {
            return self.lower_engine_descriptor(
                module,
                args,
                "__RTS_FN_NS_ENGINE_FS_OPEN_HANDLE",
                2,
                Repr::Tagged,
            );
        }
        // `str_from_any(x)` — the JS ToString bridge (1 any arg → string word),
        // used by the `.ts` `StringFactory` + `class String` ctor. It exposes the
        // engine's OWN ToString trampoline (`__rtsadp_to_string`, codegen-side,
        // covers number/bool/null/undefined/array/object) to the prelude. NOTE: the
        // #304 known-class-object custom `toString` lives in the front-end
        // (`globals::coerce_object_to_string_word`), NOT here — so `String(obj)`
        // keeps that path before reaching the factory.
        if method == "str_from_any" {
            return self.lower_engine_str_from_any(module, args);
        }
        // `display(x)` — render ONE value to its console display string (a string
        // PolyValue word). Wraps `__rtsadp_inspect(x, top_level=1)`, which already
        // decides ToString-vs-bracketed-inspect at RUNTIME (`is_object()` +
        // `looks_like_object`): scalars/strings → ToString, arrays → `[ … ]`,
        // objects → `{ … }`. This is the ONE display authority the `.ts` `console`
        // calls per arg — the front no longer makes the object-vs-scalar decision
        // statically (which was the old hardcoded `lower_console_log`).
        if method == "display" {
            return self.lower_engine_display(module, args);
        }
        // `print_line(s)` / `eprint_line(s)` — print ONE already-formatted string to
        // stdout / stderr (newline appended by the runtime), through the SAME
        // capture-aware `__rtsadp_print_line` trampoline the tests read. The `.ts`
        // `console` joins its args (via `display`) then calls one of these — so the
        // SINK choice (log→stdout, warn/error→stderr) is data in the `.ts`, not a
        // hardcoded per-method `if` in the front.
        if method == "print_line" {
            return self.lower_engine_print_line(module, args, false);
        }
        if method == "eprint_line" {
            return self.lower_engine_print_line(module, args, true);
        }
        // `obj_has(self, key)` — shape-aware own-property membership, used by the
        // `.ts` `class Object` (`hasOwnProperty`/`propertyIsEnumerable`). Both args
        // are PolyValue words (the object + the key string); wraps the codegen
        // `__rtsadp_obj_has` (slot-0 shape-id lookup), returns a bool.
        if method == "obj_has" {
            return self.lower_engine_obj_has(module, args);
        }
        // Property descriptors + extensibility bridges (real state — Object/Reflect
        // defineProperty/getOwnPropertyDescriptor/isExtensible/preventExtensions).
        // The `.ts` unpacks the descriptor object (value + flags) and packs the
        // flags int, then calls these flat bridges over the codegen side-tables.
        if method == "define_prop" {
            return self.lower_engine_define_prop(module, args);
        }
        if method == "prop_flags" {
            return self.lower_engine_descriptor(
                module,
                args,
                "__rtsadp_prop_flags",
                2,
                Repr::Int64,
            );
        }
        if method == "prevent_ext" {
            return self.lower_engine_descriptor(
                module,
                args,
                "__rtsadp_prevent_ext",
                1,
                Repr::Bool,
            );
        }
        if method == "is_extensible" {
            return self.lower_engine_descriptor(
                module,
                args,
                "__rtsadp_is_extensible",
                1,
                Repr::Bool,
            );
        }
        // `set_proto_check(obj, proto)` — Reflect.setPrototypeOf: success bool +
        // Proxy `setPrototypeOf` trap routing (#218 phase 3).
        if method == "set_proto_check" {
            return self.lower_engine_descriptor(
                module,
                args,
                "__rtsadp_set_proto_check",
                2,
                Repr::Bool,
            );
        }
        // Timer bridges (the `.ts` global setTimeout/… wrappers): all-words in,
        // a word out — the adapters wrapper decodes ms / converts the fn word.
        if method == "set_timeout" {
            return self.lower_engine_descriptor(
                module,
                args,
                "__rtsadp_set_timeout",
                2,
                Repr::Tagged,
            );
        }
        if method == "set_interval" {
            return self.lower_engine_descriptor(
                module,
                args,
                "__rtsadp_set_interval",
                2,
                Repr::Tagged,
            );
        }
        if method == "clear_timer" {
            return self.lower_engine_descriptor(
                module,
                args,
                "__rtsadp_clear_timer",
                1,
                Repr::Tagged,
            );
        }
        if method == "queue_microtask" {
            return self.lower_engine_descriptor(
                module,
                args,
                "__rtsadp_queue_microtask",
                1,
                Repr::Tagged,
            );
        }
        // `invoke_cb(cb, a0)` — invoca um callback guardado OPACO (word de fn,
        // handle cru ou double do handle — os listeners do DOM): a fachada
        // dispatchEvent do prelude usa para chamar os handlers registrados.
        if method == "invoke_cb" {
            return self.lower_engine_descriptor(
                module,
                args,
                "__rtsadp_invoke_cb",
                2,
                Repr::Tagged,
            );
        }
        if method == "set_immediate" {
            return self.lower_engine_descriptor(
                module,
                args,
                "__rtsadp_set_immediate",
                1,
                Repr::Tagged,
            );
        }
        // `own_keys_raw(obj)` — Reflect.ownKeys: a Proxy ownKeys trap VERBATIM
        // (no enumerability filter — that is Object.keys' job).
        if method == "own_keys_raw" {
            return self.lower_engine_descriptor(
                module,
                args,
                "__rtsadp_own_keys_raw",
                1,
                Repr::Tagged,
            );
        }
        // `construct(target, argsArray)` — Reflect.construct: new-thunk invoke
        // with args read from the array; Proxy `construct` trap-aware.
        if method == "construct" {
            return self.lower_engine_descriptor(
                module,
                args,
                "__rtsadp_construct",
                2,
                Repr::Tagged,
            );
        }
        // `get_own_desc(obj, key)` — the synthesized descriptor object (or
        // undefined), Proxy `getOwnPropertyDescriptor` trap-aware.
        if method == "get_own_desc" {
            return self.lower_engine_descriptor(
                module,
                args,
                "__rtsadp_obj_get_own_property_descriptor",
                2,
                Repr::Tagged,
            );
        }
        // The numeric-format bridge (`num_to_fixed`/`num_to_precision`/
        // `num_to_exponential`/`num_to_string_radix`): each takes (n, arg) — a
        // number receiver + an int arg — and returns a GC string handle. They wrap
        // the irreducible Rust formatters (one source of truth); the `.ts`
        // `class Number` methods call them.
        if let Some(symbol) = engine_num_member(method) {
            return self.lower_engine_num(module, symbol, args);
        }
        // The string-method bridge (`str_to_upper`/`str_trim`/`str_char_at`/
        // `str_slice`/`str_index_of`/`str_includes`/`str_pad_start`/`str_concat`/
        // `str_replace`/…): the `.ts` `class String` methods call these with `this`
        // (the boxed primitive string) + 0..2 string/number args. Each wraps the
        // irreducible Rust `__RTS_FN_GL_STRING_*` impl (one source of truth). The
        // arg/return marshaling is uniform (a small descriptor table), so adding a
        // member is a data row, never new Cranelift code.
        if let Some(spec) = engine_str_member(method) {
            return self.lower_engine_str(module, spec, args);
        }
        // `engine.tsa_raw(cooked, raw)` — pair a tagged template's cooked
        // strings array with its RAW strings array (the `.raw` property read)
        // and yield the cooked array (the tag call's first arg).
        if method == "tsa_raw" {
            if args.len() != 2 {
                return unsupported!("engine.tsa_raw expects 2 args, got {}", args.len());
            }
            let c = self.lower_expr(module, &args[0])?;
            let cw = self.box_value(c);
            let r = self.lower_expr(module, &args[1])?;
            let rw = self.box_value(r);
            let res = self
                .call_runtime(module, "__rtsadp_tsa_raw", &[cw, rw])?
                .expect("__rtsadp_tsa_raw returns the cooked word");
            return Ok(Val::tagged_kind(res, super::lower::JsKind::Array));
        }
        // WORD → WORD members (one PolyValue arg, PolyValue return): the
        // structuredClone transfer bridges (is_buffer/buffer_clone/buffer_detach).
        if let Some(symbol) = engine_word_member(method) {
            if args.len() != 1 {
                return unsupported!("engine.{method} expects 1 arg, got {}", args.len());
            }
            let v = self.lower_expr(module, &args[0])?;
            let w = self.box_value(v);
            let res = self
                .call_runtime(module, symbol, &[w])?
                .expect("engine word member returns a value");
            return Ok(Val::new(res, crate::repr::Repr::Tagged));
        }
        let Some((symbol, ret)) = engine_member(method) else {
            return unsupported!("engine.{method}(...) (unknown engine member)");
        };
        if !args.is_empty() {
            return unsupported!("engine.{method} takes no args (got {})", args.len());
        }
        let res = self.call_runtime(module, symbol, &[])?;
        Ok(self.rebox_engine_ret(module, ret, res))
    }

    /// Lower `engine.trace_push(file: string, fn: string, line: number, col: number)`
    /// — marshal the two string args (each `StrPtr` = ptr+len via the real pool)
    /// and the two numeric args, call the void extern, return `undefined`.
    fn lower_engine_trace_push(
        &mut self,
        module: &mut dyn Module,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        if args.len() != 4 {
            return unsupported!("engine.trace_push expects 4 args, got {}", args.len());
        }
        // file ptr+len, fn ptr+len: box each arg to a string PolyValue word,
        // recover the real handle (table-load), then split into the StrPtr 2-slot
        // (ptr, len) shape the void extern expects.
        let mut slots = Vec::with_capacity(6);
        for i in 0..2 {
            let v = self.lower_expr(module, &args[i])?;
            let boxed = self.box_value(v);
            let handle = emit_marshal::emit_table_load(module, self.builder, boxed);
            let (ptr, len) = emit_marshal::emit_string_ptr_len(module, self.builder, handle);
            slots.push(ptr);
            slots.push(len);
        }
        // line, col → native i64.
        for i in 2..4 {
            let v = self.lower_expr(module, &args[i])?;
            slots.push(self.coerce(v, Repr::Int64)?);
        }
        self.call_runtime(module, "__RTS_FN_NS_ENGINE_TRACE_PUSH", &slots)?;
        let undef = self.builder.ins().iconst(
            types::I64,
            crate::value::PolyValue::undefined().raw() as i64,
        );
        Ok(Val::tagged_kind(undef, JsKind::Undefined))
    }

    /// Lower an `engine.num_*(n, arg)` numeric-format call: marshal the number
    /// receiver to `F64` and the int arg to `I64`, call the wrapping extern, and
    /// box the returned string handle as a `TAG_STR` PolyValue. Both args come from
    /// the `.ts` `class Number` method (`this` + the digits/radix param); a `this`
    /// boxed-number word coerces to `F64` via the tag-selecting decode, and the
    /// (possibly-defaulted) int param coerces to `I64`.
    fn lower_engine_num(
        &mut self,
        module: &mut dyn Module,
        symbol: &'static str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        if args.len() != 2 {
            return unsupported!("engine.num_* expects 2 args (n, arg), got {}", args.len());
        }
        let n = self.lower_expr(module, &args[0])?;
        let n_f64 = self.coerce(n, Repr::Float64)?;
        let a = self.lower_expr(module, &args[1])?;
        let a_i64 = self.coerce(a, Repr::Int64)?;
        let res = self.call_runtime(module, symbol, &[n_f64, a_i64])?;
        let h = res.expect("engine.num_* returns a string handle");
        let w = emit_marshal::emit_box_real_string(module, self.builder, h);
        Ok(Val::tagged_kind(w, JsKind::Str))
    }

    /// Lower `engine.num_from_str(s)` — marshal the string arg to its real GC
    /// handle, call the Rust parser, and return the parsed f64 (Float64 repr).
    fn lower_engine_num_from_str(
        &mut self,
        module: &mut dyn Module,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        if args.len() != 1 {
            return unsupported!("engine.num_from_str expects 1 arg (s), got {}", args.len());
        }
        let s = self.lower_expr(module, &args[0])?;
        let s_word = self.box_value(s);
        let s_handle = emit_marshal::emit_table_load(module, self.builder, s_word);
        let res = self
            .call_runtime(module, "__RTS_FN_NS_ENGINE_NUM_FROM_STR", &[s_handle])?
            .expect("engine.num_from_str returns an f64");
        Ok(Val::new(res, Repr::Float64))
    }

    /// Lower `engine.obj_has(self, key)` — shape-aware own-property membership.
    /// Boxes the object + key words and calls `__rtsadp_obj_has`, returning a bool.
    /// Generic descriptor/extensibility bridge: box each arg to a PolyValue word,
    /// call `symbol`, rebox the i64 result to `ret`. For the all-word-arg bridges
    /// (`prop_flags`/`prevent_ext`/`is_extensible`). `define_prop` (mixed word+int
    /// args) has its own handler.
    fn lower_engine_descriptor(
        &mut self,
        module: &mut dyn Module,
        args: &[HirExpr],
        symbol: &str,
        arity: usize,
        ret: Repr,
    ) -> FrontResult<Val> {
        if args.len() != arity {
            return unsupported!("engine.{symbol} expects {arity} args, got {}", args.len());
        }
        let mut words = Vec::with_capacity(arity);
        for a in args {
            let v = self.lower_expr(module, a)?;
            words.push(self.box_value(v));
        }
        let res = self
            .call_runtime(module, symbol, &words)?
            .expect("engine descriptor bridge returns a value");
        Ok(Val::new(res, ret))
    }

    /// `engine.define_prop(obj, key, value, flags)` — obj/key/value are PolyValue
    /// words, `flags` is a RAW int (bit0 writable, bit1 enumerable, bit2
    /// configurable). Returns a Bool (define succeeded).
    fn lower_engine_define_prop(
        &mut self,
        module: &mut dyn Module,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        if args.len() != 4 {
            return unsupported!(
                "engine.define_prop expects 4 args (obj, key, value, flags), got {}",
                args.len()
            );
        }
        let o = self.lower_expr(module, &args[0])?;
        let o_word = self.box_value(o);
        let k = self.lower_expr(module, &args[1])?;
        let k_word = self.box_value(k);
        let v = self.lower_expr(module, &args[2])?;
        let v_word = self.box_value(v);
        let f = self.lower_expr(module, &args[3])?;
        // `flags` is a `number` from a bitwise-OR of ternaries → Tagged; `coerce`
        // (not `numeric_to_i64`, which bails on Tagged) decodes it to a raw i64.
        let f_int = self.coerce(f, Repr::Int64)?;
        let res = self
            .call_runtime(
                module,
                "__rtsadp_define_prop",
                &[o_word, k_word, v_word, f_int],
            )?
            .expect("engine.define_prop returns a bool");
        Ok(Val::new(res, Repr::Bool))
    }

    fn lower_engine_obj_has(
        &mut self,
        module: &mut dyn Module,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        if args.len() != 2 {
            return unsupported!(
                "engine.obj_has expects 2 args (self, key), got {}",
                args.len()
            );
        }
        let o = self.lower_expr(module, &args[0])?;
        let o_word = self.box_value(o);
        let k = self.lower_expr(module, &args[1])?;
        let k_word = self.box_value(k);
        let res = self
            .call_runtime(module, "__rtsadp_obj_has", &[o_word, k_word])?
            .expect("engine.obj_has returns a bool");
        Ok(Val::new(res, Repr::Bool))
    }

    /// Lower `engine.str_from_any(x)` — JS ToString of an arbitrary value. Boxes
    /// the arg and calls the engine's `__rtsadp_to_string` trampoline (which itself
    /// returns a `TAG_STR` PolyValue word), tagging the result `Str`.
    fn lower_engine_str_from_any(
        &mut self,
        module: &mut dyn Module,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        if args.len() != 1 {
            return unsupported!("engine.str_from_any expects 1 arg (x), got {}", args.len());
        }
        let v = self.lower_expr(module, &args[0])?;
        let w = self.box_value(v);
        let res = self
            .call_runtime(module, "__rtsadp_to_string", &[w])?
            .expect("engine.str_from_any returns a string word");
        Ok(Val::tagged_kind(res, JsKind::Str))
    }

    /// Lower `engine.display(x)` — render one value to its console display string
    /// (a `TAG_STR` PolyValue word) via `__rtsadp_inspect(x, top_level=1)`. The
    /// inspect trampoline decides ToString-vs-bracketed-inspect at RUNTIME, so the
    /// `.ts` `console` does not need the front's static object-vs-scalar split.
    fn lower_engine_display(
        &mut self,
        module: &mut dyn Module,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        if args.len() != 1 {
            return unsupported!("engine.display expects 1 arg (x), got {}", args.len());
        }
        let v = self.lower_expr(module, &args[0])?;
        let w = self.box_value(v);
        let top = self.builder.ins().iconst(types::I64, 1);
        let res = self
            .call_runtime(module, "__rtsadp_inspect", &[w, top])?
            .expect("engine.display returns a string word");
        Ok(Val::tagged_kind(res, JsKind::Str))
    }

    /// Lower `engine.print_line(s)` / `engine.eprint_line(s)` — print one already-
    /// formatted string `s` to stdout (`to_stderr=false`) or stderr (`true`), via
    /// the capture-aware `__rtsadp_print_line` (the same sink the tests read).
    /// Returns `undefined`.
    fn lower_engine_print_line(
        &mut self,
        module: &mut dyn Module,
        args: &[HirExpr],
        to_stderr: bool,
    ) -> FrontResult<Val> {
        if args.len() != 1 {
            return unsupported!("engine.print_line expects 1 arg (s), got {}", args.len());
        }
        let v = self.lower_expr(module, &args[0])?;
        let line = self.box_value(v);
        emit_marshal::emit_print_string_poly(module, self.builder, line, to_stderr);
        let undef = self.builder.ins().iconst(
            types::I64,
            crate::value::PolyValue::undefined().raw() as i64,
        );
        Ok(Val::tagged_kind(undef, JsKind::Undefined))
    }

    /// Lower an `engine.str_*(s, ...args)` string-method call: marshal the string
    /// receiver `s` (a boxed primitive-string word) to its real GC handle, marshal
    /// each declared arg (a string → real handle, a number → i64), call the
    /// wrapping `__RTS_FN_GL_STRING_*` extern, and rebox the result per its return
    /// kind (string handle → `TAG_STR` PolyValue; number → `Int64`; bool → `Bool`).
    /// The first `.ts` arg is always `this` (the primitive string); the rest are the
    /// method's own params (already defaulted by the `.ts` prologue).
    fn lower_engine_str(
        &mut self,
        module: &mut dyn Module,
        spec: EngineStr,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        let want = 1 + spec.args.len();
        if args.len() != want {
            return unsupported!(
                "{} expects {} args (s, ...), got {}",
                spec.symbol,
                want,
                args.len()
            );
        }
        let mut call_args = Vec::with_capacity(want);
        // ---- receiver string `s` → real GC handle ----
        let s = self.lower_expr(module, &args[0])?;
        let s_word = self.box_value(s);
        let s_handle = emit_marshal::emit_table_load(module, self.builder, s_word);
        call_args.push(s_handle);
        // ---- explicit args per the descriptor ----
        for (a, &kind) in args[1..].iter().zip(spec.args) {
            let v = self.lower_expr(module, a)?;
            match kind {
                StrArg::Str => {
                    let w = self.box_value(v);
                    let h = emit_marshal::emit_table_load(module, self.builder, w);
                    call_args.push(h);
                }
                StrArg::Num => {
                    // A number index/count → i64. `numeric_to_i64` accepts a proven
                    // Int32/Int64/Float64 (a `number` param / default like the
                    // `slice` "to end" sentinel arrives as Float64) and truncates
                    // toward zero; a Tagged number is decoded via `coerce`.
                    let i = match v.repr {
                        Repr::Int32 | Repr::Int64 | Repr::Float64 => self.numeric_to_i64(v)?,
                        _ => self.coerce(v, Repr::Int64)?,
                    };
                    call_args.push(i);
                }
            }
        }
        let res = self.call_runtime(module, spec.symbol, &call_args)?;
        Ok(match spec.ret {
            StrRet::Str => {
                let h = res.expect("engine.str_* string member returns a handle");
                let w = emit_marshal::emit_box_real_string(module, self.builder, h);
                Val::tagged_kind(w, JsKind::Str)
            }
            StrRet::Num => Val::new(
                res.expect("engine.str_* number member returns a value"),
                Repr::Int64,
            ),
            StrRet::Bool => Val::new(
                res.expect("engine.str_* bool member returns a value"),
                Repr::Bool,
            ),
        })
    }

    /// Rebox a 0-arg `engine.*` call's result per its declared return kind.
    fn rebox_engine_ret(
        &mut self,
        module: &mut dyn Module,
        ret: EngRet,
        res: Option<cranelift_codegen::ir::Value>,
    ) -> Val {
        match ret {
            EngRet::Str => {
                let h = res.expect("engine string member returns a handle");
                let w = emit_marshal::emit_box_real_string(module, self.builder, h);
                Val::tagged_kind(w, JsKind::Str)
            }
            EngRet::I64 => Val::new(res.expect("engine i64 member returns a value"), Repr::Int64),
            EngRet::Void => {
                let undef = self.builder.ins().iconst(
                    types::I64,
                    crate::value::PolyValue::undefined().raw() as i64,
                );
                Val::tagged_kind(undef, JsKind::Undefined)
            }
        }
    }
}

/// Resolve a 0-arg `engine.method` name to its real symbol + return kind.
/// `trace_push` is handled separately (it takes args).
/// WORD->WORD engine members (1 PolyValue arg, PolyValue ret).
fn engine_word_member(method: &str) -> Option<&'static str> {
    Some(match method {
        "is_buffer" => "__RTS_FN_NS_ENGINE_IS_BUFFER",
        "buffer_clone" => "__RTS_FN_NS_ENGINE_BUFFER_CLONE",
        "buffer_detach" => "__RTS_FN_NS_ENGINE_BUFFER_DETACH",
        _ => return None,
    })
}

fn engine_member(method: &str) -> Option<(&'static str, EngRet)> {
    Some(match method {
        "arch" => ("__RTS_FN_NS_ENGINE_ARCH", EngRet::Str),
        "now_ms" => ("__RTS_FN_NS_ENGINE_NOW_MS", EngRet::I64),
        "now_ns" => ("__RTS_FN_NS_ENGINE_NOW_NS", EngRet::I64),
        "unix_ms" => ("__RTS_FN_NS_ENGINE_UNIX_MS", EngRet::I64),
        "unix_ns" => ("__RTS_FN_NS_ENGINE_UNIX_NS", EngRet::I64),
        "trace_capture" => ("__RTS_FN_NS_ENGINE_TRACE_CAPTURE", EngRet::Str),
        "trace_pop" => ("__RTS_FN_NS_ENGINE_TRACE_POP", EngRet::Void),
        "trace_print" => ("__RTS_FN_NS_ENGINE_TRACE_PRINT", EngRet::Void),
        _ => return None,
    })
}

/// Resolve an `engine.num_*` numeric-format member name to its real symbol. Each
/// takes `(n: number, arg: number) => string` and wraps a `__RTS_FN_GL_NUMBER_*`
/// formatter (the irreducible-format bridge for the `.ts` `class Number`).
fn engine_num_member(method: &str) -> Option<&'static str> {
    Some(match method {
        "num_to_string_radix" => "__RTS_FN_NS_ENGINE_NUM_TO_STRING_RADIX",
        "num_to_fixed" => "__RTS_FN_NS_ENGINE_NUM_TO_FIXED",
        "num_to_precision" => "__RTS_FN_NS_ENGINE_NUM_TO_PRECISION",
        "num_to_exponential" => "__RTS_FN_NS_ENGINE_NUM_TO_EXPONENTIAL",
        _ => return None,
    })
}

/// How an `engine.str_*` arg (after the receiver `s`) is marshaled.
#[derive(Clone, Copy)]
enum StrArg {
    /// A string arg → boxed to a string word, table-loaded to a real GC handle.
    Str,
    /// A number arg → coerced to i64 (index/count/length).
    Num,
}

/// How an `engine.str_*` result is reboxed.
#[derive(Clone, Copy)]
enum StrRet {
    /// A GC string handle → `TAG_STR` PolyValue.
    Str,
    /// A native i64 number (index/code unit).
    Num,
    /// A boolean (extern "C" i64 0/1).
    Bool,
}

/// The marshaling descriptor of an `engine.str_*` member: the real
/// `__RTS_FN_GL_STRING_*` symbol it wraps + how its (post-receiver) args and its
/// result marshal. The receiver `s` is always the first arg (a string handle).
#[derive(Clone, Copy)]
struct EngineStr {
    symbol: &'static str,
    args: &'static [StrArg],
    ret: StrRet,
}

/// Resolve an `engine.str_*` string-method member name to its marshaling
/// descriptor. Each wraps a `__RTS_FN_GL_STRING_*` Rust impl (the irreducible
/// Unicode-aware logic — one source of truth) the `.ts` `class String` bodies call.
fn engine_str_member(method: &str) -> Option<EngineStr> {
    use StrArg::{Num, Str};
    let row = |symbol, args: &'static [StrArg], ret| EngineStr { symbol, args, ret };
    Some(match method {
        "str_to_upper" => row("__RTS_FN_NS_ENGINE_STR_TO_UPPER", &[], StrRet::Str),
        "str_to_lower" => row("__RTS_FN_NS_ENGINE_STR_TO_LOWER", &[], StrRet::Str),
        "str_trim" => row("__RTS_FN_NS_ENGINE_STR_TRIM", &[], StrRet::Str),
        "str_trim_start" => row("__RTS_FN_NS_ENGINE_STR_TRIM_START", &[], StrRet::Str),
        "str_trim_end" => row("__RTS_FN_NS_ENGINE_STR_TRIM_END", &[], StrRet::Str),
        "str_char_at" => row("__RTS_FN_NS_ENGINE_STR_CHAR_AT", &[Num], StrRet::Str),
        "str_char_code_at" => row("__RTS_FN_NS_ENGINE_STR_CHAR_CODE_AT", &[Num], StrRet::Num),
        "str_at" => row("__RTS_FN_NS_ENGINE_STR_AT", &[Num], StrRet::Str),
        "str_repeat" => row("__RTS_FN_NS_ENGINE_STR_REPEAT", &[Num], StrRet::Str),
        "str_slice" => row("__RTS_FN_NS_ENGINE_STR_SLICE", &[Num, Num], StrRet::Str),
        "str_substring" => row("__RTS_FN_NS_ENGINE_STR_SUBSTRING", &[Num, Num], StrRet::Str),
        "str_index_of" => row("__RTS_FN_NS_ENGINE_STR_INDEX_OF", &[Str], StrRet::Num),
        "str_last_index_of" => row("__RTS_FN_NS_ENGINE_STR_LAST_INDEX_OF", &[Str], StrRet::Num),
        "str_includes" => row("__RTS_FN_NS_ENGINE_STR_INCLUDES", &[Str], StrRet::Bool),
        "str_starts_with" => row("__RTS_FN_NS_ENGINE_STR_STARTS_WITH", &[Str], StrRet::Bool),
        "str_ends_with" => row("__RTS_FN_NS_ENGINE_STR_ENDS_WITH", &[Str], StrRet::Bool),
        "str_pad_start" => row("__RTS_FN_NS_ENGINE_STR_PAD_START", &[Num, Str], StrRet::Str),
        "str_pad_end" => row("__RTS_FN_NS_ENGINE_STR_PAD_END", &[Num, Str], StrRet::Str),
        "str_concat" => row("__RTS_FN_NS_ENGINE_STR_CONCAT", &[Str], StrRet::Str),
        "str_replace" => row("__RTS_FN_NS_ENGINE_STR_REPLACE", &[Str, Str], StrRet::Str),
        "str_replace_all" => row(
            "__RTS_FN_NS_ENGINE_STR_REPLACE_ALL",
            &[Str, Str],
            StrRet::Str,
        ),
        "str_normalize" => row("__RTS_FN_NS_ENGINE_STR_NORMALIZE", &[Str], StrRet::Str),
        "str_is_well_formed" => row("__RTS_FN_NS_ENGINE_STR_IS_WELL_FORMED", &[], StrRet::Bool),
        "str_to_well_formed" => row("__RTS_FN_NS_ENGINE_STR_TO_WELL_FORMED", &[], StrRet::Str),
        _ => return None,
    })
}
