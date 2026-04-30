use crate::abi::{AbiType, MemberKind, NamespaceMember, NamespaceSpec};

pub const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "now",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_GL_PERF_NOW",
        args: &[],
        returns: AbiType::F64,
        doc: "performance.now() — tempo monotônico em milissegundos (precisão sub-ms).",
        ts_signature: "now(): number",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "timeOrigin",
        kind: MemberKind::Constant,
        symbol: "__RTS_FN_GL_PERF_TIME_ORIGIN",
        args: &[],
        returns: AbiType::F64,
        doc: "performance.timeOrigin — Unix timestamp em ms do início do processo.",
        ts_signature: "timeOrigin: number",
        intrinsic: None,
        pure: true,
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "performance",
    doc: "performance.now() / performance.timeOrigin — alias de time.now_ms com precisão sub-ms.",
    members: MEMBERS,
};
