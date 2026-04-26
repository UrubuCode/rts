//! `backtrace` namespace — ABI registration.

use crate::abi::{AbiType, MemberKind, NamespaceMember, NamespaceSpec};

pub const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "capture",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_BACKTRACE_CAPTURE",
        args: &[],
        returns: AbiType::Handle,
        doc: "Captura backtrace do call stack atual. Retorna handle.",
        ts_signature: "capture(): number",
        intrinsic: None,
    },
    NamespaceMember {
        name: "capture_if_enabled",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_BACKTRACE_CAPTURE_IF_ENABLED",
        args: &[],
        returns: AbiType::Handle,
        doc: "Captura backtrace se RUST_BACKTRACE estiver set; retorna 0 caso contrario.",
        ts_signature: "capture_if_enabled(): number",
        intrinsic: None,
    },
    NamespaceMember {
        name: "is_enabled",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_BACKTRACE_IS_ENABLED",
        args: &[],
        returns: AbiType::Bool,
        doc: "True se RUST_BACKTRACE=1 (ou full) esta no env.",
        ts_signature: "is_enabled(): boolean",
        intrinsic: None,
    },
    NamespaceMember {
        name: "to_string",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_BACKTRACE_TO_STRING",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Formata o backtrace em string. Retorna handle de string GC.",
        ts_signature: "to_string(handle: number): number",
        intrinsic: None,
    },
    NamespaceMember {
        name: "free",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_BACKTRACE_FREE",
        args: &[AbiType::Handle],
        returns: AbiType::Void,
        doc: "Libera a backtrace.",
        ts_signature: "free(handle: number): void",
        intrinsic: None,
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "backtrace",
    doc: "Captura de stack traces via std::backtrace::Backtrace.",
    members: MEMBERS,
};
