//! `io.stderr_write` — raw byte writer to stderr. Mirrors the stdout variant.

use std::io::{self, Write};

use super::print::slice_from_abi;

/// Writes up to `len` bytes from `ptr` to stderr.
/// Returns bytes written, or `-1` on failure.
///
/// # Safety
/// `ptr` must be valid for `len` bytes.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_IO_STDERR_WRITE(ptr: *const u8, len: i64) -> i64 {
    let Some(slice) = slice_from_abi(ptr, len) else {
        return -1;
    };
    let stderr = io::stderr();
    let mut lock = stderr.lock();
    match lock.write(slice) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

/// Flushes the stderr buffer. Returns `0` on success, `-1` on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_IO_STDERR_FLUSH() -> i64 {
    let stderr = io::stderr();
    let mut lock = stderr.lock();
    match lock.flush() {
        Ok(()) => 0,
        Err(_) => -1,
    }
}
