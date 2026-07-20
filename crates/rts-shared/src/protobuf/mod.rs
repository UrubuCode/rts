//! `protobuf` namespace — the protobuf wire format (varint/tag/length-delimited/
//! fixed32/fixed64 encode+decode), exposed as a low-level writer/reader pair
//! over `Entry::Buffer` handles. No `.proto` schema parsing or code
//! generation — callers (hand-written TS against a known message shape, e.g.
//! WAProto) encode/decode field-by-field, the same way `protobufjs`-generated
//! code does under the hood.
//!
//! Two object-backed instances, same `Entry::Map`-tagged model `Hash`/`Cipher`
//! use: `ProtoWriter` (accumulates an `Entry::Buffer`, one `writeX` call per
//! field) and `ProtoReader` (wraps an existing buffer + a cursor offset,
//! `readTag`/`readX` calls advance it). `readTag` returns `-1` at end-of-buffer
//! so a `while (reader.readTag() >= 0)` loop in TS can walk an unknown or
//! partially-known message.
//!
//! Layout: `wire` (byte-level codec, pure functions), `state` (Writer/Reader
//! instance objects), `symbols` (extern points), `mod` (registration).

mod state;
mod symbols;
mod wire;

use rts_engine::AbiType::{self, Bool, Handle, I64};
use rts_engine::{Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

#[allow(clippy::too_many_arguments)]
fn m(name: &str, kind: MemberKind, args: Vec<AbiType>, ret: AbiType, symbol: &str, ts: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig: Sig::new(args, ret),
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: String::new(),
        pure: false,
        intrinsic: None,
        emit: None,
    }
}

/// Registers the `ProtoWriter`/`ProtoReader` classes + the `protobuf` module.
pub fn register(e: &mut Engine) {
    use symbols as s;
    use MemberKind::InstanceMethod;

    e.class("ProtoWriter")
        .doc("ProtoWriter — accumulates protobuf wire-format bytes field by field.")
        .member(m("writeTag", InstanceMethod, vec![Handle, I64, I64], Handle, "__RTS_FN_NS_PROTOBUF_WRITER_TAG", "writeTag(fieldNumber: number, wireType: number): ProtoWriter", s::__RTS_FN_NS_PROTOBUF_WRITER_TAG as *const u8))
        .member(m("writeVarint", InstanceMethod, vec![Handle, I64], Handle, "__RTS_FN_NS_PROTOBUF_WRITER_VARINT", "writeVarint(value: number): ProtoWriter", s::__RTS_FN_NS_PROTOBUF_WRITER_VARINT as *const u8))
        .member(m("writeZigzag", InstanceMethod, vec![Handle, I64], Handle, "__RTS_FN_NS_PROTOBUF_WRITER_ZIGZAG", "writeZigzag(value: number): ProtoWriter", s::__RTS_FN_NS_PROTOBUF_WRITER_ZIGZAG as *const u8))
        .member(m("writeFixed32", InstanceMethod, vec![Handle, I64], Handle, "__RTS_FN_NS_PROTOBUF_WRITER_FIXED32", "writeFixed32(value: number): ProtoWriter", s::__RTS_FN_NS_PROTOBUF_WRITER_FIXED32 as *const u8))
        .member(m("writeFixed64", InstanceMethod, vec![Handle, I64], Handle, "__RTS_FN_NS_PROTOBUF_WRITER_FIXED64", "writeFixed64(value: number): ProtoWriter", s::__RTS_FN_NS_PROTOBUF_WRITER_FIXED64 as *const u8))
        .member(m("writeBytes", InstanceMethod, vec![Handle, Handle], Handle, "__RTS_FN_NS_PROTOBUF_WRITER_BYTES", "writeBytes(data: object): ProtoWriter", s::__RTS_FN_NS_PROTOBUF_WRITER_BYTES as *const u8))
        .member(m("finish", InstanceMethod, vec![Handle], Handle, "__RTS_FN_NS_PROTOBUF_WRITER_FINISH", "finish(): number[]", s::__RTS_FN_NS_PROTOBUF_WRITER_FINISH as *const u8))
        .done();

    e.class("ProtoReader")
        .doc("ProtoReader — walks protobuf wire-format bytes tag by tag from a cursor.")
        .member(m("readTag", InstanceMethod, vec![Handle], I64, "__RTS_FN_NS_PROTOBUF_READER_TAG", "readTag(): number", s::__RTS_FN_NS_PROTOBUF_READER_TAG as *const u8))
        .member(m("lastFieldNumber", InstanceMethod, vec![Handle], I64, "__RTS_FN_NS_PROTOBUF_READER_FIELD_NUM", "lastFieldNumber(): number", s::__RTS_FN_NS_PROTOBUF_READER_FIELD_NUM as *const u8))
        .member(m("readVarint", InstanceMethod, vec![Handle], I64, "__RTS_FN_NS_PROTOBUF_READER_VARINT", "readVarint(): number", s::__RTS_FN_NS_PROTOBUF_READER_VARINT as *const u8))
        .member(m("readZigzag", InstanceMethod, vec![Handle], I64, "__RTS_FN_NS_PROTOBUF_READER_ZIGZAG", "readZigzag(): number", s::__RTS_FN_NS_PROTOBUF_READER_ZIGZAG as *const u8))
        .member(m("readFixed32", InstanceMethod, vec![Handle], I64, "__RTS_FN_NS_PROTOBUF_READER_FIXED32", "readFixed32(): number", s::__RTS_FN_NS_PROTOBUF_READER_FIXED32 as *const u8))
        .member(m("readFixed64", InstanceMethod, vec![Handle], I64, "__RTS_FN_NS_PROTOBUF_READER_FIXED64", "readFixed64(): number", s::__RTS_FN_NS_PROTOBUF_READER_FIXED64 as *const u8))
        .member(m("readBytes", InstanceMethod, vec![Handle], Handle, "__RTS_FN_NS_PROTOBUF_READER_BYTES", "readBytes(): number[]", s::__RTS_FN_NS_PROTOBUF_READER_BYTES as *const u8))
        .member(m("skip", InstanceMethod, vec![Handle], Bool, "__RTS_FN_NS_PROTOBUF_READER_SKIP", "skip(): boolean", s::__RTS_FN_NS_PROTOBUF_READER_SKIP as *const u8))
        .done();

    e.ns("protobuf")
        .doc("Protobuf wire-format codec (varint/tag/length-delimited/fixed32/fixed64) — no .proto schema parsing.")
        .member(m("newWriter", MemberKind::Function, vec![], Handle, "__RTS_FN_NS_PROTOBUF_NEW_WRITER", "newWriter(): ProtoWriter", s::__RTS_FN_NS_PROTOBUF_NEW_WRITER as *const u8))
        .member(m("newReader", MemberKind::Function, vec![Handle], Handle, "__RTS_FN_NS_PROTOBUF_NEW_READER", "newReader(data: object): ProtoReader", s::__RTS_FN_NS_PROTOBUF_NEW_READER as *const u8))
        .member(m("encodeVarint", MemberKind::Function, vec![I64], Handle, "__RTS_FN_NS_PROTOBUF_ENCODE_VARINT", "encodeVarint(value: number): number[]", s::__RTS_FN_NS_PROTOBUF_ENCODE_VARINT as *const u8))
        .member(m("decodeVarint", MemberKind::Function, vec![Handle, I64], Handle, "__RTS_FN_NS_PROTOBUF_DECODE_VARINT", "decodeVarint(data: object, offset: number): { value: number; length: number } | null", s::__RTS_FN_NS_PROTOBUF_DECODE_VARINT as *const u8))
        // Wire-type constants (protobuf.dev encoding table).
        .member(m("WIRE_VARINT", MemberKind::Constant, vec![], I64, "__RTS_FN_NS_PROTOBUF_WT_VARINT", "WIRE_VARINT: number", s::__RTS_FN_NS_PROTOBUF_WT_VARINT as *const u8))
        .member(m("WIRE_I64", MemberKind::Constant, vec![], I64, "__RTS_FN_NS_PROTOBUF_WT_I64", "WIRE_I64: number", s::__RTS_FN_NS_PROTOBUF_WT_I64 as *const u8))
        .member(m("WIRE_LEN", MemberKind::Constant, vec![], I64, "__RTS_FN_NS_PROTOBUF_WT_LEN", "WIRE_LEN: number", s::__RTS_FN_NS_PROTOBUF_WT_LEN as *const u8))
        .member(m("WIRE_I32", MemberKind::Constant, vec![], I64, "__RTS_FN_NS_PROTOBUF_WT_I32", "WIRE_I32: number", s::__RTS_FN_NS_PROTOBUF_WT_I32 as *const u8))
        .done();
}
