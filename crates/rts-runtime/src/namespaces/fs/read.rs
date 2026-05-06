//! File read operations backed by `std::fs`.
//!
//! Two flavours are exposed: byte-oriented reads into a caller buffer, and
//! metadata-style size queries. A future pass will add handle-returning
//! variants once the GC string pool is in place; for now, the byte-buffer
//! form matches the semantics of `File::read` exactly.

use std::fs::File;
use std::io::Read;

use super::common::path_from_abi;

/// Reads up to `buf_len` bytes from the file at `path` into `buf_ptr`.
///
/// Returns the number of bytes read, `0` on EOF, or `-1` on error
/// (missing file, permission denied, invalid path).
///
/// # Safety
/// Path slice must be valid UTF-8 for `path_len` bytes. Buffer must be
/// writable for `buf_len` bytes.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_READ(
    path_ptr: *const u8,
    path_len: i64,
    buf_ptr: *mut u8,
    buf_len: i64,
) -> i64 {
    let Some(path) = (unsafe { path_from_abi(path_ptr, path_len) }) else {
        return -1;
    };
    if buf_ptr.is_null() || buf_len <= 0 {
        return -1;
    }
    // SAFETY: caller guarantees writable buffer for `buf_len` bytes.
    let slot = unsafe { std::slice::from_raw_parts_mut(buf_ptr, buf_len as usize) };
    match File::open(path).and_then(|mut f| f.read(slot)) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

/// Reads the entire file into the caller buffer. Mirrors `fs::read` but
/// truncates if the file is larger than `buf_len`.
///
/// Returns bytes written, or `-1` on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FS_READ_ALL(
    path_ptr: *const u8,
    path_len: i64,
    buf_ptr: *mut u8,
    buf_len: i64,
) -> i64 {
    let Some(path) = (unsafe { path_from_abi(path_ptr, path_len) }) else {
        return -1;
    };
    if buf_ptr.is_null() || buf_len <= 0 {
        return -1;
    }
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return -1,
    };
    let copy = bytes.len().min(buf_len as usize);
    // SAFETY: buffer must be writable for `copy <= buf_len` bytes.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf_ptr, copy);
    }
    copy as i64
}
