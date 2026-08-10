//! RFC 9113 §4/§6 — the 9-byte frame header, the connection preface, and the
//! frame types a working session needs. Pure Rust, no engine dependency:
//! everything here takes and returns bytes/structs, mirroring the split
//! `http/parser.rs` already draws between "parse bytes" and "touch the
//! engine" — see that file's module doc for why that split exists.
//!
//! # Frame types NOT covered here
//!
//! `PUSH_PROMISE`, `CONTINUATION`, `ALTSVC`, `ORIGIN` — the spec names eight
//! frame types as what "a working session needs"
//! (`SETTINGS`/`HEADERS`/`DATA`/`WINDOW_UPDATE`/`RST_STREAM`/`GOAWAY`/`PING`/
//! `PRIORITY`); this file implements exactly those eight and stops there.
//! `CONTINUATION` in particular means a `HEADERS` frame without `END_HEADERS`
//! is not reassembled — [`parse_header`] reports the flag, the caller is on
//! notice, and there is no path here that hangs waiting for a frame this
//! module cannot produce.

/// The 24 bytes a client sends before its first `SETTINGS` frame — RFC 9113
/// §3.4. A server implementation checks the first 24 bytes it reads against
/// this rather than parsing them as a frame header.
pub const CONNECTION_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

/// RFC 9113 §6 frame type byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    /// `DATA` — RFC 9113 §6.1.
    Data,
    /// `HEADERS` — RFC 9113 §6.2.
    Headers,
    /// `PRIORITY` — RFC 9113 §6.3, wire-compatible only (see the module doc).
    Priority,
    /// `RST_STREAM` — RFC 9113 §6.4.
    RstStream,
    /// `SETTINGS` — RFC 9113 §6.5.
    Settings,
    /// `WINDOW_UPDATE` — RFC 9113 §6.9.
    WindowUpdate,
    /// `PING` — RFC 9113 §6.7.
    Ping,
    /// `GOAWAY` — RFC 9113 §6.8.
    Goaway,
    /// Anything this module does not implement, kept by its wire value so a
    /// caller can still skip `length` bytes and move on rather than losing
    /// framing sync.
    Other(u8),
}

impl FrameType {
    fn from_byte(byte: u8) -> Self {
        match byte {
            0x0 => FrameType::Data,
            0x1 => FrameType::Headers,
            0x2 => FrameType::Priority,
            0x3 => FrameType::RstStream,
            0x4 => FrameType::Settings,
            0x8 => FrameType::WindowUpdate,
            0x6 => FrameType::Ping,
            0x7 => FrameType::Goaway,
            other => FrameType::Other(other),
        }
    }

    fn to_byte(self) -> u8 {
        match self {
            FrameType::Data => 0x0,
            FrameType::Headers => 0x1,
            FrameType::Priority => 0x2,
            FrameType::RstStream => 0x3,
            FrameType::Settings => 0x4,
            FrameType::WindowUpdate => 0x8,
            FrameType::Ping => 0x6,
            FrameType::Goaway => 0x7,
            FrameType::Other(byte) => byte,
        }
    }
}

/// The header every frame starts with — length(24) type(8) flags(8)
/// R(1)+stream_id(31), 9 bytes total (RFC 9113 §4.1).
#[derive(Debug, Clone, Copy)]
pub struct FrameHeader {
    /// The payload length in bytes (24-bit on the wire).
    pub length: u32,
    /// Which frame type this is.
    pub frame_type: FrameType,
    /// Type-specific flags, e.g. [`FLAG_END_HEADERS`].
    pub flags: u8,
    /// The stream this frame belongs to, `0` for connection-level frames.
    pub stream_id: u32,
}

/// A header plus its unparsed payload — the unit [`read_frame`] hands back.
pub struct Frame {
    /// The 9-byte header.
    pub header: FrameHeader,
    /// The frame's raw payload, `header.length` bytes.
    pub payload: Vec<u8>,
}

/// Reads one 9-byte header out of `buf`. `None` if fewer than 9 bytes are
/// available — the caller's job to wait for more, this module holds no
/// socket and no buffering state of its own.
pub fn parse_header(buf: &[u8]) -> Option<FrameHeader> {
    if buf.len() < 9 {
        return None;
    }
    let length = u32::from(buf[0]) << 16 | u32::from(buf[1]) << 8 | u32::from(buf[2]);
    let frame_type = FrameType::from_byte(buf[3]);
    let flags = buf[4];
    let stream_id = (u32::from(buf[5]) << 24
        | u32::from(buf[6]) << 16
        | u32::from(buf[7]) << 8
        | u32::from(buf[8]))
        & 0x7fff_ffff; // top bit is reserved, RFC 9113 §4.1
    Some(FrameHeader { length, frame_type, flags, stream_id })
}

/// Writes a 9-byte header.
pub fn write_header(header: &FrameHeader) -> [u8; 9] {
    let mut out = [0u8; 9];
    out[0] = (header.length >> 16) as u8;
    out[1] = (header.length >> 8) as u8;
    out[2] = header.length as u8;
    out[3] = header.frame_type.to_byte();
    out[4] = header.flags;
    let id = header.stream_id & 0x7fff_ffff;
    out[5] = (id >> 24) as u8;
    out[6] = (id >> 16) as u8;
    out[7] = (id >> 8) as u8;
    out[8] = id as u8;
    out
}

/// One complete frame (header + `length` bytes of payload) out of the front
/// of `buf`. `None` if the payload is not fully buffered yet — same
/// incremental shape as `http/parser.rs`'s `ChunkedDecoder::step`: called
/// repeatedly by a caller that owns the byte buffer, never touching a socket
/// itself.
pub fn read_frame(buf: &[u8]) -> Option<(Frame, usize)> {
    let header = parse_header(buf)?;
    let total = 9 + header.length as usize;
    if buf.len() < total {
        return None;
    }
    let payload = buf[9..total].to_vec();
    Some((Frame { header, payload }, total))
}

/// Serializes a complete frame: header followed by `payload`.
pub fn write_frame(frame_type: FrameType, flags: u8, stream_id: u32, payload: &[u8]) -> Vec<u8> {
    let header = FrameHeader { length: payload.len() as u32, frame_type, flags, stream_id };
    let mut out = Vec::with_capacity(9 + payload.len());
    out.extend_from_slice(&write_header(&header));
    out.extend_from_slice(payload);
    out
}

// ---------------------------------------------------------------------
// SETTINGS — RFC 9113 §6.5. Payload is a sequence of (u16 id, u32 value)
// pairs; the ACK flag (0x1) means an empty payload acknowledging the peer's
// last SETTINGS rather than carrying new ones.
// ---------------------------------------------------------------------

/// `SETTINGS`/`PING` acknowledgment flag.
pub const FLAG_ACK: u8 = 0x1;
/// `DATA`/`HEADERS` — no more frames will follow on this stream.
pub const FLAG_END_STREAM: u8 = 0x1;
/// `HEADERS` — the header block is complete (no `CONTINUATION` follows).
pub const FLAG_END_HEADERS: u8 = 0x4;
/// `DATA`/`HEADERS` — a pad-length byte and trailing padding are present.
pub const FLAG_PADDED: u8 = 0x8;
/// `HEADERS` — the (inert) priority prefix is present.
pub const FLAG_PRIORITY: u8 = 0x20;

/// One `(identifier, value)` pair out of a `SETTINGS` payload.
pub fn parse_settings(payload: &[u8]) -> Vec<(u16, u32)> {
    payload
        .chunks_exact(6)
        .map(|chunk| {
            let id = u16::from(chunk[0]) << 8 | u16::from(chunk[1]);
            let value = u32::from(chunk[2]) << 24
                | u32::from(chunk[3]) << 16
                | u32::from(chunk[4]) << 8
                | u32::from(chunk[5]);
            (id, value)
        })
        .collect()
}

/// The payload half of `write_frame(FrameType::Settings, ...)`.
pub fn write_settings(pairs: &[(u16, u32)]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pairs.len() * 6);
    for (id, value) in pairs {
        out.extend_from_slice(&id.to_be_bytes());
        out.extend_from_slice(&value.to_be_bytes());
    }
    out
}

// ---------------------------------------------------------------------
// HEADERS — RFC 9113 §6.2. This module strips padding and the priority
// prefix (both deprecated as inert by Node — see the module doc in
// `http2/mod.rs`) and hands back just the header-block fragment for HPACK.
// ---------------------------------------------------------------------

/// The header-block fragment inside a `HEADERS` frame payload, with padding
/// and the (ignored) priority prefix removed. `None` on a malformed payload
/// (padding/priority length longer than the payload itself).
pub fn parse_headers_payload(payload: &[u8], flags: u8) -> Option<&[u8]> {
    let mut rest = payload;
    let mut pad_len = 0usize;
    if flags & FLAG_PADDED != 0 {
        pad_len = *rest.first()? as usize;
        rest = rest.get(1..)?;
    }
    if flags & FLAG_PRIORITY != 0 {
        // 4 bytes exclusive+dependency, 1 byte weight — RFC 9113 removed the
        // semantics; skipped rather than interpreted, matching Node pinning
        // `state.weight`/`sumDependencyWeight` to inert constants.
        rest = rest.get(5..)?;
    }
    let block_len = rest.len().checked_sub(pad_len)?;
    rest.get(..block_len)
}

// ---------------------------------------------------------------------
// DATA — RFC 9113 §6.1. Same padding shape as HEADERS.
// ---------------------------------------------------------------------

/// The data bytes inside a `DATA` frame payload, with padding removed.
pub fn parse_data_payload(payload: &[u8], flags: u8) -> Option<&[u8]> {
    let mut rest = payload;
    let mut pad_len = 0usize;
    if flags & FLAG_PADDED != 0 {
        pad_len = *rest.first()? as usize;
        rest = rest.get(1..)?;
    }
    let data_len = rest.len().checked_sub(pad_len)?;
    rest.get(..data_len)
}

// ---------------------------------------------------------------------
// WINDOW_UPDATE — RFC 9113 §6.9. 4 bytes: R(1) + increment(31).
// ---------------------------------------------------------------------

/// Parses a `WINDOW_UPDATE` payload's flow-control window increment.
pub fn parse_window_update(payload: &[u8]) -> Option<u32> {
    if payload.len() != 4 {
        return None;
    }
    let raw = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
    Some(raw & 0x7fff_ffff)
}

/// Serializes a `WINDOW_UPDATE` payload.
pub fn write_window_update(increment: u32) -> Vec<u8> {
    (increment & 0x7fff_ffff).to_be_bytes().to_vec()
}

// ---------------------------------------------------------------------
// RST_STREAM — RFC 9113 §6.4. 4 bytes: error code.
// ---------------------------------------------------------------------

/// Parses an `RST_STREAM` payload's error code.
pub fn parse_rst_stream(payload: &[u8]) -> Option<u32> {
    if payload.len() != 4 {
        return None;
    }
    Some(u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]))
}

/// The payload half of `write_frame(FrameType::RstStream, ...)`.
pub fn write_rst_stream(error_code: u32) -> Vec<u8> {
    error_code.to_be_bytes().to_vec()
}

// ---------------------------------------------------------------------
// GOAWAY — RFC 9113 §6.8. R(1)+last_stream_id(31), error_code(32), then
// opaque debug data.
// ---------------------------------------------------------------------

/// A decoded `GOAWAY` frame payload.
pub struct Goaway {
    /// The highest stream id the sender guarantees it processed.
    pub last_stream_id: u32,
    /// The error code — one of `NGHTTP2_*` (`http2/mod.rs`'s `constants`).
    pub error_code: u32,
    /// Opaque debug data, not interpreted by this module.
    pub debug_data: Vec<u8>,
}

/// Parses a `GOAWAY` payload.
pub fn parse_goaway(payload: &[u8]) -> Option<Goaway> {
    if payload.len() < 8 {
        return None;
    }
    let last_stream_id =
        u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]) & 0x7fff_ffff;
    let error_code = u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);
    Some(Goaway { last_stream_id, error_code, debug_data: payload[8..].to_vec() })
}

/// Serializes a `GOAWAY` payload.
pub fn write_goaway(last_stream_id: u32, error_code: u32, debug_data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + debug_data.len());
    out.extend_from_slice(&(last_stream_id & 0x7fff_ffff).to_be_bytes());
    out.extend_from_slice(&error_code.to_be_bytes());
    out.extend_from_slice(debug_data);
    out
}

// ---------------------------------------------------------------------
// PING — RFC 9113 §6.7. Exactly 8 opaque bytes.
// ---------------------------------------------------------------------

/// Parses a `PING` payload — must be exactly 8 bytes (RFC 9113 §6.7).
pub fn parse_ping(payload: &[u8]) -> Option<[u8; 8]> {
    payload.try_into().ok()
}

/// Serializes a `PING` payload.
pub fn write_ping(payload: [u8; 8]) -> Vec<u8> {
    payload.to_vec()
}

// ---------------------------------------------------------------------
// PRIORITY — RFC 9113 §6.3, retained for wire compatibility only. RFC 9113
// obsoleted the semantics (Node pins `state.weight`/`sumDependencyWeight` to
// 16/0 and turns `priority()` into a no-op — see `http2/mod.rs`), so this
// module parses the 5 bytes for framing correctness and does not act on them.
// ---------------------------------------------------------------------

/// A decoded `PRIORITY` frame payload — parsed for wire correctness only,
/// see the module doc: RFC 9113 obsoleted its semantics.
pub struct Priority {
    /// Whether the dependency is exclusive.
    pub exclusive: bool,
    /// The stream this one (nominally) depends on.
    pub dependency: u32,
    /// The stream's weight, 1..=256 on the wire (stored as sent, 0..=255).
    pub weight: u8,
}

/// Parses a `PRIORITY` payload.
pub fn parse_priority(payload: &[u8]) -> Option<Priority> {
    if payload.len() != 5 {
        return None;
    }
    let raw = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
    Some(Priority { exclusive: raw & 0x8000_0000 != 0, dependency: raw & 0x7fff_ffff, weight: payload[4] })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_round_trips() {
        let header = FrameHeader { length: 42, frame_type: FrameType::Headers, flags: 5, stream_id: 3 };
        let bytes = write_header(&header);
        let parsed = parse_header(&bytes).unwrap();
        assert_eq!(parsed.length, 42);
        assert_eq!(parsed.frame_type, FrameType::Headers);
        assert_eq!(parsed.flags, 5);
        assert_eq!(parsed.stream_id, 3);
    }

    #[test]
    fn read_frame_waits_for_full_payload() {
        let full = write_frame(FrameType::Ping, 0, 0, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert!(read_frame(&full[..10]).is_none());
        let (frame, consumed) = read_frame(&full).unwrap();
        assert_eq!(consumed, full.len());
        assert_eq!(frame.payload, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn settings_round_trip() {
        let pairs = vec![(0x1u16, 4096u32), (0x3, 100)];
        let payload = write_settings(&pairs);
        assert_eq!(parse_settings(&payload), pairs);
    }

    #[test]
    fn headers_payload_strips_padding_and_priority() {
        // pad_len=2, priority(5 bytes, ignored), block=[0xAA], 2 pad bytes.
        let mut payload = vec![2u8];
        payload.extend_from_slice(&[0, 0, 0, 0, 16]); // priority
        payload.push(0xAA);
        payload.extend_from_slice(&[0, 0]); // padding
        let block = parse_headers_payload(&payload, FLAG_PADDED | FLAG_PRIORITY).unwrap();
        assert_eq!(block, &[0xAA]);
    }

    #[test]
    fn goaway_round_trip() {
        let payload = write_goaway(7, 0x1, b"bye");
        let parsed = parse_goaway(&payload).unwrap();
        assert_eq!(parsed.last_stream_id, 7);
        assert_eq!(parsed.error_code, 1);
        assert_eq!(parsed.debug_data, b"bye");
    }

    #[test]
    fn preface_is_24_bytes() {
        assert_eq!(CONNECTION_PREFACE.len(), 24);
    }
}
