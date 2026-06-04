use crate::abi::{AbiType, GlobalClassSpec, MemberKind, NamespaceMember};

// ── Blob ────────────────────────────────────────────────────────────────────

pub const BLOB_MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "new",
        kind: MemberKind::Constructor,
        symbol: "__RTS_FN_GL_BLOB_NEW_EMPTY",
        args: &[],
        returns: AbiType::Handle,
        doc: "new Blob()",
        ts_signature: "new Blob()",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "new",
        kind: MemberKind::Constructor,
        symbol: "__RTS_FN_GL_BLOB_NEW",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "new Blob(parts)",
        ts_signature: "new Blob(parts: BlobPart[])",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "size",
        kind: MemberKind::InstanceGetter,
        symbol: "__RTS_FN_GL_BLOB_SIZE",
        args: &[AbiType::Handle],
        returns: AbiType::I64,
        doc: "blob.size — byte length.",
        ts_signature: "readonly size: number",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "text",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_BLOB_TEXT",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "blob.text() — UTF-8 string (Promise-resolved).",
        ts_signature: "text(): Promise<string>",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "stream",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_BLOB_STREAM",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "blob.stream() — ReadableStream dos bytes.",
        ts_signature: "stream(): ReadableStream",
        intrinsic: None,
        pure: true,
    },
];

pub const BLOB_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "Blob",
    doc: "Blob — immutable raw-data container com size/text().",
    members: BLOB_MEMBERS,
};

// ── File (extends Blob) ───────────────────────────────────────────────────────

pub const FILE_MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "new",
        kind: MemberKind::Constructor,
        symbol: "__RTS_FN_GL_FILE_NEW",
        args: &[AbiType::Handle, AbiType::StrPtr, AbiType::Handle],
        returns: AbiType::Handle,
        doc: "new File(parts, name, options?)",
        ts_signature: "new File(parts: BlobPart[], name: string, options?: FilePropertyBag)",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "name",
        kind: MemberKind::InstanceGetter,
        symbol: "__RTS_FN_GL_FILE_NAME",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "file.name",
        ts_signature: "readonly name: string",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "lastModified",
        kind: MemberKind::InstanceGetter,
        symbol: "__RTS_FN_GL_FILE_LAST_MODIFIED",
        args: &[AbiType::Handle],
        returns: AbiType::I64,
        doc: "file.lastModified — epoch ms.",
        ts_signature: "readonly lastModified: number",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "size",
        kind: MemberKind::InstanceGetter,
        symbol: "__RTS_FN_GL_BLOB_SIZE",
        args: &[AbiType::Handle],
        returns: AbiType::I64,
        doc: "file.size — byte length.",
        ts_signature: "readonly size: number",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "text",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_BLOB_TEXT",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "file.text() — UTF-8 string.",
        ts_signature: "text(): Promise<string>",
        intrinsic: None,
        pure: true,
    },
];

pub const FILE_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "File",
    doc: "File — Blob com name + lastModified.",
    members: FILE_MEMBERS,
};
