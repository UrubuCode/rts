//! `ffi` namespace — ABI registration.

use crate::abi::{AbiType, MemberKind, NamespaceMember, NamespaceSpec};

pub const MEMBERS: &[NamespaceMember] = &[
    // ── CStr (raw C string view) ────────────────────────────────────
    NamespaceMember {
        name: "cstr_from_ptr",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_FFI_CSTR_FROM_PTR",
        args: &[AbiType::U64],
        returns: AbiType::Handle,
        doc: "Reads a nul-terminated C string from `ptr` and returns a string handle (UTF-8 lossy).",
        ts_signature: "cstr_from_ptr(ptr: number): number",
        intrinsic: None,
    },
    NamespaceMember {
        name: "cstr_len",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_FFI_CSTR_LEN",
        args: &[AbiType::U64],
        returns: AbiType::I64,
        doc: "Length in bytes of the C string at `ptr`, excluding the nul terminator. -1 if ptr is null.",
        ts_signature: "cstr_len(ptr: number): number",
        intrinsic: None,
    },
    NamespaceMember {
        name: "cstr_to_str",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_FFI_CSTR_TO_STR",
        args: &[AbiType::U64],
        returns: AbiType::Handle,
        doc: "Validates the C string at `ptr` as UTF-8 and returns a string handle. 0 if invalid.",
        ts_signature: "cstr_to_str(ptr: number): number",
        intrinsic: None,
    },
    // ── CString (owned nul-terminated buffer) ───────────────────────
    NamespaceMember {
        name: "cstring_new",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_FFI_CSTRING_NEW",
        args: &[AbiType::StrPtr],
        returns: AbiType::Handle,
        doc: "Builds a nul-terminated CString from `s` and returns a handle. 0 if `s` contains an interior nul.",
        ts_signature: "cstring_new(s: string): number",
        intrinsic: None,
    },
    NamespaceMember {
        name: "cstring_ptr",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_FFI_CSTRING_PTR",
        args: &[AbiType::U64],
        returns: AbiType::U64,
        doc: "Raw pointer to the CString bytes (nul-terminated). 0 if handle invalid. Unsafe — must not outlive handle.",
        ts_signature: "cstring_ptr(handle: number): number",
        intrinsic: None,
    },
    NamespaceMember {
        name: "cstring_free",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_FFI_CSTRING_FREE",
        args: &[AbiType::U64],
        returns: AbiType::Void,
        doc: "Releases the CString handle.",
        ts_signature: "cstring_free(handle: number): void",
        intrinsic: None,
    },
    // ── OsString (platform-native string) ───────────────────────────
    NamespaceMember {
        name: "osstr_from_str",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_FFI_OSSTR_FROM_STR",
        args: &[AbiType::StrPtr],
        returns: AbiType::Handle,
        doc: "Builds an OsString from a UTF-8 source and returns a handle.",
        ts_signature: "osstr_from_str(s: string): number",
        intrinsic: None,
    },
    NamespaceMember {
        name: "osstr_to_str",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_FFI_OSSTR_TO_STR",
        args: &[AbiType::U64],
        returns: AbiType::Handle,
        doc: "Converts the OsString handle to a UTF-8 string handle. 0 if not valid UTF-8.",
        ts_signature: "osstr_to_str(handle: number): number",
        intrinsic: None,
    },
    NamespaceMember {
        name: "osstr_free",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_FFI_OSSTR_FREE",
        args: &[AbiType::U64],
        returns: AbiType::Void,
        doc: "Releases the OsString handle.",
        ts_signature: "osstr_free(handle: number): void",
        intrinsic: None,
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "ffi",
    doc: "C-string and OS-string interop via std::ffi (CStr/CString/OsStr/OsString).",
    members: MEMBERS,
};
