//! File write operations backed by `std::fs`.

use std::fs::OpenOptions;
use std::io::Write;

use super::common::path_from_abi;

/// Writes `data` to the file at `path`, truncating existing contents.
/// Creates the file when absent. Matches `std::fs::write`.
///
/// Returns bytes written, or `-1` on error.
///
/// # Safety
/// Both pointer pairs must reference valid, live memory of the declared
/// length. Path bytes must be UTF-8.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_WRITE(
    path_ptr: *const u8,
    path_len: i64,
    data_ptr: *const u8,
    data_len: i64,
) -> i64 {
    let Some(path) = (unsafe { path_from_abi(path_ptr, path_len) }) else {
        return -1;
    };
    if data_ptr.is_null() || data_len < 0 {
        return -1;
    }
    // SAFETY: caller contract requires live data for `data_len` bytes.
    let data = unsafe { std::slice::from_raw_parts(data_ptr, data_len as usize) };
    match std::fs::write(path, data) {
        Ok(()) => data_len,
        Err(_) => -1,
    }
}

/// Appends `data` to the file at `path`, creating it if missing.
/// Returns bytes written, or `-1` on error.
///
/// # Safety
/// Same contract as [`__RTS_FN_NS_FS_WRITE`].
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_APPEND(
    path_ptr: *const u8,
    path_len: i64,
    data_ptr: *const u8,
    data_len: i64,
) -> i64 {
    let Some(path) = (unsafe { path_from_abi(path_ptr, path_len) }) else {
        return -1;
    };
    if data_ptr.is_null() || data_len < 0 {
        return -1;
    }
    let data = unsafe { std::slice::from_raw_parts(data_ptr, data_len as usize) };
    let mut file = match OpenOptions::new().append(true).create(true).open(path) {
        Ok(f) => f,
        Err(_) => return -1,
    };
    match file.write_all(data) {
        Ok(()) => data_len,
        Err(_) => -1,
    }
}
