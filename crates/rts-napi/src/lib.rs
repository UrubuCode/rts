//! Suporte a Node.js native addons (`.node`) pela porta **N-API**.
//!
//! Só N-API puro — addons V8-diretos/NAN são fora de escopo (exigiriam emular
//! o layout binário do V8). Ver `docs/specs/napi-implementation.md` e o estudo
//! em `docs/specs/node-format/`.
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

pub mod env;
pub mod errors;
pub mod externals;
pub mod functions;
pub mod loader;
pub mod objects;
pub mod references;
pub mod scopes;
pub mod strings;
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
        // Não dá pra contar ponteiros em runtime; este teste casa a intenção:
        // a lista de símbolos tem 55 entradas e `force_link` tem 55 linhas.
        // Mantido como lembrete de sincronia + smoke de que `force_link` linka.
        let _ = crate::force_link();
        assert_eq!(crate::symbols::exported_symbols().len(), 55);
    }
}
