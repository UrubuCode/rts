//! `process` namespace — ABI registration.

use crate::abi::{AbiType, MemberKind, NamespaceMember, NamespaceSpec};

pub const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "exit",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_PROCESS_EXIT",
        args: &[AbiType::I32],
        returns: AbiType::Void,
        doc: "Termina o processo corrente com o exit code dado.",
        ts_signature: "exit(code: number): void",
        intrinsic: None,
    },
    NamespaceMember {
        name: "abort",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_PROCESS_ABORT",
        args: &[],
        returns: AbiType::Void,
        doc: "Aborta o processo imediatamente (sem unwind).",
        ts_signature: "abort(): void",
        intrinsic: None,
    },
    NamespaceMember {
        name: "pid",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_PROCESS_PID",
        args: &[],
        returns: AbiType::I64,
        doc: "PID do processo corrente.",
        ts_signature: "pid(): number",
        intrinsic: None,
    },
    NamespaceMember {
        name: "spawn",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_PROCESS_SPAWN",
        args: &[AbiType::StrPtr, AbiType::StrPtr],
        returns: AbiType::Handle,
        doc: "Dispara `cmd` com argumentos separados por \\n. Retorna handle do filho, ou 0 em falha.",
        ts_signature: "spawn(cmd: string, args_newline_separated: string): number",
        intrinsic: None,
    },
    NamespaceMember {
        name: "wait",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_PROCESS_WAIT",
        args: &[AbiType::U64],
        returns: AbiType::I32,
        doc: "Aguarda o filho terminar e retorna o exit code. Consome o handle.",
        ts_signature: "wait(child: number): number",
        intrinsic: None,
    },
    NamespaceMember {
        name: "kill",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_PROCESS_KILL",
        args: &[AbiType::U64],
        returns: AbiType::I64,
        doc: "Mata o processo filho. 0 em sucesso, -1 em erro.",
        ts_signature: "kill(child: number): number",
        intrinsic: None,
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "process",
    doc: "Process control: exit/abort, pid, spawn/wait/kill children.",
    members: MEMBERS,
};
