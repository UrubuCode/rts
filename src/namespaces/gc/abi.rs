//! `gc` namespace registration on the ABI surface.
//!
//! Only the string-handle API is exposed for now; object/array/buffer
//! allocators land as the rest of the runtime is rewired.

use crate::abi::{AbiType, MemberKind, NamespaceMember, NamespaceSpec};

pub const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "string_from_i64",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_GC_STRING_FROM_I64",
        args: &[AbiType::I64],
        returns: AbiType::Handle,
        doc: "Converts an i64 to its decimal string and returns a handle.",
        ts_signature: "string_from_i64(value: number): number",
    },
    NamespaceMember {
        name: "string_from_f64",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_GC_STRING_FROM_F64",
        args: &[AbiType::F64],
        returns: AbiType::Handle,
        doc: "Converts an f64 to its decimal string and returns a handle.",
        ts_signature: "string_from_f64(value: number): number",
    },
    NamespaceMember {
        name: "string_concat",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_GC_STRING_CONCAT",
        args: &[AbiType::Handle, AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Concatenates two string handles and returns a new handle.",
        ts_signature: "string_concat(a: number, b: number): number",
    },
    NamespaceMember {
        name: "string_from_static",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_GC_STRING_FROM_STATIC",
        args: &[AbiType::StrPtr],
        returns: AbiType::Handle,
        doc: "Promotes a static (ptr, len) string to a GC handle.",
        ts_signature: "string_from_static(data: string): number",
    },
    NamespaceMember {
        name: "string_new",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_GC_STRING_NEW",
        args: &[AbiType::StrPtr],
        returns: AbiType::Handle,
        doc: "Allocates a string handle from a (ptr, len) pair. Returns 0 on error.",
        ts_signature: "string_new(data: string): number",
    },
    NamespaceMember {
        name: "string_len",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_GC_STRING_LEN",
        args: &[AbiType::Handle],
        returns: AbiType::I64,
        doc: "Returns the byte length of the string, or -1 on invalid handle.",
        ts_signature: "string_len(handle: number): number",
    },
    NamespaceMember {
        name: "string_ptr",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_GC_STRING_PTR",
        args: &[AbiType::Handle],
        returns: AbiType::U64,
        doc: "Returns the raw pointer to the string buffer, or 0 on invalid handle.",
        ts_signature: "string_ptr(handle: number): number",
    },
    NamespaceMember {
        name: "string_free",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_GC_STRING_FREE",
        args: &[AbiType::Handle],
        returns: AbiType::I64,
        doc: "Frees the string handle. Returns 1 on success, 0 if already invalid.",
        ts_signature: "string_free(handle: number): number",
    },
    NamespaceMember {
        name: "object_new",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_GC_OBJECT_NEW",
        args: &[AbiType::I64],
        returns: AbiType::Handle,
        doc: "Allocates a zeroed object buffer of `size` bytes and returns a handle.",
        ts_signature: "object_new(size: number): number",
    },
    NamespaceMember {
        name: "object_ptr",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_GC_OBJECT_PTR",
        args: &[AbiType::Handle],
        returns: AbiType::U64,
        doc: "Returns the raw pointer to the object buffer, or 0 on invalid handle.",
        ts_signature: "object_ptr(handle: number): number",
    },
    NamespaceMember {
        name: "object_size",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_GC_OBJECT_SIZE",
        args: &[AbiType::Handle],
        returns: AbiType::I64,
        doc: "Returns the byte size of the object buffer, or -1 on invalid handle.",
        ts_signature: "object_size(handle: number): number",
    },
    NamespaceMember {
        name: "object_free",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_GC_OBJECT_FREE",
        args: &[AbiType::Handle],
        returns: AbiType::I64,
        doc: "Frees the object handle. Returns 1 on success, 0 if already invalid.",
        ts_signature: "object_free(handle: number): number",
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "gc",
    doc: "Runtime-managed handle table and string pool.",
    members: MEMBERS,
};
