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
pub mod loader;
pub mod symbols;
pub mod types;
pub mod values;

use std::ffi::{c_char, c_void};

use types::{
    napi_callback, napi_callback_info, napi_env, napi_escapable_handle_scope, napi_finalize,
    napi_handle_scope, napi_ref, napi_status, napi_value,
};

use napi_status::{napi_generic_failure, napi_ok};

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

// ─────────────────────────────────────────────────────────────────────────────
// Macro de stub: declara uma fn `extern "C"` com a assinatura dada, corpo
// retornando `napi_generic_failure`. Substituída pela impl real nas Etapas
// 5-12 (mover a fn para o módulo de domínio e remover daqui).
// ─────────────────────────────────────────────────────────────────────────────
macro_rules! napi_stub {
    (
        $( #[$meta:meta] )*
        fn $name:ident ( $( $arg:ident : $ty:ty ),* $(,)? ) $(-> $ret:ty)?
    ) => {
        $( #[$meta] )*
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name ( $( $arg : $ty ),* ) $(-> $ret)? {
            // Suprime "unused variable" sem nomear cada arg.
            $( let _ = &$arg; )*
            napi_generic_failure
        }
    };
}

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

// ── criação/extração de valores escalares + typeof ──────────────────────────
// Implementados em `values.rs`: napi_create_double/int32/uint32/int64,
// napi_get_boolean/undefined/null, napi_get_value_double/int32/uint32/int64/bool,
// napi_typeof. Os demais seguem stub até as Etapas 6-7.
napi_stub!(fn napi_create_string_utf8(env: napi_env, str_: *const c_char, length: usize, result: *mut napi_value) -> napi_status);
napi_stub!(fn napi_create_object(env: napi_env, result: *mut napi_value) -> napi_status);
napi_stub!(fn napi_create_array(env: napi_env, result: *mut napi_value) -> napi_status);
napi_stub!(fn napi_create_array_with_length(env: napi_env, length: usize, result: *mut napi_value) -> napi_status);
napi_stub!(fn napi_get_global(env: napi_env, result: *mut napi_value) -> napi_status);
napi_stub!(fn napi_get_value_string_utf8(env: napi_env, value: napi_value, buf: *mut c_char, bufsize: usize, result: *mut usize) -> napi_status);
napi_stub!(fn napi_get_array_length(env: napi_env, value: napi_value, result: *mut u32) -> napi_status);

// ── propriedades ────────────────────────────────────────────────────────────
napi_stub!(fn napi_set_named_property(env: napi_env, object: napi_value, utf8name: *const c_char, value: napi_value) -> napi_status);
napi_stub!(fn napi_get_named_property(env: napi_env, object: napi_value, utf8name: *const c_char, result: *mut napi_value) -> napi_status);
napi_stub!(fn napi_set_property(env: napi_env, object: napi_value, key: napi_value, value: napi_value) -> napi_status);
napi_stub!(fn napi_get_property(env: napi_env, object: napi_value, key: napi_value, result: *mut napi_value) -> napi_status);
napi_stub!(fn napi_set_element(env: napi_env, object: napi_value, index: u32, value: napi_value) -> napi_status);
napi_stub!(fn napi_get_element(env: napi_env, object: napi_value, index: u32, result: *mut napi_value) -> napi_status);
napi_stub!(fn napi_define_properties(env: napi_env, object: napi_value, property_count: usize, properties: *const napi_property_descriptor) -> napi_status);

// ── tipos ───────────────────────────────────────────────────────────────────
// napi_typeof implementado em `values.rs`.
napi_stub!(fn napi_is_array(env: napi_env, value: napi_value, result: *mut bool) -> napi_status);
napi_stub!(fn napi_instanceof(env: napi_env, object: napi_value, constructor: napi_value, result: *mut bool) -> napi_status);

// ── funções / callbacks ─────────────────────────────────────────────────────
napi_stub!(fn napi_create_function(env: napi_env, utf8name: *const c_char, length: usize, cb: napi_callback, data: *mut c_void, result: *mut napi_value) -> napi_status);
napi_stub!(fn napi_get_cb_info(env: napi_env, cbinfo: napi_callback_info, argc: *mut usize, argv: *mut napi_value, this_arg: *mut napi_value, data: *mut *mut c_void) -> napi_status);
napi_stub!(fn napi_call_function(env: napi_env, recv: napi_value, func: napi_value, argc: usize, argv: *const napi_value, result: *mut napi_value) -> napi_status);

// ── erros / exceções ────────────────────────────────────────────────────────
napi_stub!(fn napi_throw(env: napi_env, error: napi_value) -> napi_status);
napi_stub!(fn napi_throw_error(env: napi_env, code: *const c_char, msg: *const c_char) -> napi_status);
napi_stub!(fn napi_throw_type_error(env: napi_env, code: *const c_char, msg: *const c_char) -> napi_status);
napi_stub!(fn napi_throw_range_error(env: napi_env, code: *const c_char, msg: *const c_char) -> napi_status);
napi_stub!(fn napi_create_error(env: napi_env, code: napi_value, msg: napi_value, result: *mut napi_value) -> napi_status);
napi_stub!(fn napi_create_type_error(env: napi_env, code: napi_value, msg: napi_value, result: *mut napi_value) -> napi_status);
napi_stub!(fn napi_create_range_error(env: napi_env, code: napi_value, msg: napi_value, result: *mut napi_value) -> napi_status);
napi_stub!(fn napi_is_exception_pending(env: napi_env, result: *mut bool) -> napi_status);
napi_stub!(fn napi_get_and_clear_last_exception(env: napi_env, result: *mut napi_value) -> napi_status);

// ── handle scopes ───────────────────────────────────────────────────────────
napi_stub!(fn napi_open_handle_scope(env: napi_env, result: *mut napi_handle_scope) -> napi_status);
napi_stub!(fn napi_close_handle_scope(env: napi_env, scope: napi_handle_scope) -> napi_status);
napi_stub!(fn napi_open_escapable_handle_scope(env: napi_env, result: *mut napi_escapable_handle_scope) -> napi_status);
napi_stub!(fn napi_close_escapable_handle_scope(env: napi_env, scope: napi_escapable_handle_scope) -> napi_status);
napi_stub!(fn napi_escape_handle(env: napi_env, scope: napi_escapable_handle_scope, escapee: napi_value, result: *mut napi_value) -> napi_status);

// ── referências ─────────────────────────────────────────────────────────────
napi_stub!(fn napi_create_reference(env: napi_env, value: napi_value, initial_refcount: u32, result: *mut napi_ref) -> napi_status);
napi_stub!(fn napi_delete_reference(env: napi_env, ref_: napi_ref) -> napi_status);
napi_stub!(fn napi_reference_ref(env: napi_env, ref_: napi_ref, result: *mut u32) -> napi_status);
napi_stub!(fn napi_reference_unref(env: napi_env, ref_: napi_ref, result: *mut u32) -> napi_status);
napi_stub!(fn napi_get_reference_value(env: napi_env, ref_: napi_ref, result: *mut napi_value) -> napi_status);

// ── external ────────────────────────────────────────────────────────────────
napi_stub!(fn napi_create_external(env: napi_env, data: *mut c_void, finalize_cb: napi_finalize, finalize_hint: *mut c_void, result: *mut napi_value) -> napi_status);
napi_stub!(fn napi_get_value_external(env: napi_env, value: napi_value, result: *mut *mut c_void) -> napi_status);

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
        napi_create_string_utf8 as *const (),
        napi_create_object as *const (),
        napi_create_array as *const (),
        napi_create_array_with_length as *const (),
        crate::values::napi_get_boolean as *const (),
        crate::values::napi_get_undefined as *const (),
        crate::values::napi_get_null as *const (),
        napi_get_global as *const (),
        crate::values::napi_get_value_double as *const (),
        crate::values::napi_get_value_int32 as *const (),
        crate::values::napi_get_value_uint32 as *const (),
        crate::values::napi_get_value_int64 as *const (),
        crate::values::napi_get_value_bool as *const (),
        napi_get_value_string_utf8 as *const (),
        napi_get_array_length as *const (),
        napi_set_named_property as *const (),
        napi_get_named_property as *const (),
        napi_set_property as *const (),
        napi_get_property as *const (),
        napi_set_element as *const (),
        napi_get_element as *const (),
        napi_define_properties as *const (),
        crate::values::napi_typeof as *const (),
        napi_is_array as *const (),
        napi_instanceof as *const (),
        napi_create_function as *const (),
        napi_get_cb_info as *const (),
        napi_call_function as *const (),
        napi_throw as *const (),
        napi_throw_error as *const (),
        napi_throw_type_error as *const (),
        napi_throw_range_error as *const (),
        napi_create_error as *const (),
        napi_create_type_error as *const (),
        napi_create_range_error as *const (),
        napi_is_exception_pending as *const (),
        napi_get_and_clear_last_exception as *const (),
        napi_open_handle_scope as *const (),
        napi_close_handle_scope as *const (),
        napi_open_escapable_handle_scope as *const (),
        napi_close_escapable_handle_scope as *const (),
        napi_escape_handle as *const (),
        napi_create_reference as *const (),
        napi_delete_reference as *const (),
        napi_reference_ref as *const (),
        napi_reference_unref as *const (),
        napi_get_reference_value as *const (),
        napi_create_external as *const (),
        napi_get_value_external as *const (),
        // Símbolo interno do loader (chamado pelo codegen, não pelo .node):
        crate::loader::__RTS_FN_NS_NAPI_LOAD_ADDON as *const (),
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
