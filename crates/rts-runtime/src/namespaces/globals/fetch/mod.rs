//! `fetch` — Web Fetch API: `fetch()` + Response + Request + Promise.
//!
//! Migrado do `#[rts_namespace]` + `#[rts_class]` (macro) pro modelo builder
//! hand-written do `rts-engine` (rumo à remoção da `rts-macro`). Todos os
//! membros são `external`: os externs `__RTS_FN_GL_FETCH*` /
//! `__RTS_FN_GL_FETCH_RESPONSE_*` / `__RTS_FN_GL_REQUEST_*` /
//! `__RTS_FN_GL_PROMISE_*` ficam em `instance.rs` intactos (e
//! `Promise.all/race/any/allSettled` reusam `__RTS_FN_NS_PROMISE_*` do
//! namespace `promise`). Aqui só registramos a namespace `fetch` + os 3 class
//! specs (Response/Request/Promise) com `fn_ptr` null — codegen chama os
//! externs por símbolo, sem reemitir nada.

pub mod instance;

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

/// Membro hand-written (espelha `leak`/`leak_class` da macro). `fn_ptr` é null
/// para membros `external` (codegen resolve pelo `symbol`).
#[allow(clippy::too_many_arguments)]
fn m(
    name: &str,
    kind: MemberKind,
    sig: Sig,
    symbol: &str,
    ts: &str,
    doc: &str,
    pure: bool,
) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(core::ptr::null::<u8>()),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure,
        intrinsic: None,
    }
}

/// Registra a namespace `fetch` no motor (hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("fetch")
        .doc("Web Fetch API — fetch() + Promise<Response> + Response (text/json/blob/status/ok/url).")
        .member(m(
            "fetch",
            MemberKind::Function,
            Sig::new(vec![AbiType::StrPtr, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_FETCH",
            "fetch(url: string, init?: RequestInit): Promise<Response>",
            "fetch(url, init?) — HTTP request síncrono. Retorna Promise<Response>.",
            false,
        ))
        .done();
}

/// Registra a classe global `Response` no motor (hand-written, sem macro).
pub fn register_response_class_spec(e: &mut Engine) {
    e.class("Response")
        .doc("Fetch API Response — status/ok/statusText/url/text()/json()/blob()/arrayBuffer().")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::StrPtr, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_FETCH_RESPONSE_NEW",
            "new Response(body?: string, init?: ResponseInit)",
            "new Response(body, init)",
            true,
        ))
        .member(m(
            "headers",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_FETCH_RESPONSE_HEADERS",
            "readonly headers: Headers",
            "response.headers — Headers object.",
            false,
        ))
        .member(m(
            "status",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_FETCH_RESPONSE_STATUS",
            "readonly status: number",
            "response.status — HTTP status code (number).",
            false,
        ))
        .member(m(
            "ok",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Bool),
            "__RTS_FN_GL_FETCH_RESPONSE_OK",
            "readonly ok: boolean",
            "response.ok — true se status 200-299.",
            false,
        ))
        .member(m(
            "statusText",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_FETCH_RESPONSE_STATUS_TEXT",
            "readonly statusText: string",
            "response.statusText — 'OK', 'Not Found', etc.",
            false,
        ))
        .member(m(
            "url",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_FETCH_RESPONSE_URL",
            "readonly url: string",
            "response.url — URL final (após redirects).",
            false,
        ))
        .member(m(
            "text",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_FETCH_RESPONSE_TEXT",
            "text(): Promise<string>",
            "response.text() → string (RTS sync, sem Promise).",
            false,
        ))
        .member(m(
            "json",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_FETCH_RESPONSE_JSON",
            "json(): Promise<any>",
            "response.json() → JSON handle (RTS sync, sem Promise).",
            false,
        ))
        .member(m(
            "blob",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_FETCH_RESPONSE_ARRAY_BUFFER",
            "blob(): Promise<Blob>",
            "response.blob() → Buffer handle (RTS sync).",
            false,
        ))
        .member(m(
            "arrayBuffer",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_FETCH_RESPONSE_ARRAY_BUFFER",
            "arrayBuffer(): Promise<ArrayBuffer>",
            "response.arrayBuffer() → Buffer handle (RTS sync).",
            false,
        ))
        .member(m(
            "then",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::U64], AbiType::Handle),
            "__RTS_FN_GL_FETCH_RESPONSE_THEN",
            "then<T>(fn: (response: Response) => T): T",
            "response.then(fn) → fn(response). Compatibilidade com Promise.then().",
            false,
        ))
        .done();
}

/// Registra a classe global `Request` no motor (hand-written, sem macro).
pub fn register_request_class_spec(e: &mut Engine) {
    e.class("Request")
        .doc("Fetch API Request — method/url/text().")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::StrPtr, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_REQUEST_NEW",
            "new Request(url: string, init?: RequestInit)",
            "new Request(url, init)",
            true,
        ))
        .member(m(
            "method",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_REQUEST_METHOD",
            "readonly method: string",
            "request.method — 'GET'/'POST'/...",
            false,
        ))
        .member(m(
            "url",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_REQUEST_URL",
            "readonly url: string",
            "request.url",
            false,
        ))
        .member(m(
            "text",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_REQUEST_TEXT",
            "text(): Promise<string>",
            "request.text() → Promise<string> (body).",
            false,
        ))
        .done();
}

/// Registra a classe global `Promise` no motor (hand-written, sem macro).
pub fn register_promise_class_spec(e: &mut Engine) {
    e.class("Promise")
        .doc("Promise síncrona — then/catch/finally executam imediatamente. await = resolve().")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_PROMISE_NEW",
            "new (executor: (resolve: (value: any) => void, reject: (reason: any) => void) => void): Promise<any>",
            "new Promise((resolve, reject) => ...) — JS spec ctor.",
            false,
        ))
        .member(m(
            "then",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::U64], AbiType::Handle),
            "__RTS_FN_GL_PROMISE_THEN",
            "then<T>(onFulfilled: (value: any) => T): Promise<T>",
            "promise.then(onFul) — chama onFul(value) ao resolve. Suporta PromiseAsync (#411) com encadeamento real.",
            false,
        ))
        .member(m(
            "catch",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::U64], AbiType::Handle),
            "__RTS_FN_GL_PROMISE_CATCH",
            "catch(onRejected: (err: any) => any): Promise<any>",
            "promise.catch(onRej) — captura rejection. Recovers se callback retornar valor.",
            false,
        ))
        .member(m(
            "finally",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::U64], AbiType::Handle),
            "__RTS_FN_GL_PROMISE_FINALLY",
            "finally(onFinally: () => void): Promise<any>",
            "promise.finally(fn) — chama fn() ao settle. Mantem state/value original.",
            false,
        ))
        .member(m(
            "resolve",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_PROMISE_RESOLVE",
            "static resolve<T>(value: T): Promise<T>",
            "Promise.resolve(v) — cria Promise já resolvida.",
            false,
        ))
        .member(m(
            "resolve",
            MemberKind::Function,
            Sig::new(Vec::new(), AbiType::I64),
            "__RTS_FN_GL_PROMISE_RESOLVE_EMPTY",
            "static resolve(): Promise<undefined>",
            "Promise.resolve() — cria Promise já resolvida com undefined.",
            false,
        ))
        .member(m(
            "reject",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_PROMISE_REJECT",
            "static reject<T>(reason?: any): Promise<T>",
            "Promise.reject(reason) — cria PromiseAsync rejected. Distingue de resolve para .catch/.any/.allSettled.",
            false,
        ))
        .member(m(
            "all",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_NS_PROMISE_ALL",
            "static all<T>(values: Promise<T>[]): Promise<T[]>",
            "Promise.all(promises) — aguarda todas; fail-fast em rejection.",
            false,
        ))
        .member(m(
            "race",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_NS_PROMISE_RACE",
            "static race<T>(values: Promise<T>[]): Promise<T>",
            "Promise.race(promises) — settle com o resultado da primeira.",
            false,
        ))
        .member(m(
            "any",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_NS_PROMISE_ANY",
            "static any<T>(values: Promise<T>[]): Promise<T>",
            "Promise.any(promises) — primeira a fulfill; reject so' se todas falharem.",
            false,
        ))
        .member(m(
            "allSettled",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_NS_PROMISE_ALL_SETTLED",
            "static allSettled<T>(values: Promise<T>[]): Promise<any[]>",
            "Promise.allSettled(promises) — aguarda todas; sempre resolve com Vec de descriptors.",
            false,
        ))
        .member(m(
            "try",
            MemberKind::Function,
            Sig::new(vec![AbiType::U64], AbiType::I64),
            "__RTS_FN_GL_PROMISE_TRY",
            "static try<T>(fn: () => T | Promise<T>): Promise<T>",
            "Promise.try(fn) — invoca fn() sincronamente; retorna Promise.resolve(retval) ou Promise.reject(err) se fn lancar.",
            false,
        ))
        .member(m(
            "withResolvers",
            MemberKind::Function,
            Sig::new(Vec::new(), AbiType::Handle),
            "__RTS_FN_GL_PROMISE_WITH_RESOLVERS",
            "static withResolvers<T>(): { promise: Promise<T>, resolve: (v: T) => void, reject: (e: any) => void }",
            "Promise.withResolvers() — ES2024: cria { promise, resolve, reject }. Util pra fora-do-construtor settle.",
            false,
        ))
        .done();
}
