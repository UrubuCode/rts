//! Engine-direct JIT symbols + the Registry-harvest composition for the
//! `JITBuilder`.
//!
//! [`jit_symbols`] assembles the full JIT symbol table from THREE sources:
//!  1. **Registry harvest** (`front::run::registry::all_jit_symbols`) — every
//!     genuine namespace/class MEMBER that carries a real `fn_ptr` (e.g. the
//!     `string` ns via `fp_for`, io, math, Date/URL class methods). The bulk; it
//!     stays in sync with the Registry automatically, no hand list.
//!  2. **`__rtsadp_*` adapter trampolines** — the engine's OWN codegen-owned
//!     symbols (`crate::value::*`): the generic `+`, shape/IC obj access, function
//!     values, dynamic dispatch. Not runtime, not in any Registry — listed here.
//!  3. **Engine-internal runtime primitives** — `__RTS_FN_NS_GC_*` env / generator
//!     / GEN_SM / string-pool ops the engine emits DIRECTLY as codegen sentinels
//!     (NOT exposed as `rts:gc` members; the `gc` ns deliberately surfaces only
//!     `collect`/`live_count`). The harvest cannot supply them (no member to hold
//!     the address), so they are listed here too.
//!
//! Sources 2+3 are the irreducible "engine-direct" set — symbols the engine NAMES
//! itself rather than resolving via the Registry. Source 1 used to be hand-listed
//! here too; those entries drain out as each namespace's `register` is converted
//! to carry real `fn_ptr`s (the `dataview`/`string` `fp_for` pattern).
//!
//! This replaces the fake `crate::runtime::symbols()` table. It is the new
//! engine's analogue of the old engine's `register_runtime_symbols` /
//! `runtime_jit_symbols` bridge (`rts-codegen-old/src/abi/mod.rs` +
//! `codegen/jit.rs`) — but built against the `rts-runtime` FACADE, NOT by
//! depending on the frozen old crate.
//!
//! ## Why sources 2+3 are listed by hand (the harvest cannot supply them)
//!
//! The harvest (`registry().jit_symbols()`) only yields members with a real,
//! non-null `fn_ptr` (the **null-skip** invariant: alias/external members carry a
//! null fn_ptr). The engine-internal `__RTS_FN_NS_GC_*` primitives (string-pool /
//! env / generator / GEN_SM / gcell / poly-bridge) and a few directly-emitted
//! class methods (`__RTS_FN_GL_STRING_*` slice/substr/codePointAt/localeCompare,
//! emitted by `try_string_special`) are NOT registered as harvestable members —
//! the engine NAMES them itself in its lowering — so there is no member to attach
//! an address to. We take each one's address directly through the facade re-export
//! and list it here. A genuine namespace/class MEMBER, by contrast, carries its
//! real address at registration (`fp_for`) and is installed by the harvest with no
//! hand entry (string ns / Date / URL already converted).

use rts_engine::heap::env as rt_env;
use rts_runtime::namespaces::gc::collector as rt_gcoll;
use rts_runtime::namespaces::gc::handles as rt_handles;
use rts_runtime::namespaces::gc::string_pool as rt_str;
use rts_runtime::namespaces::globals::string::rt as rt_gl_str;
use rts_runtime::namespaces::globals::proxy::ops as rt_proxy;

use crate::value::{
    abi_adapter, arraycb, arrayops, ctorval, dyndispatch, errslot, funcops, genops, genops_arith,
    globalops, globalthis, inspect, iterops, objops, regexops,
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
        sym(
            "__rtsadp_to_iter_array",
            iterops::__rtsadp_to_iter_array as *const u8,
        ),
        // string→string GLOBAL fns: URI codecs (rts-shared global_this) + btoa/atob
        // (rts-std text_encoding). Wired in `globals::lower_str_global`.
        sym(
            "__RTS_FN_GL_ENCODE_URI",
            rts_runtime::namespaces::globals::global_this::rt::__RTS_FN_GL_ENCODE_URI as *const u8,
        ),
        sym(
            "__RTS_FN_GL_DECODE_URI",
            rts_runtime::namespaces::globals::global_this::rt::__RTS_FN_GL_DECODE_URI as *const u8,
        ),
        sym(
            "__RTS_FN_GL_ENCODE_URI_COMPONENT",
            rts_runtime::namespaces::globals::global_this::rt::__RTS_FN_GL_ENCODE_URI_COMPONENT
                as *const u8,
        ),
        sym(
            "__RTS_FN_GL_DECODE_URI_COMPONENT",
            rts_runtime::namespaces::globals::global_this::rt::__RTS_FN_GL_DECODE_URI_COMPONENT
                as *const u8,
        ),
        sym(
            "__RTS_FN_GL_TEXTENC_BTOA",
            rts_runtime::namespaces::globals::text_encoding::instance::__RTS_FN_GL_TEXTENC_BTOA
                as *const u8,
        ),
        sym(
            "__RTS_FN_GL_TEXTENC_ATOB",
            rts_runtime::namespaces::globals::text_encoding::instance::__RTS_FN_GL_TEXTENC_ATOB
                as *const u8,
        ),
        // TextEncoder/TextDecoder class ctor + instance methods (Registry class).
        sym(
            "__RTS_FN_GL_TEXTENC_NEW",
            rts_runtime::namespaces::globals::text_encoding::instance::__RTS_FN_GL_TEXTENC_NEW
                as *const u8,
        ),
        sym(
            "__RTS_FN_GL_TEXTENC_ENCODE_INSTANCE",
            rts_runtime::namespaces::globals::text_encoding::instance::__RTS_FN_GL_TEXTENC_ENCODE_INSTANCE
                as *const u8,
        ),
        sym(
            "__RTS_FN_GL_TEXTDEC_NEW",
            rts_runtime::namespaces::globals::text_encoding::instance::__RTS_FN_GL_TEXTDEC_NEW
                as *const u8,
        ),
        sym(
            "__RTS_FN_GL_TEXTDEC_DECODE_INSTANCE",
            rts_runtime::namespaces::globals::text_encoding::instance::__RTS_FN_GL_TEXTDEC_DECODE_INSTANCE
                as *const u8,
        ),
        // ---- codegen-owned FUNCTION-value trampolines (__rtsadp_fn_*, P4.6) ----
        sym("__rtsadp_fn_reify", funcops::__rtsadp_fn_reify as *const u8),
        sym(
            "__rtsadp_fn_invoke",
            funcops::__rtsadp_fn_invoke as *const u8,
        ),
        // ---- new <value>() through a class VALUE (slice 2) ----
        sym(
            "__rtsadp_register_ctor_thunk",
            ctorval::__rtsadp_register_ctor_thunk as *const u8,
        ),
        sym(
            "__rtsadp_new_invoke",
            ctorval::__rtsadp_new_invoke as *const u8,
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
        // ---- the `globalThis` singleton object (value get/set foundation) ----
        sym("__rtsadp_globalthis", globalthis::__rtsadp_globalthis as *const u8),
        // ---- codegen-owned DYNAMIC property access (objops, P5.5) ----
        sym("__rtsadp_obj_get", objops::__rtsadp_obj_get as *const u8),
        sym("__rtsadp_obj_set", objops::__rtsadp_obj_set as *const u8),
        sym("__rtsadp_obj_has", objops::__rtsadp_obj_has as *const u8),
        sym("__rtsadp_obj_delete", objops::__rtsadp_obj_delete as *const u8),
        sym("__rtsadp_obj_values", objops::__rtsadp_obj_values as *const u8),
        sym("__rtsadp_obj_entries", objops::__rtsadp_obj_entries as *const u8),
        sym(
            "__rtsadp_obj_from_entries",
            objops::__rtsadp_obj_from_entries as *const u8,
        ),
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
        sym("__rtsadp_idx_get", dyndispatch::__rtsadp_idx_get as *const u8),
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
    syms.extend(test_framework_symbols());
    syms.extend(gc_internal_symbols());
    syms.extend(generator_symbols());
    // Pilar 6: the REAL `__RTS_FN_GL_DATE_*` / `__RTS_FN_NS_DATE_*` symbols the
    // Registry-driven Date dispatch ([`crate::front::run::registry_call`]) emits
    // directly — replacing the `__rtsadp_date_*` trampolines that used to forward
    // to them.
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
        // String class methods the engine's OWN lowering emits DIRECTLY
        // (try_string_special 1-arg slice/substring/substr + the kept STRING_ROWS
        // codePointAt/localeCompare). These are NOT harvestable members (the rest of
        // the String surface routes via the `.ts` class, Rust→Rust), so they are
        // engine-direct and listed here.
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
        // Proxy ctor (#218): `new Proxy(target, handler)` → `Entry::Proxy`. The
        // get/set TRAPS run inside `__rtsadp_obj_get`/`_set` (engine trampolines,
        // already installed); only the ctor symbol needs installing here.
        sym(
            "__RTS_FN_GL_PROXY_NEW",
            rt_proxy::__RTS_FN_GL_PROXY_NEW as *const u8,
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
    // NOTE: the `string` namespace externs (`__RTS_FN_NS_STRING_*`) used to be
    // hand-listed here because they were declared with a NULL `fn_ptr`. They now
    // carry their real address at registration (`rts-primitives::string::fp_for`)
    // so the harvest above installs them — no manual list needed.
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
    use rts_runtime::namespaces::gc::collector as rt_gcoll;
    use rts_runtime::namespaces::gc::string_pool as rt_pool;
    vec![
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
        // string-pool inspection
        sym("__RTS_FN_NS_GC_HANDLE_LEN", rt_pool::__RTS_FN_NS_GC_HANDLE_LEN as *const u8),
        sym("__RTS_FN_NS_GC_IS_VEC", rt_pool::__RTS_FN_NS_GC_IS_VEC as *const u8),
        sym("__RTS_FN_NS_GC_IS_MAP_LIKE", rt_pool::__RTS_FN_NS_GC_IS_MAP_LIKE as *const u8),
        sym("__RTS_FN_NS_GC_IS_DATE", rt_pool::__RTS_FN_NS_GC_IS_DATE as *const u8),
        sym("__RTS_FN_NS_GC_IS_PROMISE", rt_pool::__RTS_FN_NS_GC_IS_PROMISE as *const u8),
        sym("__RTS_FN_NS_GC_IS_REGEX", rt_pool::__RTS_FN_NS_GC_IS_REGEX as *const u8),
    ]
}

/// SYNC GENERATOR runtime primitives (eager-buffer MVP). The parser desugars a
/// `function* g(){ yield a; … }` into a plain fn that builds an array `__gen_buf`
/// and ends `return __RTS_GEN_FINISH(__gen_buf, ret)`; the engine maps the
/// `__RTS_GEN_FINISH`/`__RTS_GEN_GET_RET` sentinels (see `call.rs`) to these real
/// externs, and `gen().next()` routes to `GENERATOR_NEXT`. (The lazy state-machine
/// set `GEN_SM_*` is a later phase.)
fn generator_symbols() -> Vec<JitSymbol> {
    use rts_runtime::namespaces::gc::generator as rt_gen;
    vec![
        sym(
            "__RTS_FN_NS_GC_GENERATOR_SET_RET",
            rt_gen::__RTS_FN_NS_GC_GENERATOR_SET_RET as *const u8,
        ),
        sym(
            "__RTS_FN_NS_GC_GENERATOR_GET_RET",
            rt_gen::__RTS_FN_NS_GC_GENERATOR_GET_RET as *const u8,
        ),
        sym(
            "__RTS_FN_NS_GC_GENERATOR_NEXT",
            rt_gen::__RTS_FN_NS_GC_GENERATOR_NEXT as *const u8,
        ),
        sym(
            "__RTS_FN_NS_GC_GENERATOR_NEXT_SENT",
            rt_gen::__RTS_FN_NS_GC_GENERATOR_NEXT_SENT as *const u8,
        ),
        sym(
            "__RTS_FN_NS_GC_GENERATOR_RETURN",
            rt_gen::__RTS_FN_NS_GC_GENERATOR_RETURN as *const u8,
        ),
        sym(
            "__RTS_FN_NS_GC_GENERATOR_THROW",
            rt_gen::__RTS_FN_NS_GC_GENERATOR_THROW as *const u8,
        ),
        // LAZY state-machine primitives (generators with loops / yield*).
        sym("__RTS_FN_NS_GC_GEN_SM_NEW", rt_gen::__RTS_FN_NS_GC_GEN_SM_NEW as *const u8),
        sym("__RTS_FN_NS_GC_GEN_SM_STATE", rt_gen::__RTS_FN_NS_GC_GEN_SM_STATE as *const u8),
        sym("__RTS_FN_NS_GC_GEN_SM_SETSTATE", rt_gen::__RTS_FN_NS_GC_GEN_SM_SETSTATE as *const u8),
        sym("__RTS_FN_NS_GC_GEN_SM_FGET", rt_gen::__RTS_FN_NS_GC_GEN_SM_FGET as *const u8),
        sym("__RTS_FN_NS_GC_GEN_SM_FSET", rt_gen::__RTS_FN_NS_GC_GEN_SM_FSET as *const u8),
        sym("__RTS_FN_NS_GC_GEN_SM_YIELD", rt_gen::__RTS_FN_NS_GC_GEN_SM_YIELD as *const u8),
        sym("__RTS_FN_NS_GC_GEN_SM_DONE", rt_gen::__RTS_FN_NS_GC_GEN_SM_DONE as *const u8),
        sym("__RTS_FN_NS_GC_GEN_SM_SENT", rt_gen::__RTS_FN_NS_GC_GEN_SM_SENT as *const u8),
        sym("__RTS_FN_NS_GC_GEN_SM_NEXT", rt_gen::__RTS_FN_NS_GC_GEN_SM_NEXT as *const u8),
        sym("__RTS_FN_NS_GC_GEN_SM_DRAIN", rt_gen::__RTS_FN_NS_GC_GEN_SM_DRAIN as *const u8),
        sym("__RTS_FN_NS_GC_GEN_SM_ENTER_TRY", rt_gen::__RTS_FN_NS_GC_GEN_SM_ENTER_TRY as *const u8),
        sym("__RTS_FN_NS_GC_GEN_SM_ENTER_TRY_CATCH", rt_gen::__RTS_FN_NS_GC_GEN_SM_ENTER_TRY_CATCH as *const u8),
        sym("__RTS_FN_NS_GC_GEN_SM_EXIT_TRY_CATCH", rt_gen::__RTS_FN_NS_GC_GEN_SM_EXIT_TRY_CATCH as *const u8),
        sym("__RTS_FN_NS_GC_GEN_SM_CAUGHT", rt_gen::__RTS_FN_NS_GC_GEN_SM_CAUGHT as *const u8),
        sym("__RTS_FN_NS_GC_GEN_SM_END_FINALLY", rt_gen::__RTS_FN_NS_GC_GEN_SM_END_FINALLY as *const u8),
        sym("__RTS_FN_NS_GC_GEN_DELEGATE_START", rt_gen::__RTS_FN_NS_GC_GEN_DELEGATE_START as *const u8),
        sym("__RTS_FN_NS_GC_GEN_DELEGATE_NEXT", rt_gen::__RTS_FN_NS_GC_GEN_DELEGATE_NEXT as *const u8),
        sym("__RTS_FN_NS_GC_GEN_DELEGATE_DONE", rt_gen::__RTS_FN_NS_GC_GEN_DELEGATE_DONE as *const u8),
        // `{value, done}` result-Map accessors (new engine builds its own result obj).
        sym("__RTS_FN_NS_GC_ITER_VALUE", rt_gen::__RTS_FN_NS_GC_ITER_VALUE as *const u8),
        sym("__RTS_FN_NS_GC_ITER_DONE", rt_gen::__RTS_FN_NS_GC_ITER_DONE as *const u8),
    ]
}

#[inline]
fn sym(name: &'static str, ptr: *const u8) -> JitSymbol {
    JitSymbol { name, ptr }
}

#[cfg(test)]
mod tests {
    use super::jit_symbols;
    use std::collections::HashMap;

    /// Drift guard for the JIT symbol table (the design-doc coverage assert): every
    /// installed symbol must have a REAL address (no null slipped through from an
    /// `external` member whose `fp_for` forgot it — that is the link-OK / runtime-
    /// SIGILL class), and no symbol name may be installed with TWO different
    /// addresses (a harvest entry disagreeing with a hand entry — same-address
    /// duplicates are harmless and allowed). A failure here is exactly the bug the
    /// harvest migration exists to prevent.
    #[test]
    fn jit_symbols_have_no_null_and_no_conflicting_dupes() {
        let syms = jit_symbols();
        assert!(!syms.is_empty(), "the JIT symbol table is empty");
        let mut seen: HashMap<&str, *const u8> = HashMap::new();
        for s in &syms {
            assert!(!s.ptr.is_null(), "JIT symbol `{}` has a NULL fn_ptr", s.name);
            match seen.get(s.name) {
                Some(&prev) => assert_eq!(
                    prev, s.ptr,
                    "JIT symbol `{}` installed with two different addresses",
                    s.name
                ),
                None => {
                    seen.insert(s.name, s.ptr);
                }
            }
        }
    }
}
