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

use cranelift_codegen::ir::{types, AbiParam, Signature};
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
        // Bool/I64/U64/Handle all ride an i64 register at the boundary.
        AbiType::Bool | AbiType::I64 | AbiType::U64 | AbiType::Handle => types::I64,
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

/// Resolve a symbol name to its [`SymSig`]. Covers exactly the symbols the new
/// lowering calls: the REAL runtime symbols (`__RTS_FN_*`) + the codegen-owned
/// adapter trampolines (`__rtsadp_*`). `None` for an unknown symbol (the lowering
/// turns that into an explicit `Unsupported` bail, never a guess).
pub fn sig_of(name: &str) -> Option<SymSig> {
    use AbiType::*;
    Some(match name {
        // ---- REAL string pool (rts-std collector::string_pool) ----
        // STRING_NEW(ptr,len) -> handle  — StrPtr = two slots.
        "__RTS_FN_NS_GC_STRING_NEW" | "__RTS_FN_NS_GC_STRING_FROM_STATIC" => {
            SymSig { params: &[StrPtr], ret: Handle }
        }
        "__RTS_FN_NS_GC_STRING_PTR" => SymSig { params: &[Handle], ret: U64 },
        "__RTS_FN_NS_GC_STRING_LEN" => SymSig { params: &[Handle], ret: I64 },
        "__RTS_FN_NS_GC_STRING_FREE" => SymSig { params: &[Handle], ret: I64 },
        "__RTS_FN_NS_GC_STRING_CONCAT" => SymSig { params: &[Handle, Handle], ret: Handle },
        "__RTS_FN_NS_GC_STRING_EQ" => SymSig { params: &[Handle, Handle], ret: I64 },
        "__RTS_FN_NS_GC_STRING_CMP" => SymSig { params: &[Handle, Handle], ret: I64 },
        "__RTS_FN_NS_GC_STRING_FROM_I64" => SymSig { params: &[I64], ret: Handle },
        "__RTS_FN_NS_GC_STRING_FROM_F64" => SymSig { params: &[F64], ret: Handle },

        // ---- REAL io (rts-std io) ----
        // IO_PRINT(ptr,len) -> void  — StrPtr = two slots, appends a newline.
        "__RTS_FN_NS_IO_PRINT" | "__RTS_FN_NS_IO_EPRINT" => {
            SymSig { params: &[StrPtr], ret: Void }
        }

        // ---- REAL collections Vec (rts-shared collections::vec) ----
        "__RTS_FN_NS_COLLECTIONS_VEC_NEW" => SymSig { params: &[], ret: Handle },
        "__RTS_FN_NS_COLLECTIONS_VEC_PUSH" => SymSig { params: &[U64, I64], ret: Void },
        "__RTS_FN_NS_COLLECTIONS_VEC_GET" => SymSig { params: &[U64, I64], ret: I64 },
        "__RTS_FN_NS_COLLECTIONS_VEC_LEN" => SymSig { params: &[U64], ret: I64 },
        "__RTS_FN_NS_COLLECTIONS_VEC_SET" => SymSig { params: &[U64, I64, I64], ret: Void },
        "__RTS_FN_NS_COLLECTIONS_VEC_POP" => SymSig { params: &[U64], ret: I64 },

        // ---- REAL PolyValue <-> handle bridge (rts-engine heap::handles) ----
        // FROM_HANDLE: full real handle -> bare 48-bit slot+shard payload.
        // TO_HANDLE: 48-bit payload -> full real handle (gen reconstructed from
        // the live slot). These REPLACE the old `__rtsadp_store/_load` table.
        "__RTS_FN_NS_GC_POLY_FROM_HANDLE" | "__RTS_FN_NS_GC_POLY_TO_HANDLE" => {
            SymSig { params: &[U64], ret: U64 }
        }
        // ---- codegen-owned adapter trampolines (__rtsadp_*) ----
        // Generic JS operators on PolyValue words (tagged-in/tagged-out).
        "__rtsadp_add" | "__rtsadp_strict_eq" | "__rtsadp_strict_neq" => {
            SymSig { params: &[U64, U64], ret: U64 }
        }
        "__rtsadp_typeof" | "__rtsadp_to_string" | "__rtsadp_to_boolean" => {
            SymSig { params: &[U64], ret: U64 }
        }
        // ---- generic arithmetic/comparison/bitwise (P4.8): two PolyValue words ----
        "__rtsadp_sub" | "__rtsadp_mul" | "__rtsadp_div" | "__rtsadp_mod"
        | "__rtsadp_pow" | "__rtsadp_lt" | "__rtsadp_le" | "__rtsadp_gt"
        | "__rtsadp_ge" | "__rtsadp_band" | "__rtsadp_bor" | "__rtsadp_bxor"
        | "__rtsadp_shl" | "__rtsadp_shr" | "__rtsadp_ushr" => {
            SymSig { params: &[U64, U64], ret: U64 }
        }
        // ---- generic unary (P4.8): one PolyValue word ----
        "__rtsadp_neg" | "__rtsadp_bnot" | "__rtsadp_not" => {
            SymSig { params: &[U64], ret: U64 }
        }
        // console.log line sink: takes (ptr, len) as a StrPtr (two slots), void.
        "__rtsadp_print_line" => SymSig { params: &[StrPtr], ret: Void },

        // ---- codegen-owned FUNCTION-value trampolines (__rtsadp_fn_*, P4.6) ----
        // reify(addr, nparams, has_rest) -> 48-bit slot+shard payload (U64).
        "__rtsadp_fn_reify" => SymSig { params: &[U64, U64, U64], ret: U64 },
        // invoke(fn_word, a0, a1, a2, a3, rest) -> result PolyValue word (U64).
        // All slots are raw PolyValue words (U64); the fixed uniform call ABI.
        "__rtsadp_fn_invoke" => SymSig { params: &[U64, U64, U64, U64, U64, U64], ret: U64 },

        // ---- codegen-owned Array trampolines (__rtsadp_arr_*, P4.5) ----
        // All slots are u64/i64 (no StrPtr): slot 0 is the array's REAL Vec handle
        // (`POLY_TO_HANDLE` of the array word); needle args are raw PolyValue words
        // (U64); index/range args are I64; results are PolyValue words (U64) / a
        // string Handle (join) / an i64 (index_of/push) / a bool (includes).
        "__rtsadp_arr_index_of" => SymSig { params: &[U64, U64], ret: I64 },
        "__rtsadp_arr_includes" => SymSig { params: &[U64, U64], ret: Bool },
        "__rtsadp_arr_at" => SymSig { params: &[U64, I64], ret: U64 },
        "__rtsadp_arr_join" => SymSig { params: &[U64, Handle], ret: Handle },
        "__rtsadp_arr_push" => SymSig { params: &[U64, U64], ret: I64 },
        "__rtsadp_arr_pop" => SymSig { params: &[U64], ret: U64 },
        "__rtsadp_arr_slice" => SymSig { params: &[U64, I64, I64], ret: U64 },

        // ---- codegen-owned Array CALLBACK trampolines (__rtsadp_arr_*, P4.7) ----
        // Slot 0 = the array's REAL Vec handle; slot 1 = the callback as a
        // TAG_FUNCTION PolyValue word (U64). map/filter return a fresh TAG_OBJECT
        // array word (U64); forEach returns undefined (U64); find returns the
        // element word (U64); findIndex returns an index (I64); some/every return
        // a bool. reduce takes an extra init word (U64) + a has_init flag (I64).
        "__rtsadp_arr_map" | "__rtsadp_arr_filter" | "__rtsadp_arr_for_each"
        | "__rtsadp_arr_find" => SymSig { params: &[U64, U64], ret: U64 },
        "__rtsadp_arr_find_index" => SymSig { params: &[U64, U64], ret: I64 },
        "__rtsadp_arr_some" | "__rtsadp_arr_every" => SymSig { params: &[U64, U64], ret: Bool },
        "__rtsadp_arr_reduce" => SymSig { params: &[U64, U64, U64, I64], ret: U64 },

        // ---- REAL global-class instance methods (P4 data-driven dispatch) ----
        // String methods: slot 0 = receiver (real string Handle); string args are
        // Handle, index/count args are I64; returns Handle (string) / I64 / Bool.
        // Verified against rts-primitives/src/string/rt.rs.
        "__RTS_FN_GL_STRING_TO_UPPER_CASE"
        | "__RTS_FN_GL_STRING_TO_LOWER_CASE"
        | "__RTS_FN_GL_STRING_TRIM"
        | "__RTS_FN_GL_STRING_TRIM_START"
        | "__RTS_FN_GL_STRING_TRIM_END" => SymSig { params: &[Handle], ret: Handle },
        "__RTS_FN_GL_STRING_CHAR_AT"
        | "__RTS_FN_GL_STRING_AT"
        | "__RTS_FN_GL_STRING_REPEAT" => SymSig { params: &[Handle, I64], ret: Handle },
        "__RTS_FN_GL_STRING_SLICE"
        | "__RTS_FN_GL_STRING_SUBSTRING"
        | "__RTS_FN_GL_STRING_SUBSTR" => SymSig { params: &[Handle, I64, I64], ret: Handle },
        "__RTS_FN_GL_STRING_INDEX_OF" | "__RTS_FN_GL_STRING_LAST_INDEX_OF" => {
            SymSig { params: &[Handle, Handle], ret: I64 }
        }
        "__RTS_FN_GL_STRING_INCLUDES"
        | "__RTS_FN_GL_STRING_STARTS_WITH"
        | "__RTS_FN_GL_STRING_ENDS_WITH" => SymSig { params: &[Handle, Handle], ret: Bool },
        "__RTS_FN_GL_STRING_CHAR_CODE_AT" => SymSig { params: &[Handle, I64], ret: I64 },
        "__RTS_FN_GL_STRING_REPLACE" | "__RTS_FN_GL_STRING_REPLACE_ALL" => {
            SymSig { params: &[Handle, Handle, Handle], ret: Handle }
        }
        "__RTS_FN_GL_STRING_CONCAT" => SymSig { params: &[Handle, Handle], ret: Handle },
        "__RTS_FN_GL_STRING_PAD_START" | "__RTS_FN_GL_STRING_PAD_END" => {
            SymSig { params: &[Handle, I64, Handle], ret: Handle }
        }
        // Number methods: slot 0 = receiver (the f64 primitive); digit/radix args
        // are I64; returns a string Handle.
        "__RTS_FN_GL_NUMBER_TO_FIXED"
        | "__RTS_FN_GL_NUMBER_TO_PRECISION"
        | "__RTS_FN_GL_NUMBER_TO_EXPONENTIAL"
        | "__RTS_FN_GL_NUMBER_TO_STRING_RADIX" => SymSig { params: &[F64, I64], ret: Handle },

        _ => return None,
    })
}
