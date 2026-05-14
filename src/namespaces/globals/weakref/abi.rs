//! `WeakRef` global class — ABI declarativa.

use crate::abi::{AbiType, GlobalClassSpec, MemberKind, NamespaceMember};

pub const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "new",
        kind: MemberKind::Constructor,
        symbol: "__RTS_FN_GL_WEAKREF_NEW",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Creates a new WeakRef wrapping target.",
        ts_signature: "new WeakRef(target: object): WeakRef",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "deref",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_WEAKREF_DEREF",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Returns the target object (or undefined if collected). v0 sempre retorna target.",
        ts_signature: "deref(): object | undefined",
        intrinsic: None,
        pure: true,
    },
];

pub const WEAKREF_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "WeakRef",
    doc: "Built-in WeakRef (#685 v0). Strong reference; weak semantics em PR futura.",
    members: MEMBERS,
};
