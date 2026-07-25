//! Pure-Rust JS-`String` operations — the ONE source of truth for the primordial
//! `String` value-class ([`super::value_class`]). Every function here computes its
//! result directly with std `str`/`char` + the `unicode-normalization` crate and
//! UTF-16 code-unit iteration; NONE call the legacy `__RTS_FN_GL_STRING_*` externs
//! (those are OBSOLETE and drained separately). Inputs are already-decoded owned
//! Rust `&str` (read from the string pool by the value-class); outputs are owned
//! `String`/scalars the macro marshals back into the pool.
//!
//! The behavior matches the historical `string/rt.rs` externs byte-for-byte
//! (surrogate handling, OOB sentinels, ASCII fast paths) so migrating the surface
//! to Rust is observably a no-op.

use rts_engine::heap::poly::POLY_UNDEFINED;

/// The "to the end of the string" sentinel (a large i32 the slice impls clamp to
/// length) — the default the value-class applies to omitted `end`/`fromIndex`.
pub const STR_END: i64 = 2147483647;

/// UTF-16 code-unit length (JS `.length`) without materializing the units.
pub fn utf16_len(s: &str) -> usize {
    if s.is_ascii() {
        s.len()
    } else {
        s.encode_utf16().count()
    }
}

/// The UTF-16 code unit at `idx`, without materializing the whole string.
///
/// Fast path: an ASCII string maps byte-index == code-unit-index 1:1 (O(1), no
/// alloc); otherwise a UTF-16 walk (`encode_utf16().nth`), still no alloc.
fn utf16_unit_at(s: &str, idx: i64) -> Option<u16> {
    if idx < 0 {
        return None;
    }
    let i = idx as usize;
    let bytes = s.as_bytes();
    if bytes.is_ascii() {
        return bytes.get(i).map(|b| *b as u16);
    }
    s.encode_utf16().nth(i)
}

// ── indexing (UTF-16 code-unit semantics) ───────────────────────────────────────

/// `str.charAt(idx)` — the UTF-16 code unit at `idx` as a one-unit string; a lone
/// surrogate decodes to U+FFFD (matching Bun/Node `console.log`), OOB → `""`.
pub fn char_at(s: &str, idx: i64) -> String {
    if idx < 0 {
        return String::new();
    }
    let Some(unit) = utf16_unit_at(s, idx) else {
        return String::new();
    };
    let mut out = String::new();
    match char::decode_utf16(std::iter::once(unit)).next() {
        Some(Ok(ch)) => out.push(ch),
        _ => out.push('\u{FFFD}'),
    }
    out
}

/// `str.charCodeAt(idx)` — the UTF-16 code unit as a number, NaN out of range.
pub fn char_code_at(s: &str, idx: i64) -> f64 {
    if idx < 0 {
        return f64::NAN;
    }
    match utf16_unit_at(s, idx) {
        Some(u) => u as f64,
        None => f64::NAN,
    }
}

/// `str.codePointAt(idx)` — the full code point starting at code unit `idx`
/// (combining a surrogate pair when present), or `undefined` out of range.
/// Returns a raw `PolyValue` word: an inline-float number or `POLY_UNDEFINED`.
pub fn code_point_at(s: &str, idx: i64) -> u64 {
    let num = |cp: u32| (cp as f64).to_bits();
    if idx < 0 {
        return POLY_UNDEFINED;
    }
    let Some(first) = utf16_unit_at(s, idx) else {
        return POLY_UNDEFINED;
    };
    if (0xD800..=0xDBFF).contains(&first) {
        if let Some(second) = utf16_unit_at(s, idx + 1) {
            if (0xDC00..=0xDFFF).contains(&second) {
                let high = (first as u32 - 0xD800) << 10;
                let low = second as u32 - 0xDC00;
                return num(high + low + 0x10000);
            }
        }
    }
    num(first as u32)
}

/// `str.at(idx)` — the CODE POINT at `idx` (negative counts from the end). OOB
/// yields the literal string `"undefined"` (the historical bridge's sentinel).
pub fn at(s: &str, idx: i64) -> String {
    let count = s.chars().count() as i64;
    let i = if idx < 0 { count + idx } else { idx };
    if i < 0 || i >= count {
        return "undefined".to_string();
    }
    match s.chars().nth(i as usize) {
        Some(ch) => ch.to_string(),
        None => "undefined".to_string(),
    }
}

// ── slicing (UTF-16 code-unit indices, ASCII fast path) ─────────────────────────

/// `str.slice(start, end)` — negatives count from the end.
pub fn slice(s: &str, start: i64, end: i64) -> String {
    let bytes = s.as_bytes();
    if bytes.is_ascii() {
        let count = bytes.len() as i64;
        let norm = |i: i64| -> usize {
            let n = if i < 0 { count + i } else { i };
            n.clamp(0, count) as usize
        };
        let (si, ei) = (norm(start), norm(end));
        if si >= ei {
            return String::new();
        }
        // SAFETY: ASCII — every byte boundary is a char boundary.
        return unsafe { std::str::from_utf8_unchecked(&bytes[si..ei]) }.to_string();
    }
    let units: Vec<u16> = s.encode_utf16().collect();
    let count = units.len() as i64;
    let norm = |i: i64| -> usize {
        let n = if i < 0 { count + i } else { i };
        n.clamp(0, count) as usize
    };
    let (si, ei) = (norm(start), norm(end));
    if si >= ei {
        return String::new();
    }
    String::from_utf16_lossy(&units[si..ei])
}

/// `str.substring(start, end)` — like `slice` but negatives clamp to 0 and a
/// `start > end` pair is swapped.
pub fn substring(s: &str, start: i64, end: i64) -> String {
    let bytes = s.as_bytes();
    if bytes.is_ascii() {
        let count = bytes.len() as i64;
        let clamp = |i: i64| i.clamp(0, count) as usize;
        let (a, b) = (clamp(start), clamp(end));
        let (si, ei) = if a <= b { (a, b) } else { (b, a) };
        if si >= ei {
            return String::new();
        }
        // SAFETY: ASCII — every byte boundary is a char boundary.
        return unsafe { std::str::from_utf8_unchecked(&bytes[si..ei]) }.to_string();
    }
    let units: Vec<u16> = s.encode_utf16().collect();
    let count = units.len() as i64;
    let clamp = |i: i64| i.clamp(0, count) as usize;
    let (a, b) = (clamp(start), clamp(end));
    let (si, ei) = if a <= b { (a, b) } else { (b, a) };
    if si >= ei {
        return String::new();
    }
    String::from_utf16_lossy(&units[si..ei])
}

/// `str.substr(start, length)` — the deprecated start+count form.
pub fn substr(s: &str, start: i64, length: i64) -> String {
    let bytes = s.as_bytes();
    if bytes.is_ascii() {
        let count = bytes.len() as i64;
        let si = (if start < 0 { count + start } else { start }).clamp(0, count) as usize;
        let take = if length < 0 { 0 } else { length as usize };
        let ei = si.saturating_add(take).min(bytes.len());
        if si >= ei {
            return String::new();
        }
        // SAFETY: ASCII — every byte boundary is a char boundary.
        return unsafe { std::str::from_utf8_unchecked(&bytes[si..ei]) }.to_string();
    }
    let units: Vec<u16> = s.encode_utf16().collect();
    let count = units.len() as i64;
    let si = (if start < 0 { count + start } else { start }).clamp(0, count) as usize;
    let take = if length < 0 { 0 } else { length as usize };
    let ei = si.saturating_add(take).min(units.len());
    String::from_utf16_lossy(&units[si..ei])
}

// ── pad (char-count based, cycling the pad string) ──────────────────────────────

/// `str.padStart(target_len, pad)` — pad on the left to `target_len` CODE POINTS.
/// Returns `s` unchanged when already long enough or when `pad` is empty.
pub fn pad_start(s: &str, target_len: i64, pad: &str) -> String {
    let count = s.chars().count() as i64;
    if count >= target_len || pad.is_empty() {
        return s.to_string();
    }
    let needed = (target_len - count) as usize;
    let pad_chars: Vec<char> = pad.chars().collect();
    let prefix: String = pad_chars.iter().cycle().take(needed).collect();
    format!("{prefix}{s}")
}

/// `str.padEnd(target_len, pad)` — pad on the right (see [`pad_start`]).
pub fn pad_end(s: &str, target_len: i64, pad: &str) -> String {
    let count = s.chars().count() as i64;
    if count >= target_len || pad.is_empty() {
        return s.to_string();
    }
    let needed = (target_len - count) as usize;
    let pad_chars: Vec<char> = pad.chars().collect();
    let suffix: String = pad_chars.iter().cycle().take(needed).collect();
    format!("{s}{suffix}")
}

// ── normalize / well-formed ─────────────────────────────────────────────────────

/// `str.normalize(form)` — Unicode NFC/NFD/NFKC/NFKD (default NFC).
pub fn normalize(s: &str, form: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    match form {
        "NFD" => s.nfd().collect::<String>(),
        "NFKC" => s.nfkc().collect::<String>(),
        "NFKD" => s.nfkd().collect::<String>(),
        _ => s.nfc().collect::<String>(), // NFC default
    }
}

/// `str.isWellFormed()` — true iff the raw (WTF-8) bytes carry no lone surrogate
/// (`0xED` followed by `0xA0..=0xBF`). Valid surrogate pairs never appear as WTF-8.
pub fn is_well_formed(bytes: &[u8]) -> bool {
    let mut i = 0usize;
    while i < bytes.len() {
        if i + 3 <= bytes.len() && bytes[i] == 0xED && (0xA0..=0xBF).contains(&bytes[i + 1]) {
            return false;
        }
        i += 1;
    }
    true
}

/// `str.toWellFormed()` — every lone surrogate replaced by U+FFFD (identity for
/// valid UTF-8 input). Returns the rebuilt byte buffer.
pub fn to_well_formed(bytes: &[u8]) -> Vec<u8> {
    let replacement = [0xEFu8, 0xBF, 0xBD]; // U+FFFD
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if i + 3 <= bytes.len() && bytes[i] == 0xED && (0xA0..=0xBF).contains(&bytes[i + 1]) {
            out.extend_from_slice(&replacement);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

// ── locale compare + statics ────────────────────────────────────────────────────

/// `a.localeCompare(b)` — approximate ICU/CLDR order: compare case-insensitively,
/// and on a case-insensitive tie put lowercase BEFORE uppercase (opposite of raw
/// ASCII) to match Bun/Node (`"A".localeCompare("a") === 1`).
pub fn locale_compare(a: &str, b: &str) -> i64 {
    use std::cmp::Ordering;
    let (a_lo, b_lo) = (a.to_lowercase(), b.to_lowercase());
    match a_lo.cmp(&b_lo) {
        Ordering::Less => -1,
        Ordering::Greater => 1,
        Ordering::Equal => match b.cmp(a) {
            Ordering::Less => -1,
            Ordering::Equal => 0,
            Ordering::Greater => 1,
        },
    }
}

/// `String.fromCharCode(code)` — char from a UTF-16 code unit (truncated to u16).
/// A lone surrogate (0xD800..=0xDFFF) yields `""` (matching the default encoding's
/// lossy output in Bun/Node).
pub fn from_char_code(code: i64) -> String {
    let unit = (code as i32 as u32) & 0xFFFF;
    if (0xD800..=0xDFFF).contains(&unit) {
        return String::new();
    }
    let ch = char::from_u32(unit).unwrap_or(char::REPLACEMENT_CHARACTER);
    ch.to_string()
}

/// `String.fromCodePoint(codePoint)` — char from a full Unicode code point.
pub fn from_code_point(code: i64) -> String {
    let ch = char::from_u32(code as u32).unwrap_or(char::REPLACEMENT_CHARACTER);
    ch.to_string()
}
