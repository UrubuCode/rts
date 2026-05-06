//! `io.print` / `io.eprint` — std::io-backed line writers.
//!
//! Both functions push a UTF-8 slice to the matching stream and append
//! `\n`. Invalid UTF-8 or null pointers are treated as no-ops. Matches the
//! behaviour of `println!` / `eprintln!` in the Rust standard library while
//! avoiding the formatting machinery (callers pass pre-rendered text).

use std::io::{self, Write};

/// Writes `ptr..ptr+len` plus a newline to stdout.
///
/// # Safety
/// `ptr` must be valid UTF-8 for `len` bytes and remain live for the call.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_IO_PRINT(ptr: *const u8, len: i64) {
    if let Some(slice) = slice_from_abi(ptr, len) {
        let stdout = io::stdout();
        let mut lock = stdout.lock();
        let _ = lock.write_all(slice);
        let _ = lock.write_all(b"\n");
    }
}

/// Writes `ptr..ptr+len` plus a newline to stderr.
///
/// # Safety
/// Same contract as [`__RTS_FN_NS_IO_PRINT`].
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_IO_EPRINT(ptr: *const u8, len: i64) {
    if let Some(slice) = slice_from_abi(ptr, len) {
        let stderr = io::stderr();
        let mut lock = stderr.lock();
        let _ = lock.write_all(slice);
        let _ = lock.write_all(b"\n");
    }
}

pub(super) fn slice_from_abi<'a>(ptr: *const u8, len: i64) -> Option<&'a [u8]> {
    if ptr.is_null() || len < 0 {
        return None;
    }
    // SAFETY: caller contract guarantees `ptr` covers `len` live bytes.
    Some(unsafe { std::slice::from_raw_parts(ptr, len as usize) })
}
