//! RFC 7541 §5.1 — HPACK's integer primitive: the low `prefix_bits` bits of
//! the first byte hold small values directly; a value too big for the prefix
//! is signaled by filling the prefix with 1s, then continued in 7-bit groups
//! with the continuation bit (top bit) set on every byte but the last.

/// Encodes `value` into the low `prefix_bits` bits of `first_byte_high` (the
/// caller's already-set high bits, e.g. HPACK's `0x80` "indexed" marker or
/// `0x40` "literal with incremental indexing" marker), appending continuation
/// bytes as needed. `prefix_bits` must be 1..=8.
pub fn encode(value: u64, prefix_bits: u8, first_byte_high: u8) -> Vec<u8> {
    let max_prefix = (1u64 << prefix_bits) - 1;
    if value < max_prefix {
        return vec![first_byte_high | value as u8];
    }
    let mut out = vec![first_byte_high | max_prefix as u8];
    let mut remainder = value - max_prefix;
    while remainder >= 128 {
        out.push(((remainder % 128) as u8) | 0x80);
        remainder /= 128;
    }
    out.push(remainder as u8);
    out
}

/// Decodes an integer starting at `buf[0]`, whose low `prefix_bits` bits hold
/// the prefix. Returns `(value, bytes_consumed)`. `None` on truncated input
/// or an overflow past `u64`.
pub fn decode(buf: &[u8], prefix_bits: u8) -> Option<(u64, usize)> {
    let first = *buf.first()?;
    let max_prefix = (1u64 << prefix_bits) - 1;
    let prefix_value = u64::from(first) & max_prefix;
    if prefix_value < max_prefix {
        return Some((prefix_value, 1));
    }
    let mut value = max_prefix;
    let mut shift = 0u32;
    let mut consumed = 1;
    loop {
        let byte = *buf.get(consumed)?;
        consumed += 1;
        value = value.checked_add(u64::from(byte & 0x7f).checked_shl(shift)?)?;
        if byte & 0x80 == 0 {
            return Some((value, consumed));
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_value_fits_in_prefix() {
        // RFC 7541 C.1.1 — 10, 5-bit prefix, no high bits set.
        let encoded = encode(10, 5, 0);
        assert_eq!(encoded, vec![10]);
        assert_eq!(decode(&encoded, 5), Some((10, 1)));
    }

    #[test]
    fn large_value_spans_continuation_bytes() {
        // RFC 7541 C.1.2 — 1337, 5-bit prefix -> [0x1f, 0x9a, 0x0a].
        let encoded = encode(1337, 5, 0);
        assert_eq!(encoded, vec![0x1f, 0x9a, 0x0a]);
        assert_eq!(decode(&encoded, 5), Some((1337, 3)));
    }

    #[test]
    fn boundary_value_equal_to_max_prefix() {
        // exactly max_prefix must still take the continuation path (RFC
        // requires strictly-less-than to take the direct path).
        let encoded = encode(31, 5, 0);
        assert_eq!(encoded, vec![0x1f, 0x00]);
        assert_eq!(decode(&encoded, 5), Some((31, 2)));
    }

    #[test]
    fn truncated_input_is_none() {
        assert_eq!(decode(&[0x1f], 5), None);
    }

    #[test]
    fn high_bits_preserved() {
        let encoded = encode(5, 7, 0x80); // indexed-header marker
        assert_eq!(encoded, vec![0x85]);
    }
}
