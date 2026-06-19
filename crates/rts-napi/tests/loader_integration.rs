//! Teste de integração do loader N-API (Fase 0): compila um addon `.node`
//! mínimo via `rustc` (cdylib) e valida que `__RTS_FN_NS_NAPI_LOAD_ADDON`
//! carrega, resolve `napi_register_module_v1` e devolve um handle de exports
//! válido. Ignorado se `rustc` não estiver no PATH (ambiente sem toolchain).
//!
//! Cobre o ciclo real de carga que os testes unitários (path inválido) não
//! exercitam. Ver docs/specs/napi-implementation.md (Etapa 4).

use std::path::PathBuf;
use std::process::Command;

// O rlib do rts-napi referencia `__RTS_FN_GL_FUNCTION_CALL` (de rts-primitives,
// só presente no bin). Fornecemos um stub aqui para o teste de integração
// linkar; ele não é exercitado (este teste não chama napi_call_function).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FUNCTION_CALL(_h: u64, _t: i64, _a: u64) -> i64 {
    0
}

/// Fonte do addon dummy: devolve o `exports` recebido (objeto vazio populável).
const DUMMY_ADDON_SRC: &str = r#"
use std::ffi::c_void;
#[no_mangle]
pub extern "C" fn napi_register_module_v1(_env: *mut c_void, exports: *mut c_void) -> *mut c_void {
    exports
}
"#;

fn rustc_available() -> bool {
    Command::new("rustc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Compila `DUMMY_ADDON_SRC` como cdylib e renomeia para `.node`. Retorna o path
/// do `.node`, ou `None` se a compilação falhar.
fn build_dummy_addon(dir: &std::path::Path) -> Option<PathBuf> {
    let src = dir.join("dummy_addon.rs");
    std::fs::write(&src, DUMMY_ADDON_SRC).ok()?;

    // Nome de saída do cdylib é dependente de plataforma; deixamos o rustc
    // escolher via --crate-name e localizamos o artefato depois.
    let out_dir = dir.to_path_buf();
    let status = Command::new("rustc")
        .args(["--crate-type", "cdylib", "--crate-name", "dummy_addon"])
        .arg(&src)
        .arg("--out-dir")
        .arg(&out_dir)
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }

    // Localiza o artefato (dummy_addon.dll / libdummy_addon.so / .dylib).
    for name in [
        "dummy_addon.dll",
        "libdummy_addon.so",
        "libdummy_addon.dylib",
    ] {
        let candidate = out_dir.join(name);
        if candidate.exists() {
            let node = out_dir.join("dummy_addon.node");
            std::fs::copy(&candidate, &node).ok()?;
            return Some(node);
        }
    }
    None
}

#[test]
fn load_real_addon_returns_valid_exports_handle() {
    if !rustc_available() {
        eprintln!("rustc indisponível — pulando teste de integração do loader");
        return;
    }

    // Diretório temporário único (sem Date/rand: usa o pid + nome do teste).
    let tmp = std::env::temp_dir().join(format!("rts_napi_loader_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);

    let Some(node_path) = build_dummy_addon(&tmp) else {
        eprintln!("falha ao compilar addon dummy — pulando");
        let _ = std::fs::remove_dir_all(&tmp);
        return;
    };

    let path_str = node_path.to_string_lossy().to_string();
    let handle = unsafe {
        rts_napi::loader::__RTS_FN_NS_NAPI_LOAD_ADDON(path_str.as_ptr(), path_str.len() as i64)
    };

    assert_ne!(handle, 0, "loader deve devolver um handle de exports não-nulo");

    // O handle deve ser um Entry::Map (o objeto exports) vivo na HandleTable.
    rts_engine::heap::handles::with_entry(handle, |entry| {
        assert!(
            matches!(entry, Some(rts_engine::heap::handles::Entry::Map(_))),
            "exports deve ser um Entry::Map vivo, foi {entry:?}"
        );
    });

    // Idempotência: carregar de novo o mesmo path devolve o MESMO handle.
    let handle2 = unsafe {
        rts_napi::loader::__RTS_FN_NS_NAPI_LOAD_ADDON(path_str.as_ptr(), path_str.len() as i64)
    };
    assert_eq!(handle, handle2, "mesmo path deve ser idempotente (cache)");

    let _ = std::fs::remove_dir_all(&tmp);
}
