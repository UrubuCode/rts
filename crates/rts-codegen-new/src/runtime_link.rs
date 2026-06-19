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

use rts_engine::heap::env as rt_env;
use rts_engine::heap::instance as rt_inst;
use rts_runtime::namespaces::collections::vec as rt_vec;
use rts_runtime::namespaces::globals::string::{
    replace as rt_str_replace, search as rt_str_search, split as rt_str_split,
    transform as rt_str_transform,
};
use rts_runtime::namespaces::engine as rt_engine;
use rts_runtime::namespaces::gc::collector as rt_gcoll;
use rts_runtime::namespaces::gc::handles as rt_handles;
use rts_runtime::namespaces::gc::string_pool as rt_str;
use rts_runtime::namespaces::globals::number as rt_num;
use rts_runtime::namespaces::globals::string::rt as rt_gl_str;
use rts_runtime::namespaces::io as rt_io;
use rts_runtime::namespaces::math as rt_math;

use crate::value::{
    abi_adapter, arraycb, arrayops, dyndispatch, errslot, funcops, genops, genops_arith, globalops,
    inspect, iterops, objops, regexops,
};

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
        // ---- module-level mutable global cells (epic #195) ----
        sym(
            "__RTS_FN_NS_GC_GCELL_GET",
            rt_gcoll::__RTS_FN_NS_GC_GCELL_GET as *const u8,
        ),
        sym(
            "__RTS_FN_NS_GC_GCELL_SET",
            rt_gcoll::__RTS_FN_NS_GC_GCELL_SET as *const u8,
        ),
        // ---- REAL string pool (rts-std collector::string_pool) ----
        sym(
            "__RTS_FN_NS_GC_STRING_NEW",
            rt_str::__RTS_FN_NS_GC_STRING_NEW as *const u8,
        ),
        sym(
            "__RTS_FN_NS_GC_STRING_FROM_STATIC",
            rt_str::__RTS_FN_NS_GC_STRING_FROM_STATIC as *const u8,
        ),
        sym(
            "__RTS_FN_NS_GC_STRING_PTR",
            rt_str::__RTS_FN_NS_GC_STRING_PTR as *const u8,
        ),
        sym(
            "__RTS_FN_NS_GC_STRING_LEN",
            rt_str::__RTS_FN_NS_GC_STRING_LEN as *const u8,
        ),
        sym(
            "__RTS_FN_NS_GC_STRING_FREE",
            rt_str::__RTS_FN_NS_GC_STRING_FREE as *const u8,
        ),
        sym(
            "__RTS_FN_NS_GC_STRING_CONCAT",
            rt_str::__RTS_FN_NS_GC_STRING_CONCAT as *const u8,
        ),
        sym(
            "__RTS_FN_NS_GC_STRING_EQ",
            rt_str::__RTS_FN_NS_GC_STRING_EQ as *const u8,
        ),
        sym(
            "__RTS_FN_NS_GC_STRING_CMP",
            rt_str::__RTS_FN_NS_GC_STRING_CMP as *const u8,
        ),
        sym(
            "__RTS_FN_NS_GC_STRING_FROM_I64",
            rt_str::__RTS_FN_NS_GC_STRING_FROM_I64 as *const u8,
        ),
        sym(
            "__RTS_FN_NS_GC_STRING_FROM_F64",
            rt_str::__RTS_FN_NS_GC_STRING_FROM_F64 as *const u8,
        ),
        // ---- REAL io (rts-std io) ----
        sym(
            "__RTS_FN_NS_IO_PRINT",
            rt_io::__RTS_FN_NS_IO_PRINT as *const u8,
        ),
        sym(
            "__RTS_FN_NS_IO_EPRINT",
            rt_io::__RTS_FN_NS_IO_EPRINT as *const u8,
        ),
        // ---- REAL collections Vec (rts-shared collections::vec) ----
        sym(
            "__RTS_FN_NS_COLLECTIONS_VEC_NEW",
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW as *const u8,
        ),
        sym(
            "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH as *const u8,
        ),
        sym(
            "__RTS_FN_NS_COLLECTIONS_VEC_GET",
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET as *const u8,
        ),
        sym(
            "__RTS_FN_NS_COLLECTIONS_VEC_LEN",
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN as *const u8,
        ),
        sym(
            "__RTS_FN_NS_COLLECTIONS_VEC_SET",
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SET as *const u8,
        ),
        sym(
            "__RTS_FN_NS_COLLECTIONS_VEC_POP",
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_POP as *const u8,
        ),
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
        sym(
            "__rtsadp_strict_eq",
            genops::__rtsadp_strict_eq as *const u8,
        ),
        sym(
            "__rtsadp_strict_neq",
            genops::__rtsadp_strict_neq as *const u8,
        ),
        sym("__rtsadp_loose_eq", genops::__rtsadp_loose_eq as *const u8),
        sym(
            "__rtsadp_loose_neq",
            genops::__rtsadp_loose_neq as *const u8,
        ),
        sym("__rtsadp_typeof", genops::__rtsadp_typeof as *const u8),
        sym(
            "__rtsadp_to_string",
            genops::__rtsadp_to_string as *const u8,
        ),
        sym(
            "__rtsadp_to_boolean",
            genops::__rtsadp_to_boolean as *const u8,
        ),
        sym(
            "__rtsadp_print_line",
            abi_adapter::__rtsadp_print_line as *const u8,
        ),
        sym("__rtsadp_inspect", inspect::__rtsadp_inspect as *const u8),
        sym(
            "__rtsadp_inspect_object",
            inspect::__rtsadp_inspect_object as *const u8,
        ),
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
        sym("__rtsadp_pos", genops_arith::__rtsadp_pos as *const u8),
        sym("__rtsadp_bnot", genops_arith::__rtsadp_bnot as *const u8),
        sym("__rtsadp_not", genops_arith::__rtsadp_not as *const u8),
        sym("__rtsadp_band", genops_arith::__rtsadp_band as *const u8),
        sym("__rtsadp_bor", genops_arith::__rtsadp_bor as *const u8),
        sym("__rtsadp_bxor", genops_arith::__rtsadp_bxor as *const u8),
        sym("__rtsadp_shl", genops_arith::__rtsadp_shl as *const u8),
        sym("__rtsadp_shr", genops_arith::__rtsadp_shr as *const u8),
        sym("__rtsadp_ushr", genops_arith::__rtsadp_ushr as *const u8),
        // ---- codegen-owned Array trampolines (__rtsadp_arr_*, P4.5) ----
        sym(
            "__rtsadp_arr_index_of",
            arrayops::__rtsadp_arr_index_of as *const u8,
        ),
        sym(
            "__rtsadp_arr_includes",
            arrayops::__rtsadp_arr_includes as *const u8,
        ),
        sym("__rtsadp_arr_at", arrayops::__rtsadp_arr_at as *const u8),
        sym(
            "__rtsadp_arr_join",
            arrayops::__rtsadp_arr_join as *const u8,
        ),
        sym(
            "__rtsadp_arr_push",
            arrayops::__rtsadp_arr_push as *const u8,
        ),
        sym("__rtsadp_arr_pop", arrayops::__rtsadp_arr_pop as *const u8),
        sym(
            "__rtsadp_arr_slice",
            arrayops::__rtsadp_arr_slice as *const u8,
        ),
        sym(
            "__rtsadp_arr_last_index_of",
            arrayops::__rtsadp_arr_last_index_of as *const u8,
        ),
        sym(
            "__rtsadp_arr_reverse",
            arrayops::__rtsadp_arr_reverse as *const u8,
        ),
        sym(
            "__rtsadp_arr_fill",
            arrayops::__rtsadp_arr_fill as *const u8,
        ),
        sym(
            "__rtsadp_arr_concat",
            arrayops::__rtsadp_arr_concat as *const u8,
        ),
        sym(
            "__rtsadp_arr_flat",
            arrayops::__rtsadp_arr_flat as *const u8,
        ),
        sym(
            "__rtsadp_arr_shift",
            arrayops::__rtsadp_arr_shift as *const u8,
        ),
        sym(
            "__rtsadp_arr_unshift",
            arrayops::__rtsadp_arr_unshift as *const u8,
        ),
        // ES2023 / arity-variant Array trampolines
        sym("__rtsadp_arr_slice1", arrayops::__rtsadp_arr_slice1 as *const u8),
        sym(
            "__rtsadp_arr_index_of_from",
            arrayops::__rtsadp_arr_index_of_from as *const u8,
        ),
        sym(
            "__rtsadp_arr_includes_from",
            arrayops::__rtsadp_arr_includes_from as *const u8,
        ),
        sym(
            "__rtsadp_arr_last_index_of_from",
            arrayops::__rtsadp_arr_last_index_of_from as *const u8,
        ),
        sym(
            "__rtsadp_arr_to_reversed",
            arrayops::__rtsadp_arr_to_reversed as *const u8,
        ),
        sym("__rtsadp_arr_with", arrayops::__rtsadp_arr_with as *const u8),
        sym(
            "__rtsadp_arr_flat_depth",
            arrayops::__rtsadp_arr_flat_depth as *const u8,
        ),
        sym("__rtsadp_arr_sort", arrayops::__rtsadp_arr_sort as *const u8),
        sym(
            "__rtsadp_arr_to_sorted",
            arrayops::__rtsadp_arr_to_sorted as *const u8,
        ),
        sym(
            "__rtsadp_arr_to_spliced",
            arrayops::__rtsadp_arr_to_spliced as *const u8,
        ),
        sym(
            "__rtsadp_arr_copy_within",
            arrayops::__rtsadp_arr_copy_within as *const u8,
        ),
        sym(
            "__rtsadp_arr_copy_within2",
            arrayops::__rtsadp_arr_copy_within2 as *const u8,
        ),
        // ---- codegen-owned GLOBAL constant/function + Array/String STATIC
        //      trampolines (__rtsadp_g_* / __rtsadp_arr_* / __rtsadp_str_*, P5.2) ----
        sym(
            "__rtsadp_g_number",
            globalops::__rtsadp_g_number as *const u8,
        ),
        sym(
            "__rtsadp_g_string",
            globalops::__rtsadp_g_string as *const u8,
        ),
        sym(
            "__rtsadp_g_boolean",
            globalops::__rtsadp_g_boolean as *const u8,
        ),
        sym(
            "__rtsadp_g_parse_int",
            globalops::__rtsadp_g_parse_int as *const u8,
        ),
        sym(
            "__rtsadp_g_parse_float",
            globalops::__rtsadp_g_parse_float as *const u8,
        ),
        sym(
            "__rtsadp_g_is_nan",
            globalops::__rtsadp_g_is_nan as *const u8,
        ),
        sym(
            "__rtsadp_g_is_finite",
            globalops::__rtsadp_g_is_finite as *const u8,
        ),
        sym(
            "__rtsadp_arr_is_array",
            globalops::__rtsadp_arr_is_array as *const u8,
        ),
        sym(
            "__rtsadp_arr_new_sized",
            globalops::__rtsadp_arr_new_sized as *const u8,
        ),
        sym(
            "__rtsadp_arr_from",
            globalops::__rtsadp_arr_from as *const u8,
        ),
        sym(
            "__rtsadp_str_from_char_code",
            globalops::__rtsadp_str_from_char_code as *const u8,
        ),
        sym(
            "__rtsadp_str_from_char_code_arr",
            globalops::__rtsadp_str_from_char_code_arr as *const u8,
        ),
        sym(
            "__rtsadp_str_from_code_point",
            globalops::__rtsadp_str_from_code_point as *const u8,
        ),
        sym(
            "__rtsadp_str_split",
            globalops::__rtsadp_str_split as *const u8,
        ),
        sym(
            "__rtsadp_math_reduce",
            globalops::__rtsadp_math_reduce as *const u8,
        ),
        sym(
            "__rtsadp_canon_double",
            globalops::__rtsadp_canon_double as *const u8,
        ),
        sym(
            "__rtsadp_arr_spread_append",
            globalops::__rtsadp_arr_spread_append as *const u8,
        ),
        // ---- codegen-owned ITERATION-source trampolines (iterops, P5.10) ----
        sym(
            "__rtsadp_str_chars",
            iterops::__rtsadp_str_chars as *const u8,
        ),
        sym("__rtsadp_obj_keys", iterops::__rtsadp_obj_keys as *const u8),
        // ---- codegen-owned FUNCTION-value trampolines (__rtsadp_fn_*, P4.6) ----
        sym("__rtsadp_fn_reify", funcops::__rtsadp_fn_reify as *const u8),
        sym(
            "__rtsadp_fn_invoke",
            funcops::__rtsadp_fn_invoke as *const u8,
        ),
        // ---- function-as-constructor side-table (new F() / x instanceof F) ----
        sym("__rtsadp_fn_ptr", funcops::__rtsadp_fn_ptr as *const u8),
        sym(
            "__rtsadp_ctor_mark",
            funcops::__rtsadp_ctor_mark as *const u8,
        ),
        sym(
            "__rtsadp_instanceof_fn",
            funcops::__rtsadp_instanceof_fn as *const u8,
        ),
        // ---- function-VALUE data properties (`F.foo = v` / `F.foo`) (Phase 4) ----
        sym(
            "__rtsadp_fn_get_prop",
            funcops::__rtsadp_fn_get_prop as *const u8,
        ),
        sym(
            "__rtsadp_fn_set_prop",
            funcops::__rtsadp_fn_set_prop as *const u8,
        ),
        // ---- codegen-owned Array CALLBACK trampolines (__rtsadp_arr_*, P4.7) ----
        sym("__rtsadp_arr_map", arraycb::__rtsadp_arr_map as *const u8),
        sym(
            "__rtsadp_arr_filter",
            arraycb::__rtsadp_arr_filter as *const u8,
        ),
        sym(
            "__rtsadp_arr_for_each",
            arraycb::__rtsadp_arr_for_each as *const u8,
        ),
        sym("__rtsadp_arr_find", arraycb::__rtsadp_arr_find as *const u8),
        sym(
            "__rtsadp_arr_find_index",
            arraycb::__rtsadp_arr_find_index as *const u8,
        ),
        sym("__rtsadp_arr_some", arraycb::__rtsadp_arr_some as *const u8),
        sym(
            "__rtsadp_arr_every",
            arraycb::__rtsadp_arr_every as *const u8,
        ),
        sym(
            "__rtsadp_arr_reduce",
            arraycb::__rtsadp_arr_reduce as *const u8,
        ),
        sym(
            "__rtsadp_arr_find_last",
            arraycb::__rtsadp_arr_find_last as *const u8,
        ),
        sym(
            "__rtsadp_arr_find_last_index",
            arraycb::__rtsadp_arr_find_last_index as *const u8,
        ),
        sym(
            "__rtsadp_arr_reduce_right",
            arraycb::__rtsadp_arr_reduce_right as *const u8,
        ),
        sym(
            "__rtsadp_arr_flat_map",
            arraycb::__rtsadp_arr_flat_map as *const u8,
        ),
        // ---- codegen-owned RegExp + string-regex-method trampolines (regexops, P5.12) ----
        sym(
            "__rtsadp_re_compile",
            regexops::__rtsadp_re_compile as *const u8,
        ),
        sym("__rtsadp_re_test", regexops::__rtsadp_re_test as *const u8),
        sym(
            "__rtsadp_re_source",
            regexops::__rtsadp_re_source as *const u8,
        ),
        sym(
            "__rtsadp_re_flags",
            regexops::__rtsadp_re_flags as *const u8,
        ),
        sym(
            "__rtsadp_re_global",
            regexops::__rtsadp_re_global as *const u8,
        ),
        sym(
            "__rtsadp_re_ignore_case",
            regexops::__rtsadp_re_ignore_case as *const u8,
        ),
        sym(
            "__rtsadp_re_multiline",
            regexops::__rtsadp_re_multiline as *const u8,
        ),
        sym(
            "__rtsadp_re_last_index",
            regexops::__rtsadp_re_last_index as *const u8,
        ),
        sym(
            "__rtsadp_re_str_match",
            regexops::__rtsadp_re_str_match as *const u8,
        ),
        sym(
            "__rtsadp_re_str_replace",
            regexops::__rtsadp_re_str_replace as *const u8,
        ),
        sym(
            "__rtsadp_re_str_replace_all",
            regexops::__rtsadp_re_str_replace_all as *const u8,
        ),
        sym(
            "__rtsadp_re_str_split",
            regexops::__rtsadp_re_str_split as *const u8,
        ),
        sym(
            "__rtsadp_re_str_search",
            regexops::__rtsadp_re_str_search as *const u8,
        ),
        // ---- codegen-owned DYNAMIC property access (objops, P5.5) ----
        sym("__rtsadp_obj_get", objops::__rtsadp_obj_get as *const u8),
        sym("__rtsadp_obj_set", objops::__rtsadp_obj_set as *const u8),
        sym("__rtsadp_obj_has", objops::__rtsadp_obj_has as *const u8),
        // NOTE: the wrapper ctors (`__rtsadp_w_{boolean,number,string}_new`) and the
        // Error-family trampolines are GONE — Boolean/Number/String/Error all
        // construct via their `.ts` prelude class (the user-class path). ToString for
        // the String factory/ctor uses `__rtsadp_to_string` (registered elsewhere).
        // ---- codegen-owned pending-error slot for throw / try-catch (P5.13) ----
        sym(
            "__rtsadp_throw_set",
            errslot::__rtsadp_throw_set as *const u8,
        ),
        sym(
            "__rtsadp_err_pending",
            errslot::__rtsadp_err_pending as *const u8,
        ),
        sym("__rtsadp_err_take", errslot::__rtsadp_err_take as *const u8),
        sym(
            "__rtsadp_err_clear",
            errslot::__rtsadp_err_clear as *const u8,
        ),
        // ---- codegen-owned DYNAMIC method dispatch (dyndispatch, P5.9) ----
        sym(
            "__rtsadp_dyn_to_string",
            dyndispatch::__rtsadp_dyn_to_string as *const u8,
        ),
        sym(
            "__rtsadp_dyn_length",
            dyndispatch::__rtsadp_dyn_length as *const u8,
        ),
        sym(
            "__rtsadp_dyn_index_of",
            dyndispatch::__rtsadp_dyn_index_of as *const u8,
        ),
        sym(
            "__rtsadp_dyn_includes",
            dyndispatch::__rtsadp_dyn_includes as *const u8,
        ),
        sym("__rtsadp_dyn_at", dyndispatch::__rtsadp_dyn_at as *const u8),
        sym(
            "__rtsadp_dyn_slice",
            dyndispatch::__rtsadp_dyn_slice as *const u8,
        ),
        sym(
            "__rtsadp_dyn_concat",
            dyndispatch::__rtsadp_dyn_concat as *const u8,
        ),
        sym(
            "__rtsadp_dyn_join",
            dyndispatch::__rtsadp_dyn_join as *const u8,
        ),
        sym(
            "__rtsadp_dyn_push",
            dyndispatch::__rtsadp_dyn_push as *const u8,
        ),
        sym(
            "__rtsadp_dyn_pop",
            dyndispatch::__rtsadp_dyn_pop as *const u8,
        ),
        sym(
            "__rtsadp_dyn_char_at",
            dyndispatch::__rtsadp_dyn_char_at as *const u8,
        ),
        sym(
            "__rtsadp_dyn_char_code_at",
            dyndispatch::__rtsadp_dyn_char_code_at as *const u8,
        ),
        sym(
            "__rtsadp_dyn_to_upper_case",
            dyndispatch::__rtsadp_dyn_to_upper_case as *const u8,
        ),
        sym(
            "__rtsadp_dyn_to_lower_case",
            dyndispatch::__rtsadp_dyn_to_lower_case as *const u8,
        ),
        sym(
            "__rtsadp_dyn_trim",
            dyndispatch::__rtsadp_dyn_trim as *const u8,
        ),
        sym(
            "__rtsadp_dyn_split",
            dyndispatch::__rtsadp_dyn_split as *const u8,
        ),
        sym(
            "__rtsadp_dyn_starts_with",
            dyndispatch::__rtsadp_dyn_starts_with as *const u8,
        ),
        sym(
            "__rtsadp_dyn_ends_with",
            dyndispatch::__rtsadp_dyn_ends_with as *const u8,
        ),
        sym(
            "__rtsadp_dyn_repeat",
            dyndispatch::__rtsadp_dyn_repeat as *const u8,
        ),
    ];
    syms.extend(gl_method_symbols());
    syms.extend(math_number_symbols());
    syms.extend(engine_symbols());
    syms.extend(test_framework_symbols());
    syms.extend(gc_internal_symbols());
    // Pilar 6: the REAL `__RTS_FN_GL_DATE_*` / `__RTS_FN_NS_DATE_*` symbols the
    // Registry-driven Date dispatch ([`crate::front::run::registry_call`]) emits
    // directly — replacing the `__rtsadp_date_*` trampolines that used to forward
    // to them.
    syms.extend(crate::registry_link::date_symbols());
    syms
}

/// The REAL Math namespace + Number static-predicate symbols the P5.4 `Math.*` /
/// `Number.*` lowering ([`crate::front::run::mathobj`]) emits. Each is the ACTUAL
/// `__RTS_FN_NS_MATH_*` / `__RTS_FN_GL_NUMBER_IS_*` extern (taken by address
/// through the facade). The `Math.sqrt`/`abs`/`min`/`max` intrinsics inline to
/// Cranelift IR and need NO symbol; only the genuine `call` ops appear here.
fn math_number_symbols() -> Vec<JitSymbol> {
    vec![
        // ---- Math 1-arg f64→f64 ----
        // sqrt/abs are also exposed as Cranelift INTRINSICS for the `Math.*` static
        // path (inlined, no symbol). But the BUILTIN-IMPORT path
        // (`import { sqrt } from "rts:math"`) marshals through the generic Registry
        // emitter, which always emits a real `call <symbol>` — so the actual extern
        // address must be installed here too.
        sym(
            "__RTS_FN_NS_MATH_SQRT",
            rt_math::__RTS_FN_NS_MATH_SQRT as *const u8,
        ),
        sym(
            "__RTS_FN_NS_MATH_ABS_F64",
            rt_math::__RTS_FN_NS_MATH_ABS_F64 as *const u8,
        ),
        sym(
            "__RTS_FN_NS_MATH_FLOOR",
            rt_math::__RTS_FN_NS_MATH_FLOOR as *const u8,
        ),
        sym(
            "__RTS_FN_NS_MATH_CEIL",
            rt_math::__RTS_FN_NS_MATH_CEIL as *const u8,
        ),
        sym(
            "__RTS_FN_NS_MATH_ROUND",
            rt_math::__RTS_FN_NS_MATH_ROUND as *const u8,
        ),
        sym(
            "__RTS_FN_NS_MATH_TRUNC",
            rt_math::__RTS_FN_NS_MATH_TRUNC as *const u8,
        ),
        sym(
            "__RTS_FN_NS_MATH_SIGN",
            rt_math::__RTS_FN_NS_MATH_SIGN as *const u8,
        ),
        sym(
            "__RTS_FN_NS_MATH_CBRT",
            rt_math::__RTS_FN_NS_MATH_CBRT as *const u8,
        ),
        sym(
            "__RTS_FN_NS_MATH_EXP",
            rt_math::__RTS_FN_NS_MATH_EXP as *const u8,
        ),
        sym(
            "__RTS_FN_NS_MATH_EXPM1",
            rt_math::__RTS_FN_NS_MATH_EXPM1 as *const u8,
        ),
        sym(
            "__RTS_FN_NS_MATH_LN",
            rt_math::__RTS_FN_NS_MATH_LN as *const u8,
        ),
        sym(
            "__RTS_FN_NS_MATH_LOG2",
            rt_math::__RTS_FN_NS_MATH_LOG2 as *const u8,
        ),
        sym(
            "__RTS_FN_NS_MATH_LOG10",
            rt_math::__RTS_FN_NS_MATH_LOG10 as *const u8,
        ),
        sym(
            "__RTS_FN_NS_MATH_LOG1P",
            rt_math::__RTS_FN_NS_MATH_LOG1P as *const u8,
        ),
        sym(
            "__RTS_FN_NS_MATH_SIN",
            rt_math::__RTS_FN_NS_MATH_SIN as *const u8,
        ),
        sym(
            "__RTS_FN_NS_MATH_COS",
            rt_math::__RTS_FN_NS_MATH_COS as *const u8,
        ),
        sym(
            "__RTS_FN_NS_MATH_TAN",
            rt_math::__RTS_FN_NS_MATH_TAN as *const u8,
        ),
        sym(
            "__RTS_FN_NS_MATH_ASIN",
            rt_math::__RTS_FN_NS_MATH_ASIN as *const u8,
        ),
        sym(
            "__RTS_FN_NS_MATH_ACOS",
            rt_math::__RTS_FN_NS_MATH_ACOS as *const u8,
        ),
        sym(
            "__RTS_FN_NS_MATH_ATAN",
            rt_math::__RTS_FN_NS_MATH_ATAN as *const u8,
        ),
        sym(
            "__RTS_FN_NS_MATH_SINH",
            rt_math::__RTS_FN_NS_MATH_SINH as *const u8,
        ),
        sym(
            "__RTS_FN_NS_MATH_COSH",
            rt_math::__RTS_FN_NS_MATH_COSH as *const u8,
        ),
        sym(
            "__RTS_FN_NS_MATH_TANH",
            rt_math::__RTS_FN_NS_MATH_TANH as *const u8,
        ),
        sym(
            "__RTS_FN_NS_MATH_FROUND",
            rt_math::__RTS_FN_NS_MATH_FROUND as *const u8,
        ),
        // ---- Math 2-arg f64,f64→f64 ----
        sym(
            "__RTS_FN_NS_MATH_POW",
            rt_math::__RTS_FN_NS_MATH_POW as *const u8,
        ),
        sym(
            "__RTS_FN_NS_MATH_ATAN2",
            rt_math::__RTS_FN_NS_MATH_ATAN2 as *const u8,
        ),
        sym(
            "__RTS_FN_NS_MATH_HYPOT",
            rt_math::__RTS_FN_NS_MATH_HYPOT as *const u8,
        ),
        // ---- Math no-arg (PRNG) ----
        sym(
            "__RTS_FN_NS_MATH_RANDOM_F64",
            rt_math::__RTS_FN_NS_MATH_RANDOM_F64 as *const u8,
        ),
        // ---- Number static predicates (f64→Bool) ----
        sym(
            "__RTS_FN_GL_NUMBER_IS_INTEGER",
            rt_num::__RTS_FN_GL_NUMBER_IS_INTEGER as *const u8,
        ),
        sym(
            "__RTS_FN_GL_NUMBER_IS_FINITE",
            rt_num::__RTS_FN_GL_NUMBER_IS_FINITE as *const u8,
        ),
        sym(
            "__RTS_FN_GL_NUMBER_IS_NAN",
            rt_num::__RTS_FN_GL_NUMBER_IS_NAN as *const u8,
        ),
        sym(
            "__RTS_FN_GL_NUMBER_IS_SAFE_INT",
            rt_num::__RTS_FN_GL_NUMBER_IS_SAFE_INT as *const u8,
        ),
    ]
}

/// The REAL global-class instance-method symbols the P4 data-driven dispatch
/// ([`crate::dispatch`]) resolves and the method lowering emits — String + Number
/// instance methods. Each is the ACTUAL `__RTS_FN_GL_*` extern (taken by address
/// through the facade); the dispatch metadata references these exact names, so
/// the symbol set is EXACTLY what the lowering can emit (a referenced-but-missing
/// symbol would be the SIGILL-class bug we avoid).
fn gl_method_symbols() -> Vec<JitSymbol> {
    vec![
        // ---- String instance methods STILL emitted by JIT-lowered code ----
        // The bulk of the String surface migrated to the `.ts` `class String`
        // (routed via `try_primitive_class_method`); its bodies call `engine.str_*`,
        // which call the `__RTS_FN_GL_STRING_*` impls as a normal Rust→Rust call
        // inside rts-std (linked there, NOT via JIT). The symbols below are the ones
        // the engine's OWN lowering still emits directly: the KEPT `STRING_ROWS`
        // (`codePointAt`/`localeCompare`/2-arg `substr`) and the `try_string_special`
        // 1-arg `slice`/`substring`/`substr` specials.
        sym(
            "__RTS_FN_GL_STRING_SLICE",
            rt_gl_str::__RTS_FN_GL_STRING_SLICE as *const u8,
        ),
        sym(
            "__RTS_FN_GL_STRING_SUBSTRING",
            rt_gl_str::__RTS_FN_GL_STRING_SUBSTRING as *const u8,
        ),
        sym(
            "__RTS_FN_GL_STRING_SUBSTR",
            rt_gl_str::__RTS_FN_GL_STRING_SUBSTR as *const u8,
        ),
        sym(
            "__RTS_FN_GL_STRING_CODE_POINT_AT",
            rt_gl_str::__RTS_FN_GL_STRING_CODE_POINT_AT as *const u8,
        ),
        sym(
            "__RTS_FN_GL_STRING_LOCALE_COMPARE",
            rt_gl_str::__RTS_FN_GL_STRING_LOCALE_COMPARE as *const u8,
        ),
        // ---- Number instance methods (rts-primitives number) ----
        sym(
            "__RTS_FN_GL_NUMBER_TO_FIXED",
            rt_num::__RTS_FN_GL_NUMBER_TO_FIXED as *const u8,
        ),
        sym(
            "__RTS_FN_GL_NUMBER_TO_PRECISION",
            rt_num::__RTS_FN_GL_NUMBER_TO_PRECISION as *const u8,
        ),
        sym(
            "__RTS_FN_GL_NUMBER_TO_EXPONENTIAL",
            rt_num::__RTS_FN_GL_NUMBER_TO_EXPONENTIAL as *const u8,
        ),
        sym(
            "__RTS_FN_GL_NUMBER_TO_STRING_RADIX",
            rt_num::__RTS_FN_GL_NUMBER_TO_STRING_RADIX as *const u8,
        ),
    ]
}

/// The PRIVATE `engine` namespace symbols (`__RTS_FN_NS_ENGINE_*`) the engine-
/// internal TS prelude can call (arch/time/trace passthrough). Each is the ACTUAL
/// `rts-std` extern taken by address through the facade. The privacy gate is at
/// the lowering layer (only prelude-origin code may name the `engine` global); the
/// symbols are installed unconditionally so prelude code links.
fn engine_symbols() -> Vec<JitSymbol> {
    vec![
        sym(
            "__RTS_FN_NS_ENGINE_ARCH",
            rt_engine::__RTS_FN_NS_ENGINE_ARCH as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_NOW_MS",
            rt_engine::__RTS_FN_NS_ENGINE_NOW_MS as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_NOW_NS",
            rt_engine::__RTS_FN_NS_ENGINE_NOW_NS as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_UNIX_MS",
            rt_engine::__RTS_FN_NS_ENGINE_UNIX_MS as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_UNIX_NS",
            rt_engine::__RTS_FN_NS_ENGINE_UNIX_NS as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_TRACE_PUSH",
            rt_engine::__RTS_FN_NS_ENGINE_TRACE_PUSH as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_TRACE_POP",
            rt_engine::__RTS_FN_NS_ENGINE_TRACE_POP as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_TRACE_CAPTURE",
            rt_engine::__RTS_FN_NS_ENGINE_TRACE_CAPTURE as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_TRACE_PRINT",
            rt_engine::__RTS_FN_NS_ENGINE_TRACE_PRINT as *const u8,
        ),
        // engine.num_* — the irreducible numeric FORMATTING bridge the `.ts`
        // `class Number` methods call (each wraps a `__RTS_FN_GL_NUMBER_*`).
        sym(
            "__RTS_FN_NS_ENGINE_NUM_TO_STRING_RADIX",
            rt_engine::__RTS_FN_NS_ENGINE_NUM_TO_STRING_RADIX as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_NUM_TO_FIXED",
            rt_engine::__RTS_FN_NS_ENGINE_NUM_TO_FIXED as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_NUM_TO_PRECISION",
            rt_engine::__RTS_FN_NS_ENGINE_NUM_TO_PRECISION as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_NUM_TO_EXPONENTIAL",
            rt_engine::__RTS_FN_NS_ENGINE_NUM_TO_EXPONENTIAL as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_NUM_FROM_STR",
            rt_engine::__RTS_FN_NS_ENGINE_NUM_FROM_STR as *const u8,
        ),
        // engine.str_* — the irreducible Unicode string-logic bridge the `.ts`
        // `class String` methods call (each wraps a `__RTS_FN_GL_STRING_*`).
        sym(
            "__RTS_FN_NS_ENGINE_STR_TO_UPPER",
            rt_engine::__RTS_FN_NS_ENGINE_STR_TO_UPPER as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_STR_TO_LOWER",
            rt_engine::__RTS_FN_NS_ENGINE_STR_TO_LOWER as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_STR_TRIM",
            rt_engine::__RTS_FN_NS_ENGINE_STR_TRIM as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_STR_TRIM_START",
            rt_engine::__RTS_FN_NS_ENGINE_STR_TRIM_START as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_STR_TRIM_END",
            rt_engine::__RTS_FN_NS_ENGINE_STR_TRIM_END as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_STR_CHAR_AT",
            rt_engine::__RTS_FN_NS_ENGINE_STR_CHAR_AT as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_STR_CHAR_CODE_AT",
            rt_engine::__RTS_FN_NS_ENGINE_STR_CHAR_CODE_AT as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_STR_AT",
            rt_engine::__RTS_FN_NS_ENGINE_STR_AT as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_STR_REPEAT",
            rt_engine::__RTS_FN_NS_ENGINE_STR_REPEAT as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_STR_SLICE",
            rt_engine::__RTS_FN_NS_ENGINE_STR_SLICE as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_STR_SUBSTRING",
            rt_engine::__RTS_FN_NS_ENGINE_STR_SUBSTRING as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_STR_INDEX_OF",
            rt_engine::__RTS_FN_NS_ENGINE_STR_INDEX_OF as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_STR_LAST_INDEX_OF",
            rt_engine::__RTS_FN_NS_ENGINE_STR_LAST_INDEX_OF as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_STR_INCLUDES",
            rt_engine::__RTS_FN_NS_ENGINE_STR_INCLUDES as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_STR_STARTS_WITH",
            rt_engine::__RTS_FN_NS_ENGINE_STR_STARTS_WITH as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_STR_ENDS_WITH",
            rt_engine::__RTS_FN_NS_ENGINE_STR_ENDS_WITH as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_STR_PAD_START",
            rt_engine::__RTS_FN_NS_ENGINE_STR_PAD_START as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_STR_PAD_END",
            rt_engine::__RTS_FN_NS_ENGINE_STR_PAD_END as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_STR_CONCAT",
            rt_engine::__RTS_FN_NS_ENGINE_STR_CONCAT as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_STR_REPLACE",
            rt_engine::__RTS_FN_NS_ENGINE_STR_REPLACE as *const u8,
        ),
        sym(
            "__RTS_FN_NS_ENGINE_STR_REPLACE_ALL",
            rt_engine::__RTS_FN_NS_ENGINE_STR_REPLACE_ALL as *const u8,
        ),
    ]
}

/// The `rts:test` FRAMEWORK backing symbols + the FULL `test_core`/`string`/`fmt`
/// namespace surfaces. Harvested from the REAL Registry (each member's real
/// `fn_ptr`) rather than hand-listed: once these namespaces are registered
/// ([`crate::front::run::registry`]), ANY member is resolvable via
/// `namespace_member` (`import { byte_len } from "rts:string"`), so EVERY member's
/// symbol must be installed or an emitted `call` is a link-OK/runtime-SIGILL — a
/// honesty-floor violation. Harvesting installs the whole surface in one shot and
/// stays in sync with the namespace automatically (null/alias members skipped by
/// `namespace_jit_symbols`).
fn test_framework_symbols() -> Vec<JitSymbol> {
    let mut out = Vec::new();
    // The FULL Registry harvest: every member with a real `fn_ptr` across ALL
    // registered namespaces (io/math/date/test_core/fmt + the broad std surface
    // fs/time/atomic/num/… registered in `registry::build_registry`). Installing the
    // whole table makes every `import { x } from "rts:<ns>"` SIGILL-safe. Duplicates
    // with the hand-listed entries above are harmless (JITBuilder::symbol last-wins,
    // same address).
    for (name, ptr) in crate::front::run::registry::all_jit_symbols() {
        out.push(sym(name, ptr));
    }
    // The `string` namespace declares its members with a NULL `fn_ptr` (the real
    // bodies live in `rts-primitives::string`, installed by address — the same
    // null-skip pattern as the `gc` pool). The harvest skips them, so we install
    // the FULL surface by address here; every member must be present or a user
    // `import { m } from "rts:string"` resolves then SIGILLs (honesty floor).
    out.extend([
        sym("__RTS_FN_NS_STRING_CONTAINS", rt_str_search::__RTS_FN_NS_STRING_CONTAINS as *const u8),
        sym("__RTS_FN_NS_STRING_STARTS_WITH", rt_str_search::__RTS_FN_NS_STRING_STARTS_WITH as *const u8),
        sym("__RTS_FN_NS_STRING_ENDS_WITH", rt_str_search::__RTS_FN_NS_STRING_ENDS_WITH as *const u8),
        sym("__RTS_FN_NS_STRING_FIND", rt_str_search::__RTS_FN_NS_STRING_FIND as *const u8),
        sym("__RTS_FN_NS_STRING_TO_UPPER", rt_str_transform::__RTS_FN_NS_STRING_TO_UPPER as *const u8),
        sym("__RTS_FN_NS_STRING_TO_LOWER", rt_str_transform::__RTS_FN_NS_STRING_TO_LOWER as *const u8),
        sym("__RTS_FN_NS_STRING_TRIM", rt_str_transform::__RTS_FN_NS_STRING_TRIM as *const u8),
        sym("__RTS_FN_NS_STRING_TRIM_START", rt_str_transform::__RTS_FN_NS_STRING_TRIM_START as *const u8),
        sym("__RTS_FN_NS_STRING_TRIM_END", rt_str_transform::__RTS_FN_NS_STRING_TRIM_END as *const u8),
        sym("__RTS_FN_NS_STRING_REPEAT", rt_str_transform::__RTS_FN_NS_STRING_REPEAT as *const u8),
        sym("__RTS_FN_NS_STRING_REPLACE", rt_str_replace::__RTS_FN_NS_STRING_REPLACE as *const u8),
        sym("__RTS_FN_NS_STRING_REPLACEN", rt_str_replace::__RTS_FN_NS_STRING_REPLACEN as *const u8),
        sym("__RTS_FN_NS_STRING_BYTE_LEN", rt_str_split::__RTS_FN_NS_STRING_BYTE_LEN as *const u8),
        sym("__RTS_FN_NS_STRING_CHAR_AT", rt_str_split::__RTS_FN_NS_STRING_CHAR_AT as *const u8),
        sym("__RTS_FN_NS_STRING_CHAR_CODE_AT", rt_str_split::__RTS_FN_NS_STRING_CHAR_CODE_AT as *const u8),
        sym("__RTS_FN_NS_STRING_CHAR_COUNT", rt_str_split::__RTS_FN_NS_STRING_CHAR_COUNT as *const u8),
    ]);
    out
}

/// The `gc` namespace's INTERNAL real symbols (collector / heap env+instance /
/// string-pool inspection) whose `Member.fn_ptr` is NULL (the owning submodule
/// holds the address, like the string pool), so the Registry harvest skips them.
/// The `gc` namespace is registered (the bundle uses `gc.string_*`), which makes
/// these resolvable via `gc.live_count()` / `import {…} from "rts:gc"`; install
/// every one by address or such a call link-OK/runtime-SIGILLs (§7). GCELL_GET/SET
/// and the string-pool string ops are already in the main list above.
fn gc_internal_symbols() -> Vec<JitSymbol> {
    use rts_engine::heap::closure as rt_clos;
    use rts_runtime::namespaces::gc::collector as rt_gcoll;
    use rts_runtime::namespaces::gc::string_pool as rt_pool;
    vec![
        // heap closure (closure-as-value env)
        sym("__RTS_FN_NS_GC_CLOSURE_ALLOC", rt_clos::__RTS_FN_NS_GC_CLOSURE_ALLOC as *const u8),
        sym("__RTS_FN_NS_GC_CLOSURE_ENV", rt_clos::__RTS_FN_NS_GC_CLOSURE_ENV as *const u8),
        sym("__RTS_FN_NS_GC_CLOSURE_FN_PTR", rt_clos::__RTS_FN_NS_GC_CLOSURE_FN_PTR as *const u8),
        // collector
        sym("__RTS_FN_NS_GC_COLLECT", rt_gcoll::__RTS_FN_NS_GC_COLLECT as *const u8),
        sym("__RTS_FN_NS_GC_COLLECT_DEBT", rt_gcoll::__RTS_FN_NS_GC_COLLECT_DEBT as *const u8),
        sym("__RTS_FN_NS_GC_COLLECT_VEC", rt_gcoll::__RTS_FN_NS_GC_COLLECT_VEC as *const u8),
        sym("__RTS_FN_NS_GC_LIVE_COUNT", rt_gcoll::__RTS_FN_NS_GC_LIVE_COUNT as *const u8),
        // heap env-record
        sym("__RTS_FN_NS_GC_ENV_ALLOC", rt_env::__RTS_FN_NS_GC_ENV_ALLOC as *const u8),
        sym("__RTS_FN_NS_GC_ENV_FREE", rt_env::__RTS_FN_NS_GC_ENV_FREE as *const u8),
        sym("__RTS_FN_NS_GC_ENV_GET", rt_env::__RTS_FN_NS_GC_ENV_GET as *const u8),
        sym("__RTS_FN_NS_GC_ENV_SET", rt_env::__RTS_FN_NS_GC_ENV_SET as *const u8),
        // heap instance
        sym("__RTS_FN_NS_GC_INSTANCE_NEW", rt_inst::__RTS_FN_NS_GC_INSTANCE_NEW as *const u8),
        sym("__RTS_FN_NS_GC_INSTANCE_FREE", rt_inst::__RTS_FN_NS_GC_INSTANCE_FREE as *const u8),
        sym("__RTS_FN_NS_GC_INSTANCE_CLASS", rt_inst::__RTS_FN_NS_GC_INSTANCE_CLASS as *const u8),
        sym("__RTS_FN_NS_GC_INSTANCE_LOAD_I64", rt_inst::__RTS_FN_NS_GC_INSTANCE_LOAD_I64 as *const u8),
        sym("__RTS_FN_NS_GC_INSTANCE_LOAD_I32", rt_inst::__RTS_FN_NS_GC_INSTANCE_LOAD_I32 as *const u8),
        sym("__RTS_FN_NS_GC_INSTANCE_LOAD_F64", rt_inst::__RTS_FN_NS_GC_INSTANCE_LOAD_F64 as *const u8),
        sym("__RTS_FN_NS_GC_INSTANCE_STORE_I64", rt_inst::__RTS_FN_NS_GC_INSTANCE_STORE_I64 as *const u8),
        sym("__RTS_FN_NS_GC_INSTANCE_STORE_I32", rt_inst::__RTS_FN_NS_GC_INSTANCE_STORE_I32 as *const u8),
        sym("__RTS_FN_NS_GC_INSTANCE_STORE_F64", rt_inst::__RTS_FN_NS_GC_INSTANCE_STORE_F64 as *const u8),
        // string-pool inspection
        sym("__RTS_FN_NS_GC_HANDLE_LEN", rt_pool::__RTS_FN_NS_GC_HANDLE_LEN as *const u8),
        sym("__RTS_FN_NS_GC_IS_VEC", rt_pool::__RTS_FN_NS_GC_IS_VEC as *const u8),
        sym("__RTS_FN_NS_GC_IS_MAP_LIKE", rt_pool::__RTS_FN_NS_GC_IS_MAP_LIKE as *const u8),
        sym("__RTS_FN_NS_GC_IS_DATE", rt_pool::__RTS_FN_NS_GC_IS_DATE as *const u8),
        sym("__RTS_FN_NS_GC_IS_PROMISE", rt_pool::__RTS_FN_NS_GC_IS_PROMISE as *const u8),
        sym("__RTS_FN_NS_GC_IS_REGEX", rt_pool::__RTS_FN_NS_GC_IS_REGEX as *const u8),
    ]
}

#[inline]
fn sym(name: &'static str, ptr: *const u8) -> JitSymbol {
    JitSymbol { name, ptr }
}
