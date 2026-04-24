//! `io.stdout_write` — raw byte writer to stdout.
//!
//! Mirrors `std::io::Write::write` called on `io::stdout().lock()`. Unlike
//! [`print`], no newline is appended and the actual number of bytes written
//! is returned. Callers loop on partial writes exactly like with a native
//! `std::io::Write` implementation.

use std::io::{self, Write};

use super::print::slice_from_abi;

/// Writes up to `len` bytes from `ptr` to stdout.
///
/// Returns the byte count on success, or `-1` on I/O error / invalid input.
///
/// # Safety
/// `ptr` must be valid for `len` bytes; contents are treated as opaque.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_IO_STDOUT_WRITE(ptr: *const u8, len: i64) -> i64 {
    let Some(slice) = slice_from_abi(ptr, len) else {
        return -1;
    };
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    match lock.write(slice) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

/// Flushes the stdout buffer. Returns `0` on success, `-1` on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_IO_STDOUT_FLUSH() -> i64 {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    match lock.flush() {
        Ok(()) => 0,
        Err(_) => -1,
    }
}
