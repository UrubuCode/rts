//! Extracao de partes de um caminho (parent, file_name, stem, ext).
//!
//! Todas as operacoes sao puras sobre a string — nao fazem I/O, nao
//! resolvem symlinks. Retornam handles de string via
//! `gc::string_pool::__RTS_FN_NS_GC_STRING_NEW`. Handle 0 significa
//! "nao existe" (ex: arquivo sem extensao, caminho sem parent).

use std::path::Path;

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn str_from_abi<'a>(ptr: *const u8, len: i64) -> Option<&'a str> {
    if ptr.is_null() || len < 0 {
        return None;
    }
    // SAFETY: caller contract — UTF-8 valido cobrindo `len` bytes.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).ok()
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PATH_PARENT(ptr: *const u8, len: i64) -> u64 {
    let Some(s) = str_from_abi(ptr, len) else {
        return 0;
    };
    Path::new(s)
        .parent()
        .and_then(|p| p.to_str())
        .map(intern)
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PATH_FILE_NAME(ptr: *const u8, len: i64) -> u64 {
    let Some(s) = str_from_abi(ptr, len) else {
        return 0;
    };
    Path::new(s)
        .file_name()
        .and_then(|n| n.to_str())
        .map(intern)
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PATH_STEM(ptr: *const u8, len: i64) -> u64 {
    let Some(s) = str_from_abi(ptr, len) else {
        return 0;
    };
    Path::new(s)
        .file_stem()
        .and_then(|n| n.to_str())
        .map(intern)
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PATH_EXT(ptr: *const u8, len: i64) -> u64 {
    let Some(s) = str_from_abi(ptr, len) else {
        return 0;
    };
    Path::new(s)
        .extension()
        .and_then(|n| n.to_str())
        .map(intern)
        .unwrap_or(0)
}
