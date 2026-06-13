//! JIT symbol registration bridge — install the REAL runtime symbols (and the
//! codegen-owned adapter trampolines) into a `JITModule`'s `JITBuilder`.
//!
//! This replaces the fake `crate::runtime::symbols()` table. It is the new
//! engine's analogue of the old engine's `register_runtime_symbols` /
//! `runtime_jit_symbols` bridge (`rts-codegen-old/src/abi/mod.rs` +
//! `codegen/jit.rs`) — but built against the `rts-runtime` FACADE, NOT by
//! depending on the frozen old crate.
//!
//! ## Why direct addresses instead of `Engine` + registry harvest
//!
//! The old engine builds an `rts_engine::Engine`, calls every `register_*`, then
//! reads `registry().jit_symbols()` (symbol→fn_ptr, with the **null-skip**
//! invariant: alias/external members carry a null fn_ptr and must NOT overwrite
//! the owner's real address). We honour the SAME invariant here, but cannot build
//! an `Engine`: `rts_engine::Engine` is NOT re-exported through the `rts-runtime`
//! facade (`rts_runtime::abi` is `rts_engine::abi::*`, which carries the ABI vocab
//! but not the builder), and the layering rule forbids adding `rts-engine` as a
//! second direct dependency.
//!
//! Crucially, the `gc` string-pool members are registered in the engine as
//! `external` (fn_ptr NULL — the real `#[no_mangle] extern "C"` bodies live in
//! `string_pool`), so even the registry harvest would SKIP them; the old engine
//! supplies them from a hardcoded `add_fn!` list. We do the equivalent: we take
//! the address of each REAL `__RTS_FN_*` function directly through its facade
//! re-export path. This both (a) satisfies the null-skip invariant trivially (we
//! only list real, non-null bodies) and (b) keeps the surface honest — every
//! entry is the actual runtime function the lowering calls.

use rts_runtime::namespaces::collections::vec as rt_vec;
use rts_runtime::namespaces::gc::handles as rt_handles;
use rts_runtime::namespaces::gc::string_pool as rt_str;
use rts_runtime::namespaces::io as rt_io;

use crate::value::{abi_adapter, genops};

/// One installable JIT symbol: an extern "C" name and its function pointer. The
/// pointer is to a `#[no_mangle] extern "C"` function with static lifetime.
#[derive(Clone, Copy)]
pub struct JitSymbol {
    pub name: &'static str,
    pub ptr: *const u8,
}

// SAFETY: every `ptr` is to static `extern "C"` code (the runtime binary or this
// crate), never dereferenced as data — sound to share across threads.
unsafe impl Send for JitSymbol {}
unsafe impl Sync for JitSymbol {}

/// The full set of `(symbol, fn_ptr)` pairs the JIT must install so the lowered
/// code can `call` them by name: the REAL runtime symbols the new lowering emits
/// today (gc string-pool, io, collections vec) + the codegen-owned `__rtsadp_*`
/// adapter trampolines.
///
/// Every entry has a non-null pointer (the null-skip invariant from the engine's
/// registry is satisfied by construction — we list only real bodies).
pub fn jit_symbols() -> Vec<JitSymbol> {
    vec![
        // ---- REAL string pool (rts-std collector::string_pool) ----
        sym("__RTS_FN_NS_GC_STRING_NEW", rt_str::__RTS_FN_NS_GC_STRING_NEW as *const u8),
        sym("__RTS_FN_NS_GC_STRING_FROM_STATIC", rt_str::__RTS_FN_NS_GC_STRING_FROM_STATIC as *const u8),
        sym("__RTS_FN_NS_GC_STRING_PTR", rt_str::__RTS_FN_NS_GC_STRING_PTR as *const u8),
        sym("__RTS_FN_NS_GC_STRING_LEN", rt_str::__RTS_FN_NS_GC_STRING_LEN as *const u8),
        sym("__RTS_FN_NS_GC_STRING_FREE", rt_str::__RTS_FN_NS_GC_STRING_FREE as *const u8),
        sym("__RTS_FN_NS_GC_STRING_CONCAT", rt_str::__RTS_FN_NS_GC_STRING_CONCAT as *const u8),
        sym("__RTS_FN_NS_GC_STRING_EQ", rt_str::__RTS_FN_NS_GC_STRING_EQ as *const u8),
        sym("__RTS_FN_NS_GC_STRING_CMP", rt_str::__RTS_FN_NS_GC_STRING_CMP as *const u8),
        sym("__RTS_FN_NS_GC_STRING_FROM_I64", rt_str::__RTS_FN_NS_GC_STRING_FROM_I64 as *const u8),
        sym("__RTS_FN_NS_GC_STRING_FROM_F64", rt_str::__RTS_FN_NS_GC_STRING_FROM_F64 as *const u8),
        // ---- REAL io (rts-std io) ----
        sym("__RTS_FN_NS_IO_PRINT", rt_io::__RTS_FN_NS_IO_PRINT as *const u8),
        sym("__RTS_FN_NS_IO_EPRINT", rt_io::__RTS_FN_NS_IO_EPRINT as *const u8),
        // ---- REAL collections Vec (rts-shared collections::vec) ----
        sym("__RTS_FN_NS_COLLECTIONS_VEC_NEW", rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW as *const u8),
        sym("__RTS_FN_NS_COLLECTIONS_VEC_PUSH", rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH as *const u8),
        sym("__RTS_FN_NS_COLLECTIONS_VEC_GET", rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET as *const u8),
        sym("__RTS_FN_NS_COLLECTIONS_VEC_LEN", rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN as *const u8),
        sym("__RTS_FN_NS_COLLECTIONS_VEC_SET", rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SET as *const u8),
        sym("__RTS_FN_NS_COLLECTIONS_VEC_POP", rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_POP as *const u8),
        // ---- REAL PolyValue <-> handle bridge (rts-engine heap::handles) ----
        // Replaces the old `__rtsadp_store/_load` indirection table: the payload
        // carries the bare 48-bit slot+shard and the generation is reconstructed
        // on demand from the live slot, so there is no side table to GC-root.
        sym(
            "__RTS_FN_NS_GC_POLY_FROM_HANDLE",
            rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE as *const u8,
        ),
        sym(
            "__RTS_FN_NS_GC_POLY_TO_HANDLE",
            rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE as *const u8,
        ),
        // ---- codegen-owned adapter trampolines (__rtsadp_*) ----
        sym("__rtsadp_add", genops::__rtsadp_add as *const u8),
        sym("__rtsadp_strict_eq", genops::__rtsadp_strict_eq as *const u8),
        sym("__rtsadp_strict_neq", genops::__rtsadp_strict_neq as *const u8),
        sym("__rtsadp_typeof", genops::__rtsadp_typeof as *const u8),
        sym("__rtsadp_to_string", genops::__rtsadp_to_string as *const u8),
        sym("__rtsadp_to_boolean", genops::__rtsadp_to_boolean as *const u8),
        sym("__rtsadp_print_line", abi_adapter::__rtsadp_print_line as *const u8),
    ]
}

#[inline]
fn sym(name: &'static str, ptr: *const u8) -> JitSymbol {
    JitSymbol { name, ptr }
}
