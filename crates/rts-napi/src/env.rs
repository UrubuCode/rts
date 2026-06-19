//! `RtsNapiEnv` — o estado por instância de addon que `napi_env` aponta.
//!
//! Esqueleto da Etapa 1: carrega a pilha de handle scopes, a tabela de
//! referências e o slot de exceção pendente. As estruturas concretas (chunks
//! de scope com endereço estável, RefTable) entram nas Etapas 8/9; aqui ficam
//! os campos mínimos para o loader (Etapa 4) fabricar e passar o env.

use crate::types::{napi_env, napi_value};

/// Estado de uma instância de addon. Um por `napi_register_module_v1`.
///
/// Vive enquanto o addon estiver carregado; o `napi_env` opaco que cruza para
/// o `.node` é um `*mut RtsNapiEnv` (cast em `as_raw`/`from_raw`).
pub struct RtsNapiEnv {
    /// Versão N-API anunciada por `napi_get_version` (nível implementado).
    pub api_version: u32,
    /// Exceção pendente (handle de um `Entry::ErrorObj`), ou `0` se nenhuma.
    /// `napi_throw*` seta; `napi_is_exception_pending` consulta;
    /// `napi_get_and_clear_last_exception` lê e limpa. Per-instância (síncrono,
    /// Fase 1) — não interage com o error slot do try/catch do RTS.
    pub pending_exception: u64,
    /// Pilha de handle scopes (Etapa 8). Cada scope mantém seus `napi_value`
    /// vivos como GC roots enquanto aberto. Ver `crate::scopes`.
    pub scopes: crate::scopes::ScopeStack,
    /// Tabela de referências persistentes (Etapa 9). Ver `crate::references`.
    pub refs: crate::references::RefTable,
}

impl RtsNapiEnv {
    pub fn new(api_version: u32) -> Self {
        Self {
            api_version,
            pending_exception: 0,
            scopes: crate::scopes::ScopeStack::new(),
            refs: crate::references::RefTable::new(),
        }
    }

    /// Empacota um `Box<RtsNapiEnv>` num `napi_env` opaco (transfere posse ao
    /// addon; liberado quando o addon for descarregado).
    pub fn into_raw(self: Box<Self>) -> napi_env {
        napi_env(Box::into_raw(self) as *mut std::ffi::c_void)
    }

    /// Reconstrói uma referência a partir do `napi_env` opaco. `unsafe`: o
    /// chamador garante que o ponteiro veio de `into_raw` e ainda é válido.
    ///
    /// # Safety
    /// `env.0` deve ser um ponteiro vivo produzido por [`RtsNapiEnv::into_raw`].
    pub unsafe fn from_raw<'a>(env: napi_env) -> Option<&'a mut RtsNapiEnv> {
        unsafe { (env.0 as *mut RtsNapiEnv).as_mut() }
    }
}

/// Nível N-API que o RTS implementa hoje. N-API 8 = baseline amplo
/// (Node 12.22+/14.17+/16+). Anunciado por `napi_get_version`.
pub const RTS_NAPI_VERSION: u32 = 8;

/// Converte um handle `u64` da HandleTable num `napi_value` opaco.
#[inline]
pub fn value_from_handle(handle: u64) -> napi_value {
    napi_value(handle as *mut std::ffi::c_void)
}

/// Extrai o handle `u64` de um `napi_value` opaco.
#[inline]
pub fn handle_from_value(value: napi_value) -> u64 {
    value.0 as u64
}
