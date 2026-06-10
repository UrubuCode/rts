//! `JSON` global namespace — maps `JSON.parse` / `JSON.stringify` to the
//! existing `json` namespace symbols. No new Rust code needed: same symbols,
//! JS-canonical names. Migrado ao modelo `#[rts_namespace]` (stage 2c) via
//! membros `external` apontando para `__RTS_FN_NS_JSON_*`.

#[allow(unused_imports)]
use rts_engine::abi::ty::{Handle, Str, I64, U64};
use rts_macro::rts_namespace;

/// Global JSON object — parse and stringify via RTS json namespace.
#[rts_namespace(JSON, sym = "NS_JSON")]
impl JsonNs {
    /// Parses a JSON string. Returns opaque handle; 0 on error.
    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_JSON_PARSE",
        ambiguous_ret,
        ts = "parse(text: string): unknown"
    )]
    pub fn parse(_text: Str) -> U64 {
        unreachable!()
    }

    /// Serializes a JSON handle to its compact string form.
    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_JSON_STRINGIFY",
        ts = "stringify(value: unknown): string"
    )]
    pub fn stringify(_value: U64) -> Handle {
        unreachable!()
    }

    /// Pretty-printed serialization with `indent` spaces.
    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_JSON_STRINGIFY_PRETTY",
        ts = "stringify(value: unknown, _replacer: null, indent: number): string"
    )]
    pub fn stringify_pretty(_value: U64, _indent: I64) -> Handle {
        unreachable!()
    }
}
