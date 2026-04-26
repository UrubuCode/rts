//! `thread` namespace — ABI registration.

use crate::abi::{AbiType, MemberKind, NamespaceMember, NamespaceSpec};

pub const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "spawn",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_THREAD_SPAWN",
        args: &[AbiType::U64, AbiType::U64],
        returns: AbiType::Handle,
        doc: "Cria uma nova thread executando `fn_ptr(arg)`. `fn_ptr` e um ponteiro para `extern \"C\" fn(u64) -> u64`. Retorna handle do JoinHandle, 0 em falha.",
        ts_signature: "spawn(fn_ptr: number, arg: number): number",
        intrinsic: None,
    },
    NamespaceMember {
        name: "join",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_THREAD_JOIN",
        args: &[AbiType::U64],
        returns: AbiType::U64,
        doc: "Aguarda a thread terminar e retorna o valor retornado por ela. Consome o handle. 0 se handle invalido ou a thread fez panic.",
        ts_signature: "join(thread: number): number",
        intrinsic: None,
    },
    NamespaceMember {
        name: "detach",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_THREAD_DETACH",
        args: &[AbiType::U64],
        returns: AbiType::Void,
        doc: "Libera o JoinHandle sem aguardar. A thread continua rodando ate completar.",
        ts_signature: "detach(thread: number): void",
        intrinsic: None,
    },
    NamespaceMember {
        name: "id",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_THREAD_ID",
        args: &[],
        returns: AbiType::U64,
        doc: "Id da thread atual (estavel por thread, atribuido na primeira chamada). Sempre != 0.",
        ts_signature: "id(): number",
        intrinsic: None,
    },
    NamespaceMember {
        name: "sleep_ms",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_THREAD_SLEEP_MS",
        args: &[AbiType::I64],
        returns: AbiType::Void,
        doc: "Pausa a thread atual por `ms` milissegundos. Valores negativos sao tratados como 0.",
        ts_signature: "sleep_ms(ms: number): void",
        intrinsic: None,
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "thread",
    doc: "Primitivas de threads (spawn/join/detach/id/sleep) baseadas em std::thread.",
    members: MEMBERS,
};
