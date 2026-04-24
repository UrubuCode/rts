//! `io.stdin_read` — byte reader backed by `std::io::stdin`.

use std::io::{self, BufRead, Read};

/// Reads up to `buf_len` bytes from stdin into a caller-provided buffer.
///
/// Returns the number of bytes read (possibly `0` on EOF) or `-1` on error.
///
/// # Safety
/// `buf_ptr` must be writable for `buf_len` bytes for the duration of the
/// call.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_IO_STDIN_READ(buf_ptr: *mut u8, buf_len: i64) -> i64 {
    if buf_ptr.is_null() || buf_len <= 0 {
        return -1;
    }
    // SAFETY: caller guarantees `buf_ptr` is writable for `buf_len` bytes.
    let slot = unsafe { std::slice::from_raw_parts_mut(buf_ptr, buf_len as usize) };
    let stdin = io::stdin();
    let mut lock = stdin.lock();
    match lock.read(slot) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

/// Reads a single line from stdin (terminator stripped) into the provided
/// buffer. Returns the number of bytes written, `0` on EOF, or `-1` on
/// error / invalid input.
///
/// # Safety
/// Same buffer contract as [`__RTS_FN_NS_IO_STDIN_READ`].
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_IO_STDIN_READ_LINE(buf_ptr: *mut u8, buf_len: i64) -> i64 {
    if buf_ptr.is_null() || buf_len <= 0 {
        return -1;
    }
    let stdin = io::stdin();
    let mut line = String::new();
    let read = match stdin.lock().read_line(&mut line) {
        Ok(n) => n,
        Err(_) => return -1,
    };
    let bytes = line.trim_end_matches(['\r', '\n']).as_bytes();
    let copy = bytes.len().min(buf_len as usize);
    // SAFETY: caller guarantees `buf_ptr` is writable for `buf_len` bytes.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf_ptr, copy);
    }
    if read == 0 { 0 } else { copy as i64 }
}
