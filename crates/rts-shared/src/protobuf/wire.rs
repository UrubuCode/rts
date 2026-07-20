//! Protobuf wire-format encode/decode — the raw byte-level operations (no
//! `.proto` schema parsing, no generated types). Real implementation of the
//! spec at <https://protobuf.dev/programming-guides/encoding/>: base-128
//! varints (LEB128-style, little-endian group order), the tag = (field_number
//! << 3) | wire_type encoding, and all 4 wire types RTS needs to round-trip
//! arbitrary protobuf messages (VARINT, I64, LEN, I32 — GROUP/3/4 are legacy
//! and unsupported, matching modern protobuf implementations).

/// The wire type tag occupies the low 3 bits of the varint-encoded tag byte.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum WireType {
    Varint = 0,
    I64 = 1,
    Len = 2,
    I32 = 5,
}

impl WireType {
    pub fn from_u64(v: u64) -> Option<WireType> {
        match v {
            0 => Some(WireType::Varint),
            1 => Some(WireType::I64),
            2 => Some(WireType::Len),
            5 => Some(WireType::I32),
            _ => None,
        }
    }
}

/// Append a base-128 varint (unsigned) to `out`.
pub fn write_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            break;
        }
        out.push(byte | 0x80);
    }
}

/// Read a base-128 varint starting at `pos`. Returns `(value, bytes_consumed)`,
/// or `None` on truncated/overlong (>10 bytes) input.
pub fn read_varint(buf: &[u8], pos: usize) -> Option<(u64, usize)> {
    let mut result: u64 = 0;
    let mut shift = 0u32;
    let mut i = pos;
    loop {
        if i >= buf.len() || shift >= 70 {
            return None;
        }
        let byte = buf[i];
        result |= ((byte & 0x7f) as u64) << shift;
        i += 1;
        if byte & 0x80 == 0 {
            return Some((result, i - pos));
        }
        shift += 7;
    }
}

/// ZigZag-encode a signed 64-bit value (protobuf's `sint64` representation).
pub fn zigzag_encode(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}

/// ZigZag-decode back to a signed 64-bit value.
pub fn zigzag_decode(v: u64) -> i64 {
    ((v >> 1) as i64) ^ -((v & 1) as i64)
}

/// Append a field tag: `(field_number << 3) | wire_type`.
pub fn write_tag(out: &mut Vec<u8>, field_number: u32, wire_type: WireType) {
    write_varint(out, ((field_number as u64) << 3) | (wire_type as u64));
}

/// Read a field tag, returning `(field_number, wire_type, bytes_consumed)`.
/// `None` if truncated or the wire type is unrecognized (GROUP/legacy).
pub fn read_tag(buf: &[u8], pos: usize) -> Option<(u32, WireType, usize)> {
    let (raw, n) = read_varint(buf, pos)?;
    let field_number = (raw >> 3) as u32;
    let wire_type = WireType::from_u64(raw & 0x7)?;
    Some((field_number, wire_type, n))
}

/// Append a length-delimited field's length prefix + bytes (LEN wire type
/// payload — used for strings, sub-messages, and packed repeated fields).
pub fn write_len_delimited(out: &mut Vec<u8>, data: &[u8]) {
    write_varint(out, data.len() as u64);
    out.extend_from_slice(data);
}

/// Read a length-delimited payload at `pos` (the length prefix, then that
/// many bytes). Returns `(slice, bytes_consumed_including_prefix)`.
pub fn read_len_delimited(buf: &[u8], pos: usize) -> Option<(&[u8], usize)> {
    let (len, n) = read_varint(buf, pos)?;
    let start = pos + n;
    let end = start.checked_add(len as usize)?;
    if end > buf.len() {
        return None;
    }
    Some((&buf[start..end], n + len as usize))
}

/// Append a fixed-width little-endian `u32` (I32 wire type — `fixed32`/`sfixed32`/`float`).
pub fn write_fixed32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Read a fixed-width little-endian `u32` at `pos`. `None` if truncated.
pub fn read_fixed32(buf: &[u8], pos: usize) -> Option<(u32, usize)> {
    let end = pos.checked_add(4)?;
    if end > buf.len() {
        return None;
    }
    Some((u32::from_le_bytes(buf[pos..end].try_into().ok()?), 4))
}

/// Append a fixed-width little-endian `u64` (I64 wire type — `fixed64`/`sfixed64`/`double`).
pub fn write_fixed64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Read a fixed-width little-endian `u64` at `pos`. `None` if truncated.
pub fn read_fixed64(buf: &[u8], pos: usize) -> Option<(u64, usize)> {
    let end = pos.checked_add(8)?;
    if end > buf.len() {
        return None;
    }
    Some((u64::from_le_bytes(buf[pos..end].try_into().ok()?), 8))
}

/// Skip one field's value of the given wire type, starting at `pos` (right
/// after the tag). Returns the number of bytes the value occupies — used to
/// walk past fields a reader doesn't recognize (protobuf's forward-compat
/// contract: unknown fields are preserved by skipping, not an error).
pub fn skip_value(buf: &[u8], pos: usize, wire_type: WireType) -> Option<usize> {
    match wire_type {
        WireType::Varint => read_varint(buf, pos).map(|(_, n)| n),
        WireType::I64 => (pos + 8 <= buf.len()).then_some(8),
        WireType::I32 => (pos + 4 <= buf.len()).then_some(4),
        WireType::Len => read_len_delimited(buf, pos).map(|(_, n)| n),
    }
}
