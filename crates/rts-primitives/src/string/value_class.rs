//! `String` — the REAL primordial `String` for the new engine, authored as a
//! FULLY SELF-CONTAINED pure-Rust value-class with `#[rtse::class("String",
//! value)]`. The `rts-macro` generates EVERY ABI symbol; each method BODY computes
//! its result directly in Rust (via [`super::strops`]) — there is NO delegation to
//! the legacy `__RTS_FN_GL_STRING_*` externs and NO hand-written `.member(...)`
//! builder. This REPLACES the ambient `.ts` `class String` (`string.ts`): same JS
//! surface, same JS-semantic composition, but resolved through the Registry.
//!
//! ## Dual receiver (primitive vs wrapper), unified by [`str_handle`]
//! A method reaches EITHER receiver (the engine is shape-based, not prototypes):
//!   - `"abc".toUpperCase()` — the PRIMITIVE is autoboxed as the receiver word (a
//!     `TAG_STR` PolyValue). [`str_handle`] normalizes it to the string handle.
//!   - `new String("abc").toUpperCase()` — the receiver is the WRAPPER object (an
//!     `Entry::Rtse` classed `"String"`); [`str_handle`] reads its `prim` slot.
//! Every instance method takes the receiver as its FIRST typed `Poly` param (the
//! value-class ABI): the raw NaN-boxed word, unboxed in-body by [`str_handle`].
//!
//! ## Self-contained — only the value-model ToString trampoline is external
//! The ONLY non-`strops` external symbol is `__rtsadp_to_string` (the engine's own
//! JS `ToString`), used by the ctor/`concat` to coerce an arbitrary value — the
//! value-model coercion, allowed. String reads/allocs go through the engine heap
//! (`read_string_handle`/`alloc_entry`), never the legacy string externs.
//!
//! ## Coverage
//! Everything in `string.ts` PLUS the engine-direct cases (`slice`/`substring`/
//! `substr`/`codePointAt`/`localeCompare`) so codegen can drain its hardcoded
//! String dispatch. EXCLUDED (regex-argument, kept in `regex`/`dispatch`): the
//! `match`/`matchAll`/`search`/`split` family whose first arg is a RegExp.

use rts_engine::abi::ty::Poly;
use rts_engine::heap::handles::{Entry, alloc_entry, read_string_handle, rtse_class_of, with_entry, with_rtse};
use rts_engine::heap::poly::{POLY_UNDEFINED, poly_handle_normalize};

use super::strops::{self, STR_END};

// JS `ToString` trampoline — defined in `rts-adapters` (above this crate), reached
// via a forward `extern "C"` decl (layering-safe: rts-primitives is an rlib, the
// symbol resolves at the final link). The value-model coercion the ctor/concat use.
unsafe extern "C" {
    fn __rtsadp_to_string(word: u64) -> u64;
}

/// Backs the wrapper object form `new String(x)` (typeof `"object"`): `prim` is the
/// wrapped primitive string HANDLE. A PRIMITIVE string is NEVER a `StringWrapper` —
/// it stays a `TAG_STR` PolyValue and reaches the methods through the autobox path.
/// Not exposed as a JS field (no `#[rtse::variable]`), so `prim` stays internal.
#[rtse::class("String", value)]
#[derive(Clone)]
pub struct StringWrapper {
    prim: u64,
}

// ── Receiver unboxing + small helpers (plain fns, untouched by the macro) ───────

/// The real string handle behind a receiver word: a wrapper (`new String`) reads
/// its `prim` slot; an autoboxed primitive normalizes its `TAG_STR` word; a bare
/// handle passes through.
fn str_handle(recv: u64) -> u64 {
    let h = poly_handle_normalize(recv).unwrap_or(recv);
    if rtse_class_of(h) == Some("String") {
        return with_rtse::<StringWrapper, _>(h, |w| w.map(|w| w.prim).unwrap_or(0));
    }
    h
}

/// The receiver as an owned Rust `String` (unifying both receiver forms).
fn str_val(recv: u64) -> String {
    read_string_handle(str_handle(recv)).unwrap_or_default()
}

/// The raw stored bytes behind a receiver (may be WTF-8 with lone surrogates —
/// needed by `isWellFormed`/`toWellFormed`, which must see the un-decoded bytes).
/// Unwraps a legacy `StringBox` before reading the `Entry::String` payload.
fn raw_bytes(recv: u64) -> Vec<u8> {
    let h = str_handle(recv);
    let unwrapped = with_entry(h, |e| match e {
        Some(Entry::StringBox(inner)) => *inner,
        _ => h,
    });
    with_entry(unwrapped, |e| match e {
        Some(Entry::String(b)) => b.clone(),
        _ => Vec::new(),
    })
}

/// `ToString(word)` as a real string HANDLE (via the engine trampoline).
fn to_string_handle(word: u64) -> u64 {
    let sw = unsafe { __rtsadp_to_string(word) };
    poly_handle_normalize(sw).unwrap_or(sw)
}

/// `ToString(word)` as an owned Rust `String` (the `.ts`'s `"" + x`).
fn js_to_string(word: u64) -> String {
    read_string_handle(to_string_handle(word)).unwrap_or_default()
}

/// The string a `Poly` arg denotes (`""` when the arg was omitted → `undefined`).
fn poly_str_or_empty(word: u64) -> String {
    if word == POLY_UNDEFINED {
        String::new()
    } else {
        js_to_string(word)
    }
}

/// A numeric arg with its `string.ts` default applied when omitted (the engine
/// injects `undefined`, which reads as NaN in an F64 ABI slot).
#[inline]
fn num_or(v: f64, default: i64) -> i64 {
    if v.is_nan() { default } else { v as i64 }
}

#[rtse::class("String", value)]
impl StringWrapper {
    /// `new String(value?)` — the wrapper OBJECT. `new String()` (or an explicit
    /// `undefined`, indistinguishable to the engine) → `""`; else `ToString(value)`.
    #[rtse::ctor(optional = 1)]
    fn new(value: Poly) -> Self {
        let prim = if value == POLY_UNDEFINED {
            alloc_entry(Entry::String(Vec::new()))
        } else {
            to_string_handle(value)
        };
        StringWrapper { prim }
    }

    /// `str.length` — UTF-16 code units. WRAPPER form only (a PRIMITIVE string's
    /// `.length` is read engine-direct and never routes here).
    #[rtse::getter]
    fn length(&self) -> f64 {
        let s = read_string_handle(self.prim).unwrap_or_default();
        strops::utf16_len(&s) as f64
    }

    /// `str.toString()` — the primitive string itself.
    #[rtse::method]
    fn to_string(recv: Poly) -> String {
        str_val(recv)
    }

    /// `str.valueOf()` — the primitive string itself.
    #[rtse::method]
    fn value_of(recv: Poly) -> String {
        str_val(recv)
    }

    // ── case ───────────────────────────────────────────────────────────────────

    /// `str.toUpperCase()`.
    #[rtse::method]
    fn to_upper_case(recv: Poly) -> String {
        str_val(recv).to_uppercase()
    }

    /// `str.toLowerCase()`.
    #[rtse::method]
    fn to_lower_case(recv: Poly) -> String {
        str_val(recv).to_lowercase()
    }

    /// `str.toLocaleUpperCase()` — no locale data; folds to `toUpperCase`.
    #[rtse::method]
    fn to_locale_upper_case(recv: Poly) -> String {
        str_val(recv).to_uppercase()
    }

    /// `str.toLocaleLowerCase()` — no locale data; folds to `toLowerCase`.
    #[rtse::method]
    fn to_locale_lower_case(recv: Poly) -> String {
        str_val(recv).to_lowercase()
    }

    // ── trim ───────────────────────────────────────────────────────────────────

    /// `str.trim()`.
    #[rtse::method]
    fn trim(recv: Poly) -> String {
        str_val(recv).trim().to_string()
    }

    /// `str.trimStart()`.
    #[rtse::method]
    fn trim_start(recv: Poly) -> String {
        str_val(recv).trim_start().to_string()
    }

    /// `str.trimEnd()`.
    #[rtse::method]
    fn trim_end(recv: Poly) -> String {
        str_val(recv).trim_end().to_string()
    }

    /// `str.trimLeft()` — legacy alias of `trimStart`.
    #[rtse::method]
    fn trim_left(recv: Poly) -> String {
        str_val(recv).trim_start().to_string()
    }

    /// `str.trimRight()` — legacy alias of `trimEnd`.
    #[rtse::method]
    fn trim_right(recv: Poly) -> String {
        str_val(recv).trim_end().to_string()
    }

    // ── indexing (UTF-16 semantics, computed in `strops`) ───────────────────────

    /// `str.charAt(idx)` — UTF-16 code unit at index.
    #[rtse::method]
    fn char_at(recv: Poly, idx: f64) -> String {
        strops::char_at(&str_val(recv), idx as i64)
    }

    /// `str.charCodeAt(idx)` — UTF-16 code unit as a number (NaN out of range).
    #[rtse::method]
    fn char_code_at(recv: Poly, idx: f64) -> f64 {
        strops::char_code_at(&str_val(recv), idx as i64)
    }

    /// `str.codePointAt(idx)` — full code point at `idx` (surrogate pairs combined),
    /// or `undefined` out of range. Returns the raw `PolyValue` word.
    #[rtse::method]
    fn code_point_at(recv: Poly, idx: f64) -> Poly {
        strops::code_point_at(&str_val(recv), idx as i64)
    }

    /// `str.at(idx)` — char at index, negative counts from the end.
    #[rtse::method]
    fn at(recv: Poly, idx: f64) -> String {
        strops::at(&str_val(recv), idx as i64)
    }

    /// `str.repeat(n)`.
    #[rtse::method]
    fn repeat(recv: Poly, n: f64) -> String {
        let n = n as i64;
        if n <= 0 {
            return String::new();
        }
        str_val(recv).repeat(n as usize)
    }

    // ── slicing ─────────────────────────────────────────────────────────────────

    /// `str.slice(start, end?)` — negatives count from the end.
    #[rtse::method(optional = 1)]
    fn slice(recv: Poly, start: f64, end: f64) -> String {
        strops::slice(&str_val(recv), start as i64, num_or(end, STR_END))
    }

    /// `str.substring(start, end?)` — like `slice` but clamps negatives to 0.
    #[rtse::method(optional = 1)]
    fn substring(recv: Poly, start: f64, end: f64) -> String {
        strops::substring(&str_val(recv), start as i64, num_or(end, STR_END))
    }

    /// `str.substr(start, length?)` — deprecated start+count form.
    #[rtse::method(optional = 1)]
    fn substr(recv: Poly, start: f64, length: f64) -> String {
        strops::substr(&str_val(recv), start as i64, num_or(length, STR_END))
    }

    // ── search (position applied by composing with the `strops` slice) ──────────

    /// `str.indexOf(needle, position?)` — first occurrence, or -1.
    #[rtse::method(optional = 1)]
    fn index_of(recv: Poly, needle: &str, position: f64) -> f64 {
        let position = num_or(position, 0);
        let s = str_val(recv);
        if needle.is_empty() {
            let len = strops::utf16_len(&s) as i64;
            if position < 0 {
                return 0.0;
            }
            return position.min(len) as f64;
        }
        if position > 0 {
            let tail = strops::slice(&s, position, i64::MAX);
            return match tail.find(needle) {
                Some(i) => i as f64 + position as f64,
                None => -1.0,
            };
        }
        match s.find(needle) {
            Some(i) => i as f64,
            None => -1.0,
        }
    }

    /// `str.lastIndexOf(needle, fromIndex?)` — last match starting at ≤ fromIndex.
    #[rtse::method(optional = 1)]
    fn last_index_of(recv: Poly, needle: &str, from_index: f64) -> f64 {
        let from_index = num_or(from_index, STR_END);
        let s = str_val(recv);
        if from_index < STR_END {
            let end = from_index + strops::utf16_len(needle) as i64;
            let prefix = strops::slice(&s, 0, end);
            return match prefix.rfind(needle) {
                Some(i) => i as f64,
                None => -1.0,
            };
        }
        match s.rfind(needle) {
            Some(i) => i as f64,
            None => -1.0,
        }
    }

    /// `str.includes(needle, position?)`.
    #[rtse::method(optional = 1)]
    fn includes(recv: Poly, needle: &str, position: f64) -> bool {
        let position = num_or(position, 0);
        let s = str_val(recv);
        if position > 0 {
            return strops::slice(&s, position, i64::MAX).contains(needle);
        }
        s.contains(needle)
    }

    /// `str.startsWith(prefix, position?)`.
    #[rtse::method(optional = 1)]
    fn starts_with(recv: Poly, prefix: &str, position: f64) -> bool {
        let position = num_or(position, 0);
        let s = str_val(recv);
        if position > 0 {
            return strops::slice(&s, position, i64::MAX).starts_with(prefix);
        }
        s.starts_with(prefix)
    }

    /// `str.endsWith(suffix, endPosition?)`.
    #[rtse::method(optional = 1)]
    fn ends_with(recv: Poly, suffix: &str, end_position: f64) -> bool {
        let end_position = num_or(end_position, STR_END);
        let s = str_val(recv);
        if end_position < STR_END {
            return strops::slice(&s, 0, end_position).ends_with(suffix);
        }
        s.ends_with(suffix)
    }

    // ── pad ──────────────────────────────────────────────────────────────────────

    /// `str.padStart(targetLength, padString?)`.
    #[rtse::method(optional = 1)]
    fn pad_start(recv: Poly, target_length: f64, pad: Poly) -> String {
        let pad = if pad == POLY_UNDEFINED { " ".to_string() } else { js_to_string(pad) };
        strops::pad_start(&str_val(recv), target_length as i64, &pad)
    }

    /// `str.padEnd(targetLength, padString?)`.
    #[rtse::method(optional = 1)]
    fn pad_end(recv: Poly, target_length: f64, pad: Poly) -> String {
        let pad = if pad == POLY_UNDEFINED { " ".to_string() } else { js_to_string(pad) };
        strops::pad_end(&str_val(recv), target_length as i64, &pad)
    }

    /// `str.concat(...args)` — folds up to four trailing args, each coerced through
    /// JS `ToString` (`"" + arg`); an omitted slot is the identity.
    #[rtse::method(optional = 4)]
    fn concat(recv: Poly, a: Poly, b: Poly, c: Poly, d: Poly) -> String {
        let mut r = str_val(recv);
        for w in [a, b, c, d] {
            r.push_str(&poly_str_or_empty(w));
        }
        r
    }

    // ── replace (STRING search only; a regex first arg is handled earlier) ──────

    /// `str.replace(from, to)` — replaces the FIRST occurrence of `from`.
    #[rtse::method]
    fn replace(recv: Poly, from: &str, to: &str) -> String {
        str_val(recv).replacen(from, to, 1)
    }

    /// `str.replaceAll(from, to)` — replaces EVERY occurrence of `from`.
    #[rtse::method]
    fn replace_all(recv: Poly, from: &str, to: &str) -> String {
        str_val(recv).replace(from, to)
    }

    /// `str.localeCompare(other)` — -1/0/1 (approximate ICU/CLDR order).
    #[rtse::method]
    fn locale_compare(recv: Poly, other: &str) -> f64 {
        strops::locale_compare(&str_val(recv), other) as f64
    }

    // ── normalization / well-formed ─────────────────────────────────────────────

    /// `str.normalize(form?)` — Unicode NFC/NFD/NFKC/NFKD (default NFC).
    #[rtse::method(optional = 1)]
    fn normalize(recv: Poly, form: Poly) -> String {
        let form = if form == POLY_UNDEFINED { "NFC".to_string() } else { js_to_string(form) };
        strops::normalize(&str_val(recv), &form)
    }

    /// `str.isWellFormed()` — no lone surrogates.
    #[rtse::method]
    fn is_well_formed(recv: Poly) -> bool {
        strops::is_well_formed(&raw_bytes(recv))
    }

    /// `str.toWellFormed()` — lone surrogates replaced by U+FFFD.
    #[rtse::method]
    fn to_well_formed(recv: Poly) -> String {
        String::from_utf8_lossy(&strops::to_well_formed(&raw_bytes(recv))).into_owned()
    }

    // ── statics ─────────────────────────────────────────────────────────────────

    /// `String.fromCharCode(code)` — char from a UTF-16 code unit.
    #[rtse::statical]
    fn from_char_code(code: f64) -> String {
        strops::from_char_code(code as i64)
    }

    /// `String.fromCodePoint(codePoint)` — char from a full Unicode code point.
    #[rtse::statical]
    fn from_code_point(code: f64) -> String {
        strops::from_code_point(code as i64)
    }
}
