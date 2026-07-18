//! node:fs — value helpers. The generic ones (ABI string read, interning, byte
//! input/output, options-object fields) live in the crate-shared
//! [`crate::values`]; this module re-exports them under the names `fs` uses and
//! adds the fs-specific piece: turning a `std::io::Error` into a Node-style
//! thrown error (`ENOENT: ...`).

pub use crate::values::{byte_array, intern, opt_bool, read, read_bytes, string_array};

/// Decode a boxed-string PolyValue WORD to its UTF-8 text (empty for non-strings).
pub fn word_string(word: u64) -> String {
    match crate::values::val(word) {
        crate::values::Val::Str(s) => s,
        _ => String::new(),
    }
}

unsafe extern "C" {
    fn __rtsadp_throw_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64);
}

/// The Node error code for an `io::Error`.
pub(super) fn err_code(e: &std::io::Error) -> &'static str {
    use std::io::ErrorKind::*;
    match e.kind() {
        NotFound => "ENOENT",
        PermissionDenied => "EACCES",
        AlreadyExists => "EEXIST",
        _ => match e.raw_os_error() {
            Some(13) => "EACCES",
            Some(2) => "ENOENT",
            Some(17) => "EEXIST",
            Some(20) => "ENOTDIR",
            Some(21) => "EISDIR",
            Some(39) | Some(41) | Some(145) => "ENOTEMPTY",
            _ => "EIO",
        },
    }
}

/// Throw a Node-style `Error` for a failed fs op (`<CODE>: <msg>, <op> '<path>'`).
pub fn throw_io(e: &std::io::Error, op: &str, path: &str) {
    let code = err_code(e);
    let msg = format!("{code}: {e}, {op} '{path}'");
    unsafe { __rtsadp_throw_js_error(code.as_ptr(), code.len() as i64, msg.as_ptr(), msg.len() as i64) };
}
