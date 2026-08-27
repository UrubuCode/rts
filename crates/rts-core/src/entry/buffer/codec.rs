//! The six binary/text codecs `Buffer` needs, moved here from `rts-node`.
//!
//! # Why moved rather than copied
//!
//! A `Buffer` instance method — `toString`, `write`, `fill` with a string
//! pattern — needs exactly these codecs, and `rts-node`'s copy was written
//! for the static-only surface this module replaces. Two decoders answering one
//! name is the duplication this crate's rule 2 refuses; keeping one here and
//! reaching it from `node:buffer` (which now has no bytes of its own to decode)
//! is the fix.
//!
//! Correctness is pinned by
//! `crates/rts-host/tests/node_modules.rs`'s
//! `buffer_codecs_are_correct_on_the_cases_that_catch_a_naive_one`, unchanged by
//! the move.

/// An encoding name, normalized to this module's canonical spelling —
/// case-insensitive, folding every alias onto one of the six codecs
/// [`encode`]/[`decode`] implement.
pub(in crate::entry) fn canonical_encoding(name: &str) -> Option<&'static str> {
    match name.to_ascii_lowercase().as_str() {
        "utf8" | "utf-8" => Some("utf8"),
        "ascii" => Some("ascii"),
        "latin1" | "binary" => Some("latin1"),
        "utf16le" | "utf-16le" | "ucs2" | "ucs-2" => Some("utf16le"),
        "base64" => Some("base64"),
        "base64url" => Some("base64url"),
        "hex" => Some("hex"),
        _ => None,
    }
}

/// A string encoded to bytes under the named encoding. `None` for a name
/// [`canonical_encoding`] does not recognize.
pub(in crate::entry) fn encode(text: &str, encoding: &str) -> Option<Vec<u8>> {
    match canonical_encoding(encoding)? {
        "utf8" => Some(text.as_bytes().to_vec()),
        "ascii" | "latin1" => Some(text.chars().map(|ch| ch as u32 as u8).collect()),
        "utf16le" => Some(encode_utf16le(text)),
        // `base64`/`base64url` are DECODE targets, not encode targets, for
        // `Buffer.from(string, encoding)` — Node treats the STRING as
        // already-encoded base64 there, so "encoding" a string INTO base64
        // bytes means decoding it, which is what `from` wants; both names
        // decode through the same permissive [`decode_base64`].
        "base64" | "base64url" => Some(decode_base64(text)),
        "hex" => Some(decode_hex(text)),
        _ => None,
    }
}

/// A JavaScript string encoded from its UTF-16 code units.
///
/// `Str::to_rust()` deliberately refuses lone surrogates, but Buffer follows
/// the byte codec's rules: UTF-8 writes U+FFFD, while UTF-16LE preserves the
/// original code unit. The narrow Rust-string entry point remains for callers
/// that already have well-formed text; this one is for JS strings.
pub(in crate::entry) fn encode_text(
    text: &crate::text::Str,
    encoding: &str,
) -> Option<Vec<u8>> {
    match canonical_encoding(encoding)? {
        "utf8" => Some(text.to_rust_lossy().into_bytes()),
        "ascii" => Some(text.units().map(|unit| unit as u8).collect()),
        "latin1" => Some(text.units().map(|unit| unit as u8).collect()),
        "utf16le" => {
            let mut out = Vec::with_capacity(text.len() * 2);
            for unit in text.units() {
                out.extend_from_slice(&unit.to_le_bytes());
            }
            Some(out)
        }
        "base64" | "base64url" => Some(decode_base64(&text.to_rust_lossy())),
        "hex" => Some(decode_hex(&text.to_rust_lossy())),
        _ => None,
    }
}

/// Bytes decoded to text under the named encoding.
pub(in crate::entry) fn decode(bytes: &[u8], encoding: &str) -> String {
    match canonical_encoding(encoding) {
        Some("utf8") => String::from_utf8_lossy(bytes).into_owned(),
        Some("ascii") => bytes.iter().map(|&byte| (byte & 0x7F) as char).collect(),
        Some("latin1") => bytes.iter().map(|&byte| byte as char).collect(),
        Some("utf16le") => decode_utf16le(bytes),
        Some("base64") | Some("base64url") => {
            encode_base64(bytes, matches!(canonical_encoding(encoding), Some("base64")))
        }
        Some("hex") => encode_hex(bytes),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// UTF-16LE encode: `char::encode_utf16` already emits surrogate pairs; this
/// only lays each `u16` out little-endian.
fn encode_utf16le(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 2);
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

/// UTF-16LE decode. An odd trailing byte is dropped; an unpaired surrogate
/// becomes `U+FFFD`, matching Node's `ucs2`/`utf16le` decode.
fn decode_utf16le(bytes: &[u8]) -> String {
    let units = bytes.chunks_exact(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]]));
    char::decode_utf16(units).map(|result| result.unwrap_or('\u{FFFD}')).collect()
}

/// Hex decode. Stops at the first non-hex-digit byte (Node's
/// truncate-on-invalid-nibble) and drops a trailing odd digit with no pair.
fn decode_hex(text: &str) -> Vec<u8> {
    let digits: Vec<u8> = text.bytes().take_while(|byte| byte.is_ascii_hexdigit()).collect();
    digits
        .chunks_exact(2)
        .map(|pair| {
            let hi = (pair[0] as char).to_digit(16).unwrap_or(0);
            let lo = (pair[1] as char).to_digit(16).unwrap_or(0);
            ((hi << 4) | lo) as u8
        })
        .collect()
}

/// Hex encode — `2 * bytes.len()` lowercase digits.
fn encode_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// The RFC 4648 alphabet, standard variant; [`encode_base64`] swaps the last
/// two characters (`+`/`/` → `-`/`_`) for the URL-safe form.
const BASE64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64 encode. `standard` selects `+`/`/` with `=` padding; `false` gives
/// `-`/`_` with no padding.
pub(in crate::entry) fn encode_base64(bytes: &[u8], standard: bool) -> String {
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        let indices = [
            b0 >> 2,
            ((b0 & 0x03) << 4) | (b1 >> 4),
            ((b1 & 0x0F) << 2) | (b2 >> 6),
            b2 & 0x3F,
        ];
        for (position, index) in indices.iter().enumerate() {
            let emit = position == 1 || (position == 2 && chunk.len() > 1) || (position == 3 && chunk.len() > 2);
            if position == 0 || emit {
                let ch = BASE64_ALPHABET[*index as usize] as char;
                out.push(match ch {
                    '+' if !standard => '-',
                    '/' if !standard => '_',
                    other => other,
                });
            } else if standard {
                out.push('=');
            }
        }
    }
    out
}

/// Base64/base64url decode, permissive: whitespace is ignored, both alphabets
/// are accepted regardless of which name was asked for, and padding is
/// optional.
pub(in crate::entry) fn decode_base64(text: &str) -> Vec<u8> {
    let mut bits: u32 = 0;
    let mut count = 0u32;
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    for ch in text.chars() {
        let value = match ch {
            'A'..='Z' => ch as u32 - 'A' as u32,
            'a'..='z' => ch as u32 - 'a' as u32 + 26,
            '0'..='9' => ch as u32 - '0' as u32 + 52,
            '+' | '-' => 62,
            '/' | '_' => 63,
            '=' => break,
            ch if ch.is_whitespace() => continue,
            _ => continue,
        };
        bits = (bits << 6) | value;
        count += 1;
        if count == 4 {
            out.push((bits >> 16) as u8);
            out.push((bits >> 8) as u8);
            out.push(bits as u8);
            bits = 0;
            count = 0;
        }
    }
    match count {
        3 => {
            bits <<= 6;
            out.push((bits >> 16) as u8);
            out.push((bits >> 8) as u8);
        }
        2 => {
            bits <<= 12;
            out.push((bits >> 16) as u8);
        }
        _ => {}
    }
    out
}
