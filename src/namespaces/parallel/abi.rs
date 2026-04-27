//! `parallel` namespace — ABI registration.

use crate::abi::{AbiType, MemberKind, NamespaceMember, NamespaceSpec};

pub const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "map",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_PARALLEL_MAP",
        args: &[AbiType::Handle, AbiType::U64],
        returns: AbiType::Handle,
        doc: "Aplica `fn_ptr(x)` em paralelo sobre cada elemento do Vec<i64> `vec_handle`. Retorna novo Vec<i64> com os resultados. `fn_ptr` e `extern \"C\" fn(i64) -> i64`.",
        ts_signature: "map(vec_handle: number, fn_ptr: number): number",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "for_each",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_PARALLEL_FOR_EACH",
        args: &[AbiType::Handle, AbiType::U64],
        returns: AbiType::Void,
        doc: "Executa `fn_ptr(x)` em paralelo para cada elemento do Vec<i64> `vec_handle`. `fn_ptr` e `extern \"C\" fn(i64)`.",
        ts_signature: "for_each(vec_handle: number, fn_ptr: number): void",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "reduce",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_PARALLEL_REDUCE",
        args: &[AbiType::Handle, AbiType::I64, AbiType::U64],
        returns: AbiType::I64,
        doc: "Reduz Vec<i64> `vec_handle` com `fn_ptr(acc, x) -> acc` em paralelo (divide-e-conquista). `identity` e o elemento neutro da operacao (0 para soma, 1 para produto). `fn_ptr` deve ser associativo.",
        ts_signature: "reduce(vec_handle: number, identity: number, fn_ptr: number): number",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "num_threads",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_PARALLEL_NUM_THREADS",
        args: &[],
        returns: AbiType::I64,
        doc: "Retorna o numero de threads no pool Rayon global.",
        ts_signature: "num_threads(): number",
        intrinsic: None,
        pure: false,
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "parallel",
    doc: "Paralelismo de dados via Rayon (map/for_each/reduce sobre Vec<i64>).",
    members: MEMBERS,
};
