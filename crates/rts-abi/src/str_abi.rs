//! String ABI helper shared by macro-generated externs.
//!
//! A `Str` parameter lowers to two slots `(ptr: *const u8, len: i64)` (UTF-8).
//! `#[rts_fn]` injects a guard that reconstructs the `&str` via [`from_abi`]
//! and early-returns a zero default on a null pointer / invalid UTF-8, matching
//! the hand-written convention (`str_from_abi` + `return 0`).

/// Reconstructs a UTF-8 `&str` from an ABI `(ptr, len)` pair.
///
/// Returns `None` on a null pointer, negative length, or invalid UTF-8.
///
/// # Safety
/// `ptr` must be valid for `len` bytes for the duration of the borrow, as
/// guaranteed by the codegen calling convention (the string buffer outlives
/// the call).
pub unsafe fn from_abi<'a>(ptr: *const u8, len: i64) -> Option<&'a str> {
    if ptr.is_null() || len < 0 {
        return None;
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).ok()
}
