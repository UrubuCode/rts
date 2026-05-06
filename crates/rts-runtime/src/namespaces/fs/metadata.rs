//! Filesystem metadata queries. Thin wrappers over `std::fs::metadata`.

use super::common::path_from_abi;

/// Returns `1` if the path exists, `0` otherwise. Any error (including
/// permission denied) is reported as `0`, matching `Path::exists`.
///
/// # Safety
/// Path bytes must be UTF-8, live for `len` bytes.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_EXISTS(path_ptr: *const u8, path_len: i64) -> i64 {
    let Some(path) = (unsafe { path_from_abi(path_ptr, path_len) }) else {
        return 0;
    };
    if path.exists() { 1 } else { 0 }
}

/// Returns `1` if the path resolves to a file, `0` otherwise.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_IS_FILE(path_ptr: *const u8, path_len: i64) -> i64 {
    let Some(path) = (unsafe { path_from_abi(path_ptr, path_len) }) else {
        return 0;
    };
    if path.is_file() { 1 } else { 0 }
}

/// Returns `1` if the path resolves to a directory, `0` otherwise.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_IS_DIR(path_ptr: *const u8, path_len: i64) -> i64 {
    let Some(path) = (unsafe { path_from_abi(path_ptr, path_len) }) else {
        return 0;
    };
    if path.is_dir() { 1 } else { 0 }
}

/// Returns the file size in bytes, or `-1` on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_SIZE(path_ptr: *const u8, path_len: i64) -> i64 {
    let Some(path) = (unsafe { path_from_abi(path_ptr, path_len) }) else {
        return -1;
    };
    match std::fs::metadata(path) {
        Ok(m) => m.len() as i64,
        Err(_) => -1,
    }
}

/// Returns the last modified timestamp in milliseconds since UNIX epoch,
/// or `-1` on error / unsupported filesystems.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_MODIFIED_MS(path_ptr: *const u8, path_len: i64) -> i64 {
    let Some(path) = (unsafe { path_from_abi(path_ptr, path_len) }) else {
        return -1;
    };
    let Ok(meta) = std::fs::metadata(path) else {
        return -1;
    };
    let Ok(time) = meta.modified() else { return -1 };
    match time.duration_since(std::time::UNIX_EPOCH) {
        Ok(dur) => dur.as_millis().min(i64::MAX as u128) as i64,
        Err(_) => -1,
    }
}
