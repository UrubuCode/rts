//! First half of the engine-direct JIT symbol list (split from the former
//! `adapter_symbols.rs` for the <500-line module rule). See [`super`].

use rts_runtime::namespaces::gc::collector as rt_gcoll;
use rts_runtime::namespaces::gc::handles as rt_handles;
use rts_runtime::namespaces::gc::string_pool as rt_str;

use crate::value::{abi_adapter, arrayops, genops, genops_arith, globalops, inspect};

use super::{JitSymbol, sym};

pub(super) fn symbols() -> Vec<JitSymbol> {
    vec![
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
            "__rtsadp_same_value",
            genops::__rtsadp_same_value as *const u8,
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
            "__rtsadp_arr_join0",
            arrayops::__rtsadp_arr_join0 as *const u8,
        ),
        sym(
            "__rtsadp_arr_slice0",
            arrayops::__rtsadp_arr_slice0 as *const u8,
        ),
        sym(
            "__rtsadp_arr_to_string",
            arrayops::__rtsadp_arr_to_string as *const u8,
        ),
        sym(
            "__rtsadp_arr_push",
            arrayops::__rtsadp_arr_push as *const u8,
        ),
        sym(
            "__rtsadp_arr_set_length",
            arrayops::__rtsadp_arr_set_length as *const u8,
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
            "__rtsadp_arr_splice",
            arrayops::__rtsadp_arr_splice as *const u8,
        ),
        sym(
            "__rtsadp_arr_to_spliced_var",
            arrayops::__rtsadp_arr_to_spliced_var as *const u8,
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
            "__rtsadp_arr_sort_cmp",
            arrayops::__rtsadp_arr_sort_cmp as *const u8,
        ),
        sym(
            "__rtsadp_arr_to_sorted",
            arrayops::__rtsadp_arr_to_sorted as *const u8,
        ),
        sym(
            "__rtsadp_arr_to_sorted_cmp",
            arrayops::__rtsadp_arr_to_sorted_cmp as *const u8,
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
        sym(
            "__rtsadp_arr_copy_within1",
            arrayops::__rtsadp_arr_copy_within1 as *const u8,
        ),
        sym(
            "__rtsadp_arr_fill2",
            arrayops::__rtsadp_arr_fill2 as *const u8,
        ),
        sym(
            "__rtsadp_arr_fill3",
            arrayops::__rtsadp_arr_fill3 as *const u8,
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
            "__rtsadp_num_is_nan",
            globalops::__rtsadp_num_is_nan as *const u8,
        ),
        sym(
            "__rtsadp_num_is_finite",
            globalops::__rtsadp_num_is_finite as *const u8,
        ),
        sym(
            "__rtsadp_num_is_integer",
            globalops::__rtsadp_num_is_integer as *const u8,
        ),
        sym(
            "__rtsadp_num_is_safe_integer",
            globalops::__rtsadp_num_is_safe_integer as *const u8,
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
    ]
}
