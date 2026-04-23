//! Directory creation and removal. Wraps `std::fs` directory ops.

use super::common::path_from_abi;

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
