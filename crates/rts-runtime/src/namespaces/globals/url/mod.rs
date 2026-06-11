//! `url` — WHATWG URL + URLSearchParams.
//!
//! Migrado do `#[rts_namespace]` + `#[rts_class]` (macro) pro modelo builder
//! hand-written do `rts-engine` (rumo à remoção da `rts-macro`). Todos os
//! membros são `external`: os externs `__RTS_FN_GL_URL_*` / `__RTS_FN_GL_USP_*`
//! ficam em `instance.rs` intactos. Aqui só publicamos o `SPEC` (namespace url)
//! + os dois `*_CLASS_SPEC` (URL + URLSearchParams) — sem reemitir extern, com
//! `fn_ptr` null.

pub mod instance;

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

/// Membro `external` (helper hand-written): aponta para um extern já existente
/// (em `instance.rs`) sem reemitir, `fn_ptr` null.
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

/// Registra a namespace `url` no motor (hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("url")
        .doc("URL parser: new/href/protocol/host/hostname/port/pathname/search/hash/origin/free.")
        .member(m(
            "new",
            MemberKind::Function,
            Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_GL_URL_NEW",
            "new URL(url: string): URL",
            "Parse URL string. Retorna URL handle ou 0 em caso de URL inválida.",
            true,
        ))
        .member(m(
            "href",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_URL_HREF",
            "href(url: URL): string",
            "URL completa serializada.",
            true,
        ))
        .member(m(
            "protocol",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_URL_PROTOCOL",
            "protocol(url: URL): string",
            "Scheme com dois-pontos: 'https:'.",
            true,
        ))
        .member(m(
            "host",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_URL_HOST",
            "host(url: URL): string",
            "host:port (ou só host se porta padrão).",
            true,
        ))
        .member(m(
            "hostname",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_URL_HOSTNAME",
            "hostname(url: URL): string",
            "Hostname sem porta.",
            true,
        ))
        .member(m(
            "port",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_URL_PORT",
            "port(url: URL): string",
            "Porta como string (vazio se padrão).",
            true,
        ))
        .member(m(
            "pathname",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_URL_PATHNAME",
            "pathname(url: URL): string",
            "Path da URL: '/foo/bar'.",
            true,
        ))
        .member(m(
            "search",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_URL_SEARCH",
            "search(url: URL): string",
            "Query string com '?': '?a=1&b=2' (vazio se ausente).",
            true,
        ))
        .member(m(
            "hash",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_URL_HASH",
            "hash(url: URL): string",
            "Fragment com '#': '#section' (vazio se ausente).",
            true,
        ))
        .member(m(
            "origin",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_URL_ORIGIN",
            "origin(url: URL): string",
            "'scheme://host:port' — origem da URL.",
            true,
        ))
        .member(m(
            "username",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_URL_USERNAME",
            "username(url: URL): string",
            "userinfo antes do ':' (vazio se ausente).",
            true,
        ))
        .member(m(
            "password",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_URL_PASSWORD",
            "password(url: URL): string",
            "userinfo apos ':' (vazio se ausente).",
            true,
        ))
        .member(m(
            "free",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::Void),
            "__RTS_FN_GL_URL_FREE",
            "free(url: URL): void",
            "Libera URL handle.",
            false,
        ))
        .done();
}

/// Registra a classe global `URL` no motor (hand-written, sem macro).
pub fn register_url_class_spec(e: &mut Engine) {
    e.class("URL")
        .doc("WHATWG URL API — new URL(href)/href/protocol/host/hostname/port/pathname/search/hash/origin.")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_GL_URL_NEW",
            "new URL(url: string): URL",
            "new URL(url) — parse URL. Retorna URL handle ou 0 se inválida.",
            true,
        ))
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::StrPtr, AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_GL_URL_NEW_WITH_BASE",
            "new URL(url: string, base: string): URL",
            "new URL(relative, base) — resolve relativa contra base URL.",
            true,
        ))
        .member(m(
            "href",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_URL_HREF",
            "readonly href: string",
            "url.href — URL completa serializada.",
            true,
        ))
        .member(m(
            "protocol",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_URL_PROTOCOL",
            "readonly protocol: string",
            "url.protocol — scheme com dois-pontos.",
            true,
        ))
        .member(m(
            "host",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_URL_HOST",
            "readonly host: string",
            "url.host — host:port.",
            true,
        ))
        .member(m(
            "hostname",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_URL_HOSTNAME",
            "readonly hostname: string",
            "url.hostname — hostname sem porta.",
            true,
        ))
        .member(m(
            "port",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_URL_PORT",
            "readonly port: string",
            "url.port — porta como string (vazio se padrão).",
            true,
        ))
        .member(m(
            "pathname",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_URL_PATHNAME",
            "readonly pathname: string",
            "url.pathname — path: '/foo/bar'.",
            true,
        ))
        .member(m(
            "search",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_URL_SEARCH",
            "readonly search: string",
            "url.search — query string com '?'.",
            true,
        ))
        .member(m(
            "hash",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_URL_HASH",
            "readonly hash: string",
            "url.hash — fragment com '#'.",
            true,
        ))
        .member(m(
            "origin",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_URL_ORIGIN",
            "readonly origin: string",
            "url.origin — 'scheme://host:port'.",
            true,
        ))
        .member(m(
            "username",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_URL_USERNAME",
            "readonly username: string",
            "url.username — userinfo antes do ':' (vazio se ausente).",
            true,
        ))
        .member(m(
            "password",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_URL_PASSWORD",
            "readonly password: string",
            "url.password — userinfo apos ':' (vazio se ausente).",
            true,
        ))
        .member(m(
            "toString",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_URL_TO_STRING",
            "toString(): string",
            "url.toString() — reconstroi href dinamicamente, considerando setters E mudancas em searchParams (cache).",
            false,
        ))
        .member(m(
            "searchParams",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_URL_SEARCH_PARAMS",
            "readonly searchParams: URLSearchParams",
            "url.searchParams — URLSearchParams parseada do search query.",
            false,
        ))
        .member(m(
            "canParse",
            MemberKind::Function,
            Sig::new(vec![AbiType::StrPtr], AbiType::Bool),
            "__RTS_FN_GL_URL_CAN_PARSE",
            "static canParse(href: string): boolean",
            "URL.canParse(href) — true se o URL parsa, false caso contrario.",
            true,
        ))
        .member(m(
            "canParse",
            MemberKind::Function,
            Sig::new(vec![AbiType::StrPtr, AbiType::StrPtr], AbiType::Bool),
            "__RTS_FN_GL_URL_CAN_PARSE_BASE",
            "static canParse(href: string, base: string): boolean",
            "URL.canParse(href, base) — true se href resolve em relacao a base.",
            true,
        ))
        .done();
}

/// Registra a classe global `URLSearchParams` no motor (hand-written, sem macro).
pub fn register_urlsp_class_spec(e: &mut Engine) {
    e.class("URLSearchParams")
        .doc("WHATWG URLSearchParams API minimal — get/has/set/delete/toString.")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_GL_USP_NEW",
            "new URLSearchParams(init?: string): URLSearchParams",
            "new URLSearchParams(init?) — init e' string \"a=1&b=2\".",
            true,
        ))
        .member(m(
            "get",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_GL_USP_GET",
            "get(key: string): string | null",
            "Retorna handle de string com value, ou 0 (undefined) se ausente.",
            true,
        ))
        .member(m(
            "has",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Bool),
            "__RTS_FN_GL_USP_HAS",
            "has(key: string): boolean",
            "True se key existe.",
            true,
        ))
        .member(m(
            "set",
            MemberKind::InstanceMethod,
            Sig::new(
                vec![AbiType::Handle, AbiType::StrPtr, AbiType::StrPtr],
                AbiType::Handle,
            ),
            "__RTS_FN_GL_USP_SET",
            "set(key: string, value: string): void",
            "Substitui ou adiciona pair. Retorna self.",
            false,
        ))
        .member(m(
            "delete",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_GL_USP_DELETE",
            "delete(key: string): void",
            "Remove pair. Retorna self.",
            false,
        ))
        .member(m(
            "toString",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_USP_TO_STRING",
            "toString(): string",
            "Serializa como 'a=1&b=2'.",
            true,
        ))
        .member(m(
            "append",
            MemberKind::InstanceMethod,
            Sig::new(
                vec![AbiType::Handle, AbiType::StrPtr, AbiType::StrPtr],
                AbiType::Handle,
            ),
            "__RTS_FN_GL_USP_APPEND",
            "append(key: string, value: string): void",
            "Append (v0 limitacao: equivalente a set — sem multi-value).",
            false,
        ))
        .member(m(
            "getAll",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_GL_USP_GET_ALL",
            "getAll(key: string): string[]",
            "Vec com 0 ou 1 elemento (v0 sem multi-value).",
            true,
        ))
        .member(m(
            "keys",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_USP_KEYS",
            "keys(): string[]",
            "Vec de string handles com as keys.",
            true,
        ))
        .member(m(
            "values",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_USP_VALUES",
            "values(): string[]",
            "Vec de string handles com os values.",
            true,
        ))
        .member(m(
            "sort",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_USP_SORT",
            "sort(): void",
            "Sort entries by key name (estavel, JS spec).",
            false,
        ))
        .done();
}
