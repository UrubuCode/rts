//! Loader dinâmico de addons `.node` (Fase 0).
//!
//! `__RTS_FN_NS_NAPI_LOAD_ADDON(path) -> handle` carrega o `.node` via
//! `libloading`, resolve `napi_register_module_v1`, fabrica um `RtsNapiEnv`,
//! cria o objeto `exports` (um `Entry::Map`) e devolve o handle do exports
//! populado — o valor que `import x from "./x.node"` produz no TS.
//!
//! **Idempotência por path:** um mesmo `.node` importado em vários módulos é
//! carregado uma única vez (cache global por path canônico) e o mesmo handle de
//! exports é devolvido. Isso casa com a semântica de `require`/ESM (módulo
//! singleton) e evita rodar o register duas vezes.
//!
//! **Keep-alive:** a `libloading::Library` NUNCA é descarregada enquanto o
//! processo vive — os `fn_ptr` que o addon registrou (callbacks) apontam para
//! dentro do `.node`; descarregá-lo deixaria ponteiros dangling. Guardamos a
//! `Library` num registry estático (leak intencional).

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Mutex;

use libloading::Library;
use rts_engine::heap::handles::{alloc_entry, Entry};

use crate::env::RtsNapiEnv;
use crate::types::{napi_env, napi_value};

/// Assinatura do entry point N-API exportado pelo `.node`.
type NapiRegisterFn = unsafe extern "C" fn(env: napi_env, exports: napi_value) -> napi_value;

struct LoadedAddon {
    /// Mantém o `.node` mapeado em memória pelo tempo de vida do processo.
    _lib: Library,
    /// Handle do `exports` populado pelo register (devolvido a cada import).
    exports_handle: u64,
}

// SAFETY: o `exports_handle` é um u64 simples; a `Library` é só mantida viva e
// nunca movida/tocada após o load. O acesso é serializado pelo Mutex.
unsafe impl Send for LoadedAddon {}

static LOADED_ADDONS: Mutex<Option<HashMap<String, LoadedAddon>>> = Mutex::new(None);

/// Carrega (ou recupera do cache) o addon `.node` em `path` e devolve o handle
/// do seu objeto `exports`. Em erro, devolve `0` (sentinela null-ish) — o
/// caller TS verá `undefined`/falha ao acessar membros. (Diagnóstico melhor é
/// Fase 1; aqui o foco é fechar o ciclo de carga.)
///
/// # Safety
/// `path_ptr`/`path_len` devem descrever uma UTF-8 válida (vêm de uma string
/// literal do codegen).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __RTS_FN_NS_NAPI_LOAD_ADDON(path_ptr: *const u8, path_len: i64) -> u64 {
    if path_ptr.is_null() || path_len < 0 {
        return 0;
    }
    let path = unsafe {
        let slice = std::slice::from_raw_parts(path_ptr, path_len as usize);
        match std::str::from_utf8(slice) {
            Ok(s) => s.to_string(),
            Err(_) => return 0,
        }
    };

    let mut guard = LOADED_ADDONS.lock().unwrap_or_else(|e| e.into_inner());
    let cache = guard.get_or_insert_with(HashMap::new);

    if let Some(existing) = cache.get(&path) {
        return existing.exports_handle;
    }

    let exports_handle = match unsafe { load_addon_uncached(&path) } {
        Ok((lib, handle)) => {
            cache.insert(
                path.clone(),
                LoadedAddon {
                    _lib: lib,
                    exports_handle: handle,
                },
            );
            handle
        }
        Err(_) => 0,
    };

    exports_handle
}

/// Faz o trabalho de carga propriamente dito (sem cache). Separado para manter
/// `__RTS_FN_NS_NAPI_LOAD_ADDON` curta e testável.
///
/// # Safety
/// `path` deve apontar para um arquivo `.node` (shared lib N-API) válido.
unsafe fn load_addon_uncached(path: &str) -> Result<(Library, u64), String> {
    // (registro legado) Esvazia a fila de módulos pendentes ANTES de carregar,
    // para que só o módulo deste `.node` (enfileirado pelo seu constructor
    // estático durante o `Library::new`) fique nela. Sem isso, um módulo de um
    // load anterior poderia ser confundido com o atual.
    let _ = crate::module_register::drain_pending_modules();

    // SAFETY: carregar uma shared lib roda o seu código de init — incluindo o
    // constructor que chama `napi_module_register` no caminho legado. Confiamos
    // que o usuário passou --allow-native-addons (gate no resolver).
    let lib = unsafe { Library::new(path) }
        .map_err(|e| format!("falha ao carregar addon '{path}': {e}"))?;

    // Resolve a função de init: caminho MODERNO (símbolo
    // `napi_register_module_v1`) tem prioridade; se ausente, caminho LEGADO
    // (o constructor já chamou `napi_module_register`, deixando o
    // `nm_register_func` na fila pendente). Um dos dois precisa existir.
    enum InitFn {
        Symbol(NapiRegisterFn),
        Legacy(crate::module_register::NapiAddonRegisterFunc),
    }
    let init = match unsafe { lib.get::<NapiRegisterFn>(b"napi_register_module_v1\0") } {
        Ok(sym) => InitFn::Symbol(*sym),
        Err(_) => {
            // Sem símbolo moderno: tenta o módulo legado enfileirado no load.
            let mut pending = crate::module_register::drain_pending_modules();
            match pending.pop() {
                Some(m) => InitFn::Legacy(m.register_func),
                None => {
                    return Err(format!(
                        "addon '{path}' não exporta napi_register_module_v1 nem \
                         chamou napi_module_register no load — não é um módulo \
                         N-API (V8-direto não é suportado)"
                    ))
                }
            }
        }
    };

    // Fabrica o env (uma instância por addon) e o objeto exports.
    let env_box = Box::new(RtsNapiEnv::new(crate::env::RTS_NAPI_VERSION));
    let env = env_box.into_raw();

    let exports_handle = alloc_entry(Entry::Map(Box::new(indexmap::IndexMap::new())));
    let exports = napi_value(exports_handle as *mut c_void);

    // Chama o entry point. O addon popula `exports` (ou devolve outro objeto).
    // Ambos os caminhos têm a mesma assinatura `(env, exports) -> exports`.
    let returned = match init {
        InitFn::Symbol(register) => unsafe { register(env, exports) },
        InitFn::Legacy(register) => {
            // O tipo legado usa `*mut c_void` para env/exports (forma C crua);
            // são ABI-idênticos a napi_env/napi_value (newtypes de ptr).
            unsafe { register(env.0, exports.0) }
        }
    };

    // O register pode devolver um exports diferente do passado (padrão raro mas
    // legal). Usa o retornado se for um handle não-nulo; senão o que criamos.
    let final_handle = {
        let r = returned.0 as u64;
        if r != 0 { r } else { exports_handle }
    };

    Ok((lib, final_handle))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Path inválido → handle 0, sem panic.
    #[test]
    fn load_invalid_path_returns_zero() {
        let path = "this_addon_does_not_exist_xyz.node";
        let h = unsafe { __RTS_FN_NS_NAPI_LOAD_ADDON(path.as_ptr(), path.len() as i64) };
        assert_eq!(h, 0);
    }

    /// Ptr nulo / len negativo → 0, sem panic.
    #[test]
    fn load_null_or_negative_returns_zero() {
        assert_eq!(unsafe { __RTS_FN_NS_NAPI_LOAD_ADDON(std::ptr::null(), 5) }, 0);
        let p = b"x";
        assert_eq!(unsafe { __RTS_FN_NS_NAPI_LOAD_ADDON(p.as_ptr(), -1) }, 0);
    }
}
