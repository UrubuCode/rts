//! Directory creation, removal, and enumeration. Wraps `std::fs` directory ops.

use super::common::path_from_abi;
use crate::namespaces::gc::handles::{Entry, alloc_entry};
use crate::namespaces::gc::string_pool::__RTS_FN_NS_GC_STRING_NEW;

/// Creates the directory at `path`. Fails if the parent is missing.
/// Returns `0` on success, `-1` on error.
///
/// # Safety
/// Path bytes must be UTF-8 and live for `path_len` bytes.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_CREATE_DIR(path_ptr: *const u8, path_len: i64) -> i64 {
    let Some(path) = (unsafe { path_from_abi(path_ptr, path_len) }) else {
        return -1;
    };
    match std::fs::create_dir(path) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Creates the directory and all missing parents. Returns `0` on success,
/// `-1` on error.
///
/// # Safety
/// See [`__RTS_FN_NS_FS_CREATE_DIR`].
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_CREATE_DIR_ALL(path_ptr: *const u8, path_len: i64) -> i64 {
    let Some(path) = (unsafe { path_from_abi(path_ptr, path_len) }) else {
        return -1;
    };
    match std::fs::create_dir_all(path) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Removes the empty directory at `path`. Returns `0` / `-1`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_REMOVE_DIR(path_ptr: *const u8, path_len: i64) -> i64 {
    let Some(path) = (unsafe { path_from_abi(path_ptr, path_len) }) else {
        return -1;
    };
    match std::fs::remove_dir(path) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Removes the directory at `path` recursively. Returns `0` / `-1`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_REMOVE_DIR_ALL(path_ptr: *const u8, path_len: i64) -> i64 {
    let Some(path) = (unsafe { path_from_abi(path_ptr, path_len) }) else {
        return -1;
    };
    match std::fs::remove_dir_all(path) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Lê o conteúdo do diretório em `path` e retorna um handle de
/// `Vec<i64>` contendo handles de string para cada nome de entrada
/// (apenas o `file_name`, sem o caminho completo, igual a
/// `node:fs.readdirSync`). Entradas com nomes não-UTF-8 são ignoradas.
/// Retorna `0` em qualquer erro de I/O.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_READDIR(path_ptr: *const u8, path_len: i64) -> u64 {
    let Some(path) = (unsafe { path_from_abi(path_ptr, path_len) }) else {
        return 0;
    };
    let Ok(iter) = std::fs::read_dir(path) else {
        return 0;
    };
    let mut entries: Vec<i64> = Vec::new();
    for entry in iter.flatten() {
        let name = entry.file_name();
        if let Some(s) = name.to_str() {
            let h = __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64);
            entries.push(h as i64);
        }
    }
    alloc_entry(Entry::Vec(Box::new(entries)))
}
