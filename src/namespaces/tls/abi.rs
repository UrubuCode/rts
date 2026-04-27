//! `tls` namespace — ABI registration.

use crate::abi::{AbiType, MemberKind, NamespaceMember, NamespaceSpec};

pub const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "client",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_TLS_CLIENT",
        args: &[AbiType::U64, AbiType::StrPtr],
        returns: AbiType::Handle,
        doc: "Wraps tcp_handle numa conexao TLS client. Consome o tcp_handle. SNI = sni_hostname. Retorna stream handle ou 0 (handshake falhou).",
        ts_signature: "client(tcp: number, sniHostname: string): number",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "send",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_TLS_SEND",
        args: &[AbiType::U64, AbiType::StrPtr],
        returns: AbiType::I64,
        doc: "Envia bytes encriptados pelo TLS stream. Retorna bytes plain enviados ou -1.",
        ts_signature: "send(stream: number, data: string): number",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "recv",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_TLS_RECV",
        args: &[AbiType::U64, AbiType::U64, AbiType::I64],
        returns: AbiType::I64,
        doc: "Le ate `len` bytes plain do TLS stream. Retorna bytes lidos (0 = EOF, -1 = erro).",
        ts_signature: "recv(stream: number, bufPtr: number, len: number): number",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "close",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_TLS_CLOSE",
        args: &[AbiType::U64],
        returns: AbiType::Void,
        doc: "Fecha o stream TLS (close_notify) e libera o handle.",
        ts_signature: "close(stream: number): void",
        intrinsic: None,
        pure: false,
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "tls",
    doc: "TLS 1.2/1.3 client sync via rustls (HTTPS support).",
    members: MEMBERS,
};
