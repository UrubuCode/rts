use crate::abi::{AbiType, MemberKind, NamespaceMember, NamespaceSpec};

pub const MEMBERS: &[NamespaceMember] = &[
    // fetch(url, opts?) — opts é Map handle (0 = sem opts)
    NamespaceMember {
        name: "fetch",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_GL_FETCH",
        args: &[AbiType::StrPtr, AbiType::Handle],
        returns: AbiType::Handle,
        doc: "fetch(url, init?) — HTTP request síncrono. Retorna Promise<Response>.",
        ts_signature: "fetch(url: string, init?: RequestInit): Promise<Response>",
        intrinsic: None,
        pure: false,
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "fetch",
    doc: "Web Fetch API — fetch() + Promise<Response> + Response (text/json/blob/status/ok/url).",
    members: MEMBERS,
};

// ABI separado para Promise (usado via GlobalClassSpec, não namespace direto)
pub const PROMISE_MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "then",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_PROMISE_THEN",
        args: &[AbiType::Handle, AbiType::U64],
        returns: AbiType::Handle,
        doc: "promise.then(fn) — chama fn(value) imediatamente, retorna Promise<result>.",
        ts_signature: "then<T>(fn: (value: any) => T): Promise<T>",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "catch",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_PROMISE_CATCH",
        args: &[AbiType::Handle, AbiType::U64],
        returns: AbiType::Handle,
        doc: "promise.catch(fn) — passthrough (RTS é síncrono, sem rejeição).",
        ts_signature: "catch(fn: (err: any) => any): Promise<any>",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "finally",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_PROMISE_FINALLY",
        args: &[AbiType::Handle, AbiType::U64],
        returns: AbiType::Handle,
        doc: "promise.finally(fn) — chama fn() e retorna o promise original.",
        ts_signature: "finally(fn: () => void): Promise<any>",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "resolve",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_GL_PROMISE_RESOLVE",
        args: &[AbiType::Handle],
        returns: AbiType::I64,
        doc: "Promise.resolve(v) / await — extrai o valor resolvido.",
        ts_signature: "resolve(promise: Promise<any>): any",
        intrinsic: None,
        pure: false,
    },
];
