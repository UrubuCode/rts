//! node:fs — string↔bytes encoding for the `encoding`-form read/write/append
//! (utf8/hex/base64/base64url/latin1). Shared by the encoding overloads in
//! `symbols`.

/// Encode file bytes per a Node encoding name (default utf8).
pub fn encode_bytes(bytes: &[u8], enc: &str) -> String {
    match enc.to_lowercase().as_str() {
        "hex" => bytes.iter().map(|b| format!("{b:02x}")).collect(),
        "base64" => b64_encode(bytes, false),
        "base64url" => b64_encode(bytes, true),
        "latin1" | "binary" | "ascii" => bytes.iter().map(|&b| b as char).collect(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Decode a string to bytes per a Node encoding name (default utf8).
pub fn decode_bytes(s: &str, enc: &str) -> Vec<u8> {
    match enc.to_lowercase().as_str() {
        "hex" => (0..s.len() / 2)
            .filter_map(|i| u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok())
            .collect(),
        "base64" | "base64url" => b64_decode(s),
        "latin1" | "binary" | "ascii" => s.chars().map(|c| c as u8).collect(),
        _ => s.as_bytes().to_vec(),
    }
}

fn b64_encode(data: &[u8], url: bool) -> String {
    const STD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    const URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let tbl = if url { URL } else { STD };
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(tbl[(n >> 18 & 63) as usize] as char);
        out.push(tbl[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 { tbl[(n >> 6 & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { tbl[(n & 63) as usize] as char } else { '=' });
    }
    if url { out.trim_end_matches('=').to_string() } else { out }
}

fn b64_decode(s: &str) -> Vec<u8> {
    let val = |c: u8| -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' | b'-' => Some(62),
            b'/' | b'_' => Some(63),
            _ => None,
        }
    };
    let clean: Vec<u8> = s.bytes().filter(|&c| val(c).is_some()).collect();
    let mut out = Vec::new();
    for chunk in clean.chunks(4) {
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= val(c).unwrap_or(0) << (18 - 6 * i);
        }
        out.push((n >> 16 & 0xff) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8 & 0xff) as u8);
        }
        if chunk.len() > 3 {
            out.push((n & 0xff) as u8);
        }
    }
    out
}
