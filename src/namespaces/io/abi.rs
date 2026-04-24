//! `io` namespace registration on the new ABI.
//!
//! Members declared here are consumed by codegen to emit direct
//! `call __RTS_FN_NS_IO_*` instructions. Legacy dispatch in the parent
//! `mod.rs` remains untouched until the full migration lands.

use crate::abi::{AbiType, MemberKind, NamespaceMember, NamespaceSpec};

pub const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "print",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_IO_PRINT",
        args: &[AbiType::StrPtr],
        returns: AbiType::Void,
        doc: "Writes a UTF-8 message followed by newline to stdout.",
        ts_signature: "print(message: string): void",
        intrinsic: None,
    },
    NamespaceMember {
        name: "eprint",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_IO_EPRINT",
        args: &[AbiType::StrPtr],
        returns: AbiType::Void,
        doc: "Writes a UTF-8 message followed by newline to stderr.",
        ts_signature: "eprint(message: string): void",
        intrinsic: None,
    },
    NamespaceMember {
        name: "stdout_write",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_IO_STDOUT_WRITE",
        args: &[AbiType::StrPtr],
        returns: AbiType::I64,
        doc: "Writes raw bytes to stdout, returns bytes written or -1 on error.",
        ts_signature: "stdout_write(data: string): number",
        intrinsic: None,
    },
    NamespaceMember {
        name: "stdout_flush",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_IO_STDOUT_FLUSH",
        args: &[],
        returns: AbiType::I64,
        doc: "Flushes stdout buffer. Returns 0 on success, -1 on error.",
        ts_signature: "stdout_flush(): number",
        intrinsic: None,
    },
    NamespaceMember {
        name: "stderr_write",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_IO_STDERR_WRITE",
        args: &[AbiType::StrPtr],
        returns: AbiType::I64,
        doc: "Writes raw bytes to stderr, returns bytes written or -1 on error.",
        ts_signature: "stderr_write(data: string): number",
        intrinsic: None,
    },
    NamespaceMember {
        name: "stderr_flush",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_IO_STDERR_FLUSH",
        args: &[],
        returns: AbiType::I64,
        doc: "Flushes stderr buffer. Returns 0 on success, -1 on error.",
        ts_signature: "stderr_flush(): number",
        intrinsic: None,
    },
    NamespaceMember {
        name: "stdin_read",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_IO_STDIN_READ",
        args: &[AbiType::U64, AbiType::I64],
        returns: AbiType::I64,
        doc: "Reads up to `len` bytes from stdin into buffer. Returns byte count or -1.",
        ts_signature: "stdin_read(bufPtr: number, bufLen: number): number",
        intrinsic: None,
    },
    NamespaceMember {
        name: "stdin_read_line",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_IO_STDIN_READ_LINE",
        args: &[AbiType::U64, AbiType::I64],
        returns: AbiType::I64,
        doc: "Reads a single line from stdin (no terminator) into buffer.",
        ts_signature: "stdin_read_line(bufPtr: number, bufLen: number): number",
        intrinsic: None,
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "io",
    doc: "Standard input/output primitives backed by std::io.",
    members: MEMBERS,
};
