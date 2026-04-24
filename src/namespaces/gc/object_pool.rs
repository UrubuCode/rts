//! Object allocator for the GC namespace.
//!
//! Classes and other composite values are backed by a zeroed byte buffer
//! addressed through a handle. Codegen allocates the buffer once via
//! `__RTS_FN_NS_GC_OBJECT_NEW`, keeps the handle around, and performs direct
//! `load`/`store` instructions against the pointer returned by
//! `__RTS_FN_NS_GC_OBJECT_PTR` — avoiding an ABI round-trip per field access.
//!
//! The pointer stays valid while the handle is live. Callers must not outlive
//! the handle nor exceed the buffer size.

use super::handles::{Entry, table};

/// Allocates a zeroed object buffer of `size` bytes and returns its handle.
/// Returns `0` on invalid size.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_OBJECT_NEW(size: i64) -> u64 {
    if size < 0 {
        return 0;
    }
    let bytes = vec![0u8; size as usize];
    let mut t = table().lock().expect("handle table poisoned");
    t.alloc(Entry::Object(bytes))
}

/// Returns the raw pointer to the object's buffer, or `0` on invalid handle.
/// The pointer is stable for the lifetime of the handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_OBJECT_PTR(handle: u64) -> u64 {
    let t = table().lock().expect("handle table poisoned");
    match t.get(handle) {
        Some(Entry::Object(bytes)) => bytes.as_ptr() as u64,
        _ => 0,
    }
}

/// Returns the byte size of the object buffer behind `handle`, or `-1` on
/// invalid handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_OBJECT_SIZE(handle: u64) -> i64 {
    let t = table().lock().expect("handle table poisoned");
    match t.get(handle) {
        Some(Entry::Object(bytes)) => bytes.len() as i64,
        _ => -1,
    }
}

/// Frees the object handle. Returns `1` on success, `0` if already invalid.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_OBJECT_FREE(handle: u64) -> i64 {
    let mut t = table().lock().expect("handle table poisoned");
    if t.free(handle) { 1 } else { 0 }
}
