//! `node:punycode` — full Node-25 surface (deprecated DEP0040, implemented for
//! drop-in compat). Native RFC 3492 Bootstring core + IDN domain wrappers +
//! the `ucs2` UTF-16⇄code-point helpers. No stubs.
//!
//! `ucs2` is a real object whose `decode`/`encode` are callable `Entry::Function`
//! values (word-ABI natives) — so `punycode.ucs2.decode(...)` dispatches like
//! any method, no `.ts` shim needed. Module layout: `bootstring` (RFC 3492),
//! `core` (label/domain layer), `words` (value helpers), `symbols` (externs).

mod bootstring;
mod core;
mod symbols;
mod words;

use rts_engine::{sig, Engine, FnPtr, Member, MemberFlags, MemberKind};

fn throws_func(name: &str, symbol: &str, sig: rts_engine::Sig, ts: &str, fp: *const u8) -> Member {
    make(name, symbol, sig, ts, fp, MemberKind::Function, MemberFlags::THROWS)
}

fn constant(name: &str, symbol: &str, ts: &str, fp: *const u8) -> Member {
    make(name, symbol, sig!(=> Handle), ts, fp, MemberKind::Constant, MemberFlags::NONE)
}

fn make(
    name: &str,
    symbol: &str,
    sig: rts_engine::Sig,
    ts: &str,
    fp: *const u8,
    kind: MemberKind,
    flags: MemberFlags,
) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: String::new(),
        ret_class: None,
        pure: false,
        emit: None,
    }
}

/// Registers the `node:punycode` surface into the engine Registry.
pub fn register(e: &mut Engine) {
    e.ns("node:punycode")
        .doc(
            "RFC 3492 Punycode/IDN transcoder (node:punycode, deprecated): \
             decode/encode (single label), toASCII/toUnicode (domains), the \
             ucs2 UTF-16⇄code-point helpers, and version.",
        )
        .member(throws_func(
            "decode",
            "__RTS_FN_NODE_PUNYCODE_DECODE",
            sig!(StrPtr => Handle),
            "decode(string: string): string",
            symbols::__RTS_FN_NODE_PUNYCODE_DECODE as *const u8,
        ))
        .member(throws_func(
            "encode",
            "__RTS_FN_NODE_PUNYCODE_ENCODE",
            sig!(StrPtr => Handle),
            "encode(string: string): string",
            symbols::__RTS_FN_NODE_PUNYCODE_ENCODE as *const u8,
        ))
        .member(throws_func(
            "toASCII",
            "__RTS_FN_NODE_PUNYCODE_TOASCII",
            sig!(StrPtr => Handle),
            "toASCII(domain: string): string",
            symbols::__RTS_FN_NODE_PUNYCODE_TOASCII as *const u8,
        ))
        .member(throws_func(
            "toUnicode",
            "__RTS_FN_NODE_PUNYCODE_TOUNICODE",
            sig!(StrPtr => Handle),
            "toUnicode(domain: string): string",
            symbols::__RTS_FN_NODE_PUNYCODE_TOUNICODE as *const u8,
        ))
        .member(constant(
            "version",
            "__RTS_FN_NODE_PUNYCODE_VERSION",
            "version: string",
            symbols::__RTS_FN_NODE_PUNYCODE_VERSION as *const u8,
        ))
        .member(constant(
            "ucs2",
            "__RTS_FN_NODE_PUNYCODE_UCS2",
            "ucs2: object",
            symbols::__RTS_FN_NODE_PUNYCODE_UCS2 as *const u8,
        ))
        .done();
}
