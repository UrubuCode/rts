//! `JSON` global namespace — maps `JSON.parse` / `JSON.stringify` to the
//! existing `json` namespace symbols. No new Rust code needed: same symbols,
//! JS-canonical names. Migrado do `#[rts_namespace]` pro modelo builder
//! hand-written do `rts-engine` (rumo à remoção da `rts-macro`) via membros
//! `external` apontando para `__RTS_FN_NS_JSON_*` (`fn_ptr` null).

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

/// Registra a namespace `JSON` no motor (hand-written, sem macro). Todos os
/// membros são `external` — os externs pertencem ao namespace `json`.
pub fn register(e: &mut Engine) {
    e.ns("JSON")
        .doc("Global JSON object — parse and stringify via RTS json namespace.")
        .member(Member {
            name: "parse".to_string(),
            kind: MemberKind::Function,
            sig: Sig::new(vec![AbiType::StrPtr], AbiType::U64),
            symbol: "__RTS_FN_NS_JSON_PARSE".to_string(),
            fn_ptr: FnPtr(core::ptr::null::<u8>()),
            flags: MemberFlags::AMBIGUOUS_RET,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: "parse(text: string): unknown".to_string(),
            doc: "Parses a JSON string. Returns opaque handle; 0 on error.".to_string(),
            pure: false,
            intrinsic: None,
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
            doc: "Serializes a JSON handle to its compact string form.".to_string(),
            pure: false,
            intrinsic: None,
        })
        .member(Member {
            name: "stringify_pretty".to_string(),
            kind: MemberKind::Function,
            sig: Sig::new(vec![AbiType::U64, AbiType::I64], AbiType::Handle),
            symbol: "__RTS_FN_NS_JSON_STRINGIFY_PRETTY".to_string(),
            fn_ptr: FnPtr(core::ptr::null::<u8>()),
            flags: MemberFlags::NONE,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: "stringify(value: unknown, _replacer: null, indent: number): string"
                .to_string(),
            doc: "Pretty-printed serialization with `indent` spaces.".to_string(),
            pure: false,
            intrinsic: None,
        })
        .done();
}
