//! `node:querystring` — the legacy query-string codec, full Node-25 surface.
//!
//! Real, native, no stubs: `parse`/`stringify` (+ their `decode`/`encode`
//! aliases) and `escape`/`unescape`. `parse` returns a genuine JS object
//! (repeated keys → arrays); `stringify` reads a genuine JS object argument
//! (via the engine value model, no JSON round-trip). Optional `sep`/`eq`
//! separators resolve by arity (1 vs 3 args) — no shim needed.
//!
//! Module layout: `codec` (percent escape/unescape + shared string helpers),
//! `parse` (→ object), `stringify` (object →), `words` (value-word build/decode).

mod codec;
mod parse;
mod stringify;
mod words;

use rts_engine::{sig, Engine, FnPtr, Member, MemberFlags, MemberKind};

fn pure_func(name: &str, symbol: &str, sig: rts_engine::Sig, ts: &str, fp: *const u8) -> Member {
    make(name, symbol, sig, ts, fp, true)
}

fn func(name: &str, symbol: &str, sig: rts_engine::Sig, ts: &str, fp: *const u8) -> Member {
    make(name, symbol, sig, ts, fp, false)
}

fn make(
    name: &str,
    symbol: &str,
    sig: rts_engine::Sig,
    ts: &str,
    fp: *const u8,
    pure: bool,
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
        doc: String::new(),
        pure,
        intrinsic: None,
    }
}

/// Registers the `node:querystring` surface into the engine Registry.
pub fn register(e: &mut Engine) {
    let mut ns = e.ns("node:querystring");
    ns = ns.doc(
        "Legacy query-string codec (node:querystring): parse/stringify \
         (+ decode/encode aliases) and escape/unescape. parse returns a real \
         object (repeated keys → arrays); stringify reads a real object.",
    );

    // parse / decode — string → object (default and explicit sep/eq).
    for name in ["parse", "decode"] {
        ns = ns
            .member(func(
                name,
                "__RTS_FN_NODE_QUERYSTRING_PARSE",
                sig!(StrPtr => Handle),
                "parse(str: string): object",
                parse::__RTS_FN_NODE_QUERYSTRING_PARSE as *const u8,
            ))
            .member(func(
                name,
                "__RTS_FN_NODE_QUERYSTRING_PARSE_SEP",
                sig!(StrPtr, StrPtr, StrPtr => Handle),
                "parse(str: string, sep: string, eq: string): object",
                parse::__RTS_FN_NODE_QUERYSTRING_PARSE_SEP as *const u8,
            ));
    }

    // stringify / encode — object → string (default and explicit sep/eq).
    for name in ["stringify", "encode"] {
        ns = ns
            .member(func(
                name,
                "__RTS_FN_NODE_QUERYSTRING_STRINGIFY",
                sig!(Handle => Handle),
                "stringify(obj: object): string",
                stringify::__RTS_FN_NODE_QUERYSTRING_STRINGIFY as *const u8,
            ))
            .member(func(
                name,
                "__RTS_FN_NODE_QUERYSTRING_STRINGIFY_SEP",
                sig!(Handle, StrPtr, StrPtr => Handle),
                "stringify(obj: object, sep: string, eq: string): string",
                stringify::__RTS_FN_NODE_QUERYSTRING_STRINGIFY_SEP as *const u8,
            ));
    }

    ns.member(pure_func(
        "escape",
        "__RTS_FN_NODE_QUERYSTRING_ESCAPE",
        sig!(StrPtr => Handle),
        "escape(str: string): string",
        codec::__RTS_FN_NODE_QUERYSTRING_ESCAPE as *const u8,
    ))
    .member(pure_func(
        "unescape",
        "__RTS_FN_NODE_QUERYSTRING_UNESCAPE",
        sig!(StrPtr => Handle),
        "unescape(str: string): string",
        codec::__RTS_FN_NODE_QUERYSTRING_UNESCAPE as *const u8,
    ))
    .done();
}
