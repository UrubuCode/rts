//! `JSON` global namespace — maps `JSON.parse` / `JSON.stringify` to the
//! existing `json` namespace symbols. No new Rust code needed: same symbols,
//! JS-canonical names.

use crate::abi::{AbiType, MemberKind, NamespaceMember, NamespaceSpec};

pub const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "parse",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_JSON_PARSE",
        args: &[AbiType::StrPtr],
        returns: AbiType::U64,
        doc: "Parses a JSON string. Returns opaque handle; 0 on error.",
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
        doc: "Serializes a JSON handle to its compact string form.",
        ts_signature: "stringify(value: unknown): string",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "stringify_pretty",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_JSON_STRINGIFY_PRETTY",
        args: &[AbiType::U64, AbiType::I64],
        returns: AbiType::Handle,
        doc: "Pretty-printed serialization with `indent` spaces.",
        ts_signature: "stringify(value: unknown, _replacer: null, indent: number): string",
        intrinsic: None,
        pure: false,
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "JSON",
    doc: "Global JSON object — parse and stringify via RTS json namespace.",
    members: MEMBERS,
};
