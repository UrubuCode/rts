//! `node:punycode` — Registry registration for the Punycode (RFC 3492) + IDNA
//! ToASCII/ToUnicode surface. Implementations live in `symbols.rs`.

mod promises;
mod symbols;

use rts_engine::{sig, Engine, FnPtr, Member, MemberFlags, MemberKind};

fn pure_func(
    name: &str,
    symbol: &str,
    sig: rts_engine::Sig,
    ts: &str,
    doc: &str,
    fp: *const u8,
) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: true,
        intrinsic: None,
    }
}

/// Registers the `node:punycode` surface into the engine Registry.
pub fn register(e: &mut Engine) {
    e.ns("node:punycode")
        .doc("Punycode (RFC 3492) + IDNA ToASCII/ToUnicode (node:punycode).")
        .member(pure_func(
            "encode",
            "__RTS_FN_NODE_PUNYCODE_ENCODE",
            sig!(StrPtr => Handle),
            "encode(input: string): string",
            "Bootstring-encodes a Unicode string to Punycode.",
            symbols::__RTS_FN_NODE_PUNYCODE_ENCODE as *const u8,
        ))
        .member(pure_func(
            "decode",
            "__RTS_FN_NODE_PUNYCODE_DECODE",
            sig!(StrPtr => Handle),
            "decode(input: string): string",
            "Bootstring-decodes Punycode back to a Unicode string.",
            symbols::__RTS_FN_NODE_PUNYCODE_DECODE as *const u8,
        ))
        .member(pure_func(
            "toASCII",
            "__RTS_FN_NODE_PUNYCODE_TO_ASCII",
            sig!(StrPtr => Handle),
            "toASCII(domain: string): string",
            "Converts a domain to ASCII, ACE-encoding non-ASCII labels (xn--).",
            symbols::__RTS_FN_NODE_PUNYCODE_TO_ASCII as *const u8,
        ))
        .member(pure_func(
            "toUnicode",
            "__RTS_FN_NODE_PUNYCODE_TO_UNICODE",
            sig!(StrPtr => Handle),
            "toUnicode(domain: string): string",
            "Converts an ACE (xn--) domain back to Unicode.",
            symbols::__RTS_FN_NODE_PUNYCODE_TO_UNICODE as *const u8,
        ))
        .done();
}
