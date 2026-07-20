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

/// Default User-Agent every RTS-originated HTTP request identifies with —
/// browser-style: `Rts v<version>` on release builds, `Rts development` on
/// debug builds. Shared by the global `fetch()` and the CLI URL runner.
pub fn default_user_agent() -> &'static str {
    if cfg!(debug_assertions) {
        "Rts development"
    } else {
        concat!("Rts v", env!("CARGO_PKG_VERSION"))
    }
}

/// User-Agent de NAVEGADOR — usado pelo caminho do mini-browser (`fetchText`/
/// `fetchTextAsync`/`fetchBytes`), NÃO pelo `fetch()` genérico de API. Muitos
/// sites (google, cloudflare, etc.) servem HTML DIFERENTE por User-Agent: com
/// um UA de bot vem uma página legada/degradada; com UA de Chrome vem a página
/// moderna com todo o CSS inline. Como o RTS-browser É um browser, ele se
/// identifica como um. Emparelhado com `browser_headers` (Accept/Accept-Language)
/// que alguns servidores também exigem para servir o conteúdo completo.
pub fn browser_user_agent() -> &'static str {
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"
}

/// Os cabeçalhos que um navegador manda além do User-Agent — `Accept`
/// (prioriza HTML), `Accept-Language`. Aplicados no caminho do browser para o
/// servidor entregar a página humana completa. (`Accept-Encoding` fica de fora:
/// o ureq não descomprime brotli e pediríamos algo que não sabemos ler.)
pub fn browser_headers() -> &'static [(&'static str, &'static str)] {
    &[
        (
            "Accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,\
             image/webp,image/apng,*/*;q=0.8",
        ),
        ("Accept-Language", "pt-BR,pt;q=0.9,en;q=0.8"),
        // Fetch Metadata (`Sec-Fetch-*`): TODO browser moderno os manda numa
        // navegação de topo. Sites com anti-bot (WhatsApp Web, Meta, …) RECUSAM
        // (HTTP 400 / página de erro) a requisição sem eles — é o que separava
        // o app real da página "Browser not supported". Valores de uma
        // navegação de documento de topo iniciada pelo usuário.
        ("Sec-Fetch-Dest", "document"),
        ("Sec-Fetch-Mode", "navigate"),
        ("Sec-Fetch-Site", "none"),
        ("Sec-Fetch-User", "?1"),
        ("Upgrade-Insecure-Requests", "1"),
    ]
}

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
        emit: None,
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
        .member(m(
            "fetchText",
            MemberKind::Function,
            Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_NS_FETCH_FETCH_TEXT",
            "fetchText(url: string): string",
            "fetchText(url) — GET the URL and return the body as a string (sync). '' on network error.",
            false,
        ))
        .member(m(
            "fetchTextAsync",
            MemberKind::Function,
            Sig::new(vec![AbiType::StrPtr], AbiType::U64),
            "__RTS_FN_NS_FETCH_FETCH_TEXT_ASYNC",
            "fetchTextAsync(url: string): number",
            "fetchTextAsync(url) — start a GET on a background thread; returns a ticket immediately (no UI block).",
            false,
        ))
        // ── Overrides de request (o app se apresenta na rede como quiser) ──────
        .member(m(
            "setUserAgent",
            MemberKind::Function,
            Sig::new(vec![AbiType::StrPtr], AbiType::Void),
            "__RTS_FN_NS_FETCH_SET_USER_AGENT",
            "setUserAgent(ua: string): void",
            "setUserAgent(ua) — override the browser-path User-Agent for all subsequent fetches ('' = default).",
            false,
        ))
        .member(m(
            "setHeader",
            MemberKind::Function,
            Sig::new(vec![AbiType::StrPtr, AbiType::StrPtr], AbiType::Void),
            "__RTS_FN_NS_FETCH_SET_HEADER",
            "setHeader(name: string, value: string): void",
            "setHeader(name, value) — inject/override a request header (Cookie/Referer/Sec-Fetch-*/…); '' value removes it.",
            false,
        ))
        .member(m(
            "clearOverrides",
            MemberKind::Function,
            Sig::new(vec![], AbiType::Void),
            "__RTS_FN_NS_FETCH_CLEAR_OVERRIDES",
            "clearOverrides(): void",
            "clearOverrides() — drop all setUserAgent/setHeader overrides (back to browser defaults).",
            false,
        ))
        .member(m(
            "fetchPoll",
            MemberKind::Function,
            Sig::new(vec![AbiType::U64], AbiType::I64),
            "__RTS_FN_NS_FETCH_FETCH_POLL",
            "fetchPoll(ticket: number): number",
            "fetchPoll(ticket) — 1 if done, 0 if downloading, -1 if invalid.",
            false,
        ))
        .member(m(
            "fetchTake",
            MemberKind::Function,
            Sig::new(vec![AbiType::U64], AbiType::Handle),
            "__RTS_FN_NS_FETCH_FETCH_TAKE",
            "fetchTake(ticket: number): string",
            "fetchTake(ticket) — the downloaded body as a string, and removes the ticket. '' if not ready.",
            false,
        ))
        .member(m(
            "fetchBytesAsync",
            MemberKind::Function,
            Sig::new(vec![AbiType::StrPtr], AbiType::U64),
            "__RTS_FN_NS_FETCH_FETCH_BYTES_ASYNC",
            "fetchBytesAsync(url: string): number",
            "fetchBytesAsync(url) — start a BINARY GET on a thread (raw bytes, for images); ticket immediately.",
            false,
        ))
        .member(m(
            "fetchBytesPoll",
            MemberKind::Function,
            Sig::new(vec![AbiType::U64], AbiType::I64),
            "__RTS_FN_NS_FETCH_FETCH_BYTES_POLL",
            "fetchBytesPoll(ticket: number): number",
            "fetchBytesPoll(ticket) — 1 done, 0 downloading, -1 invalid.",
            false,
        ))
        .member(m(
            "fetchBytesTake",
            MemberKind::Function,
            Sig::new(vec![AbiType::U64], AbiType::Handle),
            "__RTS_FN_NS_FETCH_FETCH_BYTES_TAKE",
            "fetchBytesTake(ticket: number): number",
            "fetchBytesTake(ticket) — the raw bytes as a Buffer handle; pass buffer.ptr/len to imgdec.",
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

// NOTA (Fase 2): `register_promise_class_spec` migrou p/ `rts-primitives/
// promise.rs` (Promise é primordial). A impl tokio (corpos __RTS_FN_GL_
// PROMISE_* / __RTS_FN_NS_PROMISE_*) fica aqui em rts-std; só a spec
// (metadata pura, fn_ptr null) saiu. Resolve por símbolo (JIT/AOT).
