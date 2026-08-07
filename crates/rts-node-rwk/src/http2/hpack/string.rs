//! RFC 7541 §5.2 — the string primitive: a 1-bit Huffman flag, a 7-bit-prefix
//! length, then that many bytes (literal) or Huffman-coded bytes.

use super::{huffman, integer};

/// Reads one length-prefixed string starting at `buf[0]`. Returns
/// `(text, bytes_consumed)`. `None` on truncated input, on a Huffman-coded
/// string [`huffman::decode`] rejects, or on bytes that are not valid UTF-8
/// once decoded — HPACK header text is opaque octets in the spec, but this
/// crate's namespace surface hands text to JS, so undecodable bytes are
/// reported as a decode failure rather than passed through lossily.
pub fn decode(buf: &[u8]) -> Option<(String, usize)> {
    let first = *buf.first()?;
    let huffman_coded = first & 0x80 != 0;
    let (length, prefix_len) = integer::decode(buf, 7)?;
    let length = length as usize;
    let start = prefix_len;
    let end = start.checked_add(length)?;
    let raw = buf.get(start..end)?;
    let text = if huffman_coded {
        String::from_utf8(huffman::decode(raw)?).ok()?
    } else {
        String::from_utf8(raw.to_vec()).ok()?
    };
    Some((text, end))
}

/// Writes `text` as a literal (non-Huffman) string — see the module doc in
/// `hpack/huffman.rs` for why encoding never uses Huffman here.
pub fn encode(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = integer::encode(bytes.len() as u64, 7, 0x00);
    out.extend_from_slice(bytes);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_round_trips() {
        let encoded = encode("hello");
        let (text, consumed) = decode(&encoded).unwrap();
        assert_eq!(text, "hello");
        assert_eq!(consumed, encoded.len());
    }

    #[test]
    fn decodes_huffman_coded_peer_string() {
        // RFC 7541 C.4.1 Huffman "www.example.com", with the length prefix
        // this primitive expects prepended (0x8c = huffman flag + length 12).
        let mut buf = vec![0x8c];
        buf.extend_from_slice(&[0xf1, 0xe3, 0xc2, 0xe5, 0xf2, 0x3a, 0x6b, 0xa0, 0xab, 0x90, 0xf4, 0xff]);
        let (text, consumed) = decode(&buf).unwrap();
        assert_eq!(text, "www.example.com");
        assert_eq!(consumed, buf.len());
    }

    #[test]
    fn truncated_string_is_none() {
        let mut encoded = encode("hello");
        encoded.truncate(2);
        assert_eq!(decode(&encoded), None);
    }
}
