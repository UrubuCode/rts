//! Real-symbol signature descriptors — the fix for the StrPtr=2-slots bug.
//!
//! The fake mini-runtime treated EVERY slot as `i64` (correct only for the
//! PolyValue-in/out `__rtsn_*` symbols). The REAL symbols use the runtime's
//! `AbiType` (`Void`/`Bool`/`I32`/`I64`/`U64`/`F64`/`StrPtr`/`Handle`), and
//! `StrPtr` lowers to **two** Cranelift slots (`ptr` + `len`). Mis-marshaling a
//! single-slot string where the ABI expects two → SIGILL.
//!
//! This module hand-writes a small static table covering EXACTLY the symbols the
//! new lowering calls, each as `&[AbiType]` params + an `AbiType` return, and
//! lowers each to a Cranelift `Signature` (expanding `StrPtr` per the runtime's
//! own [`rts_runtime::abi::signature::lower_params`] rule). It deliberately does
//! NOT iterate the whole `SPECS`/registry — the new engine emits a tiny, known
//! surface, and an explicit table is the smallest honest source of truth.

use cranelift_codegen::ir::{AbiParam, Signature, types};
use cranelift_module::Module;

use rts_runtime::abi::AbiType;

/// The ABI shape of one callable symbol: its param `AbiType`s (pre-expansion,
/// `StrPtr` still one entry) and its return `AbiType`.
#[derive(Clone, Copy)]
pub struct SymSig {
    pub params: &'static [AbiType],
    pub ret: AbiType,
}

/// The Cranelift IR type a scalar `AbiType` lowers to. `StrPtr` is NEVER passed
/// here (it is expanded into two `I64` entries before this is reached); `Void` is
/// only legal as a return and contributes no slot.
fn scalar_to_cl(ty: AbiType) -> types::Type {
    match ty {
        AbiType::F64 => types::F64,
        AbiType::I32 => types::I32,
        // Bool/I64/U64/Handle/PolyValue all ride an i64 register at the boundary.
        AbiType::Bool | AbiType::I64 | AbiType::U64 | AbiType::Handle | AbiType::PolyValue => {
            types::I64
        }
        AbiType::StrPtr => unreachable!("StrPtr must be expanded before scalar_to_cl"),
        AbiType::Void => unreachable!("Void has no Cranelift slot"),
    }
}

impl SymSig {
    /// Build the Cranelift `Signature` for this symbol under the module's default
    /// call convention, expanding each `StrPtr` param into `(ptr: i64, len: i64)`.
    pub fn to_cranelift(&self, module: &dyn Module) -> Signature {
        let mut sig = Signature::new(module.isa().default_call_conv());
        for &p in self.params {
            match p {
                AbiType::StrPtr => {
                    // ptr + len, two slots (matches the runtime's lower_params).
                    sig.params.push(AbiParam::new(types::I64));
                    sig.params.push(AbiParam::new(types::I64));
                }
                AbiType::Void => panic!("Void is not a valid parameter type"),
                other => sig.params.push(AbiParam::new(scalar_to_cl(other))),
            }
        }
        if !matches!(self.ret, AbiType::Void) {
            sig.returns.push(AbiParam::new(scalar_to_cl(self.ret)));
        }
        sig
    }

    /// Number of Cranelift param slots (StrPtr counts as 2). The lowering uses
    /// this to assert it passes the right number of marshaled values.
    pub fn param_slot_count(&self) -> usize {
        self.params
            .iter()
            .map(|p| if matches!(p, AbiType::StrPtr) { 2 } else { 1 })
            .sum()
    }

    /// Whether this symbol returns a value.
    pub fn returns(&self) -> bool {
        !matches!(self.ret, AbiType::Void)
    }
}

/// Build a Cranelift `Signature` directly from a parameter `AbiType` slice + a
/// return `AbiType`, under the module's default call convention, expanding each
/// `StrPtr` into `(ptr: i64, len: i64)`. This is the Registry-driven path (Pilar
/// 6): the signature comes from the real `Member.sig` (`AbiType`s), NOT from the
/// hand-written [`sig_of`] table — so a runtime/Registry class method is called
/// with the EXACT ABI the runtime published, with no codegen-side metadata.
pub fn cranelift_sig_from_abis(module: &dyn Module, params: &[AbiType], ret: AbiType) -> Signature {
    let mut sig = Signature::new(module.isa().default_call_conv());
    for &p in params {
        match p {
            AbiType::StrPtr => {
                sig.params.push(AbiParam::new(types::I64));
                sig.params.push(AbiParam::new(types::I64));
            }
            AbiType::Void => panic!("Void is not a valid parameter type"),
            other => sig.params.push(AbiParam::new(scalar_to_cl(other))),
        }
    }
    if !matches!(ret, AbiType::Void) {
        sig.returns.push(AbiParam::new(scalar_to_cl(ret)));
    }
    sig
}

/// Resolve a symbol name to its [`SymSig`]. Covers exactly the symbols the new
/// lowering calls: the REAL runtime symbols (`__RTS_FN_*`) + the codegen-owned
/// adapter trampolines (`__rtsadp_*`). `None` for an unknown symbol (the lowering
/// turns that into an explicit `Unsupported` bail, never a guess).
pub fn sig_of(name: &str) -> Option<SymSig> {
    use AbiType::*;
    Some(match name {
        // ---- REAL string pool (rts-std collector::string_pool) ----
        // STRING_NEW(ptr,len) -> handle  — StrPtr = two slots.
        // module-level mutable global cells (epic #195): GET(id)->word, SET(id,word).
        "__RTS_FN_NS_GC_GCELL_GET" => SymSig {
            params: &[U64],
            ret: U64,
        },
        "__RTS_FN_NS_GC_GCELL_SET" => SymSig {
            params: &[U64, U64],
            ret: Void,
        },
        "__RTS_FN_NS_GC_STRING_NEW" | "__RTS_FN_NS_GC_STRING_FROM_STATIC" => SymSig {
            params: &[StrPtr],
            ret: Handle,
        },
        "__RTS_FN_NS_GC_STRING_PTR" => SymSig {
            params: &[Handle],
            ret: U64,
        },
        "__RTS_FN_NS_GC_STRING_LEN" => SymSig {
            params: &[Handle],
            ret: I64,
        },
        "__RTS_FN_NS_GC_STRING_FREE" => SymSig {
            params: &[Handle],
            ret: I64,
        },
        "__RTS_FN_NS_GC_STRING_CONCAT" => SymSig {
            params: &[Handle, Handle],
            ret: Handle,
        },
        "__RTS_FN_NS_GC_STRING_EQ" => SymSig {
            params: &[Handle, Handle],
            ret: I64,
        },
        "__RTS_FN_NS_GC_STRING_CMP" => SymSig {
            params: &[Handle, Handle],
            ret: I64,
        },
        "__RTS_FN_NS_GC_STRING_FROM_I64" => SymSig {
            params: &[I64],
            ret: Handle,
        },
        "__RTS_FN_NS_GC_STRING_FROM_F64" => SymSig {
            params: &[F64],
            ret: Handle,
        },

        // ---- REAL io (rts-std io) ----
        // IO_PRINT(ptr,len) -> void  — StrPtr = two slots, appends a newline.
        "__RTS_FN_NS_IO_PRINT" | "__RTS_FN_NS_IO_EPRINT" => SymSig {
            params: &[StrPtr],
            ret: Void,
        },

        // ---- PRIVATE engine namespace (rts-std engine) — arch/time/trace ----
        // arch / trace_capture return a GC string handle; the timestamp + trace
        // fns are I64/Void. trace_push takes (file: StrPtr, fn: StrPtr, line, col).
        "__RTS_FN_NS_ENGINE_ARCH" | "__RTS_FN_NS_ENGINE_TRACE_CAPTURE" => SymSig {
            params: &[],
            ret: Handle,
        },
        "__RTS_FN_NS_ENGINE_NOW_MS"
        | "__RTS_FN_NS_ENGINE_NOW_NS"
        | "__RTS_FN_NS_ENGINE_UNIX_MS"
        | "__RTS_FN_NS_ENGINE_UNIX_NS" => SymSig {
            params: &[],
            ret: I64,
        },
        "__RTS_FN_NS_ENGINE_TRACE_POP" | "__RTS_FN_NS_ENGINE_TRACE_PRINT" => SymSig {
            params: &[],
            ret: Void,
        },
        "__RTS_FN_NS_ENGINE_TRACE_PUSH" => SymSig {
            params: &[StrPtr, StrPtr, I64, I64],
            ret: Void,
        },
        // engine.num_* — the irreducible numeric FORMATTING bridge: (n: F64,
        // arg: I64) -> a GC string handle. Each wraps a `__RTS_FN_GL_NUMBER_*`
        // formatter; the `.ts` `class Number` methods call these.
        "__RTS_FN_NS_ENGINE_NUM_TO_STRING_RADIX"
        | "__RTS_FN_NS_ENGINE_NUM_TO_FIXED"
        | "__RTS_FN_NS_ENGINE_NUM_TO_PRECISION"
        | "__RTS_FN_NS_ENGINE_NUM_TO_EXPONENTIAL" => SymSig {
            params: &[F64, I64],
            ret: Handle,
        },
        // engine.num_from_str — string→number parse bridge: (s: Handle) -> F64.
        // Wraps `__RTS_FN_GL_NUMBER_FROM_STR`; the `.ts` `NumberFactory` calls it.
        "__RTS_FN_NS_ENGINE_NUM_FROM_STR" => SymSig {
            params: &[Handle],
            ret: F64,
        },
        // engine.str_* — the irreducible Unicode string-logic bridge: the receiver
        // `s` + string args are GC string `Handle`s, indices/counts are `I64`. Each
        // wraps a `__RTS_FN_GL_STRING_*` impl; the `.ts` `class String` methods call
        // these. Grouped by `(params, ret)` shape.
        // (s) -> string.
        "__RTS_FN_NS_ENGINE_STR_TO_UPPER"
        | "__RTS_FN_NS_ENGINE_STR_TO_LOWER"
        | "__RTS_FN_NS_ENGINE_STR_TRIM"
        | "__RTS_FN_NS_ENGINE_STR_TRIM_START"
        | "__RTS_FN_NS_ENGINE_STR_TRIM_END"
        | "__RTS_FN_NS_ENGINE_STR_TO_WELL_FORMED" => SymSig {
            params: &[Handle],
            ret: Handle,
        },
        // (s) -> boolean.
        "__RTS_FN_NS_ENGINE_STR_IS_WELL_FORMED" => SymSig {
            params: &[Handle],
            ret: Bool,
        },
        // (s, i: number) -> string.
        "__RTS_FN_NS_ENGINE_STR_CHAR_AT"
        | "__RTS_FN_NS_ENGINE_STR_AT"
        | "__RTS_FN_NS_ENGINE_STR_REPEAT" => SymSig {
            params: &[Handle, I64],
            ret: Handle,
        },
        // (s, i: number) -> number.
        "__RTS_FN_NS_ENGINE_STR_CHAR_CODE_AT" => SymSig {
            params: &[Handle, I64],
            ret: I64,
        },
        // (s, start: number, end: number) -> string.
        "__RTS_FN_NS_ENGINE_STR_SLICE" | "__RTS_FN_NS_ENGINE_STR_SUBSTRING" => SymSig {
            params: &[Handle, I64, I64],
            ret: Handle,
        },
        // (s, needle: string) -> number.
        "__RTS_FN_NS_ENGINE_STR_INDEX_OF" | "__RTS_FN_NS_ENGINE_STR_LAST_INDEX_OF" => SymSig {
            params: &[Handle, Handle],
            ret: I64,
        },
        // (s, needle: string) -> boolean.
        "__RTS_FN_NS_ENGINE_STR_INCLUDES"
        | "__RTS_FN_NS_ENGINE_STR_STARTS_WITH"
        | "__RTS_FN_NS_ENGINE_STR_ENDS_WITH" => SymSig {
            params: &[Handle, Handle],
            ret: Bool,
        },
        // (s, targetLen: number, pad: string) -> string.
        "__RTS_FN_NS_ENGINE_STR_PAD_START" | "__RTS_FN_NS_ENGINE_STR_PAD_END" => SymSig {
            params: &[Handle, I64, Handle],
            ret: Handle,
        },
        // (s, other/form: string) -> string.
        "__RTS_FN_NS_ENGINE_STR_CONCAT" | "__RTS_FN_NS_ENGINE_STR_NORMALIZE" => SymSig {
            params: &[Handle, Handle],
            ret: Handle,
        },
        // (s, from: string, to: string) -> string.
        "__RTS_FN_NS_ENGINE_STR_REPLACE" | "__RTS_FN_NS_ENGINE_STR_REPLACE_ALL" => SymSig {
            params: &[Handle, Handle, Handle],
            ret: Handle,
        },

        // `promise.create(fn_addr, args_vec)` — the async-fn CALL spawn
        // (front/run/asyncspawn.rs): runs the body on the shared runtime,
        // returns the pending PromiseAsync handle.
        "__RTS_FN_NS_PROMISE_CREATE" => SymSig {
            params: &[U64, U64],
            ret: U64,
        },
        // The typed async spawn: registers the callee's packed param/ret kinds
        // (an f64 param travels as bits and reads back into xmm), then
        // delegates to `promise.create`.
        "__rtsadp_promise_spawn" => SymSig {
            params: &[U64, U64, U64, U64],
            ret: U64,
        },
        // Register a user fn's packed ABI for raw-pointer consumers
        // (`getPointer` emission — thread.spawn f64 workers, #247).
        "__rtsadp_register_fn_abi" => SymSig {
            params: &[U64, U64, U64],
            ret: Void,
        },
        // Counted-args fn-value invoke through INVOKE_AUTO (variadic packing;
        // the singleton-override dispatch).
        "__rtsadp_invoke_auto_word" => SymSig {
            params: &[U64, U64, U64],
            ret: U64,
        },
        // `new Function(p…, body)` — args vec of PolyValue words → TAG_FUNCTION
        // word (`undefined` on a failed compile).
        "__rtsadp_fn_new" => SymSig {
            params: &[U64],
            ret: U64,
        },
        // `f.bind(thisArg, …partial)` — clone the fn value; a THIS-FIRST fn
        // binds thisArg into bound_args[0], partial args append after.
        "__rtsadp_string_raw" => SymSig {
            params: &[U64, U64],
            ret: U64,
        },
        "__rtsadp_tsa_raw" => SymSig {
            params: &[U64, U64],
            ret: U64,
        },
        "__rtsadp_dyn_to_string_radix" => SymSig {
            params: &[U64, U64],
            ret: U64,
        },
        "__rtsadp_fn_bind" => SymSig {
            params: &[U64, U64, U64],
            ret: U64,
        },

        // ---- REAL collections Vec (rts-shared collections::vec) ----
        "__RTS_FN_NS_COLLECTIONS_VEC_NEW" => SymSig {
            params: &[],
            ret: Handle,
        },
        "__RTS_FN_NS_COLLECTIONS_VEC_PUSH" => SymSig {
            params: &[U64, I64],
            ret: Void,
        },
        "__RTS_FN_NS_COLLECTIONS_VEC_GET" => SymSig {
            params: &[U64, I64],
            ret: I64,
        },
        "__RTS_FN_NS_COLLECTIONS_VEC_LEN" => SymSig {
            params: &[U64],
            ret: I64,
        },
        "__RTS_FN_NS_COLLECTIONS_VEC_SET" => SymSig {
            params: &[U64, I64, I64],
            ret: Void,
        },
        "__RTS_FN_NS_COLLECTIONS_VEC_POP" => SymSig {
            params: &[U64],
            ret: I64,
        },

        // ---- REAL PolyValue <-> handle bridge (rts-engine heap::handles) ----
        // FROM_HANDLE: full real handle -> bare 48-bit slot+shard payload.
        // TO_HANDLE: 48-bit payload -> full real handle (gen reconstructed from
        // the live slot). These REPLACE the old `__rtsadp_store/_load` table.
        "__RTS_FN_NS_GC_POLY_FROM_HANDLE" | "__RTS_FN_NS_GC_POLY_TO_HANDLE" => SymSig {
            params: &[U64],
            ret: U64,
        },
        // ---- codegen-owned adapter trampolines (__rtsadp_*) ----
        // Generic JS operators on PolyValue words (tagged-in/tagged-out).
        "__rtsadp_add"
        | "__rtsadp_strict_eq"
        | "__rtsadp_strict_neq"
        | "__rtsadp_loose_eq"
        | "__rtsadp_loose_neq"
        | "__rtsadp_same_value" => SymSig {
            params: &[U64, U64],
            ret: U64,
        },
        "__rtsadp_typeof" | "__rtsadp_to_string" | "__rtsadp_to_boolean"
        | "__rtsadp_await" | "__rtsadp_word_to_abi_i64" | "__rtsadp_box_handle_auto"
        | "__rtsadp_class_proto"
        | "__RTS_FN_NS_ENGINE_IS_BUFFER"
        | "__RTS_FN_NS_ENGINE_BUFFER_CLONE"
        | "__RTS_FN_NS_ENGINE_BUFFER_DETACH" => SymSig {
            params: &[U64],
            ret: U64,
        },
        // ---- generic arithmetic/comparison/bitwise (P4.8): two PolyValue words ----
        "__rtsadp_sub" | "__rtsadp_mul" | "__rtsadp_div" | "__rtsadp_mod" | "__rtsadp_pow"
        | "__rtsadp_lt" | "__rtsadp_le" | "__rtsadp_gt" | "__rtsadp_ge" | "__rtsadp_band"
        | "__rtsadp_bor" | "__rtsadp_bxor" | "__rtsadp_shl" | "__rtsadp_shr" | "__rtsadp_ushr" => {
            SymSig {
                params: &[U64, U64],
                ret: U64,
            }
        }
        // ---- generic unary (P4.8 + P5.6): one PolyValue word ----
        "__rtsadp_neg" | "__rtsadp_bnot" | "__rtsadp_not" | "__rtsadp_pos" => SymSig {
            params: &[U64],
            ret: U64,
        },
        // console.* line sink: (ptr, len) StrPtr (two slots) + a to_stderr flag
        // (I64: 0 = stdout/IO_PRINT, !=0 = stderr/IO_EPRINT), void.
        "__rtsadp_print_line" => SymSig {
            params: &[StrPtr, I64],
            ret: Void,
        },
        // inspect(value_word, top_level) -> string PolyValue word. value is a raw
        // PolyValue word (U64); top_level is an I64 flag (1 = direct arg / bare
        // string, 0 = nested / quoted string); the result is a string PolyValue.
        "__rtsadp_inspect" => SymSig {
            params: &[U64, I64],
            ret: U64,
        },
        // inspect_object(value_word, top_level) -> string PolyValue word. Same ABI
        // as `__rtsadp_inspect` but renders the `TAG_OBJECT` word as an OBJECT
        // (`{ k: v }`), recovering keys from the slot-0 global shape-id. The
        // lowering calls this for a statically-OBJECT console.log arg (P3.6).
        "__rtsadp_inspect_object" => SymSig {
            params: &[U64, I64],
            ret: U64,
        },

        // ---- codegen-owned FUNCTION-value trampolines (__rtsadp_fn_*, P4.6) ----
        // reify(addr, nparams, has_rest, env_word) -> 48-bit slot+shard payload
        // (U64). env_word is the captured-env PolyValue word (0 = non-capturing).
        "__rtsadp_fn_reify" | "__rtsadp_fn_reify_this" => SymSig {
            params: &[U64, U64, U64, U64],
            ret: U64,
        },
        // invoke(fn_word, a0, a1, a2, a3, rest) -> result PolyValue word (U64).
        // All slots are raw PolyValue words (U64); the fixed uniform call ABI.
        "__rtsadp_fn_invoke" | "__rtsadp_fn_invoke_method" | "__rtsadp_pack_rest" => SymSig {
            params: &[U64, U64, U64, U64, U64, U64],
            ret: U64,
        },
        // ---- new <value>() through a class VALUE (slice 2) ----
        // register_ctor_thunk(addr) -> void: mark a class NEW-THUNK address valid.
        "__rtsadp_register_ctor_thunk" => SymSig {
            params: &[U64],
            ret: Void,
        },
        // new_invoke(fn_word, a0, a1, a2, a3, rest) -> instance PolyValue word (U64);
        // throws (pending error) when the value is not a registered constructor.
        "__rtsadp_new_invoke" => SymSig {
            params: &[U64, U64, U64, U64, U64, U64],
            ret: U64,
        },
        // ---- function-as-constructor side-table (new F() / x instanceof F) ----
        // fn_ptr(fn_word) -> ctor identity (U64, 0 = not a fn).
        "__rtsadp_fn_ptr" => SymSig {
            params: &[U64],
            ret: U64,
        },
        // ctor_mark(obj_word, fn_ptr) -> void (record instance→ctor identity).
        "__rtsadp_ctor_mark" => SymSig {
            params: &[U64, U64],
            ret: Void,
        },
        // instanceof_fn(obj_word, fn_ptr) -> bool PolyValue word (U64).
        "__rtsadp_instanceof_fn" => SymSig {
            params: &[U64, U64],
            ret: U64,
        },
        // ---- function-VALUE data properties (`F.foo = v` / `F.foo`) (Phase 4) ----
        // get_prop(fn_word, key_word) -> stored value PolyValue word, or undefined.
        // set_prop(fn_word, key_word, value_word) -> value_word. key_word is an
        // interned string PolyValue (the static property name), like __rtsadp_obj_get.
        "__rtsadp_fn_get_prop" => SymSig {
            params: &[U64, U64],
            ret: U64,
        },
        "__rtsadp_fn_set_prop" | "__rtsadp_arr_ta_set" | "__rtsadp_arr_subarray" => SymSig {
            params: &[U64, U64, U64],
            ret: U64,
        },

        // ---- codegen-owned Array trampolines (__rtsadp_arr_*, P4.5) ----
        // All slots are u64/i64 (no StrPtr): slot 0 is the array's REAL Vec handle
        // (`POLY_TO_HANDLE` of the array word); needle args are raw PolyValue words
        // (U64); index/range args are I64; results are PolyValue words (U64) / a
        // string Handle (join) / an i64 (index_of/push) / a bool (includes).
        "__rtsadp_arr_index_of" => SymSig {
            params: &[U64, U64],
            ret: I64,
        },
        "__rtsadp_arr_includes" => SymSig {
            params: &[U64, U64],
            ret: Bool,
        },
        "__rtsadp_arr_at" => SymSig {
            params: &[U64, I64],
            ret: U64,
        },
        "__rtsadp_arr_join" => SymSig {
            params: &[U64, Handle],
            ret: Handle,
        },
        "__rtsadp_arr_join0" => SymSig {
            params: &[U64],
            ret: Handle,
        },
        "__rtsadp_arr_slice0" => SymSig {
            params: &[U64],
            ret: U64,
        },
        "__rtsadp_arr_to_string" => SymSig {
            params: &[U64],
            ret: Handle,
        },
        "__rtsadp_arr_push" => SymSig {
            params: &[U64, U64],
            ret: I64,
        },
        "__rtsadp_arr_set_length" => SymSig {
            params: &[U64, U64],
            ret: U64,
        },
        "__rtsadp_arr_pop" => SymSig {
            params: &[U64],
            ret: U64,
        },
        "__rtsadp_arr_slice" => SymSig {
            params: &[U64, I64, I64],
            ret: U64,
        },
        "__rtsadp_arr_last_index_of" => SymSig {
            params: &[U64, U64],
            ret: I64,
        },
        "__rtsadp_arr_reverse"
        | "__rtsadp_arr_flat"
        | "__rtsadp_arr_shift"
        | "__rtsadp_arr_to_reversed"
        | "__rtsadp_arr_sort"
        | "__rtsadp_arr_to_sorted"
        | "__rtsadp_arr_entries"
        | "__rtsadp_arr_keys"
        | "__rtsadp_arr_values" => SymSig {
            params: &[U64],
            ret: U64,
        },
        // arity-variant + ES2023 Array trampolines (recv U64 + the explicit args)
        "__rtsadp_arr_slice1" | "__rtsadp_arr_flat_depth" => SymSig {
            params: &[U64, I64],
            ret: U64,
        },
        "__rtsadp_arr_to_spliced" | "__rtsadp_arr_copy_within2" | "__rtsadp_arr_fill2" => SymSig {
            params: &[U64, I64, I64],
            ret: U64,
        },
        "__rtsadp_arr_copy_within" | "__rtsadp_arr_fill3" => SymSig {
            params: &[U64, I64, I64, I64],
            ret: U64,
        },
        "__rtsadp_arr_copy_within1" => SymSig {
            params: &[U64, I64],
            ret: U64,
        },
        "__rtsadp_arr_index_of_from" | "__rtsadp_arr_last_index_of_from" => SymSig {
            params: &[U64, U64, I64],
            ret: I64,
        },
        "__rtsadp_arr_includes_from" => SymSig {
            params: &[U64, U64, I64],
            ret: Bool,
        },
        "__rtsadp_arr_with" => SymSig {
            params: &[U64, I64, U64],
            ret: U64,
        },
        "__rtsadp_arr_fill"
        | "__rtsadp_arr_concat"
        | "__rtsadp_arr_ta_set1"
        | "__rtsadp_arr_subarray1"
        | "__rtsadp_arr_splice"
        | "__rtsadp_arr_to_spliced_var" => SymSig {
            params: &[U64, U64],
            ret: U64,
        },
        "__rtsadp_arr_unshift" => SymSig {
            params: &[U64, U64],
            ret: I64,
        },

        // ---- codegen-owned GLOBAL / static trampolines (P5.2) ----
        // Coercions + predicates: one PolyValue word in, one PolyValue word out.
        "__rtsadp_g_number"
        | "__rtsadp_g_string"
        | "__rtsadp_g_boolean"
        | "__rtsadp_g_parse_float"
        | "__rtsadp_g_is_nan"
        | "__rtsadp_g_is_finite"
        | "__rtsadp_num_is_nan"
        | "__rtsadp_num_is_finite"
        | "__rtsadp_num_is_integer"
        | "__rtsadp_num_is_safe_integer"
        | "__rtsadp_arr_is_array"
        | "__rtsadp_arr_new_sized"
        | "__rtsadp_arr_from"
        | "__rtsadp_str_from_char_code"
        | "__rtsadp_str_from_code_point"
        | "__rtsadp_str_from_char_code_arr" => SymSig {
            params: &[U64],
            ret: U64,
        },
        // parseInt(value_word, radix): radix is an I64 (0 = auto).
        "__rtsadp_g_parse_int" => SymSig {
            params: &[U64, U64],
            ret: U64,
        },
        // str.split(recvHandle, sepHandle, limit) -> a TAG_OBJECT array word.
        "__rtsadp_str_split" => SymSig {
            params: &[Handle, Handle, I64],
            ret: U64,
        },
        // math_reduce(arr_word, op) -> f64 (op: 0=min, 1=max, 2=hypot).
        "__rtsadp_math_reduce" => SymSig {
            params: &[U64, I64],
            ret: F64,
        },
        // canon_double(word) -> a guaranteed inline-double PolyValue word.
        "__rtsadp_canon_double" => SymSig {
            params: &[U64],
            ret: U64,
        },
        // arr_spread_append(dst_word, src_word) -> void (push all src elements).
        "__rtsadp_arr_spread_append" => SymSig {
            params: &[U64, U64],
            ret: Void,
        },

        // ---- codegen-owned ITERATION-source trampolines (P5.10) ----
        // str_chars(str_word) -> TAG_OBJECT array word of one-char strings;
        // obj_keys(obj_word) -> TAG_OBJECT array word of key strings. Both take one
        // PolyValue word and return a fresh array PolyValue word.
        // SYNC generator primitives (eager MVP): all u64/i64 (vec handle / value).
        "__RTS_FN_NS_GC_GENERATOR_SET_RET"
        | "__RTS_FN_NS_GC_GENERATOR_NEXT_SENT"
        | "__RTS_FN_NS_GC_GENERATOR_RETURN"
        | "__RTS_FN_NS_GC_GENERATOR_THROW" => SymSig {
            params: &[U64, U64],
            ret: U64,
        },
        "__RTS_FN_NS_GC_GENERATOR_NEXT" => SymSig {
            params: &[U64],
            ret: U64,
        },
        // LAZY state-machine primitives. `h`/`it` = raw GenState/iterator handle;
        // value args/returns = PolyValue words; state/idx args = raw ints. (ABI: all
        // i64.) Void-returning ones write through `h` (FSET/SETSTATE/ENTER_TRY/…).
        "__RTS_FN_NS_GC_GEN_SM_STATE"
        | "__RTS_FN_NS_GC_GEN_SM_SENT"
        | "__RTS_FN_NS_GC_GEN_SM_NEXT"
        | "__RTS_FN_NS_GC_GEN_SM_DRAIN"
        | "__RTS_FN_NS_GC_GEN_SM_CAUGHT"
        | "__RTS_FN_NS_GC_GEN_SM_END_FINALLY"
        | "__RTS_FN_NS_GC_GEN_DELEGATE_START"
        | "__RTS_FN_NS_GC_GEN_DELEGATE_NEXT"
        | "__RTS_FN_NS_GC_GEN_DELEGATE_DONE" => SymSig {
            params: &[U64],
            ret: U64,
        },
        "__RTS_FN_NS_GC_GEN_SM_NEW"
        | "__RTS_FN_NS_GC_GEN_SM_FGET"
        | "__RTS_FN_NS_GC_GEN_SM_YIELD"
        | "__RTS_FN_NS_GC_GEN_SM_DONE" => SymSig {
            params: &[U64, U64],
            ret: U64,
        },
        "__RTS_FN_NS_GC_GEN_SM_FSET" => SymSig {
            params: &[U64, U64, U64],
            ret: Void,
        },
        "__RTS_FN_NS_GC_GEN_SM_SETSTATE"
        | "__RTS_FN_NS_GC_GEN_SM_ENTER_TRY"
        | "__RTS_FN_NS_GC_GEN_SM_ENTER_TRY_CATCH" => SymSig {
            params: &[U64, U64],
            ret: Void,
        },
        "__RTS_FN_NS_GC_GEN_SM_EXIT_TRY_CATCH" => SymSig {
            params: &[U64],
            ret: Void,
        },
        "__RTS_FN_NS_GC_GENERATOR_GET_RET"
        | "__RTS_FN_NS_GC_ITER_VALUE"
        | "__RTS_FN_NS_GC_ITER_DONE" => SymSig {
            params: &[U64],
            ret: U64,
        },
        // string→string GLOBAL fns (encodeURIComponent/btoa/…): one StrPtr arg
        // (ptr+len) → a string Handle.
        "__RTS_FN_GL_ENCODE_URI"
        | "__RTS_FN_GL_DECODE_URI"
        | "__RTS_FN_GL_ENCODE_URI_COMPONENT"
        | "__RTS_FN_GL_DECODE_URI_COMPONENT"
        | "__RTS_FN_GL_TEXTENC_BTOA"
        | "__RTS_FN_GL_TEXTENC_ATOB" => SymSig {
            params: &[StrPtr],
            ret: Handle,
        },
        "__rtsadp_str_chars"
        | "__rtsadp_obj_keys"
        | "__rtsadp_own_keys_raw"
        | "__rtsadp_obj_own_names"
        | "__rtsadp_obj_own_symbols"
        | "__rtsadp_to_iter_array" => SymSig {
            params: &[U64],
            ret: U64,
        },

        // ---- codegen-owned Map / Set instance trampolines (P5.3) ----
        // All PolyValue words (U64): slot 0 is the instance word, args are key/value/
        // element words, results are PolyValue words (instance / value / bool / size).
        "__rtsadp_map_new" | "__rtsadp_set_new" => SymSig {
            params: &[],
            ret: U64,
        },
        "__rtsadp_map_set" => SymSig {
            params: &[U64, U64, U64],
            ret: U64,
        },
        "__rtsadp_map_get"
        | "__rtsadp_map_has"
        | "__rtsadp_map_delete"
        | "__rtsadp_set_add"
        | "__rtsadp_set_has"
        | "__rtsadp_set_delete" => SymSig {
            params: &[U64, U64],
            ret: U64,
        },
        "__rtsadp_map_clear" | "__rtsadp_map_size" | "__rtsadp_set_clear" | "__rtsadp_set_size" => {
            SymSig {
                params: &[U64],
                ret: U64,
            }
        }

        // ---- codegen-owned pending-error slot for throw / try-catch (P5.13) ----
        // throw_set(word) -> void; err_pending() -> i64 (0/1); err_take() -> U64
        // (the thrown PolyValue word); err_clear() -> void.
        "__rtsadp_throw_set" => SymSig {
            params: &[U64],
            ret: Void,
        },
        "__rtsadp_err_pending" => SymSig {
            params: &[],
            ret: I64,
        },
        "__rtsadp_err_take" => SymSig {
            params: &[],
            ret: U64,
        },
        "__rtsadp_err_clear" => SymSig {
            params: &[],
            ret: Void,
        },

        // ---- codegen-owned DYNAMIC method dispatch (P5.9) ----
        // Uniform PolyValue-word ABI: the receiver word + 0..2 PolyValue-word args,
        // one PolyValue-word result. The trampoline branches on the receiver's tag
        // at runtime and delegates to the per-class op; an unexpected tag returns
        // the `undefined` word (sound — never a wrong value).
        // 1-word (receiver only): toString/length/pop/toUpper/toLower/trim.
        "__rtsadp_dyn_to_string"
        | "__rtsadp_dyn_length"
        | "__rtsadp_dyn_pop"
        | "__rtsadp_dyn_reverse"
        | "__rtsadp_dyn_sort"
        | "__rtsadp_dyn_to_upper_case"
        | "__rtsadp_dyn_to_lower_case"
        | "__rtsadp_dyn_trim" => SymSig {
            params: &[U64],
            ret: U64,
        },
        // 2-word (receiver + 1 arg): indexOf/includes/at/concat/join/push/charAt/
        // charCodeAt/split/startsWith/endsWith/repeat.
        "__rtsadp_dyn_index_of"
        | "__rtsadp_dyn_includes"
        | "__rtsadp_dyn_at"
        | "__rtsadp_idx_get"
        | "__rtsadp_dyn_concat"
        | "__rtsadp_dyn_join"
        | "__rtsadp_dyn_push"
        | "__rtsadp_dyn_char_at"
        | "__rtsadp_dyn_char_code_at"
        | "__rtsadp_dyn_split"
        | "__rtsadp_dyn_starts_with"
        | "__rtsadp_dyn_ends_with"
        | "__rtsadp_dyn_repeat" => SymSig {
            params: &[U64, U64],
            ret: U64,
        },
        // 3-word (receiver + 2 args): slice(start, end).
        "__rtsadp_dyn_slice" => SymSig {
            params: &[U64, U64, U64],
            ret: U64,
        },

        // ---- codegen-owned RegExp + string-regex-method trampolines (P5.12) ----
        // All slots are raw PolyValue words (U64): compile takes (pattern, flags)
        // string words → a RegExp instance word; test takes (re, subject) → a bool
        // word; source/flags/global take the re word → a string/bool word; the
        // string-regex methods take (subject, re[, repl]) words → string/array/
        // number/null words. The trampolines do the string-handle marshaling.
        "__rtsadp_re_compile" => SymSig {
            params: &[U64, U64],
            ret: U64,
        },
        "__rtsadp_re_test" | "__rtsadp_re_exec" => SymSig {
            params: &[U64, U64],
            ret: U64,
        },
        "__rtsadp_re_source"
        | "__rtsadp_re_flags"
        | "__rtsadp_re_global"
        | "__rtsadp_re_ignore_case"
        | "__rtsadp_re_multiline"
        | "__rtsadp_re_last_index" => SymSig {
            params: &[U64],
            ret: U64,
        },
        "__rtsadp_re_str_match" | "__rtsadp_re_str_split" | "__rtsadp_re_str_search" => SymSig {
            params: &[U64, U64],
            ret: U64,
        },
        "__rtsadp_re_str_replace" | "__rtsadp_re_str_replace_all" => SymSig {
            params: &[U64, U64, U64],
            ret: U64,
        },

        // ---- codegen-owned DYNAMIC property access (P5.5) ----
        // obj_get(obj_word, key_str_handle) -> PolyValue word; obj_set adds a
        // value-word param and returns the value word; obj_has returns a Bool
        // (i64 0/1). The key is a string PolyValue word (U64) — a literal interned
        // at lowering or a computed key's ToString.
        // The `globalThis` singleton object word (no args; returns a TAG_OBJECT
        // PolyValue). `globalThis.prop` get/set then reuse __rtsadp_obj_get/_set.
        "__rtsadp_globalthis" => SymSig {
            params: &[],
            ret: U64,
        },
        "__rtsadp_obj_get" => SymSig {
            params: &[U64, U64],
            ret: U64,
        },
        "__rtsadp_obj_set" => SymSig {
            params: &[U64, U64, U64],
            ret: U64,
        },
        "__rtsadp_obj_assign" => SymSig {
            params: &[U64, U64],
            ret: U64,
        },
        "__rtsadp_obj_define_property" => SymSig {
            params: &[U64, U64, U64],
            ret: U64,
        },
        "__rtsadp_obj_get_own_property_descriptor" => SymSig {
            params: &[U64, U64],
            ret: U64,
        },
        "__rtsadp_obj_get_own_property_descriptors" => SymSig {
            params: &[U64],
            ret: U64,
        },
        "__rtsadp_obj_has" => SymSig {
            params: &[U64, U64],
            ret: Bool,
        },
        // Prototype chain (Object.create): create(proto)->obj, proto_of(obj)->proto,
        // is_prototype_of(proto, obj)->bool word. All raw PolyValue words.
        "__rtsadp_obj_create" | "__rtsadp_obj_proto_of" => SymSig {
            params: &[U64],
            ret: U64,
        },
        "__rtsadp_is_prototype_of" | "__rtsadp_has_own" | "__rtsadp_prop_is_enumerable" | "__rtsadp_obj_set_proto" => SymSig {
            params: &[U64, U64],
            ret: U64,
        },
        // Property descriptors + extensibility (real state). prop_flags → packed
        // flags / -1 (I64); define_prop → 1/0 (I64); prevent_ext/is_extensible → 1/0.
        "__rtsadp_prop_flags" => SymSig {
            params: &[U64, U64],
            ret: I64,
        },
        "__rtsadp_define_prop" => SymSig {
            params: &[U64, U64, U64, I64],
            ret: I64,
        },
        "__rtsadp_prevent_ext" | "__rtsadp_freeze" | "__rtsadp_seal" | "__rtsadp_is_frozen"
        | "__rtsadp_is_sealed" | "__rtsadp_is_extensible" => SymSig {
            params: &[U64],
            ret: I64,
        },
        "__rtsadp_set_proto_check" => SymSig {
            params: &[U64, U64],
            ret: I64,
        },
        "__rtsadp_construct" | "__rtsadp_class_proto_init" | "__rtsadp_class_proto_set"
        | "__rtsadp_set_timeout" | "__rtsadp_set_interval" => SymSig {
            params: &[U64, U64],
            ret: U64,
        },
        "__rtsadp_clear_timer" | "__rtsadp_queue_microtask" | "__rtsadp_set_immediate" => SymSig {
            params: &[U64],
            ret: U64,
        },
        "__rtsadp_obj_delete" => SymSig {
            params: &[U64, U64],
            ret: Bool,
        },
        // obj_keys/values/entries(obj_word) -> fresh array PolyValue word (dynamic
        // Object.keys/values/entries; Object is primordial → engine-direct).
        "__rtsadp_obj_values" | "__rtsadp_obj_entries" | "__rtsadp_obj_from_entries" => SymSig {
            params: &[U64],
            ret: U64,
        },

        // ---- Map/Set instanceof tags (P5.3). One PolyValue word in, one PolyValue
        // word out. (The wrapper ctors + Error family moved to `.ts` prelude classes
        // constructed via the user-class path; their `__rtsadp_w_*`/`__rtsadp_err_*`/
        // `__rtsadp_is_error` symbols are gone.) ----
        "__rtsadp_is_map" | "__rtsadp_is_set" => SymSig {
            params: &[U64],
            ret: U64,
        },

        // ---- codegen-owned Array CALLBACK trampolines (__rtsadp_arr_*, P4.7) ----
        // Slot 0 = the array's REAL Vec handle; slot 1 = the callback as a
        // TAG_FUNCTION PolyValue word (U64). map/filter return a fresh TAG_OBJECT
        // array word (U64); forEach returns undefined (U64); find returns the
        // element word (U64); findIndex returns an index (I64); some/every return
        // a bool. reduce takes an extra init word (U64) + a has_init flag (I64).
        "__rtsadp_arr_map"
        | "__rtsadp_arr_filter"
        | "__rtsadp_arr_for_each"
        | "__rtsadp_arr_find"
        | "__rtsadp_arr_find_last"
        | "__rtsadp_arr_flat_map"
        | "__rtsadp_arr_sort_cmp"
        | "__rtsadp_arr_to_sorted_cmp" => SymSig {
            params: &[U64, U64],
            ret: U64,
        },
        "__rtsadp_arr_find_index" | "__rtsadp_arr_find_last_index" => SymSig {
            params: &[U64, U64],
            ret: I64,
        },
        "__rtsadp_arr_some" | "__rtsadp_arr_every" => SymSig {
            params: &[U64, U64],
            ret: Bool,
        },
        "__rtsadp_arr_reduce" | "__rtsadp_arr_reduce_right" => SymSig {
            params: &[U64, U64, U64, I64],
            ret: U64,
        },

        // ---- REAL global-class instance methods (P4 data-driven dispatch) ----
        // String methods: slot 0 = receiver (real string Handle); string args are
        // Handle, index/count args are I64; returns Handle (string) / I64 / Bool.
        // Verified against rts-primitives/src/string/rt.rs.
        "__RTS_FN_GL_STRING_TO_UPPER_CASE"
        | "__RTS_FN_GL_STRING_TO_LOWER_CASE"
        | "__RTS_FN_GL_STRING_TRIM"
        | "__RTS_FN_GL_STRING_TRIM_START"
        | "__RTS_FN_GL_STRING_TRIM_END" => SymSig {
            params: &[Handle],
            ret: Handle,
        },
        "__RTS_FN_GL_STRING_CHAR_AT" | "__RTS_FN_GL_STRING_AT" | "__RTS_FN_GL_STRING_REPEAT" => {
            SymSig {
                params: &[Handle, I64],
                ret: Handle,
            }
        }
        "__RTS_FN_GL_STRING_SLICE"
        | "__RTS_FN_GL_STRING_SUBSTRING"
        | "__RTS_FN_GL_STRING_SUBSTR" => SymSig {
            params: &[Handle, I64, I64],
            ret: Handle,
        },
        "__RTS_FN_GL_STRING_INDEX_OF" | "__RTS_FN_GL_STRING_LAST_INDEX_OF" => SymSig {
            params: &[Handle, Handle],
            ret: I64,
        },
        "__RTS_FN_GL_STRING_INCLUDES"
        | "__RTS_FN_GL_STRING_STARTS_WITH"
        | "__RTS_FN_GL_STRING_ENDS_WITH" => SymSig {
            params: &[Handle, Handle],
            ret: Bool,
        },
        "__RTS_FN_GL_STRING_CHAR_CODE_AT" | "__RTS_FN_GL_STRING_CODE_POINT_AT" => SymSig {
            params: &[Handle, I64],
            ret: I64,
        },
        "__RTS_FN_GL_STRING_LOCALE_COMPARE" | "__RTS_FN_GL_STRING_SEARCH_AUTO" => SymSig {
            params: &[Handle, Handle],
            ret: I64,
        },
        // match/matchAll AUTO: (recv handle, pattern handle string-or-regex) →
        // a Vec-of-strings handle (0 ⇒ JS null).
        "__RTS_FN_GL_STRING_MATCH_AUTO" | "__RTS_FN_GL_STRING_MATCH_ALL_AUTO" => SymSig {
            params: &[Handle, Handle],
            ret: Handle,
        },
        "__RTS_FN_GL_STRING_REPLACE" | "__RTS_FN_GL_STRING_REPLACE_ALL" => SymSig {
            params: &[Handle, Handle, Handle],
            ret: Handle,
        },
        "__RTS_FN_GL_STRING_CONCAT" => SymSig {
            params: &[Handle, Handle],
            ret: Handle,
        },
        "__RTS_FN_GL_STRING_PAD_START" | "__RTS_FN_GL_STRING_PAD_END" => SymSig {
            params: &[Handle, I64, Handle],
            ret: Handle,
        },
        // Number methods: slot 0 = receiver (the f64 primitive); digit/radix args
        // are I64; returns a string Handle.
        "__RTS_FN_GL_NUMBER_TO_FIXED"
        | "__RTS_FN_GL_NUMBER_TO_PRECISION"
        | "__RTS_FN_GL_NUMBER_TO_EXPONENTIAL"
        | "__RTS_FN_GL_NUMBER_TO_STRING_RADIX" => SymSig {
            params: &[F64, I64],
            ret: Handle,
        },

        // ---- REAL Math namespace symbols (P5.4, rts-shared math) ----
        // 1-arg f64 → f64.
        "__RTS_FN_NS_MATH_FLOOR"
        | "__RTS_FN_NS_MATH_CEIL"
        | "__RTS_FN_NS_MATH_ROUND"
        | "__RTS_FN_NS_MATH_TRUNC"
        | "__RTS_FN_NS_MATH_SIGN"
        | "__RTS_FN_NS_MATH_CBRT"
        | "__RTS_FN_NS_MATH_EXP"
        | "__RTS_FN_NS_MATH_EXPM1"
        | "__RTS_FN_NS_MATH_LN"
        | "__RTS_FN_NS_MATH_LOG2"
        | "__RTS_FN_NS_MATH_LOG10"
        | "__RTS_FN_NS_MATH_LOG1P"
        | "__RTS_FN_NS_MATH_SIN"
        | "__RTS_FN_NS_MATH_COS"
        | "__RTS_FN_NS_MATH_TAN"
        | "__RTS_FN_NS_MATH_ASIN"
        | "__RTS_FN_NS_MATH_ACOS"
        | "__RTS_FN_NS_MATH_ATAN"
        | "__RTS_FN_NS_MATH_SINH"
        | "__RTS_FN_NS_MATH_COSH"
        | "__RTS_FN_NS_MATH_TANH"
        | "__RTS_FN_NS_MATH_FROUND"
        | "__RTS_FN_NS_MATH_F16ROUND" => SymSig {
            params: &[F64],
            ret: F64,
        },
        // 2-arg f64 → f64.
        "__RTS_FN_NS_MATH_POW" | "__RTS_FN_NS_MATH_ATAN2" | "__RTS_FN_NS_MATH_HYPOT" => SymSig {
            params: &[F64, F64],
            ret: F64,
        },
        // no-arg f64 (the seeded PRNG).
        "__RTS_FN_NS_MATH_RANDOM_F64" => SymSig {
            params: &[],
            ret: F64,
        },

        // ---- REAL Number static predicates (P5.4, rts-primitives number) ----
        // f64 → Bool (extern "C" i64 0/1).
        "__RTS_FN_GL_NUMBER_IS_INTEGER"
        | "__RTS_FN_GL_NUMBER_IS_FINITE"
        | "__RTS_FN_GL_NUMBER_IS_NAN"
        | "__RTS_FN_GL_NUMBER_IS_SAFE_INT" => SymSig {
            params: &[F64],
            ret: Bool,
        },

        _ => return None,
    })
}
