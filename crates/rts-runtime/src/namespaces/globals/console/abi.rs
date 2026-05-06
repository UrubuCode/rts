//! `console` global namespace — variadic print methods.
//!
//! These members are listed for type-checking and `rts apis` output.
//! Codegen still special-cases `console.*` because the methods are variadic
//! (arbitrary number of args of any type) which cannot be expressed in the
//! fixed `AbiType[]` ABI. The `symbol` fields point to `io.*` targets that
//! codegen emits directly after concatenating all args into a single string.

use crate::abi::{AbiType, MemberKind, NamespaceMember, NamespaceSpec};

pub const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "log",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_IO_PRINT",
        args: &[AbiType::StrPtr],
        doc: "Prints args separated by spaces to stdout.",
        ts_signature: "log(...args: unknown[]): void",
        returns: AbiType::Void,
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "info",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_IO_PRINT",
        args: &[AbiType::StrPtr],
        doc: "Alias for console.log.",
        ts_signature: "info(...args: unknown[]): void",
        returns: AbiType::Void,
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "debug",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_IO_PRINT",
        args: &[AbiType::StrPtr],
        doc: "Alias for console.log.",
        ts_signature: "debug(...args: unknown[]): void",
        returns: AbiType::Void,
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "error",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_IO_EPRINT",
        args: &[AbiType::StrPtr],
        doc: "Prints args separated by spaces to stderr.",
        ts_signature: "error(...args: unknown[]): void",
        returns: AbiType::Void,
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "warn",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_IO_EPRINT",
        args: &[AbiType::StrPtr],
        doc: "Alias for console.error.",
        ts_signature: "warn(...args: unknown[]): void",
        returns: AbiType::Void,
        intrinsic: None,
        pure: false,
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "console",
    doc: "Global console object — variadic print to stdout/stderr.",
    members: MEMBERS,
};
