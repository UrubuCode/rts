//! Text <-> bytes conversion for the encoding argument `readFileSync`/
//! `writeFileSync`/`appendFileSync` take: `"hex"`, `"base64"`/`"base64url"`,
//! `"utf16le"`/`"ucs2"`/`"ucs-2"`, and `"utf8"` (the default).
//!
//! Hand-rolled rather than a new dependency: nothing in `Cargo.toml` covers
//! this, `docs/reference/node/crates.md` was not asked to vet one for it, and
//! each of the three is a lookup table and a loop — not worth a manifest
//! change every other agent editing this crate has to avoid colliding on.

/// A string argument, decoded to the bytes it names under `encoding` — what
/// `writeFileSync(path, "616263", "hex")` must produce before the write, and
/// what was missing: every encoding used to fall through to `text.as_bytes()`
/// unconditionally, so `"616263"` in hex was written as its own six ASCII
/// digits instead of the three bytes they name.
pub(super) fn decode(encoding_name: &str, text: &str) -> Option<Vec<u8>> {
    match encoding_name {
        "hex" => hex_decode(text),
        "base64" | "base64url" => base64_decode(text),
        "utf16le" | "ucs2" | "ucs-2" => Some(utf16le_encode(text)),
        _ => Some(text.as_bytes().to_vec()),
    }
}

/// Raw bytes, read back as text under `encoding` — the other half: what
/// `readFileSync(path, "hex")` must answer is the hex text FOR those bytes,
/// not the UTF-8 decoding of them (which is what always ran before, and is
/// wrong — and often not even valid UTF-8 — for arbitrary bytes).
pub(super) fn encode(encoding_name: &str, bytes: &[u8]) -> Option<String> {
    match encoding_name {
        "hex" => Some(hex_encode(bytes)),
        "base64" => Some(base64_encode(bytes)),
        "base64url" => Some(base64_encode(bytes).replace('+', "-").replace('/', "_").trim_end_matches('=').to_string()),
        "utf16le" | "ucs2" | "ucs-2" => utf16le_decode(bytes),
        _ => String::from_utf8(bytes.to_vec()).ok(),
    }
}

fn hex_decode(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if bytes.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut index = 0;
    while index < bytes.len() {
        let hi = (bytes[index] as char).to_digit(16)?;
        let lo = (bytes[index + 1] as char).to_digit(16)?;
        out.push(((hi << 4) | lo) as u8);
        index += 2;
    }
    Some(out)
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

const BASE64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        let word = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);
        out.push(BASE64_ALPHABET[((word >> 18) & 0x3f) as usize] as char);
        out.push(BASE64_ALPHABET[((word >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 { BASE64_ALPHABET[((word >> 6) & 0x3f) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { BASE64_ALPHABET[(word & 0x3f) as usize] as char } else { '=' });
    }
    out
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut table = [255u8; 256];
    for (index, &byte) in BASE64_ALPHABET.iter().enumerate() {
        table[byte as usize] = index as u8;
    }
    // `base64url` swaps two symbols; accepted on the decode side unconditionally
    // rather than as a second encoding name — a caller that WROTE `base64` and
    // READS `base64url` (or the reverse) gets the same bytes either way, which
    // matches every real decoder's own tolerance here.
    table[b'-' as usize] = table[b'+' as usize];
    table[b'_' as usize] = table[b'/' as usize];
    let clean: Vec<u8> = text.bytes().filter(|byte| *byte != b'=' && !byte.is_ascii_whitespace()).collect();
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for byte in clean {
        let value = table[byte as usize];
        if value == 255 {
            return None;
        }
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    Some(out)
}

fn utf16le_encode(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 2);
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

fn utf16le_decode(bytes: &[u8]) -> Option<String> {
    let mut units = Vec::with_capacity(bytes.len() / 2);
    let mut index = 0;
    while index + 1 < bytes.len() {
        units.push(u16::from_le_bytes([bytes[index], bytes[index + 1]]));
        index += 2;
    }
    String::from_utf16(&units).ok()
}
