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
        doc: "promise.then(onFul) — chama onFul(value) ao resolve, retorna Promise<result>. Para PromiseAsync (F1+ epic #411), spawna task tokio que aguarda settle e chama callback. Para Promise sync legacy (fetch), executa imediato.",
        ts_signature: "then<T>(onFul: (value: any) => T): Promise<T>",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "then2",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_PROMISE_THEN2",
        args: &[AbiType::Handle, AbiType::U64, AbiType::U64],
        returns: AbiType::Handle,
        doc: "promise.then(onFul, onRej) — versao 2-arg. onRej captura rejection (recovers se retornar valor). Equivalente a `Promise.prototype.then(onFul, onRej)` JS completo.",
        ts_signature: "then<T>(onFul: (v: any) => T, onRej: (e: any) => any): Promise<T>",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "catch",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_PROMISE_CATCH",
        args: &[AbiType::Handle, AbiType::U64],
        returns: AbiType::Handle,
        doc: "promise.catch(onRej) — atalho de then(undefined, onRej). Captura rejection e recupera (Promise resultante eh fulfilled com retorno do callback). Sem callback, retorna passthrough.",
        ts_signature: "catch(onRej: (err: any) => any): Promise<any>",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "finally",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_PROMISE_FINALLY",
        args: &[AbiType::Handle, AbiType::U64],
        returns: AbiType::Handle,
        doc: "promise.finally(fn) — chama fn() ao settle (independente de fulfilled/rejected). Promise resultante mantem o state/value original.",
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
