//! `node:querystring` — legacy query-string utilities. Registration only; the
//! extern "C" implementations live in `symbols.rs` (`promises.rs` is doc-only —
//! this namespace has no promise sub-API).
//!
//! **Deferred:** Node's `parse(str, sep, eq, options)` / `stringify(obj, sep,
//! eq, options)` optional 2nd-4th arguments (custom separator/equals strings,
//! a custom `encodeURIComponent`/`decodeURIComponent`, `maxKeys`) — this slice
//! only registers the fixed 1-arg forms (`&`/`=`, the built-in percent-codec,
//! no key cap), matching every other member's fixed-arity convention here.

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
        .doc(
            "Legacy query-string utilities (node:querystring). parse()/stringify() \
             round-trip through the engine's REAL shaped-object representation \
             (rts_engine::heap::shapes) — a parsed object is a genuine JS object, \
             and stringify reads back any genuine JS object a caller passes in.",
        )
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
        .member(pure_func(
            "parse",
            "__RTS_FN_NODE_QUERYSTRING_PARSE",
            sig!(StrPtr => Handle),
            "parse(str: string): object",
            "Parses 'a=1&b=2' into { a: '1', b: '2' }. Splits on '&', each pair on \
             the first '=', percent-decodes both key and value and converts '+' to \
             a space. A repeated key collects into a string[] value (parse('a=1&a=2') \
             -> { a: ['1', '2'] }).",
            symbols::__RTS_FN_NODE_QUERYSTRING_PARSE as *const u8,
        ))
        .member(pure_func(
            "stringify",
            "__RTS_FN_NODE_QUERYSTRING_STRINGIFY",
            sig!(Handle => Handle),
            "stringify(obj: object): string",
            "The inverse of parse(): joins 'key=value' pairs with '&', percent- \
             encoding both. An array value is flattened to one pair per element \
             (empty array -> no pair for that key); a non-primitive/non-finite \
             value (null, undefined, NaN, a nested object) renders as an empty \
             value ('key='), matching Node's real stringifyPrimitive fallback.",
            symbols::__RTS_FN_NODE_QUERYSTRING_STRINGIFY as *const u8,
        ))
        .done();
}
