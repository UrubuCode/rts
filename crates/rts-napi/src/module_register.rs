//! Registro de módulo N-API **legado** (`napi_module_register`).
//!
//! Mecanismo do Node antes do registro simbólico (`napi_register_module_v1`):
//! o addon chama `napi_module_register(&mod)` de dentro de um **constructor
//! estático** (a macro `NAPI_MODULE` expande para `__attribute__((constructor))`
//! / `#pragma section` no MSVC) que roda quando o `.node` é `dlopen`ado. O
//! `napi_module` carrega o `nm_register_func` — o MESMO tipo de função de
//! inicialização que o `napi_register_module_v1` exporta — mais metadados
//! (`nm_modname`, `nm_version`).
//!
//! Não é V8-acoplado: a struct é ABI-estável C pura e o `nm_register_func` tem a
//! assinatura `(env, exports) -> exports`, idêntica ao entry simbólico. Logo é
//! implementável no RTS sem motor V8.
//!
//! **Fluxo:** `napi_module_register` empilha `(register_fn, modname)` numa fila
//! global. O loader (`loader.rs`), ao carregar um `.node`, primeiro DRENA a
//! fila, depois carrega a `Library` (o constructor roda aqui e enfileira o
//! módulo) e então — se algo foi enfileirado — usa esse `nm_register_func` em
//! vez de procurar `napi_register_module_v1`. Addons modernos não tocam esta
//! fila (exportam o símbolo direto); o caminho legado só ativa quando a fila
//! tem entrada após o load. Ver docs/specs/napi-implementation.md.

use std::ffi::{c_char, c_void};
use std::sync::Mutex;

use crate::types::napi_value;

/// Função de inicialização do addon — `(env, exports) -> exports`. Mesma
/// assinatura de `napi_register_module_v1`. O `napi_env`/`napi_value` são
/// ponteiros opacos; mantemos a forma C crua aqui para casar com o header.
pub type NapiAddonRegisterFunc =
    unsafe extern "C" fn(env: *mut c_void, exports: *mut c_void) -> napi_value;

/// `napi_module` — espelho ABI-estável de `struct napi_module` do `node_api.h`.
/// O layout (ordem/tipos dos campos) é contrato com o addon compilado; NÃO
/// reordenar. `reserved[4]` preserva o tamanho/alinhamento do struct original.
#[repr(C)]
pub struct napi_module {
    pub nm_version: i32,
    pub nm_flags: u32,
    pub nm_filename: *const c_char,
    pub nm_register_func: Option<NapiAddonRegisterFunc>,
    pub nm_modname: *const c_char,
    pub nm_priv: *mut c_void,
    pub reserved: [*mut c_void; 4],
}

/// Um módulo legado enfileirado pelo constructor do addon, aguardando o loader.
pub struct PendingModule {
    pub register_func: NapiAddonRegisterFunc,
    /// `nm_modname` copiado como String (o ponteiro original aponta para dentro
    /// do `.node`, vivo enquanto a Library viver — mas copiamos por robustez).
    pub modname: String,
}

// SAFETY: `register_func` é um fn-ptr para dentro do `.node` (mantido vivo pela
// Library no loader); `modname` é uma String própria. O acesso é serializado
// pelo Mutex.
unsafe impl Send for PendingModule {}

static PENDING_MODULES: Mutex<Vec<PendingModule>> = Mutex::new(Vec::new());

/// `napi_module_register(mod*)` — registro legado. O addon chama isto de um
/// constructor estático no `dlopen`. Guardamos o `nm_register_func` para o
/// loader consumir logo após carregar a Library.
///
/// # Safety
/// `module` deve apontar para um `napi_module` válido (vem do `.node`). Lemos
/// `nm_register_func` e `nm_modname` sem reter o ponteiro.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_module_register(module: *mut napi_module) {
    if module.is_null() {
        return;
    }
    let m = unsafe { &*module };
    let Some(register_func) = m.nm_register_func else {
        return;
    };
    let modname = if m.nm_modname.is_null() {
        String::new()
    } else {
        unsafe { std::ffi::CStr::from_ptr(m.nm_modname) }
            .to_string_lossy()
            .into_owned()
    };
    PENDING_MODULES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(PendingModule {
            register_func,
            modname,
        });
}

/// Esvazia a fila de módulos pendentes (chamado pelo loader ANTES de carregar a
/// Library, para garantir que só o módulo recém-carregado fique na fila).
pub fn drain_pending_modules() -> Vec<PendingModule> {
    std::mem::take(&mut *PENDING_MODULES.lock().unwrap_or_else(|e| e.into_inner()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::ptr;

    unsafe extern "C" fn dummy_register(
        _env: *mut c_void,
        exports: *mut c_void,
    ) -> napi_value {
        napi_value(exports)
    }

    #[test]
    fn register_enqueues_and_drain_clears() {
        // Limpa qualquer resíduo de outros testes.
        let _ = drain_pending_modules();
        let name = CString::new("legacy_addon").unwrap();
        let mut m = napi_module {
            nm_version: 1,
            nm_flags: 0,
            nm_filename: ptr::null(),
            nm_register_func: Some(dummy_register),
            nm_modname: name.as_ptr(),
            nm_priv: ptr::null_mut(),
            reserved: [ptr::null_mut(); 4],
        };
        unsafe { napi_module_register(&mut m) };
        let pending = drain_pending_modules();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].modname, "legacy_addon");
        // Segunda drenagem vem vazia.
        assert!(drain_pending_modules().is_empty());
    }

    #[test]
    fn null_module_is_noop() {
        let _ = drain_pending_modules();
        unsafe { napi_module_register(ptr::null_mut()) };
        assert!(drain_pending_modules().is_empty());
    }

    #[test]
    fn module_without_register_func_is_skipped() {
        let _ = drain_pending_modules();
        let mut m = napi_module {
            nm_version: 1,
            nm_flags: 0,
            nm_filename: ptr::null(),
            nm_register_func: None,
            nm_modname: ptr::null(),
            nm_priv: ptr::null_mut(),
            reserved: [ptr::null_mut(); 4],
        };
        unsafe { napi_module_register(&mut m) };
        assert!(drain_pending_modules().is_empty());
    }
}
