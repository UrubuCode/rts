//! `RegExp` global class — ABI registration as `GlobalClassSpec`.

use crate::abi::{AbiType, GlobalClassSpec, MemberKind, NamespaceMember};

pub const MEMBERS: &[NamespaceMember] = &[
    // ── Constructors ──────────────────────────────────────────────────────────
    NamespaceMember {
        name: "new",
        kind: MemberKind::Constructor,
        symbol: "__RTS_FN_GL_REGEXP_NEW",
        args: &[AbiType::StrPtr],
        returns: AbiType::Handle,
        doc: "Creates a RegExp from a pattern string.",
        ts_signature: "new RegExp(pattern: string): RegExp",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "new_with_flags",
        kind: MemberKind::Constructor,
        symbol: "__RTS_FN_GL_REGEXP_NEW_WITH_FLAGS",
        args: &[AbiType::StrPtr, AbiType::StrPtr],
        returns: AbiType::Handle,
        doc: "Creates a RegExp from a pattern string and flags (e.g. 'gi').",
        ts_signature: "new RegExp(pattern: string, flags: string): RegExp",
        intrinsic: None,
        pure: false,
    },
    // ── Instance methods ──────────────────────────────────────────────────────
    NamespaceMember {
        name: "test",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_REGEXP_TEST",
        args: &[AbiType::Handle, AbiType::StrPtr],
        returns: AbiType::Bool,
        doc: "Tests whether the pattern matches the string. Returns true/false.",
        ts_signature: "test(str: string): boolean",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "exec",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_REGEXP_EXEC",
        args: &[AbiType::Handle, AbiType::StrPtr],
        returns: AbiType::Handle,
        doc: "Returns the first match as a string handle, or 0 if no match.",
        ts_signature: "exec(str: string): string | null",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "source",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_REGEXP_SOURCE",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Returns the pattern source string.",
        ts_signature: "source: string",
        intrinsic: None,
        pure: true,
    },
];

pub const CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "RegExp",
    doc: "Built-in RegExp class. Backed by the Rust `regex` crate (RE2 semantics).",
    members: MEMBERS,
};
