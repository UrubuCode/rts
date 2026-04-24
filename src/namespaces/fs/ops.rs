//! File-level operations: remove, rename, copy.

use super::common::path_from_abi;

/// Removes the file at `path`. Returns `0` on success, `-1` on error.
///
/// # Safety
/// Path bytes must be UTF-8 and live for `path_len` bytes.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_REMOVE_FILE(path_ptr: *const u8, path_len: i64) -> i64 {
    let Some(path) = (unsafe { path_from_abi(path_ptr, path_len) }) else {
        return -1;
    };
    match std::fs::remove_file(path) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Renames the filesystem entry at `from` to `to`. Returns `0` / `-1`.
///
/// # Safety
/// Both path pairs must reference live UTF-8 bytes for their declared
/// lengths.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_RENAME(
    from_ptr: *const u8,
    from_len: i64,
    to_ptr: *const u8,
    to_len: i64,
) -> i64 {
    let Some(from) = (unsafe { path_from_abi(from_ptr, from_len) }) else {
        return -1;
    };
    let Some(to) = (unsafe { path_from_abi(to_ptr, to_len) }) else {
        return -1;
    };
    match std::fs::rename(from, to) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Copies the file contents from `from` to `to`. Returns bytes copied or
/// `-1` on error. Matches `std::fs::copy`.
///
/// # Safety
/// Both path pairs must reference live UTF-8 bytes for their declared
/// lengths.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_COPY(
    from_ptr: *const u8,
    from_len: i64,
    to_ptr: *const u8,
    to_len: i64,
) -> i64 {
    let Some(from) = (unsafe { path_from_abi(from_ptr, from_len) }) else {
        return -1;
    };
    let Some(to) = (unsafe { path_from_abi(to_ptr, to_len) }) else {
        return -1;
    };
    match std::fs::copy(from, to) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}
