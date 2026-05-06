//! `alloc` namespace — ABI registration.

use crate::abi::{AbiType, MemberKind, NamespaceMember, NamespaceSpec};

pub const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "alloc",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_ALLOC_ALLOC",
        args: &[AbiType::I64, AbiType::I64],
        returns: AbiType::I64,
        doc: "Aloca size bytes alinhados a `align`. Retorna ponteiro ou 0 em falha.",
        ts_signature: "alloc(size: number, align: number): number",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "alloc_zeroed",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_ALLOC_ALLOC_ZEROED",
        args: &[AbiType::I64, AbiType::I64],
        returns: AbiType::I64,
        doc: "Aloca size bytes zerados, alinhados a `align`.",
        ts_signature: "alloc_zeroed(size: number, align: number): number",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "dealloc",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_ALLOC_DEALLOC",
        args: &[AbiType::I64, AbiType::I64, AbiType::I64],
        returns: AbiType::Void,
        doc: "Libera ptr previamente alocado com mesmo size/align.",
        ts_signature: "dealloc(ptr: number, size: number, align: number): void",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "realloc",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_ALLOC_REALLOC",
        args: &[AbiType::I64, AbiType::I64, AbiType::I64, AbiType::I64],
        returns: AbiType::I64,
        doc: "Realoca ptr (size_old, align) para new_size. Retorna novo ptr ou 0.",
        ts_signature:
            "realloc(ptr: number, size_old: number, align: number, new_size: number): number",
        intrinsic: None,
        pure: false,
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "alloc",
    doc: "Allocator raw via std::alloc. UNSAFE — pareie alloc/dealloc com mesmo size/align.",
    members: MEMBERS,
};
