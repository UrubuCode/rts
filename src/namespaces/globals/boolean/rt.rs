//! Boolean — runtime implementations.

use crate::abi::handles::{alloc_str_handle, StrHandle};

/// Boolean(value) — coerce value to boolean (identity for bool).
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_BOOLEAN_NEW(value: bool) -> bool {
    value
}

/// Boolean() — returns false.
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_BOOLEAN_NEW_EMPTY() -> bool {
    false
}

/// b.toString() — returns "true" or "false" as a string handle.
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_BOOLEAN_TO_STRING(value: bool) -> u64 {
    let s = if value { "true" } else { "false" };
    alloc_str_handle(s.as_bytes())
}

/// b.valueOf() — returns the primitive boolean value.
#[no_mangle]
pub extern "C" fn __RTS_FN_GL_BOOLEAN_VALUE_OF(value: bool) -> bool {
    value
}

