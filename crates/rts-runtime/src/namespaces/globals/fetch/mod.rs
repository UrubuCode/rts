//! `fetch` — Web Fetch API: `fetch()` + Response + Request + Promise. Migrado
//! ao modelo `#[rts_namespace]` + `#[rts_class]` (stage 2c) via membros
//! `external`: os externs `__RTS_FN_GL_FETCH*` / `__RTS_FN_GL_RESPONSE*` /
//! `__RTS_FN_GL_REQUEST*` / `__RTS_FN_GL_PROMISE_*` ficam em `instance.rs`
//! intactos (e `Promise.all/race/any/allSettled` reusam `__RTS_FN_NS_PROMISE_*`
//! do namespace `promise`); os macros derivam só o `SPEC` + os 3 `*_CLASS_SPEC`.
//!
//! O antigo `abi::PROMISE_MEMBERS` (then/then2/catch/finally/resolve) era uma
//! tabela morta — `then2` é registrado direto no jit; o spec real do Promise é
//! o `PROMISE_CLASS_SPEC` abaixo.

pub mod instance;

#[allow(unused_imports)]
use rts_engine::abi::ty::{Bool, Handle, Str, I64, U64};
use rts_macro::{rts_class, rts_namespace};

/// Web Fetch API — fetch() + Promise<Response> + Response (text/json/blob/status/ok/url).
#[rts_namespace(fetch, sym = "GL")]
impl FetchNs {
    /// fetch(url, init?) — HTTP request síncrono. Retorna Promise<Response>.
    #[rts_fn(
        external,
        ts = "fetch(url: string, init?: RequestInit): Promise<Response>"
    )]
    pub fn fetch(_url: Str, _opts: Handle) -> Handle {
        unreachable!()
    }
}

/// Fetch API Response — status/ok/statusText/url/text()/json()/blob()/arrayBuffer().
#[rts_class(Response, prefix = "FETCH_RESPONSE", spec = "RESPONSE_CLASS_SPEC")]
impl ResponseClass {
    /// new Response(body, init)
    #[rts_ctor(
        external,
        ts = "new Response(body?: string, init?: ResponseInit)",
        pure
    )]
    pub fn new(_body: Str, _init: Handle) -> Handle {
        unreachable!()
    }
    /// response.headers — Headers object.
    #[rts_method(external, name = "headers", ts = "readonly headers: Headers")]
    pub fn headers(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// response.status — HTTP status code (number).
    #[rts_method(external, name = "status", ts = "readonly status: number")]
    pub fn status(_recv: Handle) -> I64 {
        unreachable!()
    }
    /// response.ok — true se status 200-299.
    #[rts_method(external, name = "ok", ts = "readonly ok: boolean")]
    pub fn ok(_recv: Handle) -> Bool {
        unreachable!()
    }
    /// response.statusText — 'OK', 'Not Found', etc.
    #[rts_method(external, name = "statusText", ts = "readonly statusText: string")]
    pub fn status_text(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// response.url — URL final (após redirects).
    #[rts_method(external, name = "url", ts = "readonly url: string")]
    pub fn url(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// response.text() → string (RTS sync, sem Promise).
    #[rts_method(external, name = "text", ts = "text(): Promise<string>")]
    pub fn text(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// response.json() → JSON handle (RTS sync, sem Promise).
    #[rts_method(external, name = "json", ts = "json(): Promise<any>")]
    pub fn json(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// response.blob() → Buffer handle (RTS sync).
    #[rts_method(
        external,
        name = "blob",
        symbol = "__RTS_FN_GL_FETCH_RESPONSE_ARRAY_BUFFER",
        ts = "blob(): Promise<Blob>"
    )]
    pub fn blob(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// response.arrayBuffer() → Buffer handle (RTS sync).
    #[rts_method(
        external,
        name = "arrayBuffer",
        ts = "arrayBuffer(): Promise<ArrayBuffer>"
    )]
    pub fn array_buffer(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// response.then(fn) → fn(response). Compatibilidade com Promise.then().
    #[rts_method(
        external,
        name = "then",
        ts = "then<T>(fn: (response: Response) => T): T"
    )]
    pub fn then(_recv: Handle, _fn: U64) -> Handle {
        unreachable!()
    }
}

/// Fetch API Request — method/url/text().
#[rts_class(Request, prefix = "REQUEST", spec = "REQUEST_CLASS_SPEC")]
impl RequestClass {
    /// new Request(url, init)
    #[rts_ctor(external, ts = "new Request(url: string, init?: RequestInit)", pure)]
    pub fn new(_url: Str, _init: Handle) -> Handle {
        unreachable!()
    }
    /// request.method — 'GET'/'POST'/...
    #[rts_method(external, name = "method", ts = "readonly method: string")]
    pub fn method(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// request.url
    #[rts_method(external, name = "url", ts = "readonly url: string")]
    pub fn url(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// request.text() → Promise<string> (body).
    #[rts_method(external, name = "text", ts = "text(): Promise<string>")]
    pub fn text(_recv: Handle) -> Handle {
        unreachable!()
    }
}

/// Promise síncrona — then/catch/finally executam imediatamente. await = resolve().
#[rts_class(Promise, prefix = "PROMISE", spec = "PROMISE_CLASS_SPEC")]
impl PromiseClass {
    /// new Promise((resolve, reject) => ...) — JS spec ctor.
    #[rts_ctor(
        external,
        ts = "new (executor: (resolve: (value: any) => void, reject: (reason: any) => void) => void): Promise<any>"
    )]
    pub fn new(_executor: Handle) -> Handle {
        unreachable!()
    }
    /// promise.then(onFul) — chama onFul(value) ao resolve. Suporta PromiseAsync (#411) com encadeamento real.
    #[rts_method(
        external,
        name = "then",
        ts = "then<T>(onFulfilled: (value: any) => T): Promise<T>"
    )]
    pub fn then(_recv: Handle, _on_ful: U64) -> Handle {
        unreachable!()
    }
    /// promise.catch(onRej) — captura rejection. Recovers se callback retornar valor.
    #[rts_method(
        external,
        name = "catch",
        symbol = "__RTS_FN_GL_PROMISE_CATCH",
        ts = "catch(onRejected: (err: any) => any): Promise<any>"
    )]
    pub fn catch_(_recv: Handle, _on_rej: U64) -> Handle {
        unreachable!()
    }
    /// promise.finally(fn) — chama fn() ao settle. Mantem state/value original.
    #[rts_method(
        external,
        name = "finally",
        symbol = "__RTS_FN_GL_PROMISE_FINALLY",
        ts = "finally(onFinally: () => void): Promise<any>"
    )]
    pub fn finally_(_recv: Handle, _fn: U64) -> Handle {
        unreachable!()
    }
    /// Promise.resolve(v) — cria Promise já resolvida.
    #[rts_fn(
        external,
        name = "resolve",
        ts = "static resolve<T>(value: T): Promise<T>"
    )]
    pub fn resolve(_value: Handle) -> I64 {
        unreachable!()
    }
    /// Promise.resolve() — cria Promise já resolvida com undefined.
    #[rts_fn(
        external,
        name = "resolve",
        symbol = "__RTS_FN_GL_PROMISE_RESOLVE_EMPTY",
        ts = "static resolve(): Promise<undefined>"
    )]
    pub fn resolve_empty() -> I64 {
        unreachable!()
    }
    /// Promise.reject(reason) — cria PromiseAsync rejected. Distingue de resolve para .catch/.any/.allSettled.
    #[rts_fn(
        external,
        name = "reject",
        ts = "static reject<T>(reason?: any): Promise<T>"
    )]
    pub fn reject(_reason: Handle) -> Handle {
        unreachable!()
    }
    /// Promise.all(promises) — aguarda todas; fail-fast em rejection.
    #[rts_fn(
        external,
        name = "all",
        symbol = "__RTS_FN_NS_PROMISE_ALL",
        ts = "static all<T>(values: Promise<T>[]): Promise<T[]>"
    )]
    pub fn all(_values: Handle) -> Handle {
        unreachable!()
    }
    /// Promise.race(promises) — settle com o resultado da primeira.
    #[rts_fn(
        external,
        name = "race",
        symbol = "__RTS_FN_NS_PROMISE_RACE",
        ts = "static race<T>(values: Promise<T>[]): Promise<T>"
    )]
    pub fn race(_values: Handle) -> Handle {
        unreachable!()
    }
    /// Promise.any(promises) — primeira a fulfill; reject so' se todas falharem.
    #[rts_fn(
        external,
        name = "any",
        symbol = "__RTS_FN_NS_PROMISE_ANY",
        ts = "static any<T>(values: Promise<T>[]): Promise<T>"
    )]
    pub fn any(_values: Handle) -> Handle {
        unreachable!()
    }
    /// Promise.allSettled(promises) — aguarda todas; sempre resolve com Vec de descriptors.
    #[rts_fn(
        external,
        name = "allSettled",
        symbol = "__RTS_FN_NS_PROMISE_ALL_SETTLED",
        ts = "static allSettled<T>(values: Promise<T>[]): Promise<any[]>"
    )]
    pub fn all_settled(_values: Handle) -> Handle {
        unreachable!()
    }
    /// Promise.try(fn) — invoca fn() sincronamente; retorna Promise.resolve(retval) ou Promise.reject(err) se fn lancar.
    #[rts_fn(
        external,
        name = "try",
        symbol = "__RTS_FN_GL_PROMISE_TRY",
        ts = "static try<T>(fn: () => T | Promise<T>): Promise<T>"
    )]
    pub fn try_(_fn: U64) -> I64 {
        unreachable!()
    }
    /// Promise.withResolvers() — ES2024: cria { promise, resolve, reject }. Util pra fora-do-construtor settle.
    #[rts_fn(
        external,
        name = "withResolvers",
        ts = "static withResolvers<T>(): { promise: Promise<T>, resolve: (v: T) => void, reject: (e: any) => void }"
    )]
    pub fn with_resolvers() -> Handle {
        unreachable!()
    }
}
