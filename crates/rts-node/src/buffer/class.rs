//! The `Buffer` class: every member is a STATIC (`alloc`/`allocUnsafe`/`from`/
//! `isBuffer`/`isEncoding`/`byteLength`/`compare`/`concat`/`toString`), so the
//! `#[rtse::class]` struct is a zero-field marker — there is no ctor to
//! allocate an `Entry::Rtse` instance from. See `mod.rs`'s doc comment for why
//! `toString` stays a static (`Buffer.toString(buf[, enc])`), not an instance
//! method.

use rts_engine::abi::ty::Handle;

use super::ops::{base64_decode, base64_encode, byte_array, checked_size, word_bytes};

/// The `Buffer` class marker.
#[rtse::class("Buffer")]
pub struct Buffer;

#[rtse::class("Buffer")]
impl Buffer {
    /// `Buffer.alloc(size)` — a zeroed byte array.
    #[rtse::statical(throws)]
    fn alloc(size: i64) -> Handle {
        byte_array(&vec![0u8; checked_size(size)])
    }

    /// `Buffer.alloc(size, fill)` — `size` bytes all set to `fill` (a byte 0..255).
    #[rtse::statical(name = "alloc", throws)]
    fn alloc_fill(size: i64, fill: i64) -> Handle {
        byte_array(&vec![fill as u8; checked_size(size)])
    }

    /// `Buffer.allocUnsafe(size)` — a zeroed byte array (no real "unsafe"
    /// uninitialized-memory path exists here; same as `alloc`).
    #[rtse::statical(throws)]
    fn alloc_unsafe(size: i64) -> Handle {
        byte_array(&vec![0u8; checked_size(size)])
    }

    /// `Buffer.from(string)` — UTF-8 bytes.
    #[rtse::statical]
    fn from(string: &str) -> Handle {
        byte_array(string.as_bytes())
    }

    /// `Buffer.from(string, encoding)` — utf8 / hex / base64 / latin1.
    #[rtse::statical(name = "from")]
    fn from_enc(string: &str, encoding: &str) -> Handle {
        let bytes = match encoding.to_lowercase().as_str() {
            "hex" => (0..string.len() / 2)
                .filter_map(|i| u8::from_str_radix(string.get(i * 2..i * 2 + 2)?, 16).ok())
                .collect(),
            "base64" | "base64url" => base64_decode(string),
            "latin1" | "binary" => string.chars().map(|c| c as u8).collect(),
            "utf16le" | "utf-16le" | "ucs2" | "ucs-2" => {
                string.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
            }
            _ => string.as_bytes().to_vec(),
        };
        byte_array(&bytes)
    }

    /// `Buffer.from(arrayOrBuffer)` — a Uint8Array/number[]/existing Buffer,
    /// copied byte-for-byte (Node's array/TypedArray/Buffer overload).
    #[rtse::statical(name = "from")]
    fn from_array(data: Handle) -> Handle {
        byte_array(&word_bytes(data))
    }

    /// `Buffer.isBuffer(obj)` — a Uint8Array-shaped array or a Buffer.
    #[rtse::statical]
    fn is_buffer(obj: Handle) -> bool {
        use rts_engine::heap::handles::{with_entry, Entry};
        use rts_engine::heap::poly::poly_handle_normalize;
        let h = poly_handle_normalize(obj).unwrap_or(obj);
        with_entry(h, |e| matches!(e, Some(Entry::Buffer(_)) | Some(Entry::Vec(_))))
    }

    /// `Buffer.isEncoding(encoding)` — whether `encoding` names a Buffer encoding.
    #[rtse::statical]
    fn is_encoding(encoding: &str) -> bool {
        matches!(
            encoding.to_lowercase().as_str(),
            "utf8" | "utf-8" | "hex" | "base64" | "base64url" | "latin1" | "binary"
                | "ascii" | "ucs2" | "ucs-2" | "utf16le" | "utf-16le"
        )
    }

    /// `Buffer.byteLength(string)` — UTF-8 byte count.
    #[rtse::statical]
    fn byte_length(string: &str) -> i64 {
        string.len() as i64
    }

    /// `Buffer.byteLength(string, encoding)` — the decoded byte length for the
    /// encoding (hex → len/2, base64 → decoded length, latin1/ascii → char count).
    #[rtse::statical(name = "byteLength")]
    fn byte_length_enc(string: &str, encoding: &str) -> i64 {
        let n = match encoding.to_lowercase().as_str() {
            "hex" => string.len() / 2,
            "base64" | "base64url" => base64_decode(string).len(),
            "latin1" | "binary" | "ascii" => string.chars().count(),
            _ => string.len(),
        };
        n as i64
    }

    /// `Buffer.compare(a, b)` — lexicographic (-1 / 0 / 1).
    #[rtse::statical]
    fn compare(a: Handle, b: Handle) -> i64 {
        match word_bytes(a).cmp(&word_bytes(b)) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        }
    }

    /// `Buffer.concat(list)` — concatenate an array of buffers.
    #[rtse::statical]
    fn concat(list: Handle) -> Handle {
        byte_array(&concat_bytes(list))
    }

    /// `Buffer.concat(list, totalLength)` — truncated or zero-padded to totalLength.
    #[rtse::statical(name = "concat")]
    fn concat_len(list: Handle, total_length: i64) -> Handle {
        let mut out = concat_bytes(list);
        out.resize(total_length.max(0) as usize, 0);
        byte_array(&out)
    }

    /// `Buffer.toString(buf)` — UTF-8 decode (Node's default `buf.toString()`
    /// encoding). A STATIC method, not an instance call — see the module doc.
    #[rtse::statical(name = "toString")]
    fn to_string_default(buf: Handle) -> String {
        String::from_utf8_lossy(&word_bytes(buf)).to_string()
    }

    /// `Buffer.toString(buf, encoding)` — utf8 / hex / base64 / base64url / latin1.
    #[rtse::statical(name = "toString")]
    fn to_string_enc(buf: Handle, encoding: &str) -> String {
        let bytes = word_bytes(buf);
        match encoding.to_lowercase().as_str() {
            "hex" => bytes.iter().map(|b| format!("{b:02x}")).collect(),
            "base64" => base64_encode(&bytes),
            "base64url" => base64_encode(&bytes).replace('+', "-").replace('/', "_").trim_end_matches('=').to_string(),
            "latin1" | "binary" | "ascii" => bytes.iter().map(|&b| b as char).collect(),
            _ => String::from_utf8_lossy(&bytes).to_string(),
        }
    }
}

/// The concatenated bytes of an array-of-buffers word.
fn concat_bytes(list: u64) -> Vec<u8> {
    use rts_engine::heap::handles::{with_entry, Entry};
    use rts_engine::heap::poly::poly_handle_normalize;
    let mut out = Vec::new();
    let h = poly_handle_normalize(list).unwrap_or(list);
    with_entry(h, |e| {
        if let Some(Entry::Vec(v)) = e {
            for &w in v.iter() {
                out.extend_from_slice(&word_bytes(w as u64));
            }
        }
    });
    out
}
