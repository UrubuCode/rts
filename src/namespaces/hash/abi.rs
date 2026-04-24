//! `hash` namespace — ABI registration.

use crate::abi::{AbiType, MemberKind, NamespaceMember, NamespaceSpec};

pub const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "hash_str",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_HASH_HASH_STR",
        args: &[AbiType::StrPtr],
        returns: AbiType::I64,
        doc: "SipHash de uma string UTF-8.",
        ts_signature: "hash_str(s: string): number",
        intrinsic: None,
    },
    NamespaceMember {
        name: "hash_bytes",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_HASH_HASH_BYTES",
        args: &[AbiType::I64, AbiType::I64],
        returns: AbiType::I64,
        doc: "SipHash de uma regiao de memoria (ptr + len). Use com buffer.ptr(handle).",
        ts_signature: "hash_bytes(ptr: number, len: number): number",
        intrinsic: None,
    },
    NamespaceMember {
        name: "hash_i64",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_HASH_HASH_I64",
        args: &[AbiType::I64],
        returns: AbiType::I64,
        doc: "SipHash de um inteiro de 64 bits.",
        ts_signature: "hash_i64(value: number): number",
        intrinsic: None,
    },
    NamespaceMember {
        name: "hash_combine",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_HASH_HASH_COMBINE",
        args: &[AbiType::I64, AbiType::I64],
        returns: AbiType::I64,
        doc: "Combina dois hashes preservando entropia (estilo boost::hash_combine).",
        ts_signature: "hash_combine(h1: number, h2: number): number",
        intrinsic: None,
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "hash",
    doc: "Non-cryptographic hashing via std::hash::DefaultHasher (SipHash-1-3).",
    members: MEMBERS,
};
