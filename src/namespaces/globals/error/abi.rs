//! `Error` global class family — ABI registration as `GlobalClassSpec`.
//!
//! Each error type (Error, TypeError, RangeError, ReferenceError, SyntaxError)
//! shares the same instance layout and methods; only the `.name` field differs.

use crate::abi::{AbiType, GlobalClassSpec, MemberKind, NamespaceMember};

// ── Error ─────────────────────────────────────────────────────────────────────

pub const ERROR_MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "new",
        kind: MemberKind::Constructor,
        symbol: "__RTS_FN_GL_ERROR_NEW",
        args: &[AbiType::StrPtr],
        returns: AbiType::Handle,
        doc: "Creates an Error with a message string.",
        ts_signature: "new Error(message?: string): Error",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "message",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_ERROR_MESSAGE",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "The error message string.",
        ts_signature: "message: string",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "name",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_ERROR_NAME",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "The error name (\"Error\", \"TypeError\", etc.).",
        ts_signature: "name: string",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "toString",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_ERROR_TO_STRING",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Returns \"<name>: <message>\".",
        ts_signature: "toString(): string",
        intrinsic: None,
        pure: true,
    },
];

pub const CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "Error",
    doc: "Built-in Error class.",
    members: ERROR_MEMBERS,
};

// ── TypeError ─────────────────────────────────────────────────────────────────

pub const TYPE_ERROR_MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "new",
        kind: MemberKind::Constructor,
        symbol: "__RTS_FN_GL_TYPE_ERROR_NEW",
        args: &[AbiType::StrPtr],
        returns: AbiType::Handle,
        doc: "Creates a TypeError with a message string.",
        ts_signature: "new TypeError(message?: string): TypeError",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "message",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_ERROR_MESSAGE",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "The error message string.",
        ts_signature: "message: string",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "name",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_ERROR_NAME",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "The error name (\"TypeError\").",
        ts_signature: "name: string",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "toString",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_ERROR_TO_STRING",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Returns \"TypeError: <message>\".",
        ts_signature: "toString(): string",
        intrinsic: None,
        pure: true,
    },
];

pub const TYPE_ERROR_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "TypeError",
    doc: "Built-in TypeError class.",
    members: TYPE_ERROR_MEMBERS,
};

// ── RangeError ────────────────────────────────────────────────────────────────

pub const RANGE_ERROR_MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "new",
        kind: MemberKind::Constructor,
        symbol: "__RTS_FN_GL_RANGE_ERROR_NEW",
        args: &[AbiType::StrPtr],
        returns: AbiType::Handle,
        doc: "Creates a RangeError with a message string.",
        ts_signature: "new RangeError(message?: string): RangeError",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "message",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_ERROR_MESSAGE",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "The error message string.",
        ts_signature: "message: string",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "name",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_ERROR_NAME",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "The error name (\"RangeError\").",
        ts_signature: "name: string",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "toString",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_ERROR_TO_STRING",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Returns \"RangeError: <message>\".",
        ts_signature: "toString(): string",
        intrinsic: None,
        pure: true,
    },
];

pub const RANGE_ERROR_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "RangeError",
    doc: "Built-in RangeError class.",
    members: RANGE_ERROR_MEMBERS,
};

// ── ReferenceError ────────────────────────────────────────────────────────────

pub const REF_ERROR_MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "new",
        kind: MemberKind::Constructor,
        symbol: "__RTS_FN_GL_REF_ERROR_NEW",
        args: &[AbiType::StrPtr],
        returns: AbiType::Handle,
        doc: "Creates a ReferenceError with a message string.",
        ts_signature: "new ReferenceError(message?: string): ReferenceError",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "message",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_ERROR_MESSAGE",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "The error message string.",
        ts_signature: "message: string",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "name",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_ERROR_NAME",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "The error name (\"ReferenceError\").",
        ts_signature: "name: string",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "toString",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_ERROR_TO_STRING",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Returns \"ReferenceError: <message>\".",
        ts_signature: "toString(): string",
        intrinsic: None,
        pure: true,
    },
];

pub const REF_ERROR_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "ReferenceError",
    doc: "Built-in ReferenceError class.",
    members: REF_ERROR_MEMBERS,
};

// ── SyntaxError ───────────────────────────────────────────────────────────────

pub const SYNTAX_ERROR_MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "new",
        kind: MemberKind::Constructor,
        symbol: "__RTS_FN_GL_SYNTAX_ERROR_NEW",
        args: &[AbiType::StrPtr],
        returns: AbiType::Handle,
        doc: "Creates a SyntaxError with a message string.",
        ts_signature: "new SyntaxError(message?: string): SyntaxError",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "message",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_ERROR_MESSAGE",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "The error message string.",
        ts_signature: "message: string",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "name",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_ERROR_NAME",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "The error name (\"SyntaxError\").",
        ts_signature: "name: string",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "toString",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_ERROR_TO_STRING",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Returns \"SyntaxError: <message>\".",
        ts_signature: "toString(): string",
        intrinsic: None,
        pure: true,
    },
];

pub const SYNTAX_ERROR_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "SyntaxError",
    doc: "Built-in SyntaxError class.",
    members: SYNTAX_ERROR_MEMBERS,
};
