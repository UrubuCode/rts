//! `node:string_decoder` — the full surface: the single `StringDecoder` class
//! (`new StringDecoder([encoding])`, `.write(buffer)`, `.end([buffer])`,
//! `.encoding`), a faithful native port of Node's incremental decoder. No
//! stubs — real multi-byte/multi-unit boundary handling for utf8/utf16le/
//! base64/base64url/latin1/ascii/hex.
//!
//! `StringDecoder` is registered as an object-backed Registry class (the same
//! model as `DOMException`): the instance is an `Entry::Map` tagged
//! `__rts_class = "StringDecoder"`, so method dispatch and `new` resolve
//! data-driven, the engine naming no non-primordial. Members carry an explicit
//! `ts_signature` so a `Handle` string return reboxes as a string (not an
//! opaque object). A `node:string_decoder` namespace is registered so the
//! module specifier resolves.

mod decoder;
mod state;
mod symbols;

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

#[allow(clippy::too_many_arguments)]
fn member(
    name: &str,
    kind: MemberKind,
    args: Vec<AbiType>,
    ret: AbiType,
    symbol: &str,
    ts: &str,
    fp: *const u8,
    throws: bool,
) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig: Sig::new(args, ret),
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: if throws { MemberFlags::THROWS } else { MemberFlags::NONE },
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: String::new(),
        pure: false,
        emit: None,
    }
}

/// Registers the `StringDecoder` class + the `node:string_decoder` module.
pub fn register(e: &mut Engine) {
    use rts_engine::AbiType::Handle;
    use MemberKind::{Constructor, InstanceGetter, InstanceMethod};
    e.class("StringDecoder")
        .doc("StringDecoder — incremental byte→string decoder (node:string_decoder).")
        .member(member(
            "new",
            Constructor,
            Vec::new(),
            Handle,
            "__RTS_FN_NODE_STRING_DECODER_NEW",
            "new StringDecoder()",
            symbols::__RTS_FN_NODE_STRING_DECODER_NEW as *const u8,
            false,
        ))
        .member(member(
            "new",
            Constructor,
            vec![AbiType::StrPtr],
            Handle,
            "__RTS_FN_NODE_STRING_DECODER_NEW_ENC",
            "new StringDecoder(encoding: string)",
            symbols::__RTS_FN_NODE_STRING_DECODER_NEW_ENC as *const u8,
            true,
        ))
        .member(member(
            "write",
            InstanceMethod,
            vec![Handle, Handle],
            Handle,
            "__RTS_FN_NODE_STRING_DECODER_WRITE",
            "write(buffer: string): string",
            symbols::__RTS_FN_NODE_STRING_DECODER_WRITE as *const u8,
            false,
        ))
        .member(member(
            "end",
            InstanceMethod,
            vec![Handle],
            Handle,
            "__RTS_FN_NODE_STRING_DECODER_END",
            "end(): string",
            symbols::__RTS_FN_NODE_STRING_DECODER_END as *const u8,
            false,
        ))
        .member(member(
            "end",
            InstanceMethod,
            vec![Handle, Handle],
            Handle,
            "__RTS_FN_NODE_STRING_DECODER_END_BUF",
            "end(buffer: string): string",
            symbols::__RTS_FN_NODE_STRING_DECODER_END_BUF as *const u8,
            false,
        ))
        .member(member(
            "text",
            InstanceMethod,
            vec![Handle, Handle, AbiType::I64],
            Handle,
            "__RTS_FN_NODE_STRING_DECODER_TEXT",
            "text(buffer: object, offset: number): string",
            symbols::__RTS_FN_NODE_STRING_DECODER_TEXT as *const u8,
            false,
        ))
        .member(member(
            "encoding",
            InstanceGetter,
            vec![Handle],
            Handle,
            "__RTS_FN_NODE_STRING_DECODER_ENCODING",
            "encoding: string",
            symbols::__RTS_FN_NODE_STRING_DECODER_ENCODING as *const u8,
            false,
        ))
        .done();

    e.ns("node:string_decoder")
        .doc("Incremental byte→string decoder. Exports the StringDecoder class.")
        .done();
}
