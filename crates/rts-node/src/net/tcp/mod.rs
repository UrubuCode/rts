//! node:net — the TCP classes: `Server` and `Socket`, plus the shared helpers
//! their impls need (address shapes, the byte encodings `write`/`setEncoding`
//! speak).
//!
//! Layout: `state` (live state + queues + tables), `opts` (option objects,
//! errors, the module config), `server`, `socket`, `props` (the Socket's tuning
//! + properties), `pump` (JS-thread delivery).

pub mod opts;
pub mod props;
pub mod pump;
pub mod server;
pub mod socket;
pub mod state;

/// The bytes a string carries under `encoding` — the encodings Node's
/// `BufferEncoding` names, decoded/encoded here rather than reinvented per call
/// site. An unknown label falls back to UTF-8, which is Node's default.
pub fn encode(s: &str, encoding: &str) -> Vec<u8> {
    match encoding {
        "hex" => (0..s.len() / 2)
            .filter_map(|i| u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok())
            .collect(),
        "base64" | "base64url" => base64_decode(s),
        "latin1" | "binary" | "ascii" => s.chars().map(|c| c as u32 as u8).collect(),
        "utf16le" | "ucs2" | "ucs-2" | "utf-16le" => {
            s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
        }
        _ => s.as_bytes().to_vec(),
    }
}

/// The string `bytes` decode to under `encoding` (the `'data'` side of
/// `setEncoding`).
pub fn decode(bytes: &[u8], encoding: &str) -> String {
    match encoding {
        "hex" => bytes.iter().map(|b| format!("{b:02x}")).collect(),
        "base64" => base64_encode(bytes, false),
        "base64url" => base64_encode(bytes, true),
        "latin1" | "binary" | "ascii" => bytes.iter().map(|&b| b as char).collect(),
        "utf16le" | "ucs2" | "ucs-2" | "utf-16le" => {
            let units: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&units)
        }
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const B64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn base64_encode(data: &[u8], url: bool) -> String {
    let table = if url { B64URL } else { B64 };
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(table[(n >> 18 & 63) as usize] as char);
        out.push(table[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 { table[(n >> 6 & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { table[(n & 63) as usize] as char } else { '=' });
    }
    if url {
        out.retain(|c| c != '=');
    }
    out
}

fn base64_decode(s: &str) -> Vec<u8> {
    let val = |c: u8| -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a') as u32 + 26),
            b'0'..=b'9' => Some((c - b'0') as u32 + 52),
            b'+' | b'-' => Some(62),
            b'/' | b'_' => Some(63),
            _ => None,
        }
    };
    let digits: Vec<u32> = s.bytes().filter_map(val).collect();
    let mut out = Vec::with_capacity(digits.len() * 3 / 4);
    for chunk in digits.chunks(4) {
        let mut n = 0u32;
        for (i, &d) in chunk.iter().enumerate() {
            n |= d << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    out
}
