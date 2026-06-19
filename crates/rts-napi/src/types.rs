//! Tipos ABI da N-API — layout fixo, ditado pelo Node (o addon `.node` os
//! conhece de cabeça). NÃO reordenar enums nem mudar `#[repr(C)]`.
//!
//! `napi_value` e `napi_env` são ponteiros opacos do ponto de vista do addon;
//! internamente o RTS os mapeia para um handle `u64` da `HandleTable`
//! (`napi_value`) e para `*mut RtsNapiEnv` (`napi_env`). Ver
//! docs/specs/napi-implementation.md (invariante "napi_value sempre handle
//! estável ou sentinela, nunca i64 cru").

use std::ffi::c_void;

/// Ponteiro opaco para um valor JS. Binariamente um ponteiro; o RTS encapsula
/// um handle `u64` da `HandleTable` (ou uma das sentinelas JS
/// `i64::MIN..=MIN+4`). O addon nunca o dereferencia.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct napi_value(pub *mut c_void);

/// Contexto opaco passado a toda função N-API. Aponta para um `RtsNapiEnv` (um
/// por instância de addon). Não pode ser cacheado entre Worker threads nem
/// usado após o addon ser descarregado.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct napi_env(pub *mut c_void);

/// Handle opaco de um handle scope (ver `scopes.rs`).
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct napi_handle_scope(pub *mut c_void);

/// Handle opaco de um escapable handle scope.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct napi_escapable_handle_scope(pub *mut c_void);

/// Referência persistente a um valor (sobrevive ao handle scope). Ver
/// `references.rs`.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct napi_ref(pub *mut c_void);

/// Info de uma chamada de callback (argc/argv/this/data). Ver `functions.rs`.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct napi_callback_info(pub *mut c_void);

/// Código de status retornado por toda função N-API. **Ordem ABI-estável** —
/// o valor numérico é parte do contrato; nunca reordenar nem inserir no meio.
/// Espelha `napi_status` em `js_native_api_types.h`.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum napi_status {
    napi_ok = 0,
    napi_invalid_arg,
    napi_object_expected,
    napi_string_expected,
    napi_name_expected,
    napi_function_expected,
    napi_number_expected,
    napi_boolean_expected,
    napi_array_expected,
    napi_generic_failure,
    napi_pending_exception,
    napi_cancelled,
    napi_escape_called_twice,
    napi_handle_scope_mismatch,
    napi_callback_scope_mismatch,
    napi_queue_full,
    napi_closing,
    napi_bigint_expected,
    napi_date_expected,
    napi_arraybuffer_expected,
    napi_detachable_arraybuffer_expected,
    napi_would_deadlock,
    napi_no_external_buffers_allowed,
    napi_cannot_run_js,
}

/// Tipo de um valor JS via `napi_typeof`. **Ordem ABI-estável.** Espelha
/// `napi_valuetype`.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum napi_valuetype {
    napi_undefined = 0,
    napi_null,
    napi_boolean,
    napi_number,
    napi_string,
    napi_symbol,
    napi_object,
    napi_function,
    napi_external,
    napi_bigint,
}

/// `length` "auto" para `napi_create_string_utf8` etc. — o runtime mede via
/// `strlen`. Valor: `(size_t)-1`.
pub const NAPI_AUTO_LENGTH: usize = usize::MAX;

/// Assinatura de um callback nativo registrado pelo addon
/// (`napi_create_function`). CDECL/`extern "C"`.
pub type napi_callback =
    Option<unsafe extern "C" fn(env: napi_env, info: napi_callback_info) -> napi_value>;

/// Finalizer associado a um valor externo / wrap. CDECL.
pub type napi_finalize =
    Option<unsafe extern "C" fn(env: napi_env, finalize_data: *mut c_void, finalize_hint: *mut c_void)>;

// `napi_value`/`napi_env` carregam ponteiros que cruzam para Rust-managed
// state; o addon roda single-threaded no caminho síncrono da Fase 1. Marcamos
// como não-Send por padrão (sem impls), forçando o uso só dentro da thread JS.
