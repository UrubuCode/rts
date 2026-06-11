//! `gc` namespace — runtime value storage.
//!
//! Owns the handle table that backs dynamically allocated strings (and,
//! eventually, objects, arrays, buffers). Every handle-returning function
//! across the codebase ultimately calls into this module's tables.
//!
//! Migrated off the `#[rts_namespace]` macro to the hand-written `rts-engine`
//! builder model (rumo à remoção da `rts-macro`). The members are declared
//! `external`: their `#[no_mangle] extern "C"` bodies live in the submodules
//! below (`string_pool`, `env`, `closure`, `instance`, `collector`, `handles`),
//! so this file only registers the SPEC referencing those symbols (fn_ptr null;
//! the owning submodule provides the real pointer at JIT/AOT link time).

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

pub mod collector;
pub mod error;
pub mod generator;
// `global_roots`, `stack_map_registry`, `thread_registry`, `debug` e o `scan`
// (scanner conservativo) migraram pro `rts-engine` (SPLIT fatias 3a-3c —
// mecanismo std/FFI escrito pelo codegen / usado pelo scanner). Re-exportados
// aqui como fachada: os `super::` internos + `namespaces::gc::*` (alias) +
// jit.rs resolvem pro MESMO static no engine, sem editar consumidores.
pub use rts_engine::collector::{debug, global_roots, scan, stack_map_registry, thread_registry};
/// `handles` (Entry + HandleTable + heap GC tipado) migrou pro motor
/// (`rts_engine::heap::handles`, Fase 1a). Facade → `super::handles` (dos siblings)
/// + `crate::namespaces::gc::handles::*` (consumidores) seguem resolvendo.
pub use rts_engine::heap::handles;
/// Alocadores/helpers do heap migrados pro motor (env-record, closure, instance
/// de classe, this-slot, tagged-raw, class-registry). Facade →
/// `crate::namespaces::gc::<X>::*` + `super::<X>` (siblings) seguem resolvendo.
pub use rts_engine::heap::{class_registry, closure, env, instance, tagged_raw, this_slot};
pub use rts_std::promise_slot;
pub mod stack;
pub mod string_pool;

/// Membro `external`: a SPEC referencia o `symbol` cujo `#[no_mangle] extern
/// "C"` vive num submódulo. `fn_ptr` é null (o submódulo dono registra o ponteiro
/// real); `doc` é vazio (estes métodos não tinham doc-comments).
fn ext(name: &str, sig: Sig, symbol: &str, ts: &str, pure: bool) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(core::ptr::null::<u8>()),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: String::new(),
        pure,
        intrinsic: None,
    }
}

/// Registra a namespace `gc` no motor (hand-written, sem macro). Todos os
/// membros são `external` — os externs pertencem aos submódulos.
pub fn register(e: &mut Engine) {
    e.ns("gc")
        .doc("Runtime-managed handle table and string pool. All members are `external` —\nthe externs are owned by the submodules; the macro emits only the SPEC.")
        .member(ext("string_from_i64", Sig::new(vec![AbiType::I64], AbiType::Handle), "__RTS_FN_NS_GC_STRING_FROM_I64", "string_from_i64(value: number): number", false))
        .member(ext("string_from_f64", Sig::new(vec![AbiType::F64], AbiType::Handle), "__RTS_FN_NS_GC_STRING_FROM_F64", "string_from_f64(value: number): number", false))
        .member(ext("string_concat", Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Handle), "__RTS_FN_NS_GC_STRING_CONCAT", "string_concat(a: number, b: number): number", false))
        .member(ext("string_eq", Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::I64), "__RTS_FN_NS_GC_STRING_EQ", "string_eq(a: number, b: number): number", false))
        .member(ext("string_cmp", Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::I64), "__RTS_FN_NS_GC_STRING_CMP", "string_cmp(a: number, b: number): number", false))
        .member(ext("string_from_static", Sig::new(vec![AbiType::StrPtr], AbiType::Handle), "__RTS_FN_NS_GC_STRING_FROM_STATIC", "string_from_static(data: string): number", false))
        .member(ext("string_new", Sig::new(vec![AbiType::StrPtr], AbiType::Handle), "__RTS_FN_NS_GC_STRING_NEW", "string_new(data: string): number", false))
        .member(ext("string_len", Sig::new(vec![AbiType::Handle], AbiType::I64), "__RTS_FN_NS_GC_STRING_LEN", "string_len(handle: number): number", false))
        .member(ext("string_ptr", Sig::new(vec![AbiType::Handle], AbiType::U64), "__RTS_FN_NS_GC_STRING_PTR", "string_ptr(handle: number): number", false))
        .member(ext("string_free", Sig::new(vec![AbiType::Handle], AbiType::I64), "__RTS_FN_NS_GC_STRING_FREE", "string_free(handle: number): number", false))
        .member(ext("handle_len", Sig::new(vec![AbiType::U64], AbiType::I64), "__RTS_FN_NS_GC_HANDLE_LEN", "handle_len(h: number): number", true))
        .member(ext("env_alloc", Sig::new(vec![AbiType::I32], AbiType::Handle), "__RTS_FN_NS_GC_ENV_ALLOC", "env_alloc(slot_count: number): number", false))
        .member(ext("env_get", Sig::new(vec![AbiType::Handle, AbiType::I32], AbiType::I64), "__RTS_FN_NS_GC_ENV_GET", "env_get(env: number, slot: number): number", false))
        .member(ext("env_set", Sig::new(vec![AbiType::Handle, AbiType::I32, AbiType::I64], AbiType::I64), "__RTS_FN_NS_GC_ENV_SET", "env_set(env: number, slot: number, value: number): number", false))
        .member(ext("closure_alloc", Sig::new(vec![AbiType::I64, AbiType::Handle], AbiType::Handle), "__RTS_FN_NS_GC_CLOSURE_ALLOC", "closure_alloc(fn_ptr: number, env: number): number", false))
        .member(ext("closure_fn_ptr", Sig::new(vec![AbiType::Handle], AbiType::I64), "__RTS_FN_NS_GC_CLOSURE_FN_PTR", "closure_fn_ptr(handle: number): number", true))
        .member(ext("closure_env", Sig::new(vec![AbiType::Handle], AbiType::Handle), "__RTS_FN_NS_GC_CLOSURE_ENV", "closure_env(handle: number): number", true))
        .member(ext("instance_new", Sig::new(vec![AbiType::I32, AbiType::Handle], AbiType::Handle), "__RTS_FN_NS_GC_INSTANCE_NEW", "instance_new(size: number, class_handle: number): number", false))
        .member(ext("instance_class", Sig::new(vec![AbiType::Handle], AbiType::Handle), "__RTS_FN_NS_GC_INSTANCE_CLASS", "instance_class(handle: number): number", false))
        .member(ext("instance_free", Sig::new(vec![AbiType::Handle], AbiType::I64), "__RTS_FN_NS_GC_INSTANCE_FREE", "instance_free(handle: number): number", false))
        .member(ext("instance_load_i64", Sig::new(vec![AbiType::Handle, AbiType::I32], AbiType::I64), "__RTS_FN_NS_GC_INSTANCE_LOAD_I64", "instance_load_i64(handle: number, offset: number): number", false))
        .member(ext("instance_store_i64", Sig::new(vec![AbiType::Handle, AbiType::I32, AbiType::I64], AbiType::I64), "__RTS_FN_NS_GC_INSTANCE_STORE_I64", "instance_store_i64(handle: number, offset: number, value: number): number", false))
        .member(ext("instance_load_i32", Sig::new(vec![AbiType::Handle, AbiType::I32], AbiType::I32), "__RTS_FN_NS_GC_INSTANCE_LOAD_I32", "instance_load_i32(handle: number, offset: number): number", false))
        .member(ext("instance_store_i32", Sig::new(vec![AbiType::Handle, AbiType::I32, AbiType::I32], AbiType::I64), "__RTS_FN_NS_GC_INSTANCE_STORE_I32", "instance_store_i32(handle: number, offset: number, value: number): number", false))
        .member(ext("instance_load_f64", Sig::new(vec![AbiType::Handle, AbiType::I32], AbiType::F64), "__RTS_FN_NS_GC_INSTANCE_LOAD_F64", "instance_load_f64(handle: number, offset: number): number", false))
        .member(ext("instance_store_f64", Sig::new(vec![AbiType::Handle, AbiType::I32, AbiType::F64], AbiType::I64), "__RTS_FN_NS_GC_INSTANCE_STORE_F64", "instance_store_f64(handle: number, offset: number, value: number): number", false))
        .member(ext("env_free", Sig::new(vec![AbiType::Handle], AbiType::I64), "__RTS_FN_NS_GC_ENV_FREE", "env_free(env: number): number", false))
        .member(ext("collect", Sig::new(vec![AbiType::Handle], AbiType::I64), "__RTS_FN_NS_GC_COLLECT", "collect(root: number): number", false))
        .member(ext("collect_vec", Sig::new(vec![AbiType::Handle], AbiType::I64), "__RTS_FN_NS_GC_COLLECT_VEC", "collect_vec(roots: number): number", false))
        .member(ext("live_count", Sig::new(Vec::new(), AbiType::I64), "__RTS_FN_NS_GC_LIVE_COUNT", "live_count(): number", false))
        .done();
}
