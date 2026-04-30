use crate::abi::{AbiType, GlobalClassSpec, MemberKind, NamespaceMember};

pub const TEXT_ENCODER_MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "new",
        kind: MemberKind::Constructor,
        symbol: "__RTS_FN_GL_TEXTENC_NEW",
        args: &[],
        returns: AbiType::Handle,
        doc: "new TextEncoder() — sem args (sempre UTF-8).",
        ts_signature: "new TextEncoder()",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "encode",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_TEXTENC_ENCODE",
        args: &[AbiType::Handle, AbiType::StrPtr],
        returns: AbiType::Handle,
        doc: "encoder.encode(text) — UTF-8 bytes como Buffer handle.",
        ts_signature: "encode(text: string): Uint8Array",
        intrinsic: None,
        pure: true,
    },
];

pub const TEXT_ENCODER_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "TextEncoder",
    doc: "TextEncoder — encode string para UTF-8 Uint8Array.",
    members: TEXT_ENCODER_MEMBERS,
};

pub const TEXT_DECODER_MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "new",
        kind: MemberKind::Constructor,
        symbol: "__RTS_FN_GL_TEXTDEC_NEW",
        args: &[],
        returns: AbiType::Handle,
        doc: "new TextDecoder() — sem args (sempre UTF-8).",
        ts_signature: "new TextDecoder()",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "decode",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_TEXTENC_DECODE",
        args: &[AbiType::Handle, AbiType::Handle],
        returns: AbiType::Handle,
        doc: "decoder.decode(buf) — Buffer handle para string handle.",
        ts_signature: "decode(buf: Uint8Array): string",
        intrinsic: None,
        pure: true,
    },
];

pub const TEXT_DECODER_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "TextDecoder",
    doc: "TextDecoder — decode Uint8Array UTF-8 para string.",
    members: TEXT_DECODER_MEMBERS,
};
