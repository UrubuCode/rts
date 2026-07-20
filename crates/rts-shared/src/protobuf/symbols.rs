//! `protobuf` — the `extern "C"` entry points: `ProtoWriter`/`ProtoReader`
//! instance methods and the module-level `newWriter`/`newReader`/
//! `encodeVarint`/`decodeVarint`/wire-type constants.

use super::state::{byte_array, build_reader, build_writer, read_bytes, reader_advance, reader_last_field_number, reader_last_wire_type, reader_state, reader_set_last_tag, writer_append, writer_bytes};
use super::wire::{self, WireType};

// ---- Module-level functions ----

/// `protobuf.newWriter()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROTOBUF_NEW_WRITER() -> u64 {
    build_writer()
}

/// `protobuf.newReader(data)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROTOBUF_NEW_READER(data: u64) -> u64 {
    build_reader(&read_bytes(data))
}

/// `protobuf.encodeVarint(value)` → standalone byte array (no writer needed).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROTOBUF_ENCODE_VARINT(value: i64) -> u64 {
    let mut out = Vec::new();
    wire::write_varint(&mut out, value as u64);
    byte_array(&out)
}

/// `protobuf.decodeVarint(data, offset)` → `{ value, length }` object, or
/// `null` (handle 0) if truncated. Standalone decode, no reader needed.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROTOBUF_DECODE_VARINT(data: u64, offset: i64) -> u64 {
    let bytes = read_bytes(data);
    match wire::read_varint(&bytes, offset.max(0) as usize) {
        Some((value, len)) => {
            let num = |x: f64| x.to_bits() as i64;
            rts_engine::heap::shapes::alloc_shaped_object(&["value", "length"], &[num(value as f64), num(len as f64)])
        }
        None => 0,
    }
}

// ---- Wire-type constants ----

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROTOBUF_WT_VARINT() -> i64 {
    WireType::Varint as i64
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROTOBUF_WT_I64() -> i64 {
    WireType::I64 as i64
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROTOBUF_WT_LEN() -> i64 {
    WireType::Len as i64
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROTOBUF_WT_I32() -> i64 {
    WireType::I32 as i64
}

// ---- ProtoWriter instance methods ----

/// `writer.writeTag(fieldNumber, wireType)` — returns `this` (chainable).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROTOBUF_WRITER_TAG(this: u64, field_number: i64, wire_type: i64) -> u64 {
    if let Some(wt) = WireType::from_u64(wire_type.max(0) as u64) {
        let mut out = Vec::new();
        wire::write_tag(&mut out, field_number.max(0) as u32, wt);
        writer_append(this, &out);
    }
    this
}

/// `writer.writeVarint(value)` — returns `this` (chainable).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROTOBUF_WRITER_VARINT(this: u64, value: i64) -> u64 {
    let mut out = Vec::new();
    wire::write_varint(&mut out, value as u64);
    writer_append(this, &out);
    this
}

/// `writer.writeZigzag(value)` — signed value, zigzag + varint encoded.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROTOBUF_WRITER_ZIGZAG(this: u64, value: i64) -> u64 {
    let mut out = Vec::new();
    wire::write_varint(&mut out, wire::zigzag_encode(value));
    writer_append(this, &out);
    this
}

/// `writer.writeFixed32(value)` — returns `this` (chainable).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROTOBUF_WRITER_FIXED32(this: u64, value: i64) -> u64 {
    let mut out = Vec::new();
    wire::write_fixed32(&mut out, value as u32);
    writer_append(this, &out);
    this
}

/// `writer.writeFixed64(value)` — returns `this` (chainable).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROTOBUF_WRITER_FIXED64(this: u64, value: i64) -> u64 {
    let mut out = Vec::new();
    wire::write_fixed64(&mut out, value as u64);
    writer_append(this, &out);
    this
}

/// `writer.writeBytes(data)` — length-delimited (LEN wire-type payload:
/// varint length prefix + raw bytes). Returns `this` (chainable).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROTOBUF_WRITER_BYTES(this: u64, data: u64) -> u64 {
    let mut out = Vec::new();
    wire::write_len_delimited(&mut out, &read_bytes(data));
    writer_append(this, &out);
    this
}

/// `writer.finish()` → the accumulated bytes as a Uint8Array-shaped Buffer.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROTOBUF_WRITER_FINISH(this: u64) -> u64 {
    byte_array(&writer_bytes(this))
}

// ---- ProtoReader instance methods ----

/// `reader.readTag()` → the field number (`lastFieldNumber()` and the wire
/// type are then available via their own getters — Cranelift's ABI can't
/// return a tuple, so this is split across 3 calls). Returns `-1` at
/// end-of-buffer or on a malformed/unrecognized tag.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROTOBUF_READER_TAG(this: u64) -> i64 {
    let Some((buf, pos)) = reader_state(this) else { return -1 };
    if pos >= buf.len() {
        return -1;
    }
    match wire::read_tag(&buf, pos) {
        Some((field_number, wt, n)) => {
            reader_advance(this, n);
            reader_set_last_tag(this, field_number, wt);
            field_number as i64
        }
        None => -1,
    }
}

/// `reader.lastFieldNumber()` — the field number from the last `readTag()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROTOBUF_READER_FIELD_NUM(this: u64) -> i64 {
    reader_last_field_number(this)
}

/// `reader.readVarint()` — reads an unsigned varint at the cursor, advancing
/// it. Returns `-1` on truncation (ambiguous with a genuine `-1`-decoding
/// value only for `sint64`-style fields, which should use `readZigzag`).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROTOBUF_READER_VARINT(this: u64) -> i64 {
    let Some((buf, pos)) = reader_state(this) else { return -1 };
    match wire::read_varint(&buf, pos) {
        Some((v, n)) => {
            reader_advance(this, n);
            v as i64
        }
        None => -1,
    }
}

/// `reader.readZigzag()` — reads a zigzag-encoded signed varint.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROTOBUF_READER_ZIGZAG(this: u64) -> i64 {
    let Some((buf, pos)) = reader_state(this) else { return 0 };
    match wire::read_varint(&buf, pos) {
        Some((v, n)) => {
            reader_advance(this, n);
            wire::zigzag_decode(v)
        }
        None => 0,
    }
}

/// `reader.readFixed32()` — reads a little-endian `u32` (`fixed32`/`sfixed32`/`float` payload).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROTOBUF_READER_FIXED32(this: u64) -> i64 {
    let Some((buf, pos)) = reader_state(this) else { return 0 };
    match wire::read_fixed32(&buf, pos) {
        Some((v, n)) => {
            reader_advance(this, n);
            v as i64
        }
        None => 0,
    }
}

/// `reader.readFixed64()` — reads a little-endian `u64` (`fixed64`/`sfixed64`/`double` payload).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROTOBUF_READER_FIXED64(this: u64) -> i64 {
    let Some((buf, pos)) = reader_state(this) else { return 0 };
    match wire::read_fixed64(&buf, pos) {
        Some((v, n)) => {
            reader_advance(this, n);
            v as i64
        }
        None => 0,
    }
}

/// `reader.readBytes()` — reads a length-delimited payload (LEN wire type:
/// used for strings, sub-messages, packed repeated fields) → Uint8Array-shaped
/// Buffer. Empty array on truncation.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROTOBUF_READER_BYTES(this: u64) -> u64 {
    let Some((buf, pos)) = reader_state(this) else { return byte_array(&[]) };
    match wire::read_len_delimited(&buf, pos) {
        Some((slice, n)) => {
            let out = byte_array(slice);
            reader_advance(this, n);
            out
        }
        None => byte_array(&[]),
    }
}

/// `reader.skip()` — skips the value for the wire type from the last
/// `readTag()` (protobuf's forward-compat contract for unknown fields).
/// Returns `false` if there's no recorded tag or the value is truncated.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PROTOBUF_READER_SKIP(this: u64) -> i64 {
    let Some(wt) = reader_last_wire_type(this) else { return 0 };
    let Some((buf, pos)) = reader_state(this) else { return 0 };
    match wire::skip_value(&buf, pos, wt) {
        Some(n) => {
            reader_advance(this, n);
            1
        }
        None => 0,
    }
}

