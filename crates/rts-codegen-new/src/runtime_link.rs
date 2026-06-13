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
use rts_runtime::namespaces::globals::number as rt_num;
use rts_runtime::namespaces::globals::string::rt as rt_gl_str;
use rts_runtime::namespaces::io as rt_io;

use crate::value::{abi_adapter, arraycb, arrayops, funcops, genops, genops_arith};

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
    let mut syms = vec![
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
        // ---- generic arithmetic / comparison / unary / bitwise (P4.8) ----
        sym("__rtsadp_sub", genops_arith::__rtsadp_sub as *const u8),
        sym("__rtsadp_mul", genops_arith::__rtsadp_mul as *const u8),
        sym("__rtsadp_div", genops_arith::__rtsadp_div as *const u8),
        sym("__rtsadp_mod", genops_arith::__rtsadp_mod as *const u8),
        sym("__rtsadp_pow", genops_arith::__rtsadp_pow as *const u8),
        sym("__rtsadp_lt", genops_arith::__rtsadp_lt as *const u8),
        sym("__rtsadp_le", genops_arith::__rtsadp_le as *const u8),
        sym("__rtsadp_gt", genops_arith::__rtsadp_gt as *const u8),
        sym("__rtsadp_ge", genops_arith::__rtsadp_ge as *const u8),
        sym("__rtsadp_neg", genops_arith::__rtsadp_neg as *const u8),
        sym("__rtsadp_bnot", genops_arith::__rtsadp_bnot as *const u8),
        sym("__rtsadp_not", genops_arith::__rtsadp_not as *const u8),
        sym("__rtsadp_band", genops_arith::__rtsadp_band as *const u8),
        sym("__rtsadp_bor", genops_arith::__rtsadp_bor as *const u8),
        sym("__rtsadp_bxor", genops_arith::__rtsadp_bxor as *const u8),
        sym("__rtsadp_shl", genops_arith::__rtsadp_shl as *const u8),
        sym("__rtsadp_shr", genops_arith::__rtsadp_shr as *const u8),
        sym("__rtsadp_ushr", genops_arith::__rtsadp_ushr as *const u8),
        // ---- codegen-owned Array trampolines (__rtsadp_arr_*, P4.5) ----
        sym("__rtsadp_arr_index_of", arrayops::__rtsadp_arr_index_of as *const u8),
        sym("__rtsadp_arr_includes", arrayops::__rtsadp_arr_includes as *const u8),
        sym("__rtsadp_arr_at", arrayops::__rtsadp_arr_at as *const u8),
        sym("__rtsadp_arr_join", arrayops::__rtsadp_arr_join as *const u8),
        sym("__rtsadp_arr_push", arrayops::__rtsadp_arr_push as *const u8),
        sym("__rtsadp_arr_pop", arrayops::__rtsadp_arr_pop as *const u8),
        sym("__rtsadp_arr_slice", arrayops::__rtsadp_arr_slice as *const u8),
        // ---- codegen-owned FUNCTION-value trampolines (__rtsadp_fn_*, P4.6) ----
        sym("__rtsadp_fn_reify", funcops::__rtsadp_fn_reify as *const u8),
        sym("__rtsadp_fn_invoke", funcops::__rtsadp_fn_invoke as *const u8),
        // ---- codegen-owned Array CALLBACK trampolines (__rtsadp_arr_*, P4.7) ----
        sym("__rtsadp_arr_map", arraycb::__rtsadp_arr_map as *const u8),
        sym("__rtsadp_arr_filter", arraycb::__rtsadp_arr_filter as *const u8),
        sym("__rtsadp_arr_for_each", arraycb::__rtsadp_arr_for_each as *const u8),
        sym("__rtsadp_arr_find", arraycb::__rtsadp_arr_find as *const u8),
        sym("__rtsadp_arr_find_index", arraycb::__rtsadp_arr_find_index as *const u8),
        sym("__rtsadp_arr_some", arraycb::__rtsadp_arr_some as *const u8),
        sym("__rtsadp_arr_every", arraycb::__rtsadp_arr_every as *const u8),
        sym("__rtsadp_arr_reduce", arraycb::__rtsadp_arr_reduce as *const u8),
    ];
    syms.extend(gl_method_symbols());
    syms
}

/// The REAL global-class instance-method symbols the P4 data-driven dispatch
/// ([`crate::dispatch`]) resolves and the method lowering emits — String + Number
/// instance methods. Each is the ACTUAL `__RTS_FN_GL_*` extern (taken by address
/// through the facade); the dispatch metadata references these exact names, so
/// the symbol set is EXACTLY what the lowering can emit (a referenced-but-missing
/// symbol would be the SIGILL-class bug we avoid).
fn gl_method_symbols() -> Vec<JitSymbol> {
    vec![
        // ---- String instance methods (rts-primitives string::rt) ----
        sym("__RTS_FN_GL_STRING_TO_UPPER_CASE", rt_gl_str::__RTS_FN_GL_STRING_TO_UPPER_CASE as *const u8),
        sym("__RTS_FN_GL_STRING_TO_LOWER_CASE", rt_gl_str::__RTS_FN_GL_STRING_TO_LOWER_CASE as *const u8),
        sym("__RTS_FN_GL_STRING_TRIM", rt_gl_str::__RTS_FN_GL_STRING_TRIM as *const u8),
        sym("__RTS_FN_GL_STRING_TRIM_START", rt_gl_str::__RTS_FN_GL_STRING_TRIM_START as *const u8),
        sym("__RTS_FN_GL_STRING_TRIM_END", rt_gl_str::__RTS_FN_GL_STRING_TRIM_END as *const u8),
        sym("__RTS_FN_GL_STRING_CHAR_AT", rt_gl_str::__RTS_FN_GL_STRING_CHAR_AT as *const u8),
        sym("__RTS_FN_GL_STRING_AT", rt_gl_str::__RTS_FN_GL_STRING_AT as *const u8),
        sym("__RTS_FN_GL_STRING_REPEAT", rt_gl_str::__RTS_FN_GL_STRING_REPEAT as *const u8),
        sym("__RTS_FN_GL_STRING_SLICE", rt_gl_str::__RTS_FN_GL_STRING_SLICE as *const u8),
        sym("__RTS_FN_GL_STRING_SUBSTRING", rt_gl_str::__RTS_FN_GL_STRING_SUBSTRING as *const u8),
        sym("__RTS_FN_GL_STRING_SUBSTR", rt_gl_str::__RTS_FN_GL_STRING_SUBSTR as *const u8),
        sym("__RTS_FN_GL_STRING_INDEX_OF", rt_gl_str::__RTS_FN_GL_STRING_INDEX_OF as *const u8),
        sym("__RTS_FN_GL_STRING_LAST_INDEX_OF", rt_gl_str::__RTS_FN_GL_STRING_LAST_INDEX_OF as *const u8),
        sym("__RTS_FN_GL_STRING_INCLUDES", rt_gl_str::__RTS_FN_GL_STRING_INCLUDES as *const u8),
        sym("__RTS_FN_GL_STRING_STARTS_WITH", rt_gl_str::__RTS_FN_GL_STRING_STARTS_WITH as *const u8),
        sym("__RTS_FN_GL_STRING_ENDS_WITH", rt_gl_str::__RTS_FN_GL_STRING_ENDS_WITH as *const u8),
        sym("__RTS_FN_GL_STRING_CHAR_CODE_AT", rt_gl_str::__RTS_FN_GL_STRING_CHAR_CODE_AT as *const u8),
        sym("__RTS_FN_GL_STRING_REPLACE", rt_gl_str::__RTS_FN_GL_STRING_REPLACE as *const u8),
        sym("__RTS_FN_GL_STRING_REPLACE_ALL", rt_gl_str::__RTS_FN_GL_STRING_REPLACE_ALL as *const u8),
        sym("__RTS_FN_GL_STRING_CONCAT", rt_gl_str::__RTS_FN_GL_STRING_CONCAT as *const u8),
        sym("__RTS_FN_GL_STRING_PAD_START", rt_gl_str::__RTS_FN_GL_STRING_PAD_START as *const u8),
        sym("__RTS_FN_GL_STRING_PAD_END", rt_gl_str::__RTS_FN_GL_STRING_PAD_END as *const u8),
        // ---- Number instance methods (rts-primitives number) ----
        sym("__RTS_FN_GL_NUMBER_TO_FIXED", rt_num::__RTS_FN_GL_NUMBER_TO_FIXED as *const u8),
        sym("__RTS_FN_GL_NUMBER_TO_PRECISION", rt_num::__RTS_FN_GL_NUMBER_TO_PRECISION as *const u8),
        sym("__RTS_FN_GL_NUMBER_TO_EXPONENTIAL", rt_num::__RTS_FN_GL_NUMBER_TO_EXPONENTIAL as *const u8),
        sym("__RTS_FN_GL_NUMBER_TO_STRING_RADIX", rt_num::__RTS_FN_GL_NUMBER_TO_STRING_RADIX as *const u8),
    ]
}

#[inline]
fn sym(name: &'static str, ptr: *const u8) -> JitSymbol {
    JitSymbol { name, ptr }
}
