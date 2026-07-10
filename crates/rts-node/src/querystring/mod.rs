//! `node:querystring` — legacy query-string utilities. Registration only; the
//! extern "C" implementations live in `symbols.rs` (`promises.rs` is doc-only —
//! this namespace has no promise sub-API).

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

/// Registers the `node:querystring` surface into the engine Registry.
pub fn register(e: &mut Engine) {
    e.ns("node:querystring")
        .doc("Legacy query-string utilities (node:querystring).")
        .member(pure_func(
            "escape",
            "__RTS_FN_NODE_QUERYSTRING_ESCAPE",
            sig!(StrPtr => Handle),
            "escape(str: string): string",
            "Percent-encodes a string per encodeURIComponent semantics.",
            symbols::__RTS_FN_NODE_QUERYSTRING_ESCAPE as *const u8,
        ))
        .member(pure_func(
            "unescape",
            "__RTS_FN_NODE_QUERYSTRING_UNESCAPE",
            sig!(StrPtr => Handle),
            "unescape(str: string): string",
            "Percent-decodes a string, tolerant of malformed % sequences.",
            symbols::__RTS_FN_NODE_QUERYSTRING_UNESCAPE as *const u8,
        ))
        .done();
}
