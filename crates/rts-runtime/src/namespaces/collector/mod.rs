//! `gc` namespace — runtime value storage.
//!
//! Owns the handle table that backs dynamically allocated strings (and,
//! eventually, objects, arrays, buffers). Every handle-returning function
//! across the codebase ultimately calls into this module's tables.
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`). The 34 members are declared `external`:
//! their `#[no_mangle] extern "C"` bodies live in the submodules below
//! (`string_pool`, `env`, `closure`, `instance`, `collector`, `handles`), and
//! the macro only derives the `MEMBERS`/`SPEC` table referencing those symbols
//! — it does not re-emit the externs.

#[allow(unused_imports)]
use rts_engine::abi::ty::{F64, Handle, I32, I64, Str, U64};
use rts_macro::rts_namespace;

pub mod class_registry;
pub mod closure;
pub mod collector;
pub mod env;
pub mod error;
pub mod generator;
// `global_roots`, `stack_map_registry`, `thread_registry`, `debug` e o `scan`
// (scanner conservativo) migraram pro `rts-engine` (SPLIT fatias 3a-3c —
// mecanismo std/FFI escrito pelo codegen / usado pelo scanner). Re-exportados
// aqui como fachada: os `super::` internos + `namespaces::gc::*` (alias) +
// jit.rs resolvem pro MESMO static no engine, sem editar consumidores.
pub use rts_engine::collector::{debug, global_roots, scan, stack_map_registry, thread_registry};
pub mod handles;
pub mod instance;
pub mod promise_slot;
pub mod stack;
pub mod string_pool;
pub mod tagged_raw;
pub mod this_slot;

/// Runtime-managed handle table and string pool. All members are `external` —
/// the externs are owned by the submodules; the macro emits only the SPEC.
#[rts_namespace(gc)]
impl GcNs {
    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_STRING_FROM_I64",
        ts = "string_from_i64(value: number): number"
    )]
    pub fn string_from_i64(_value: I64) -> Handle {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_STRING_FROM_F64",
        ts = "string_from_f64(value: number): number"
    )]
    pub fn string_from_f64(_value: F64) -> Handle {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_STRING_CONCAT",
        ts = "string_concat(a: number, b: number): number"
    )]
    pub fn string_concat(_a: Handle, _b: Handle) -> Handle {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_STRING_EQ",
        ts = "string_eq(a: number, b: number): number"
    )]
    pub fn string_eq(_a: Handle, _b: Handle) -> I64 {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_STRING_CMP",
        ts = "string_cmp(a: number, b: number): number"
    )]
    pub fn string_cmp(_a: Handle, _b: Handle) -> I64 {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_STRING_FROM_STATIC",
        ts = "string_from_static(data: string): number"
    )]
    pub fn string_from_static(_data: Str) -> Handle {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_STRING_NEW",
        ts = "string_new(data: string): number"
    )]
    pub fn string_new(_data: Str) -> Handle {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_STRING_LEN",
        ts = "string_len(handle: number): number"
    )]
    pub fn string_len(_handle: Handle) -> I64 {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_STRING_PTR",
        ts = "string_ptr(handle: number): number"
    )]
    pub fn string_ptr(_handle: Handle) -> U64 {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_STRING_FREE",
        ts = "string_free(handle: number): number"
    )]
    pub fn string_free(_handle: Handle) -> I64 {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_HANDLE_LEN",
        ts = "handle_len(h: number): number",
        pure
    )]
    pub fn handle_len(_h: U64) -> I64 {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_ENV_ALLOC",
        ts = "env_alloc(slot_count: number): number"
    )]
    pub fn env_alloc(_slot_count: I32) -> Handle {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_ENV_GET",
        ts = "env_get(env: number, slot: number): number"
    )]
    pub fn env_get(_env: Handle, _slot: I32) -> I64 {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_ENV_SET",
        ts = "env_set(env: number, slot: number, value: number): number"
    )]
    pub fn env_set(_env: Handle, _slot: I32, _value: I64) -> I64 {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_CLOSURE_ALLOC",
        ts = "closure_alloc(fn_ptr: number, env: number): number"
    )]
    pub fn closure_alloc(_fn_ptr: I64, _env: Handle) -> Handle {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_CLOSURE_FN_PTR",
        ts = "closure_fn_ptr(handle: number): number",
        pure
    )]
    pub fn closure_fn_ptr(_handle: Handle) -> I64 {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_CLOSURE_ENV",
        ts = "closure_env(handle: number): number",
        pure
    )]
    pub fn closure_env(_handle: Handle) -> Handle {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_INSTANCE_NEW",
        ts = "instance_new(size: number, class_handle: number): number"
    )]
    pub fn instance_new(_size: I32, _class_handle: Handle) -> Handle {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_INSTANCE_CLASS",
        ts = "instance_class(handle: number): number"
    )]
    pub fn instance_class(_handle: Handle) -> Handle {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_INSTANCE_FREE",
        ts = "instance_free(handle: number): number"
    )]
    pub fn instance_free(_handle: Handle) -> I64 {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_INSTANCE_LOAD_I64",
        ts = "instance_load_i64(handle: number, offset: number): number"
    )]
    pub fn instance_load_i64(_handle: Handle, _offset: I32) -> I64 {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_INSTANCE_STORE_I64",
        ts = "instance_store_i64(handle: number, offset: number, value: number): number"
    )]
    pub fn instance_store_i64(_handle: Handle, _offset: I32, _value: I64) -> I64 {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_INSTANCE_LOAD_I32",
        ts = "instance_load_i32(handle: number, offset: number): number"
    )]
    pub fn instance_load_i32(_handle: Handle, _offset: I32) -> I32 {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_INSTANCE_STORE_I32",
        ts = "instance_store_i32(handle: number, offset: number, value: number): number"
    )]
    pub fn instance_store_i32(_handle: Handle, _offset: I32, _value: I32) -> I64 {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_INSTANCE_LOAD_F64",
        ts = "instance_load_f64(handle: number, offset: number): number"
    )]
    pub fn instance_load_f64(_handle: Handle, _offset: I32) -> F64 {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_INSTANCE_STORE_F64",
        ts = "instance_store_f64(handle: number, offset: number, value: number): number"
    )]
    pub fn instance_store_f64(_handle: Handle, _offset: I32, _value: F64) -> I64 {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_ENV_FREE",
        ts = "env_free(env: number): number"
    )]
    pub fn env_free(_env: Handle) -> I64 {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_COLLECT",
        ts = "collect(root: number): number"
    )]
    pub fn collect(_root: Handle) -> I64 {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_COLLECT_VEC",
        ts = "collect_vec(roots: number): number"
    )]
    pub fn collect_vec(_roots: Handle) -> I64 {
        unreachable!()
    }

    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_GC_LIVE_COUNT",
        ts = "live_count(): number"
    )]
    pub fn live_count() -> I64 {
        unreachable!()
    }
}
