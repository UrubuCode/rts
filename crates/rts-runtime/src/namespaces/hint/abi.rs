//! `hint` namespace — ABI registration.

use crate::abi::{AbiType, MemberKind, NamespaceMember, NamespaceSpec};

pub const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "spin_loop",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_HINT_SPIN_LOOP",
        args: &[],
        returns: AbiType::Void,
        doc: "Hint para spin-wait loop (PAUSE em x86, YIELD em ARM).",
        ts_signature: "spin_loop(): void",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "black_box_i64",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_HINT_BLACK_BOX_I64",
        args: &[AbiType::I64],
        returns: AbiType::I64,
        doc: "Opaque pra otimizador — impede que o valor seja eliminado.",
        ts_signature: "black_box_i64(value: number): number",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "black_box_f64",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_HINT_BLACK_BOX_F64",
        args: &[AbiType::F64],
        returns: AbiType::F64,
        doc: "Opaque pra otimizador (variante f64).",
        ts_signature: "black_box_f64(value: number): number",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "unreachable",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_HINT_UNREACHABLE",
        args: &[],
        returns: AbiType::Void,
        doc: "Marca codigo inalcancavel — em debug aborta, em release eh UB.",
        ts_signature: "unreachable(): never",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "assert_unchecked",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_HINT_ASSERT_UNCHECKED",
        args: &[AbiType::Bool],
        returns: AbiType::Void,
        doc: "Assume cond=true sem verificar. Cond falsa = UB em release.",
        ts_signature: "assert_unchecked(cond: boolean): void",
        intrinsic: None,
        pure: false,
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "hint",
    doc: "Performance hints (std::hint): spin_loop, black_box, unreachable, assert_unchecked.",
    members: MEMBERS,
};
