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

use rts_engine::Engine;

/// Registers the `node:querystring` surface into the engine Registry.
pub fn register(e: &mut Engine) {
    e.module("node:querystring", |m| {
        m.doc(
            "Legacy query-string codec (node:querystring): parse/stringify \
             (+ decode/encode aliases) and escape/unescape. parse returns a real \
             object (repeated keys → arrays); stringify reads a real object.",
        );
        m.registry(parse::parse_entry());
        m.registry(parse::decode_entry());
        m.registry(stringify::stringify_entry());
        m.registry(stringify::encode_entry());
        m.registry(codec::qs_escape_entry());
        m.registry(codec::qs_unescape_entry());
    });
}
