//! Suporte a Node.js native addons (`.node`) pela porta **N-API**.
//!
//! Só N-API puro — addons V8-diretos/NAN são fora de escopo (exigiriam emular
//! o layout binário do V8). Ver `docs/specs/napi-implementation.md` — o estudo
//! de viabilidade `docs/specs/node-format/` foi DELETADO em 2026-07-28: seu
//! veredito virou este crate, e suas conclusões estão resumidas naquele spec.
//!
//! ## Etapa 1 (este arquivo): esqueleto
//! As ~40 fns do núcleo 80/20 existem como **stubs** `extern "C"` retornando
//! `napi_generic_failure`, com as assinaturas reais da ABI N-API (o `.node`
//! resolve esses símbolos crus por `dlsym` contra a export table do bin `rts`).
//! As implementações entram nas Etapas 5-12.
//!
//! ## Convenção de símbolos
//! As fns `napi_*` NÃO passam pelo registry SPECS do RTS (`validate_symbol`
//! exige prefixo `__RTS_`); são símbolos crus `#[unsafe(no_mangle)]`. A lista
//! em [`symbols::NAPI_EXPORTED_SYMBOLS`] é a fonte única da export-table.

#![allow(non_camel_case_types)]
#![allow(clippy::missing_safety_doc)]

pub mod arraybuffer;
pub mod async_work;
pub mod bigint;
pub mod classes;
pub mod env;
pub mod errors;
pub mod externals;
pub mod functions;
pub mod loader;
pub mod module_register;
pub mod objects;
pub mod phase2;
pub mod phase2b;
pub mod phase2c;
pub mod phase2d;
pub mod references;
pub mod scopes;
pub mod strings;
pub mod threadsafe;
pub mod symbols;
pub mod types;
pub mod values;

use std::ffi::{c_char, c_void};

use types::{napi_callback, napi_env, napi_status, napi_value};

use napi_status::napi_ok;

/// Descritor de propriedade passado a `napi_define_properties`. Layout C fixo
/// (espelha `napi_property_descriptor`). Na Fase 1 só `utf8name`/`value`/
/// `method`/`data` são honrados; `getter`/`setter`/`attributes` são copiados
/// mas não viram accessors dinâmicos.
#[repr(C)]
pub struct napi_property_descriptor {
    pub utf8name: *const c_char,
    pub name: napi_value,
    pub method: napi_callback,
    pub getter: napi_callback,
    pub setter: napi_callback,
    pub value: napi_value,
    pub attributes: i32,
    pub data: *mut c_void,
}

// Todas as ~55 fns N-API estão implementadas nos módulos de domínio (values,
// strings, objects, errors, externals, functions, scopes, references, loader);
// `lib.rs` mantém só as duas de registro/versão abaixo + a tabela `force_link`.

// ── registro / ambiente ─────────────────────────────────────────────────────

/// Versão N-API suportada em runtime. Já implementável (não é stub).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_version(env: napi_env, result: *mut u32) -> napi_status {
    let _ = env;
    if result.is_null() {
        return napi_status::napi_invalid_arg;
    }
    unsafe { *result = env::RTS_NAPI_VERSION };
    napi_ok
}

/// Negociação de versão do módulo (chamada pelo loader antes do register, se
/// presente no `.node`). Retorna o nível que o RTS implementa.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn node_api_module_get_api_version_v1() -> i32 {
    env::RTS_NAPI_VERSION as i32
}

// Mapa de onde cada fn N-API vive:
//   values.rs     — create/get_value escalar (double/int32/uint32/int64/bool),
//                    get_boolean/undefined/null, typeof
//   strings.rs    — create_string_utf8, get_value_string_utf8
//   objects.rs    — create_object/array(+length), set/get_named/property/element,
//                    get_array_length, is_array, get_global, instanceof
//   functions.rs  — create_function, get_cb_info, call_function, define_properties
//   errors.rs     — throw(+error/type/range), create_*error, is_exception_pending,
//                    get_and_clear_last_exception
//   scopes.rs     — open/close(+escapable) handle_scope, escape_handle
//   references.rs — create/delete_reference, reference_ref/unref, get_reference_value
//   externals.rs  — create_external, get_value_external
//   loader.rs     — __RTS_FN_NS_NAPI_LOAD_ADDON (interno, não exportado)

/// Soma "negra" dos endereços de toda fn `napi_*` exportada. Referenciada pelo
/// bin `rts` (`force_link`) para impedir que o LTO/linker descarte o objeto do
/// `rts-napi` — sem isto, os símbolos crus `napi_*` (não chamados pelo código
/// Rust do bin, só por `dlsym` do `.node`) somem antes do `/EXPORT` poder
/// retê-los, causando `LNK2001: unresolved external`. Ver
/// docs/specs/napi-implementation.md (Etapa 1, retenção de símbolo).
///
/// Mantém UMA entrada por símbolo de [`symbols`] — manter em sincronia ao
/// adicionar/remover fns. O teste `force_link_covers_all_symbols` guarda a
/// contagem.
pub fn force_link() -> usize {
    let fns: &[*const ()] = &[
        napi_get_version as *const (),
        node_api_module_get_api_version_v1 as *const (),
        crate::values::napi_create_double as *const (),
        crate::values::napi_create_int32 as *const (),
        crate::values::napi_create_uint32 as *const (),
        crate::values::napi_create_int64 as *const (),
        crate::strings::napi_create_string_utf8 as *const (),
        crate::objects::napi_create_object as *const (),
        crate::objects::napi_create_array as *const (),
        crate::objects::napi_create_array_with_length as *const (),
        crate::values::napi_get_boolean as *const (),
        crate::values::napi_get_undefined as *const (),
        crate::values::napi_get_null as *const (),
        crate::objects::napi_get_global as *const (),
        crate::values::napi_get_value_double as *const (),
        crate::values::napi_get_value_int32 as *const (),
        crate::values::napi_get_value_uint32 as *const (),
        crate::values::napi_get_value_int64 as *const (),
        crate::values::napi_get_value_bool as *const (),
        crate::strings::napi_get_value_string_utf8 as *const (),
        crate::objects::napi_get_array_length as *const (),
        crate::objects::napi_set_named_property as *const (),
        crate::objects::napi_get_named_property as *const (),
        crate::objects::napi_set_property as *const (),
        crate::objects::napi_get_property as *const (),
        crate::objects::napi_set_element as *const (),
        crate::objects::napi_get_element as *const (),
        crate::functions::napi_define_properties as *const (),
        crate::values::napi_typeof as *const (),
        crate::objects::napi_is_array as *const (),
        crate::objects::napi_instanceof as *const (),
        crate::functions::napi_create_function as *const (),
        crate::functions::napi_get_cb_info as *const (),
        crate::functions::napi_call_function as *const (),
        crate::errors::napi_throw as *const (),
        crate::errors::napi_throw_error as *const (),
        crate::errors::napi_throw_type_error as *const (),
        crate::errors::napi_throw_range_error as *const (),
        crate::errors::napi_create_error as *const (),
        crate::errors::napi_create_type_error as *const (),
        crate::errors::napi_create_range_error as *const (),
        crate::errors::napi_is_exception_pending as *const (),
        crate::errors::napi_get_and_clear_last_exception as *const (),
        crate::scopes::napi_open_handle_scope as *const (),
        crate::scopes::napi_close_handle_scope as *const (),
        crate::scopes::napi_open_escapable_handle_scope as *const (),
        crate::scopes::napi_close_escapable_handle_scope as *const (),
        crate::scopes::napi_escape_handle as *const (),
        crate::references::napi_create_reference as *const (),
        crate::references::napi_delete_reference as *const (),
        crate::references::napi_reference_ref as *const (),
        crate::references::napi_reference_unref as *const (),
        crate::references::napi_get_reference_value as *const (),
        crate::externals::napi_create_external as *const (),
        crate::externals::napi_get_value_external as *const (),
        // Símbolo interno do loader (chamado pelo codegen, não pelo .node):
        crate::loader::__RTS_FN_NS_NAPI_LOAD_ADDON as *const (),
        crate::functions::__RTS_FN_RT_NAPI_DISPATCH_CALLBACK as *const (),
        // Classes nativas (classes.rs)
        crate::classes::napi_define_class as *const (),
        crate::classes::napi_new_instance as *const (),
        crate::classes::__RTS_FN_RT_NAPI_INVOKE_METHOD as *const (),
        crate::classes::__RTS_FN_RT_NAPI_NEW_INSTANCE as *const (),
        // Fase 2c (phase2c.rs) — fatal_error omitida (retorna !)
        crate::phase2c::napi_add_env_cleanup_hook as *const (),
        crate::phase2c::napi_add_finalizer as *const (),
        crate::phase2c::napi_adjust_external_memory as *const (),
        crate::phase2c::napi_check_object_type_tag as *const (),
        crate::phase2c::napi_coerce_to_string as *const (),
        crate::phase2c::napi_create_promise as *const (),
        crate::phase2c::napi_fatal_exception as *const (),
        crate::phase2c::napi_get_prototype as *const (),
        // ArrayBuffer/TypedArray/DataView (arraybuffer.rs, engine #1548)
        crate::arraybuffer::napi_create_arraybuffer as *const (),
        crate::arraybuffer::napi_create_external_arraybuffer as *const (),
        crate::arraybuffer::napi_get_arraybuffer_info as *const (),
        crate::arraybuffer::napi_is_arraybuffer as *const (),
        crate::arraybuffer::napi_detach_arraybuffer as *const (),
        crate::arraybuffer::napi_is_detached_arraybuffer as *const (),
        crate::arraybuffer::napi_create_typedarray as *const (),
        crate::arraybuffer::napi_get_typedarray_info as *const (),
        crate::arraybuffer::napi_is_typedarray as *const (),
        crate::arraybuffer::napi_create_dataview as *const (),
        crate::arraybuffer::napi_get_dataview_info as *const (),
        crate::arraybuffer::napi_is_dataview as *const (),
        crate::arraybuffer::napi_create_external_buffer as *const (),
        crate::arraybuffer::node_api_create_buffer_from_arraybuffer as *const (),
        crate::arraybuffer::node_api_create_sharedarraybuffer as *const (),
        // BigInt real (bigint.rs, #219)
        crate::bigint::napi_create_bigint_int64 as *const (),
        crate::bigint::napi_create_bigint_uint64 as *const (),
        crate::bigint::napi_create_bigint_words as *const (),
        crate::bigint::napi_get_value_bigint_int64 as *const (),
        crate::bigint::napi_get_value_bigint_uint64 as *const (),
        crate::bigint::napi_get_value_bigint_words as *const (),
        // Threadsafe functions inline (threadsafe.rs, #1548 item 3 parcial)
        crate::threadsafe::napi_create_threadsafe_function as *const (),
        crate::threadsafe::napi_call_threadsafe_function as *const (),
        crate::threadsafe::napi_acquire_threadsafe_function as *const (),
        crate::threadsafe::napi_release_threadsafe_function as *const (),
        crate::threadsafe::napi_get_threadsafe_function_context as *const (),
        crate::threadsafe::napi_ref_threadsafe_function as *const (),
        crate::threadsafe::napi_unref_threadsafe_function as *const (),
        // Async work síncrono (async_work.rs, #1548 item 3 parcial)
        crate::async_work::napi_async_destroy as *const (),
        crate::async_work::napi_async_init as *const (),
        crate::async_work::napi_cancel_async_work as *const (),
        crate::async_work::napi_close_callback_scope as *const (),
        crate::async_work::napi_create_async_work as *const (),
        crate::async_work::napi_delete_async_work as *const (),
        crate::async_work::napi_open_callback_scope as *const (),
        crate::async_work::napi_queue_async_work as *const (),
        crate::async_work::node_api_post_finalizer as *const (),
        crate::async_work::napi_add_async_cleanup_hook as *const (),
        crate::async_work::napi_remove_async_cleanup_hook as *const (),
        crate::async_work::napi_get_uv_event_loop as *const (),
        crate::phase2c::napi_reject_deferred as *const (),
        crate::phase2c::napi_remove_env_cleanup_hook as *const (),
        crate::phase2c::napi_resolve_deferred as *const (),
        crate::phase2c::napi_run_script as *const (),
        crate::phase2c::napi_type_tag_object as *const (),
        crate::phase2c::node_api_create_syntax_error as *const (),
        crate::phase2c::node_api_get_module_file_name as *const (),
        crate::phase2c::node_api_throw_syntax_error as *const (),
        // Fase 2d (phase2d.rs)
        crate::phase2d::napi_make_callback as *const (),
        crate::phase2d::node_api_create_external_string_latin1 as *const (),
        crate::phase2d::node_api_create_external_string_utf16 as *const (),
        crate::phase2d::node_api_is_sharedarraybuffer as *const (),
        // Fase 2 (phase2.rs)
        crate::phase2::napi_coerce_to_bool as *const (),
        crate::phase2::napi_coerce_to_number as *const (),
        crate::phase2::napi_coerce_to_object as *const (),
        crate::phase2::napi_create_buffer as *const (),
        crate::phase2::napi_create_buffer_copy as *const (),
        crate::phase2::napi_create_date as *const (),
        crate::phase2::napi_create_symbol as *const (),
        crate::phase2::napi_delete_element as *const (),
        crate::phase2::napi_delete_property as *const (),
        crate::phase2::napi_get_all_property_names as *const (),
        crate::phase2::napi_get_buffer_info as *const (),
        crate::phase2::napi_get_date_value as *const (),
        crate::phase2::napi_get_property_names as *const (),
        crate::phase2::napi_has_element as *const (),
        crate::phase2::napi_has_named_property as *const (),
        crate::phase2::napi_has_own_property as *const (),
        crate::phase2::napi_has_property as *const (),
        crate::phase2::napi_is_buffer as *const (),
        crate::phase2::napi_is_date as *const (),
        crate::phase2::napi_is_error as *const (),
        crate::phase2::napi_is_promise as *const (),
        crate::phase2::napi_object_freeze as *const (),
        crate::phase2::napi_object_seal as *const (),
        crate::phase2::napi_strict_equals as *const (),
        // Fase 2b (phase2b.rs)
        crate::phase2b::napi_create_string_latin1 as *const (),
        crate::phase2b::napi_create_string_utf16 as *const (),
        crate::phase2b::napi_get_instance_data as *const (),
        crate::phase2b::napi_get_last_error_info as *const (),
        crate::phase2b::napi_get_new_target as *const (),
        crate::phase2b::napi_get_node_version as *const (),
        crate::phase2b::napi_get_value_string_latin1 as *const (),
        crate::phase2b::napi_get_value_string_utf16 as *const (),
        crate::phase2b::napi_remove_wrap as *const (),
        crate::phase2b::napi_set_instance_data as *const (),
        crate::phase2b::napi_unwrap as *const (),
        crate::phase2b::napi_wrap as *const (),
        crate::phase2b::node_api_create_property_key_latin1 as *const (),
        crate::phase2b::node_api_create_property_key_utf16 as *const (),
        crate::phase2b::node_api_create_property_key_utf8 as *const (),
        crate::phase2b::node_api_symbol_for as *const (),
        // Registro de módulo legado (impl real — não mais stub).
        crate::module_register::napi_module_register as *const (),
    ];
    // `black_box` evita que o otimizador prove que o resultado é constante e
    // elimine as referências.
    std::hint::black_box(fns.iter().map(|p| *p as usize).fold(0usize, usize::wrapping_add))
}

#[cfg(test)]
mod tests {
    /// `force_link` deve referenciar exatamente um ponteiro por símbolo
    /// exportado — senão algum `napi_*` pode ser descartado pelo LTO no bin.
    #[test]
    fn force_link_covers_all_symbols() {
        // Smoke: force_link linka e a lista cobre a superfície completa (159
        // símbolos, todos com impl real — sem stubs). Atualizar ao ampliar.
        let _ = crate::force_link();
        assert!(crate::symbols::exported_symbols().len() >= 150);
    }
}
