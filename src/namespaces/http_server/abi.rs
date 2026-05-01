//! `http_server` namespace — ABI registration.

use crate::abi::{AbiType, MemberKind, NamespaceMember, NamespaceSpec};

pub const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "serve",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_HTTP_SERVER_SERVE",
        args: &[AbiType::StrPtr, AbiType::U64],
        returns: AbiType::Void,
        doc: "Inicia um servidor HTTP/1.1 em `addr` (ex: \"127.0.0.1:8080\"). `handler` e' um fn pointer `extern \"C\" fn(req: u64) -> void`. Bloqueia indefinidamente. Backend: actix-web sobre tokio multi-thread runtime.",
        ts_signature: "serve(addr: string, handler: (req: number) => void): void",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "req_method",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_HTTP_SERVER_REQ_METHOD",
        args: &[AbiType::U64],
        returns: AbiType::Handle,
        doc: "Retorna o metodo HTTP (GET/POST/...) como string handle.",
        ts_signature: "req_method(req: number): string",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "req_path",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_HTTP_SERVER_REQ_PATH",
        args: &[AbiType::U64],
        returns: AbiType::Handle,
        doc: "Retorna o path da request (ex: \"/api/info\") como string handle.",
        ts_signature: "req_path(req: number): string",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "req_body",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_HTTP_SERVER_REQ_BODY",
        args: &[AbiType::U64],
        returns: AbiType::Handle,
        doc: "Retorna o body da request como string handle (UTF-8). Vazio se nao houver body.",
        ts_signature: "req_body(req: number): string",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "respond",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_HTTP_SERVER_RESPOND",
        args: &[AbiType::U64, AbiType::I64, AbiType::StrPtr, AbiType::StrPtr],
        returns: AbiType::Void,
        doc: "Envia resposta HTTP: status (ex: 200), content_type (ex: \"application/json\"), body. Consome o handle do request.",
        ts_signature: "respond(req: number, status: number, contentType: string, body: string): void",
        intrinsic: None,
        pure: false,
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "http_server",
    doc: "Servidor HTTP/1.1 nativo via actix-web. Suporta keep-alive, thread pool, parsing correto. Modelo: serve(addr, handler) bloqueia; handler recebe req handle e chama respond().",
    members: MEMBERS,
};
