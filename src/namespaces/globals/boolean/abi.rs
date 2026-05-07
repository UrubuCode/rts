//! `Boolean` global class — ABI declarativa.

use crate::abi::{AbiType, GlobalClassSpec, MemberKind, NamespaceMember};

pub const MEMBERS: &[NamespaceMember] = &[
    // ── Static / coercion ────────────────────────────────────────────────────
    NamespaceMember {
        name: "coerce",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_GL_BOOLEAN_COERCE",
        args: &[AbiType::I64],
        returns: AbiType::I64,
        doc: "Coerces any value to boolean (truthy/falsy). 0 / NaN bits / undefined sentinel → false.",
        ts_signature: "coerce(value: any): boolean",
        intrinsic: None,
        pure: true,
    },
    // ── Instance ─────────────────────────────────────────────────────────────
    NamespaceMember {
        name: "toString",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_BOOLEAN_TO_STRING",
        args: &[AbiType::I64],
        returns: AbiType::Handle,
        doc: "Returns 'true' or 'false' string handle.",
        ts_signature: "toString(): string",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "valueOf",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_BOOLEAN_VALUE_OF",
        args: &[AbiType::I64],
        returns: AbiType::I64,
        doc: "Returns the underlying boolean value (0 or 1).",
        ts_signature: "valueOf(): boolean",
        intrinsic: None,
        pure: true,
    },
];

pub const BOOLEAN_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "Boolean",
    doc: "Built-in Boolean primitive (#208 #226). Boolean(x) coerces any value to boolean.",
    members: MEMBERS,
};
