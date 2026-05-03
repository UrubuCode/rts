//! `WeakMap` global class — ABI declarativa.

use crate::abi::{AbiType, GlobalClassSpec, MemberKind, NamespaceMember};

pub const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "new",
        kind: MemberKind::Constructor,
        symbol: "__RTS_FN_GL_WEAKMAP_NEW",
        args: &[],
        returns: AbiType::Handle,
        doc: "Creates a new empty WeakMap.",
        ts_signature: "new WeakMap(): WeakMap",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "set",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_WEAKMAP_SET",
        args: &[AbiType::Handle, AbiType::Handle, AbiType::I64],
        returns: AbiType::Handle,
        doc: "Sets value for key (handle). Returns the WeakMap.",
        ts_signature: "set(key: object, value: any): WeakMap",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "get",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_WEAKMAP_GET",
        args: &[AbiType::Handle, AbiType::Handle],
        returns: AbiType::I64,
        doc: "Returns value for key, or 0 if absent.",
        ts_signature: "get(key: object): any",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "has",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_WEAKMAP_HAS",
        args: &[AbiType::Handle, AbiType::Handle],
        returns: AbiType::I64,
        doc: "Returns 1 if key exists, 0 otherwise.",
        ts_signature: "has(key: object): boolean",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "delete",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_WEAKMAP_DELETE",
        args: &[AbiType::Handle, AbiType::Handle],
        returns: AbiType::I64,
        doc: "Removes key. Returns 1 if existed, 0 otherwise.",
        ts_signature: "delete(key: object): boolean",
        intrinsic: None,
        pure: false,
    },
];

pub const WEAKMAP_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "WeakMap",
    doc: "Built-in WeakMap (#217 v0). Comporta como Map forte; weak semantics em PR futura.",
    members: MEMBERS,
};
