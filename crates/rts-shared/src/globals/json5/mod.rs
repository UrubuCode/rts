//! `JSON5` global — JSON5 superset (comments, trailing commas, unquoted
//! keys, single-quote strings, hex, NaN/Infinity, etc.). Migrado do
//! `#[rts_namespace]` pro modelo builder hand-written do `rts-engine` (rumo à
//! remoção da `rts-macro`) via membros `external`: os símbolos
//! `__RTS_FN_NS_JSON_PARSE5`/`STRINGIFY` pertencem ao namespace `json`
//! (parse via crate `json5`; stringify reusa `JSON.stringify`); `fn_ptr` null.

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

/// Registra a namespace `JSON5` no motor (hand-written, sem macro). Todos os
/// membros são `external` — os externs pertencem ao namespace `json`.
pub fn register(e: &mut Engine) {
    e.ns("JSON5")
        .doc("Global JSON5 object — superset do JSON com comentarios, trailing commas, unquoted keys, single-quote strings, hex, NaN/Infinity.")
        .member(Member {
            name: "parse".to_string(),
            kind: MemberKind::Function,
            sig: Sig::new(vec![AbiType::StrPtr], AbiType::U64),
            symbol: "__RTS_FN_NS_JSON_PARSE5".to_string(),
            fn_ptr: FnPtr(core::ptr::null::<u8>()),
            flags: MemberFlags::AMBIGUOUS_RET,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: "parse(text: string): unknown".to_string(),
            doc: "Parses a JSON5 string (comments, trailing commas, unquoted keys, etc.). Returns opaque handle; 0 on error.".to_string(),
            pure: false,
            intrinsic: None,
            emit: None,
        })
        .member(Member {
            name: "stringify".to_string(),
            kind: MemberKind::Function,
            sig: Sig::new(vec![AbiType::U64], AbiType::Handle),
            symbol: "__RTS_FN_NS_JSON_STRINGIFY".to_string(),
            fn_ptr: FnPtr(core::ptr::null::<u8>()),
            flags: MemberFlags::NONE,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: "stringify(value: unknown): string".to_string(),
            doc: "Serializes a JSON5 handle to compact JSON form (subset compativel).".to_string(),
            pure: false,
            intrinsic: None,
            emit: None,
        })
        .done();
}
