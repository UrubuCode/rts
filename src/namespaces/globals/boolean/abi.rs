//! Boolean — ABI registration: GlobalClassSpec (JS Boolean class).
//!
//! Metodos de instancia: toString() -> string, valueOf() -> bool.

use crate::abi::{AbiType, MemberKind, NamespaceMember};
use crate::abi::global_class::GlobalClassSpec;

pub const BOOLEAN_CLASS_MEMBERS: &[NamespaceMember] = &[
    // ── Constructors ─────────────────────────────────────────────────────
    NamespaceMember {
        name: "new",
        kind: MemberKind::Constructor,
        symbol: "__RTS_FN_GL_BOOLEAN_NEW",
        args: &[AbiType::Bool],
        returns: AbiType::Bool,
        doc: "Boolean(value) — coerce value to boolean.",
        ts_signature: "new(value: boolean): boolean",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "new",
        kind: MemberKind::Constructor,
        symbol: "__RTS_FN_GL_BOOLEAN_NEW_EMPTY",
        args: &[],
        returns: AbiType::Bool,
        doc: "Boolean() — returns false.",
        ts_signature: "new(): boolean",
        intrinsic: None,
        pure: true,
    },
    // ── Instance methods ──────────────────────────────────────────────────
    NamespaceMember {
        name: "toString",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_BOOLEAN_TO_STRING",
        args: &[AbiType::Bool],
        returns: AbiType::Handle,
        doc: "Returns \"true\" or \"false\" as a string handle.",
        ts_signature: "toString(): string",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "valueOf",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_BOOLEAN_VALUE_OF",
        args: &[AbiType::Bool],
        returns: AbiType::Bool,
        doc: "Returns the primitive boolean value.",
        ts_signature: "valueOf(): boolean",
        intrinsic: None,
        pure: true,
    },
];

pub const BOOLEAN_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "Boolean",
    doc: "JS Boolean primitive class — Boolean(value), b.toString(), b.valueOf().",
    members: BOOLEAN_CLASS_MEMBERS,
};
