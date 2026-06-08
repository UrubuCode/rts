//! `JSON5` global — JSON5 superset (comments, trailing commas, unquoted
//! keys, single-quote strings, hex, NaN/Infinity, etc.). Migrado ao modelo
//! `#[rts_namespace]` (stage 2c) via membros `external`: os símbolos
//! `__RTS_FN_NS_JSON_PARSE5`/`STRINGIFY` pertencem ao namespace `json`
//! (parse via crate `json5`; stringify reusa `JSON.stringify`).

#[allow(unused_imports)]
use rts_abi::ty::{Handle, Str, U64};
use rts_macro::rts_namespace;

/// Global JSON5 object — superset do JSON com comentarios, trailing commas, unquoted keys, single-quote strings, hex, NaN/Infinity.
#[rts_namespace(JSON5, sym = "NS_JSON")]
impl Json5Ns {
    /// Parses a JSON5 string (comments, trailing commas, unquoted keys, etc.). Returns opaque handle; 0 on error.
    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_JSON_PARSE5",
        ts = "parse(text: string): unknown"
    )]
    pub fn parse(_text: Str) -> U64 {
        unreachable!()
    }

    /// Serializes a JSON5 handle to compact JSON form (subset compativel).
    #[rts_fn(
        external,
        symbol = "__RTS_FN_NS_JSON_STRINGIFY",
        ts = "stringify(value: unknown): string"
    )]
    pub fn stringify(_value: U64) -> Handle {
        unreachable!()
    }
}
