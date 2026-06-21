//! `engine.str_*` — the PRIVATE String-method bridge of the `engine` namespace.
//!
//! `string` is a PRIMITIVE (native literal syntax `""`), so the VALUE stays a
//! `TAG_STR` PolyValue (a string-pool handle) — only its METHOD library moved to
//! the `.ts` `class String` (`rts-primitives/src/string.ts`). The irreducible
//! Unicode-aware string logic (case folding, trim, UTF-16 code-unit indexing,
//! slice/substring by char, pad, replace) already exists ONCE in Rust
//! (`rts-primitives/src/string/`, the `__RTS_FN_GL_STRING_*` externs). Each
//! `engine.str_*` member below WRAPS the matching impl (calling it, never
//! reimplementing) so the `.ts` method bodies are one source of truth.
//!
//! Receiver + string args are GC string `Handle`s; indices/counts are `I64`.
//! Symbols follow `__RTS_FN_NS_ENGINE_STR_*`. The privacy gate (only prelude-origin
//! code may name the `engine` global) lives at the lowering layer.

use rts_engine::abi::ty::Handle;
use rts_engine::{sig, ModuleBuilder};

use super::func;

// The existing Rust impls we wrap (by symbol — their bodies live in
// `rts-primitives` string::rt, linked into the same runtime).
unsafe extern "C" {
    fn __RTS_FN_GL_STRING_TO_UPPER_CASE(recv: u64) -> u64;
    fn __RTS_FN_GL_STRING_TO_LOWER_CASE(recv: u64) -> u64;
    fn __RTS_FN_GL_STRING_TRIM(recv: u64) -> u64;
    fn __RTS_FN_GL_STRING_TRIM_START(recv: u64) -> u64;
    fn __RTS_FN_GL_STRING_TRIM_END(recv: u64) -> u64;
    fn __RTS_FN_GL_STRING_CHAR_AT(recv: u64, idx: i64) -> u64;
    fn __RTS_FN_GL_STRING_CHAR_CODE_AT(recv: u64, idx: i64) -> i64;
    fn __RTS_FN_GL_STRING_AT(recv: u64, idx: i64) -> u64;
    fn __RTS_FN_GL_STRING_REPEAT(recv: u64, n: i64) -> u64;
    fn __RTS_FN_GL_STRING_SLICE(recv: u64, start: i64, end: i64) -> u64;
    fn __RTS_FN_GL_STRING_SUBSTRING(recv: u64, start: i64, end: i64) -> u64;
    fn __RTS_FN_GL_STRING_INDEX_OF(recv: u64, needle: u64) -> i64;
    fn __RTS_FN_GL_STRING_LAST_INDEX_OF(recv: u64, needle: u64) -> i64;
    fn __RTS_FN_GL_STRING_INCLUDES(recv: u64, needle: u64) -> i64;
    fn __RTS_FN_GL_STRING_STARTS_WITH(recv: u64, prefix: u64) -> i64;
    fn __RTS_FN_GL_STRING_ENDS_WITH(recv: u64, suffix: u64) -> i64;
    fn __RTS_FN_GL_STRING_PAD_START(recv: u64, target_len: i64, pad: u64) -> u64;
    fn __RTS_FN_GL_STRING_PAD_END(recv: u64, target_len: i64, pad: u64) -> u64;
    fn __RTS_FN_GL_STRING_CONCAT(recv: u64, other: u64) -> u64;
    fn __RTS_FN_GL_STRING_REPLACE(recv: u64, from: u64, to: u64) -> u64;
    fn __RTS_FN_GL_STRING_REPLACE_ALL(recv: u64, from: u64, to: u64) -> u64;
    fn __RTS_FN_GL_STRING_NORMALIZE(recv: u64, form_h: u64) -> u64;
}

/// `s.normalize(form?)` — Unicode normalization (NFC/NFD/NFKC/NFKD). Wraps
/// GL_STRING_NORMALIZE; `form_h` is the form string handle (0 ⇒ NFC default).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_STR_NORMALIZE(s: u64, form_h: u64) -> Handle {
    unsafe { __RTS_FN_GL_STRING_NORMALIZE(s, form_h) }
}

/// `s.toUpperCase()`. Wraps GL_STRING_TO_UPPER_CASE.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_STR_TO_UPPER(s: u64) -> Handle {
    unsafe { __RTS_FN_GL_STRING_TO_UPPER_CASE(s) }
}

/// `s.toLowerCase()`. Wraps GL_STRING_TO_LOWER_CASE.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_STR_TO_LOWER(s: u64) -> Handle {
    unsafe { __RTS_FN_GL_STRING_TO_LOWER_CASE(s) }
}

/// `s.trim()`. Wraps GL_STRING_TRIM.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_STR_TRIM(s: u64) -> Handle {
    unsafe { __RTS_FN_GL_STRING_TRIM(s) }
}

/// `s.trimStart()`. Wraps GL_STRING_TRIM_START.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_STR_TRIM_START(s: u64) -> Handle {
    unsafe { __RTS_FN_GL_STRING_TRIM_START(s) }
}

/// `s.trimEnd()`. Wraps GL_STRING_TRIM_END.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_STR_TRIM_END(s: u64) -> Handle {
    unsafe { __RTS_FN_GL_STRING_TRIM_END(s) }
}

/// `s.charAt(i)`. Wraps GL_STRING_CHAR_AT.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_STR_CHAR_AT(s: u64, i: i64) -> Handle {
    unsafe { __RTS_FN_GL_STRING_CHAR_AT(s, i) }
}

/// `s.charCodeAt(i)` — UTF-16 code unit (i64; -1 ⇒ out of range, the JS NaN sentinel).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_STR_CHAR_CODE_AT(s: u64, i: i64) -> i64 {
    unsafe { __RTS_FN_GL_STRING_CHAR_CODE_AT(s, i) }
}

/// `s.at(i)` — char by index (negative counts from end). Wraps GL_STRING_AT.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_STR_AT(s: u64, i: i64) -> Handle {
    unsafe { __RTS_FN_GL_STRING_AT(s, i) }
}

/// `s.repeat(n)`. Wraps GL_STRING_REPEAT.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_STR_REPEAT(s: u64, n: i64) -> Handle {
    unsafe { __RTS_FN_GL_STRING_REPEAT(s, n) }
}

/// `s.slice(start, end)`. Wraps GL_STRING_SLICE (i64::MAX end ⇒ "to end").
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_STR_SLICE(s: u64, start: i64, end: i64) -> Handle {
    unsafe { __RTS_FN_GL_STRING_SLICE(s, start, end) }
}

/// `s.substring(start, end)`. Wraps GL_STRING_SUBSTRING (i64::MAX end ⇒ "to end").
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_STR_SUBSTRING(s: u64, start: i64, end: i64) -> Handle {
    unsafe { __RTS_FN_GL_STRING_SUBSTRING(s, start, end) }
}

/// `s.indexOf(needle)` (i64; -1 ⇒ not found). Wraps GL_STRING_INDEX_OF.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_STR_INDEX_OF(s: u64, needle: u64) -> i64 {
    unsafe { __RTS_FN_GL_STRING_INDEX_OF(s, needle) }
}

/// `s.lastIndexOf(needle)` (i64; -1 ⇒ not found). Wraps GL_STRING_LAST_INDEX_OF.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_STR_LAST_INDEX_OF(s: u64, needle: u64) -> i64 {
    unsafe { __RTS_FN_GL_STRING_LAST_INDEX_OF(s, needle) }
}

/// `s.includes(needle)` (i64 0/1 boolean). Wraps GL_STRING_INCLUDES.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_STR_INCLUDES(s: u64, needle: u64) -> i64 {
    unsafe { __RTS_FN_GL_STRING_INCLUDES(s, needle) }
}

/// `s.startsWith(prefix)` (i64 0/1). Wraps GL_STRING_STARTS_WITH.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_STR_STARTS_WITH(s: u64, prefix: u64) -> i64 {
    unsafe { __RTS_FN_GL_STRING_STARTS_WITH(s, prefix) }
}

/// `s.endsWith(suffix)` (i64 0/1). Wraps GL_STRING_ENDS_WITH.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_STR_ENDS_WITH(s: u64, suffix: u64) -> i64 {
    unsafe { __RTS_FN_GL_STRING_ENDS_WITH(s, suffix) }
}

/// `s.padStart(targetLen, pad)`. Wraps GL_STRING_PAD_START.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_STR_PAD_START(s: u64, target_len: i64, pad: u64) -> Handle {
    unsafe { __RTS_FN_GL_STRING_PAD_START(s, target_len, pad) }
}

/// `s.padEnd(targetLen, pad)`. Wraps GL_STRING_PAD_END.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_STR_PAD_END(s: u64, target_len: i64, pad: u64) -> Handle {
    unsafe { __RTS_FN_GL_STRING_PAD_END(s, target_len, pad) }
}

/// `s.concat(other)` — single-arg form; the `.ts` body folds the rest. Wraps
/// GL_STRING_CONCAT.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_STR_CONCAT(s: u64, other: u64) -> Handle {
    unsafe { __RTS_FN_GL_STRING_CONCAT(s, other) }
}

/// `s.replace(from, to)` — first occurrence (string search). Wraps GL_STRING_REPLACE.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_STR_REPLACE(s: u64, from: u64, to: u64) -> Handle {
    unsafe { __RTS_FN_GL_STRING_REPLACE(s, from, to) }
}

/// `s.replaceAll(from, to)` (string search). Wraps GL_STRING_REPLACE_ALL.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_STR_REPLACE_ALL(s: u64, from: u64, to: u64) -> Handle {
    unsafe { __RTS_FN_GL_STRING_REPLACE_ALL(s, from, to) }
}

/// Add every `engine.str_*` member to the `engine` namespace builder (chained).
/// The receiver `s` + string args are GC string `Handle`s; indices/counts are `I64`.
pub(super) fn register_members(b: ModuleBuilder<'_>) -> ModuleBuilder<'_> {
    b.member(func(
        "str_to_upper",
        "__RTS_FN_NS_ENGINE_STR_TO_UPPER",
        sig!(Handle => Handle),
        "str_to_upper(s: string): string",
        "s.toUpperCase(). Wraps GL_STRING_TO_UPPER_CASE.",
        __RTS_FN_NS_ENGINE_STR_TO_UPPER as *const u8,
    ))
    .member(func(
        "str_to_lower",
        "__RTS_FN_NS_ENGINE_STR_TO_LOWER",
        sig!(Handle => Handle),
        "str_to_lower(s: string): string",
        "s.toLowerCase(). Wraps GL_STRING_TO_LOWER_CASE.",
        __RTS_FN_NS_ENGINE_STR_TO_LOWER as *const u8,
    ))
    .member(func(
        "str_trim",
        "__RTS_FN_NS_ENGINE_STR_TRIM",
        sig!(Handle => Handle),
        "str_trim(s: string): string",
        "s.trim(). Wraps GL_STRING_TRIM.",
        __RTS_FN_NS_ENGINE_STR_TRIM as *const u8,
    ))
    .member(func(
        "str_trim_start",
        "__RTS_FN_NS_ENGINE_STR_TRIM_START",
        sig!(Handle => Handle),
        "str_trim_start(s: string): string",
        "s.trimStart(). Wraps GL_STRING_TRIM_START.",
        __RTS_FN_NS_ENGINE_STR_TRIM_START as *const u8,
    ))
    .member(func(
        "str_trim_end",
        "__RTS_FN_NS_ENGINE_STR_TRIM_END",
        sig!(Handle => Handle),
        "str_trim_end(s: string): string",
        "s.trimEnd(). Wraps GL_STRING_TRIM_END.",
        __RTS_FN_NS_ENGINE_STR_TRIM_END as *const u8,
    ))
    .member(func(
        "str_char_at",
        "__RTS_FN_NS_ENGINE_STR_CHAR_AT",
        sig!(Handle, I64 => Handle),
        "str_char_at(s: string, i: number): string",
        "s.charAt(i). Wraps GL_STRING_CHAR_AT.",
        __RTS_FN_NS_ENGINE_STR_CHAR_AT as *const u8,
    ))
    .member(func(
        "str_char_code_at",
        "__RTS_FN_NS_ENGINE_STR_CHAR_CODE_AT",
        sig!(Handle, I64 => I64),
        "str_char_code_at(s: string, i: number): number",
        "s.charCodeAt(i). Wraps GL_STRING_CHAR_CODE_AT.",
        __RTS_FN_NS_ENGINE_STR_CHAR_CODE_AT as *const u8,
    ))
    .member(func(
        "str_at",
        "__RTS_FN_NS_ENGINE_STR_AT",
        sig!(Handle, I64 => Handle),
        "str_at(s: string, i: number): string",
        "s.at(i). Wraps GL_STRING_AT.",
        __RTS_FN_NS_ENGINE_STR_AT as *const u8,
    ))
    .member(func(
        "str_repeat",
        "__RTS_FN_NS_ENGINE_STR_REPEAT",
        sig!(Handle, I64 => Handle),
        "str_repeat(s: string, n: number): string",
        "s.repeat(n). Wraps GL_STRING_REPEAT.",
        __RTS_FN_NS_ENGINE_STR_REPEAT as *const u8,
    ))
    .member(func(
        "str_slice",
        "__RTS_FN_NS_ENGINE_STR_SLICE",
        sig!(Handle, I64, I64 => Handle),
        "str_slice(s: string, start: number, end: number): string",
        "s.slice(start, end). Wraps GL_STRING_SLICE.",
        __RTS_FN_NS_ENGINE_STR_SLICE as *const u8,
    ))
    .member(func(
        "str_substring",
        "__RTS_FN_NS_ENGINE_STR_SUBSTRING",
        sig!(Handle, I64, I64 => Handle),
        "str_substring(s: string, start: number, end: number): string",
        "s.substring(start, end). Wraps GL_STRING_SUBSTRING.",
        __RTS_FN_NS_ENGINE_STR_SUBSTRING as *const u8,
    ))
    .member(func(
        "str_index_of",
        "__RTS_FN_NS_ENGINE_STR_INDEX_OF",
        sig!(Handle, Handle => I64),
        "str_index_of(s: string, needle: string): number",
        "s.indexOf(needle). Wraps GL_STRING_INDEX_OF.",
        __RTS_FN_NS_ENGINE_STR_INDEX_OF as *const u8,
    ))
    .member(func(
        "str_last_index_of",
        "__RTS_FN_NS_ENGINE_STR_LAST_INDEX_OF",
        sig!(Handle, Handle => I64),
        "str_last_index_of(s: string, needle: string): number",
        "s.lastIndexOf(needle). Wraps GL_STRING_LAST_INDEX_OF.",
        __RTS_FN_NS_ENGINE_STR_LAST_INDEX_OF as *const u8,
    ))
    .member(func(
        "str_includes",
        "__RTS_FN_NS_ENGINE_STR_INCLUDES",
        sig!(Handle, Handle => Bool),
        "str_includes(s: string, needle: string): boolean",
        "s.includes(needle). Wraps GL_STRING_INCLUDES.",
        __RTS_FN_NS_ENGINE_STR_INCLUDES as *const u8,
    ))
    .member(func(
        "str_starts_with",
        "__RTS_FN_NS_ENGINE_STR_STARTS_WITH",
        sig!(Handle, Handle => Bool),
        "str_starts_with(s: string, prefix: string): boolean",
        "s.startsWith(prefix). Wraps GL_STRING_STARTS_WITH.",
        __RTS_FN_NS_ENGINE_STR_STARTS_WITH as *const u8,
    ))
    .member(func(
        "str_ends_with",
        "__RTS_FN_NS_ENGINE_STR_ENDS_WITH",
        sig!(Handle, Handle => Bool),
        "str_ends_with(s: string, suffix: string): boolean",
        "s.endsWith(suffix). Wraps GL_STRING_ENDS_WITH.",
        __RTS_FN_NS_ENGINE_STR_ENDS_WITH as *const u8,
    ))
    .member(func(
        "str_pad_start",
        "__RTS_FN_NS_ENGINE_STR_PAD_START",
        sig!(Handle, I64, Handle => Handle),
        "str_pad_start(s: string, targetLen: number, pad: string): string",
        "s.padStart(targetLen, pad). Wraps GL_STRING_PAD_START.",
        __RTS_FN_NS_ENGINE_STR_PAD_START as *const u8,
    ))
    .member(func(
        "str_pad_end",
        "__RTS_FN_NS_ENGINE_STR_PAD_END",
        sig!(Handle, I64, Handle => Handle),
        "str_pad_end(s: string, targetLen: number, pad: string): string",
        "s.padEnd(targetLen, pad). Wraps GL_STRING_PAD_END.",
        __RTS_FN_NS_ENGINE_STR_PAD_END as *const u8,
    ))
    .member(func(
        "str_concat",
        "__RTS_FN_NS_ENGINE_STR_CONCAT",
        sig!(Handle, Handle => Handle),
        "str_concat(s: string, other: string): string",
        "s.concat(other) (single-arg fold). Wraps GL_STRING_CONCAT.",
        __RTS_FN_NS_ENGINE_STR_CONCAT as *const u8,
    ))
    .member(func(
        "str_replace",
        "__RTS_FN_NS_ENGINE_STR_REPLACE",
        sig!(Handle, Handle, Handle => Handle),
        "str_replace(s: string, from: string, to: string): string",
        "s.replace(from, to). Wraps GL_STRING_REPLACE.",
        __RTS_FN_NS_ENGINE_STR_REPLACE as *const u8,
    ))
    .member(func(
        "str_replace_all",
        "__RTS_FN_NS_ENGINE_STR_REPLACE_ALL",
        sig!(Handle, Handle, Handle => Handle),
        "str_replace_all(s: string, from: string, to: string): string",
        "s.replaceAll(from, to). Wraps GL_STRING_REPLACE_ALL.",
        __RTS_FN_NS_ENGINE_STR_REPLACE_ALL as *const u8,
    ))
    .member(func(
        "str_normalize",
        "__RTS_FN_NS_ENGINE_STR_NORMALIZE",
        sig!(Handle, Handle => Handle),
        "str_normalize(s: string, form: string): string",
        "s.normalize(form). Wraps GL_STRING_NORMALIZE.",
        __RTS_FN_NS_ENGINE_STR_NORMALIZE as *const u8,
    ))
}
