//! Composicao, normalizacao e testes de caminho.

use std::path::{Component, Path, PathBuf};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn str_from_abi<'a>(ptr: *const u8, len: i64) -> Option<&'a str> {
    if ptr.is_null() || len < 0 {
        return None;
    }
    // SAFETY: caller contract.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).ok()
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PATH_JOIN(
    base_ptr: *const u8,
    base_len: i64,
    part_ptr: *const u8,
    part_len: i64,
) -> u64 {
    let Some(base) = str_from_abi(base_ptr, base_len) else {
        return 0;
    };
    let Some(part) = str_from_abi(part_ptr, part_len) else {
        return 0;
    };
    let joined = PathBuf::from(base).join(part);
    joined.to_str().map(intern).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PATH_IS_ABSOLUTE(ptr: *const u8, len: i64) -> i64 {
    match str_from_abi(ptr, len) {
        Some(s) if Path::new(s).is_absolute() => 1,
        _ => 0,
    }
}

/// Normaliza removendo componentes `.` e resolvendo `..` onde possivel
/// (sem tocar o filesystem; preserva `..` iniciais que apontam para
/// fora do caminho base).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PATH_NORMALIZE(ptr: *const u8, len: i64) -> u64 {
    let Some(s) = str_from_abi(ptr, len) else {
        return 0;
    };
    let mut out = PathBuf::new();
    for comp in Path::new(s).components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                // So apaga o ultimo se ele for um componente "normal";
                // senao preserva o `..` (ex: path relativo fora da
                // raiz corrente).
                let pop_ok = out
                    .components()
                    .next_back()
                    .map(|c| matches!(c, Component::Normal(_)))
                    .unwrap_or(false);
                if pop_ok {
                    out.pop();
                } else {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    let rendered = out.to_string_lossy();
    intern(&rendered)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PATH_WITH_EXT(
    path_ptr: *const u8,
    path_len: i64,
    ext_ptr: *const u8,
    ext_len: i64,
) -> u64 {
    let Some(path) = str_from_abi(path_ptr, path_len) else {
        return 0;
    };
    let Some(ext) = str_from_abi(ext_ptr, ext_len) else {
        return 0;
    };
    let result = Path::new(path).with_extension(ext);
    result.to_str().map(intern).unwrap_or(0)
}
