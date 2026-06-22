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
/// + `rts_engine::heap::handles::*` (consumidores) seguem resolvendo.
pub use rts_engine::heap::handles;
/// Alocadores/helpers do heap migrados pro motor (env-record, closure, instance
/// de classe, this-slot, tagged-raw, class-registry). Facade →
/// `crate::namespaces::gc::<X>::*` + `super::<X>` (siblings) seguem resolvendo.
pub use rts_engine::heap::{class_registry, closure, env, instance, tagged_raw, this_slot};
pub use crate::promise_slot;
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
        // SUPERFÍCIE gc.* LEGACY REMOVIDA (motor antigo) — zero-legacy, gc-new-api-plan
        // C1/C2/C4. O motor NOVO não emite NENHUM destes membros:
        //   - strings: PolyValue TAG_STR (`String()`/template/`+`/`===`), não string_*;
        //   - objetos/instâncias: shapes + Vec keyed (`__rtsadp_obj_*`), não instance_*;
        //   - closures: cells PolyValue (#195), não env_*/closure_*.
        // Removidos da spec TS: string_* (C1), handle_len, env_alloc/get/set/free,
        // closure_alloc/fn_ptr/closure_env, instance_new/class/free/load_*/store_* e
        // collect_vec. Os símbolos `__RTS_FN_NS_GC_*` continuam definidos
        // (string_pool/handles) e registrados direto no `runtime_link` do motor para
        // uso INTERNO; o que sai é só a superfície exposta ao TS. Drenar as Entry/
        // símbolos mortos é a fase B (handles.rs). Sobra só o núcleo chamável de TS:
        .member(ext("collect", Sig::new(vec![AbiType::Handle], AbiType::I64), "__RTS_FN_NS_GC_COLLECT", "collect(root: number): number", false))
        .member(ext("live_count", Sig::new(Vec::new(), AbiType::I64), "__RTS_FN_NS_GC_LIVE_COUNT", "live_count(): number", false))
        .done();
}
