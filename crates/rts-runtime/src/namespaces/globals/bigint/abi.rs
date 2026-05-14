//! `BigInt.asIntN` / `BigInt.asUintN` — ABI declarativa.

use crate::abi::{AbiType, GlobalClassSpec, MemberKind, NamespaceMember};

pub const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "asIntN",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_GL_BIGINT_AS_INT_N",
        args: &[AbiType::I64, AbiType::I64],
        returns: AbiType::I64,
        doc: "BigInt.asIntN(bits, n) — n modulo 2^bits, interpretado como signed.",
        ts_signature: "static asIntN(bits: number, n: bigint): bigint",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "asUintN",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_GL_BIGINT_AS_UINT_N",
        args: &[AbiType::I64, AbiType::I64],
        returns: AbiType::I64,
        doc: "BigInt.asUintN(bits, n) — n modulo 2^bits, sem signo.",
        ts_signature: "static asUintN(bits: number, n: bigint): bigint",
        intrinsic: None,
        pure: true,
    },
];

pub const BIGINT_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "BigInt",
    doc: "BigInt namespace (helpers staticos asIntN/asUintN). RTS trata BigInt como i64.",
    members: MEMBERS,
};
