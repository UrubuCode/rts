//! node:util — the `extern "C"` entry points. `format` is variadic → fixed-arity
//! overloads (0..4); `isDeepStrictEqual` reuses the `assert` deep-equal; the
//! string utilities are pure.

use super::format::format;
use super::inspect::{inspect, inspect_with_options};
use super::words::{error_name, intern, strip_vt, style_text, to_usv_string};

fn read(ptr: *const u8, len: i64) -> String {
    unsafe { rts_engine::abi::str_abi::from_abi(ptr, len) }
        .unwrap_or("")
        .to_string()
}

/// `util.format(fmt)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_UTIL_FORMAT0(fp: *const u8, fl: i64) -> u64 {
    intern(&format(&read(fp, fl), &[]))
}
/// `util.format(fmt, a0)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_UTIL_FORMAT1(fp: *const u8, fl: i64, a0: u64) -> u64 {
    intern(&format(&read(fp, fl), &[a0]))
}
/// `util.format(fmt, a0, a1)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_UTIL_FORMAT2(fp: *const u8, fl: i64, a0: u64, a1: u64) -> u64 {
    intern(&format(&read(fp, fl), &[a0, a1]))
}
/// `util.format(fmt, a0, a1, a2)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_UTIL_FORMAT3(fp: *const u8, fl: i64, a0: u64, a1: u64, a2: u64) -> u64 {
    intern(&format(&read(fp, fl), &[a0, a1, a2]))
}
/// `util.format(fmt, a0, a1, a2, a3)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_UTIL_FORMAT4(fp: *const u8, fl: i64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    intern(&format(&read(fp, fl), &[a0, a1, a2, a3]))
}

/// `util.inspect(value)` → its textual representation.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_UTIL_INSPECT(value: u64) -> u64 {
    intern(&inspect(value))
}

/// `util.inspect(value, options)` — honors `options.depth`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_UTIL_INSPECT_OPTS(value: u64, options: u64) -> u64 {
    intern(&inspect_with_options(value, options))
}

/// `util.isDeepStrictEqual(a, b)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_UTIL_IS_DEEP_STRICT_EQUAL(a: u64, b: u64) -> i64 {
    crate::assert::words::deep_equal(a, b, true) as i64
}

/// `util.toUSVString(str)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_UTIL_TO_USV_STRING(p: *const u8, l: i64) -> u64 {
    intern(&to_usv_string(&read(p, l)))
}

/// `util.stripVTControlCharacters(str)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_UTIL_STRIP_VT(p: *const u8, l: i64) -> u64 {
    intern(&strip_vt(&read(p, l)))
}

/// `util.getSystemErrorName(err)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_UTIL_GET_SYSTEM_ERROR_NAME(err: i64) -> u64 {
    intern(&error_name(err))
}

/// `util.styleText(format, text)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_UTIL_STYLE_TEXT(
    fp: *const u8,
    fl: i64,
    tp: *const u8,
    tl: i64,
) -> u64 {
    intern(&style_text(&read(fp, fl), &read(tp, tl)))
}
