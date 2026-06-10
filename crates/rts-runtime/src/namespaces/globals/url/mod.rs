//! `url` — WHATWG URL + URLSearchParams. Migrado ao modelo `#[rts_namespace]`
//! + `#[rts_class]` (stage 2c) via membros `external`: os externs
//! `__RTS_FN_GL_URL_*` / `__RTS_FN_GL_USP_*` ficam em `instance.rs` intactos; os
//! macros derivam só o `SPEC` (namespace url) + os dois `*_CLASS_SPEC`.

pub mod instance;

#[allow(unused_imports)]
use rts_engine::abi::ty::{Bool, Handle, Str};
use rts_macro::{rts_class, rts_namespace};

/// URL parser: new/href/protocol/host/hostname/port/pathname/search/hash/origin/free.
#[rts_namespace(url, sym = "GL_URL")]
impl UrlNs {
    /// Parse URL string. Retorna URL handle ou 0 em caso de URL inválida.
    #[rts_fn(external, ts = "new URL(url: string): URL", pure)]
    pub fn new(_url: Str) -> Handle {
        unreachable!()
    }
    /// URL completa serializada.
    #[rts_fn(external, ts = "href(url: URL): string", pure)]
    pub fn href(_url: Handle) -> Handle {
        unreachable!()
    }
    /// Scheme com dois-pontos: 'https:'.
    #[rts_fn(external, ts = "protocol(url: URL): string", pure)]
    pub fn protocol(_url: Handle) -> Handle {
        unreachable!()
    }
    /// host:port (ou só host se porta padrão).
    #[rts_fn(external, ts = "host(url: URL): string", pure)]
    pub fn host(_url: Handle) -> Handle {
        unreachable!()
    }
    /// Hostname sem porta.
    #[rts_fn(external, ts = "hostname(url: URL): string", pure)]
    pub fn hostname(_url: Handle) -> Handle {
        unreachable!()
    }
    /// Porta como string (vazio se padrão).
    #[rts_fn(external, ts = "port(url: URL): string", pure)]
    pub fn port(_url: Handle) -> Handle {
        unreachable!()
    }
    /// Path da URL: '/foo/bar'.
    #[rts_fn(external, ts = "pathname(url: URL): string", pure)]
    pub fn pathname(_url: Handle) -> Handle {
        unreachable!()
    }
    /// Query string com '?': '?a=1&b=2' (vazio se ausente).
    #[rts_fn(external, ts = "search(url: URL): string", pure)]
    pub fn search(_url: Handle) -> Handle {
        unreachable!()
    }
    /// Fragment com '#': '#section' (vazio se ausente).
    #[rts_fn(external, ts = "hash(url: URL): string", pure)]
    pub fn hash(_url: Handle) -> Handle {
        unreachable!()
    }
    /// 'scheme://host:port' — origem da URL.
    #[rts_fn(external, ts = "origin(url: URL): string", pure)]
    pub fn origin(_url: Handle) -> Handle {
        unreachable!()
    }
    /// userinfo antes do ':' (vazio se ausente).
    #[rts_fn(external, ts = "username(url: URL): string", pure)]
    pub fn username(_url: Handle) -> Handle {
        unreachable!()
    }
    /// userinfo apos ':' (vazio se ausente).
    #[rts_fn(external, ts = "password(url: URL): string", pure)]
    pub fn password(_url: Handle) -> Handle {
        unreachable!()
    }
    /// Libera URL handle.
    #[rts_fn(external, ts = "free(url: URL): void")]
    pub fn free(_url: Handle) {
        unreachable!()
    }
}

/// WHATWG URL API — new URL(href)/href/protocol/host/hostname/port/pathname/search/hash/origin.
#[rts_class(URL, prefix = "URL", spec = "URL_CLASS_SPEC")]
impl UrlClass {
    /// new URL(url) — parse URL. Retorna URL handle ou 0 se inválida.
    #[rts_ctor(external, ts = "new URL(url: string): URL", pure)]
    pub fn new(_url: Str) -> Handle {
        unreachable!()
    }
    /// new URL(relative, base) — resolve relativa contra base URL.
    #[rts_ctor(external, ts = "new URL(url: string, base: string): URL", pure)]
    pub fn new_with_base(_url: Str, _base: Str) -> Handle {
        unreachable!()
    }
    /// url.href — URL completa serializada.
    #[rts_method(external, name = "href", ts = "readonly href: string", pure)]
    pub fn href(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// url.protocol — scheme com dois-pontos.
    #[rts_method(external, name = "protocol", ts = "readonly protocol: string", pure)]
    pub fn protocol(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// url.host — host:port.
    #[rts_method(external, name = "host", ts = "readonly host: string", pure)]
    pub fn host(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// url.hostname — hostname sem porta.
    #[rts_method(external, name = "hostname", ts = "readonly hostname: string", pure)]
    pub fn hostname(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// url.port — porta como string (vazio se padrão).
    #[rts_method(external, name = "port", ts = "readonly port: string", pure)]
    pub fn port(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// url.pathname — path: '/foo/bar'.
    #[rts_method(external, name = "pathname", ts = "readonly pathname: string", pure)]
    pub fn pathname(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// url.search — query string com '?'.
    #[rts_method(external, name = "search", ts = "readonly search: string", pure)]
    pub fn search(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// url.hash — fragment com '#'.
    #[rts_method(external, name = "hash", ts = "readonly hash: string", pure)]
    pub fn hash(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// url.origin — 'scheme://host:port'.
    #[rts_method(external, name = "origin", ts = "readonly origin: string", pure)]
    pub fn origin(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// url.username — userinfo antes do ':' (vazio se ausente).
    #[rts_method(external, name = "username", ts = "readonly username: string", pure)]
    pub fn username(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// url.password — userinfo apos ':' (vazio se ausente).
    #[rts_method(external, name = "password", ts = "readonly password: string", pure)]
    pub fn password(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// url.toString() — reconstroi href dinamicamente, considerando setters E mudancas em searchParams (cache).
    #[rts_method(external, name = "toString", ts = "toString(): string")]
    pub fn to_string(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// url.searchParams — URLSearchParams parseada do search query.
    #[rts_method(
        external,
        name = "searchParams",
        ts = "readonly searchParams: URLSearchParams"
    )]
    pub fn search_params(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// URL.canParse(href) — true se o URL parsa, false caso contrario.
    #[rts_fn(
        external,
        name = "canParse",
        ts = "static canParse(href: string): boolean",
        pure
    )]
    pub fn can_parse(_href: Str) -> Bool {
        unreachable!()
    }
    /// URL.canParse(href, base) — true se href resolve em relacao a base.
    #[rts_fn(
        external,
        name = "canParse",
        ts = "static canParse(href: string, base: string): boolean",
        pure
    )]
    pub fn can_parse_base(_href: Str, _base: Str) -> Bool {
        unreachable!()
    }
}

/// WHATWG URLSearchParams API minimal — get/has/set/delete/toString.
#[rts_class(URLSearchParams, prefix = "USP", spec = "URLSP_CLASS_SPEC")]
impl UrlSearchParamsClass {
    /// new URLSearchParams(init?) — init e' string "a=1&b=2".
    #[rts_ctor(
        external,
        ts = "new URLSearchParams(init?: string): URLSearchParams",
        pure
    )]
    pub fn new(_init: Str) -> Handle {
        unreachable!()
    }
    /// Retorna handle de string com value, ou 0 (undefined) se ausente.
    #[rts_method(external, name = "get", ts = "get(key: string): string | null", pure)]
    pub fn get(_recv: Handle, _key: Str) -> Handle {
        unreachable!()
    }
    /// True se key existe.
    #[rts_method(external, name = "has", ts = "has(key: string): boolean", pure)]
    pub fn has(_recv: Handle, _key: Str) -> Bool {
        unreachable!()
    }
    /// Substitui ou adiciona pair. Retorna self.
    #[rts_method(external, name = "set", ts = "set(key: string, value: string): void")]
    pub fn set(_recv: Handle, _key: Str, _value: Str) -> Handle {
        unreachable!()
    }
    /// Remove pair. Retorna self.
    #[rts_method(
        external,
        name = "delete",
        symbol = "__RTS_FN_GL_USP_DELETE",
        ts = "delete(key: string): void"
    )]
    pub fn delete_(_recv: Handle, _key: Str) -> Handle {
        unreachable!()
    }
    /// Serializa como 'a=1&b=2'.
    #[rts_method(external, name = "toString", ts = "toString(): string", pure)]
    pub fn to_string(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// Append (v0 limitacao: equivalente a set — sem multi-value).
    #[rts_method(
        external,
        name = "append",
        ts = "append(key: string, value: string): void"
    )]
    pub fn append(_recv: Handle, _key: Str, _value: Str) -> Handle {
        unreachable!()
    }
    /// Vec com 0 ou 1 elemento (v0 sem multi-value).
    #[rts_method(external, name = "getAll", ts = "getAll(key: string): string[]", pure)]
    pub fn get_all(_recv: Handle, _key: Str) -> Handle {
        unreachable!()
    }
    /// Vec de string handles com as keys.
    #[rts_method(external, name = "keys", ts = "keys(): string[]", pure)]
    pub fn keys(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// Vec de string handles com os values.
    #[rts_method(external, name = "values", ts = "values(): string[]", pure)]
    pub fn values(_recv: Handle) -> Handle {
        unreachable!()
    }
    /// Sort entries by key name (estavel, JS spec).
    #[rts_method(external, name = "sort", ts = "sort(): void")]
    pub fn sort(_recv: Handle) -> Handle {
        unreachable!()
    }
}
