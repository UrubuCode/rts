//! `FinalizationRegistry` global class — ABI declarativa.

use crate::abi::{AbiType, GlobalClassSpec, MemberKind, NamespaceMember};

pub const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "new",
        kind: MemberKind::Constructor,
        symbol: "__RTS_FN_GL_FINREG_NEW",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Creates a new FinalizationRegistry with cleanup callback.",
        ts_signature: "new FinalizationRegistry(callback: (heldValue: any) => void): FinalizationRegistry",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "register",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_FINREG_REGISTER",
        args: &[AbiType::Handle, AbiType::Handle, AbiType::I64],
        returns: AbiType::Void,
        doc: "Registers target for cleanup with held value. v0: noop (sem weak ref real).",
        ts_signature: "register(target: object, heldValue: any, unregisterToken?: object): void",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "unregister",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_FINREG_UNREGISTER",
        args: &[AbiType::Handle, AbiType::Handle],
        returns: AbiType::Bool,
        doc: "Unregisters target. v0: sempre retorna false (nada registrado).",
        ts_signature: "unregister(unregisterToken: object): boolean",
        intrinsic: None,
        pure: false,
    },
];

pub const FINALIZATION_REGISTRY_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "FinalizationRegistry",
    doc: "Built-in FinalizationRegistry (#685 v0). Stub — register/unregister sao noops; callback nunca dispara.",
    members: MEMBERS,
};
