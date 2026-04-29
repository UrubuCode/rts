//! `globalThis` — JS global object aliases.
//!
//! In a browser `globalThis === window`; in Node.js `globalThis === global`.
//! RTS does not have a heap-allocated global object, but we expose the
//! identity properties that are most commonly accessed in portable code.
//! Each member here aliases an existing RTS symbol.

use crate::abi::{AbiType, MemberKind, NamespaceMember, NamespaceSpec};

pub const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "undefined",
        kind: MemberKind::Constant,
        symbol: "__RTS_FN_NS_GC_STRING_NEW",
        args: &[],
        returns: AbiType::I64,
        doc: "The undefined value (0 in RTS).",
        ts_signature: "undefined: undefined",
        intrinsic: None,
        pure: true,
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "globalThis",
    doc: "Global object aliases — process, global, self, undefined.",
    members: MEMBERS,
};
