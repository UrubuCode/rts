//! `time` namespace — ABI registration.

use crate::abi::{AbiType, MemberKind, NamespaceMember, NamespaceSpec};

pub const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "now_ms",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_TIME_NOW_MS",
        args: &[],
        returns: AbiType::I64,
        doc: "Monotonic milliseconds since process start.",
        ts_signature: "now_ms(): number",
        intrinsic: None,
    },
    NamespaceMember {
        name: "now_ns",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_TIME_NOW_NS",
        args: &[],
        returns: AbiType::I64,
        doc: "Monotonic nanoseconds since process start.",
        ts_signature: "now_ns(): number",
        intrinsic: None,
    },
    NamespaceMember {
        name: "unix_ms",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_TIME_UNIX_MS",
        args: &[],
        returns: AbiType::I64,
        doc: "Wall-clock milliseconds since the UNIX epoch.",
        ts_signature: "unix_ms(): number",
        intrinsic: None,
    },
    NamespaceMember {
        name: "unix_ns",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_TIME_UNIX_NS",
        args: &[],
        returns: AbiType::I64,
        doc: "Wall-clock nanoseconds since the UNIX epoch.",
        ts_signature: "unix_ns(): number",
        intrinsic: None,
    },
    NamespaceMember {
        name: "sleep_ms",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_TIME_SLEEP_MS",
        args: &[AbiType::I64],
        returns: AbiType::Void,
        doc: "Sleeps the current thread for `ms` milliseconds.",
        ts_signature: "sleep_ms(ms: number): void",
        intrinsic: None,
    },
    NamespaceMember {
        name: "sleep_ns",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_TIME_SLEEP_NS",
        args: &[AbiType::I64],
        returns: AbiType::Void,
        doc: "Sleeps the current thread for `ns` nanoseconds.",
        ts_signature: "sleep_ns(ns: number): void",
        intrinsic: None,
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "time",
    doc: "Monotonic and wall-clock timestamps, plus blocking sleeps.",
    members: MEMBERS,
};
