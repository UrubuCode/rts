//! Shared helpers for the `fs` ABI surface.
//!
//! Path arguments arrive as `(ptr, len)` pairs pointing at UTF-8 bytes.
//! These helpers reject malformed inputs early so every operation can
//! treat paths as a validated `&Path`.

use std::path::Path;

/// Materialises a `&Path` from ABI `(ptr, len)`.
///
/// Returns `None` for null pointers, negative lengths or non-UTF-8 bytes.
///
/// # Safety
/// `ptr` must be valid UTF-8 for `len` bytes and live for the borrow.
pub(super) unsafe fn path_from_abi<'a>(ptr: *const u8, len: i64) -> Option<&'a Path> {
    if ptr.is_null() || len < 0 {
        return None;
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).ok().map(Path::new)
}
