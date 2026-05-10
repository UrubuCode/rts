//! `JSON5` global — JSON5 superset (comments, trailing commas, unquoted
//! keys, single-quote strings, hex, NaN/Infinity, etc.). Parse via crate
//! `json5`; stringify reusa `JSON.stringify` (saida valida em ambos).

use crate::abi::{AbiType, MemberKind, NamespaceMember, NamespaceSpec};

pub const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "parse",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_JSON_PARSE5",
        args: &[AbiType::StrPtr],
        returns: AbiType::U64,
        doc: "Parses a JSON5 string (comments, trailing commas, unquoted keys, etc.). Returns opaque handle; 0 on error.",
        ts_signature: "parse(text: string): unknown",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "stringify",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_JSON_STRINGIFY",
        args: &[AbiType::U64],
        returns: AbiType::Handle,
        doc: "Serializes a JSON5 handle to compact JSON form (subset compativel).",
        ts_signature: "stringify(value: unknown): string",
        intrinsic: None,
        pure: false,
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "JSON5",
    doc: "Global JSON5 object — superset do JSON com comentarios, trailing commas, unquoted keys, single-quote strings, hex, NaN/Infinity.",
    members: MEMBERS,
};
