//! String — namespace ABI + GlobalClassSpec para o tipo primitivo JS String.
//! Migrado ao modelo `#[rts_namespace]` + `#[rts_class]` (stage 2c) via membros
//! `external`: os externs `__RTS_FN_NS_STRING_*` (namespace) e
//! `__RTS_FN_GL_STRING_*` (classe JS) ficam em search/transform/replace/split.rs
//! + rt.rs intactos; os macros derivam só o `SPEC` + o `STRING_CLASS_SPEC`.
//!
//! Metodos de namespace (rts:string) em search/transform/replace/split.
//! Metodos de instancia JS (str.slice, str.split, etc.) em rt.rs.

pub mod replace;
pub mod rt;
pub mod search;
pub mod split;
pub mod transform;

#[allow(unused_imports)]
use rts_engine::abi::ty::{Bool, Handle, Str, I64};
use rts_macro::{rts_class, rts_namespace};

/// Rich string operations beyond the basic gc pool.
#[rts_namespace(string, sym = "NS_STRING")]
impl StringNs {
    /// True when `haystack` contains `needle`.
    #[rts_fn(
        external,
        ts = "contains(haystack: string, needle: string): boolean",
        pure
    )]
    pub fn contains(_haystack: Str, _needle: Str) -> Bool {
        unreachable!()
    }
    /// True when `s` starts with `prefix`.
    #[rts_fn(external, ts = "starts_with(s: string, prefix: string): boolean", pure)]
    pub fn starts_with(_s: Str, _prefix: Str) -> Bool {
        unreachable!()
    }
    /// True when `s` ends with `suffix`.
    #[rts_fn(external, ts = "ends_with(s: string, suffix: string): boolean", pure)]
    pub fn ends_with(_s: Str, _suffix: Str) -> Bool {
        unreachable!()
    }
    /// Byte index of first occurrence of `needle`, or -1 when absent.
    #[rts_fn(external, ts = "find(s: string, needle: string): number", pure)]
    pub fn find(_s: Str, _needle: Str) -> I64 {
        unreachable!()
    }
    /// Uppercase copy (Unicode-aware).
    #[rts_fn(external, ts = "to_upper(s: string): string", pure)]
    pub fn to_upper(_s: Str) -> Handle {
        unreachable!()
    }
    /// Lowercase copy (Unicode-aware).
    #[rts_fn(external, ts = "to_lower(s: string): string", pure)]
    pub fn to_lower(_s: Str) -> Handle {
        unreachable!()
    }
    /// Removes ASCII + Unicode whitespace from both ends.
    #[rts_fn(external, ts = "trim(s: string): string", pure)]
    pub fn trim(_s: Str) -> Handle {
        unreachable!()
    }
    /// Removes whitespace from the start.
    #[rts_fn(external, ts = "trim_start(s: string): string", pure)]
    pub fn trim_start(_s: Str) -> Handle {
        unreachable!()
    }
    /// Removes whitespace from the end.
    #[rts_fn(external, ts = "trim_end(s: string): string", pure)]
    pub fn trim_end(_s: Str) -> Handle {
        unreachable!()
    }
    /// Concatenates `s` with itself `n` times.
    #[rts_fn(external, ts = "repeat(s: string, n: number): string", pure)]
    pub fn repeat(_s: Str, _n: I64) -> Handle {
        unreachable!()
    }
    /// Replaces every occurrence of `from` with `to`.
    #[rts_fn(
        external,
        ts = "replace(s: string, from: string, to: string): string",
        pure
    )]
    pub fn replace(_s: Str, _from: Str, _to: Str) -> Handle {
        unreachable!()
    }
    /// Replaces the first `n` occurrences of `from` with `to`.
    #[rts_fn(
        external,
        ts = "replacen(s: string, from: string, to: string, n: number): string",
        pure
    )]
    pub fn replacen(_s: Str, _from: Str, _to: Str, _n: I64) -> Handle {
        unreachable!()
    }
    /// Unicode codepoint count (chars).
    #[rts_fn(external, ts = "char_count(s: string): number", pure)]
    pub fn char_count(_s: Str) -> I64 {
        unreachable!()
    }
    /// Length in UTF-8 bytes.
    #[rts_fn(external, ts = "byte_len(s: string): number", pure)]
    pub fn byte_len(_s: Str) -> I64 {
        unreachable!()
    }
    /// Character at Unicode index `idx` as a single-char string handle, or 0 out of range.
    #[rts_fn(external, ts = "char_at(s: string, idx: number): string", pure)]
    pub fn char_at(_s: Str, _idx: I64) -> Handle {
        unreachable!()
    }
    /// Unicode code point at `idx`, or -1 out of range.
    #[rts_fn(external, ts = "char_code_at(s: string, idx: number): number", pure)]
    pub fn char_code_at(_s: Str, _idx: I64) -> I64 {
        unreachable!()
    }
}

/// JS String primitive class — new String(), String.fromCharCode(), str.slice() etc.
#[rts_class(String, prefix = "STRING", spec = "STRING_CLASS_SPEC")]
impl StringClass {
    /// new String(value) — wraps value como StringBox (typeof returns 'object').
    #[rts_ctor(external, ts = "new(value: string): string", pure)]
    pub fn new_boxed(_value: Handle) -> Handle {
        unreachable!()
    }
    /// new String() — empty string.
    #[rts_ctor(external, ts = "new(): string", pure)]
    pub fn new_empty() -> Handle {
        unreachable!()
    }
    /// String.fromCharCode(code) — char from UTF-16 code unit.
    #[rts_smethod(
        external,
        name = "fromCharCode",
        ts = "fromCharCode(code: number): string",
        pure
    )]
    pub fn from_char_code(_code: I64) -> Handle {
        unreachable!()
    }
    /// String.fromCodePoint(codePoint) — char from full Unicode code point.
    #[rts_smethod(
        external,
        name = "fromCodePoint",
        ts = "fromCodePoint(codePoint: number): string",
        pure
    )]
    pub fn from_code_point(_code_point: I64) -> Handle {
        unreachable!()
    }
    /// str.length — number of UTF-16 code units (JS spec).
    #[rts_getter(
        external,
        name = "length",
        symbol = "__RTS_FN_GL_STRING_LENGTH_UTF16",
        ts = "length: number",
        pure
    )]
    pub fn length(_recv: Handle) -> I64 {
        unreachable!()
    }
    /// str.indexOf(needle) — first occurrence index, or -1.
    #[rts_method(
        external,
        name = "indexOf",
        ts = "indexOf(needle: string): number",
        pure
    )]
    pub fn index_of(_recv: Handle, _needle: Handle) -> I64 {
        unreachable!()
    }
    /// str.lastIndexOf(needle) — last occurrence index, or -1.
    #[rts_method(
        external,
        name = "lastIndexOf",
        ts = "lastIndexOf(needle: string): number",
        pure
    )]
    pub fn last_index_of(_recv: Handle, _needle: Handle) -> I64 {
        unreachable!()
    }
    /// str.includes(needle) — true when needle is found.
    #[rts_method(
        external,
        name = "includes",
        ts = "includes(needle: string): boolean",
        pure
    )]
    pub fn includes(_recv: Handle, _needle: Handle) -> Bool {
        unreachable!()
    }
    /// str.startsWith(prefix).
    #[rts_method(
        external,
        name = "startsWith",
        ts = "startsWith(prefix: string): boolean",
        pure
    )]
    pub fn starts_with(_recv: Handle, _prefix: Handle) -> Bool {
        unreachable!()
    }
    /// str.endsWith(suffix).
    #[rts_method(
        external,
        name = "endsWith",
        ts = "endsWith(suffix: string): boolean",
        pure
    )]
    pub fn ends_with(_recv: Handle, _suffix: Handle) -> Bool {
        unreachable!()
    }
    /// str.charAt(idx).
    #[rts_method(external, name = "charAt", ts = "charAt(idx: number): string", pure)]
    pub fn char_at(_recv: Handle, _idx: I64) -> Handle {
        unreachable!()
    }
    /// str.charCodeAt(idx) — UTF-16 code unit at index.
    #[rts_method(
        external,
        name = "charCodeAt",
        ts = "charCodeAt(idx: number): number",
        pure
    )]
    pub fn char_code_at(_recv: Handle, _idx: I64) -> I64 {
        unreachable!()
    }
    /// str.codePointAt(idx) — full Unicode code point at index.
    #[rts_method(
        external,
        name = "codePointAt",
        ts = "codePointAt(idx: number): number",
        pure
    )]
    pub fn code_point_at(_recv: Handle, _idx: I64) -> I64 {
        unreachable!()
    }
    /// str.at(idx) — char at index (negative counts from end).
    #[rts_method(external, name = "at", ts = "at(idx: number): string", pure)]
    pub fn at(_recv: Handle, _idx: I64) -> Handle {
        unreachable!()
    }
    /// str.slice(start, end) — negatives from end.
    #[rts_method(
        external,
        name = "slice",
        ts = "slice(start: number, end?: number): string",
        pure
    )]
    pub fn slice(_recv: Handle, _start: I64, _end: I64) -> Handle {
        unreachable!()
    }
    /// str.substring(start, end) — like slice but clamps negatives to 0.
    #[rts_method(
        external,
        name = "substring",
        ts = "substring(start: number, end?: number): string",
        pure
    )]
    pub fn substring(_recv: Handle, _start: I64, _end: I64) -> Handle {
        unreachable!()
    }
    /// str.substr(start, length) — deprecated start+count form.
    #[rts_method(
        external,
        name = "substr",
        ts = "substr(start: number, length?: number): string",
        pure
    )]
    pub fn substr(_recv: Handle, _start: I64, _length: I64) -> Handle {
        unreachable!()
    }
    /// str.toUpperCase().
    #[rts_method(external, name = "toUpperCase", ts = "toUpperCase(): string", pure)]
    pub fn to_upper_case(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// str.toLocaleUpperCase() — alias for toUpperCase.
    #[rts_method(
        external,
        name = "toLocaleUpperCase",
        symbol = "__RTS_FN_GL_STRING_TO_UPPER_CASE",
        ts = "toLocaleUpperCase(): string",
        pure
    )]
    pub fn to_locale_upper_case(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// str.toLowerCase().
    #[rts_method(external, name = "toLowerCase", ts = "toLowerCase(): string", pure)]
    pub fn to_lower_case(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// str.toLocaleLowerCase() — alias for toLowerCase.
    #[rts_method(
        external,
        name = "toLocaleLowerCase",
        symbol = "__RTS_FN_GL_STRING_TO_LOWER_CASE",
        ts = "toLocaleLowerCase(): string",
        pure
    )]
    pub fn to_locale_lower_case(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// str.trim().
    #[rts_method(external, name = "trim", ts = "trim(): string", pure)]
    pub fn trim(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// str.trimStart().
    #[rts_method(external, name = "trimStart", ts = "trimStart(): string", pure)]
    pub fn trim_start(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// str.trimLeft() — alias for trimStart.
    #[rts_method(
        external,
        name = "trimLeft",
        symbol = "__RTS_FN_GL_STRING_TRIM_START",
        ts = "trimLeft(): string",
        pure
    )]
    pub fn trim_left(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// str.trimEnd().
    #[rts_method(external, name = "trimEnd", ts = "trimEnd(): string", pure)]
    pub fn trim_end(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// str.trimRight() — alias for trimEnd.
    #[rts_method(
        external,
        name = "trimRight",
        symbol = "__RTS_FN_GL_STRING_TRIM_END",
        ts = "trimRight(): string",
        pure
    )]
    pub fn trim_right(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// str.repeat(n).
    #[rts_method(external, name = "repeat", ts = "repeat(n: number): string", pure)]
    pub fn repeat(_recv: Handle, _n: I64) -> Handle {
        unreachable!()
    }
    /// str.replace(from, to) — replace first occurrence.
    #[rts_method(
        external,
        name = "replace",
        ts = "replace(from: string, to: string): string",
        pure
    )]
    pub fn replace(_recv: Handle, _from: Handle, _to: Handle) -> Handle {
        unreachable!()
    }
    /// str.replaceAll(from, to) — replace all occurrences.
    #[rts_method(
        external,
        name = "replaceAll",
        ts = "replaceAll(from: string, to: string): string",
        pure
    )]
    pub fn replace_all(_recv: Handle, _from: Handle, _to: Handle) -> Handle {
        unreachable!()
    }
    /// str.concat(other) — concatenate two strings.
    #[rts_method(external, name = "concat", ts = "concat(other: string): string", pure)]
    pub fn concat(_recv: Handle, _other: Handle) -> Handle {
        unreachable!()
    }
    /// str.padStart(targetLength, padString).
    #[rts_method(
        external,
        name = "padStart",
        ts = "padStart(targetLength: number, padString?: string): string",
        pure
    )]
    pub fn pad_start(_recv: Handle, _target_length: I64, _pad_string: Handle) -> Handle {
        unreachable!()
    }
    /// str.padEnd(targetLength, padString).
    #[rts_method(
        external,
        name = "padEnd",
        ts = "padEnd(targetLength: number, padString?: string): string",
        pure
    )]
    pub fn pad_end(_recv: Handle, _target_length: I64, _pad_string: Handle) -> Handle {
        unreachable!()
    }
    /// str.split(sep) — Vec handle of string handles.
    #[rts_method(external, name = "split", ts = "split(sep: string): string[]", pure)]
    pub fn split(_recv: Handle, _sep: Handle) -> Handle {
        unreachable!()
    }
    /// str.localeCompare(other) — -1 / 0 / 1.
    #[rts_method(
        external,
        name = "localeCompare",
        ts = "localeCompare(other: string): number",
        pure
    )]
    pub fn locale_compare(_recv: Handle, _other: Handle) -> I64 {
        unreachable!()
    }
    /// str.toString() — identity.
    #[rts_method(external, name = "toString", ts = "toString(): string", pure)]
    pub fn to_string(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// str.valueOf() — identity para string primitive, unwrap para StringBox.
    #[rts_method(
        external,
        name = "valueOf",
        symbol = "__RTS_FN_GL_STRING_BOX_VALUE_OF",
        ts = "valueOf(): string",
        pure
    )]
    pub fn value_of(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// str.isWellFormed() — always true for RTS UTF-8 strings.
    #[rts_method(external, name = "isWellFormed", ts = "isWellFormed(): boolean", pure)]
    pub fn is_well_formed(_recv: Handle) -> Bool {
        unreachable!()
    }
    /// str.toWellFormed() — substitui lone surrogates por U+FFFD.
    #[rts_method(external, name = "toWellFormed", ts = "toWellFormed(): string", pure)]
    pub fn to_well_formed(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// str.normalize() — stub: identity (full NFC/NFD is a follow-up).
    #[rts_method(
        external,
        name = "normalize",
        symbol = "__RTS_FN_GL_STRING_TO_STRING",
        ts = "normalize(form?: string): string",
        pure
    )]
    pub fn normalize(_recv: Handle) -> Handle {
        unreachable!()
    }
}
