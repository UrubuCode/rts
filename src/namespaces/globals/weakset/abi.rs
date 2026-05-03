//! `WeakSet` global class — ABI declarativa.

use crate::abi::{AbiType, GlobalClassSpec, MemberKind, NamespaceMember};

pub const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "new",
        kind: MemberKind::Constructor,
        symbol: "__RTS_FN_GL_WEAKSET_NEW",
        args: &[],
        returns: AbiType::Handle,
        doc: "Creates a new empty WeakSet.",
        ts_signature: "new WeakSet(): WeakSet",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "add",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_WEAKSET_ADD",
        args: &[AbiType::Handle, AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Adds object handle to set. Returns the WeakSet.",
        ts_signature: "add(value: object): WeakSet",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "has",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_WEAKSET_HAS",
        args: &[AbiType::Handle, AbiType::Handle],
        returns: AbiType::I64,
        doc: "Returns 1 if value present, 0 otherwise.",
        ts_signature: "has(value: object): boolean",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "delete",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_WEAKSET_DELETE",
        args: &[AbiType::Handle, AbiType::Handle],
        returns: AbiType::I64,
        doc: "Removes value. Returns 1 if existed, 0 otherwise.",
        ts_signature: "delete(value: object): boolean",
        intrinsic: None,
        pure: false,
    },
];

pub const WEAKSET_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "WeakSet",
    doc: "Built-in WeakSet (#217 v0). Comporta como Set forte; weak semantics em PR futura.",
    members: MEMBERS,
};
