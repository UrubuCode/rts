//! `url` — WHATWG URL + URLSearchParams.
//!
//! `URL` (DRAIN_MOTOR §8-9): stays fully hand-written in `instance.rs` — the
//! WHATWG URL API is almost entirely MUTABLE `String` properties
//! (`url.protocol = "http:"`, `url.pathname = "/x"`, …), and today's
//! `#[rtse::class]` macro only generates an `InstanceSetter` for a SCALAR
//! `#[rtse::variable]` field (`f64`/`i64`/`i32`/`bool`) — there is no
//! `String`-typed instance-setter authoring path yet (a real gap; see the
//! doc-comment on `register_url_class_spec`). `URL`'s READONLY getters (`href`,
//! `host`, …) already resolve as 0-arg `InstanceMethod`s, same as a macro
//! `#[rtse::method]` would emit — only the setters block a full migration.
//!
//! `URLSearchParams` (`search_params.rs`): migrated. Ctor + get/has/set/delete/
//! append/sort/toString/size are `#[rtse::class]`-authored (idiomatic Rust,
//! `Entry::Rtse<UrlSearchParams>` storage) — no `String` PROPERTY setters are
//! needed here (its whole JS surface is METHOD calls), so the macro's
//! `InstanceMethod`-only authoring is sufficient. `getAll`/`keys`/`values`/
//! `entries` (return a classified `string[]`/`[string,string][]` — F8, not
//! landed) and `forEach` (a function-VALUE callback param — no `PolyValue`
//! param support) stay hand-written in `search_params.rs`, reading the same
//! struct via `with_rtse`.

pub mod instance;
pub mod search_params;

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

/// Endereço real do extern `__RTS_FN_GL_URL_*` / `__RTS_FN_GL_USP_*` (vivem em
/// `instance.rs`, neste mesmo crate). Os membros eram `external` (fn_ptr null) e
/// supridos à mão pelo `registry_link` do motor; agora carregam o endereço real e
/// o HARVEST do Registry (`all_jit_symbols`) instala o símbolo JIT — sem lista-mão.
fn fp_for(symbol: &str) -> *const u8 {
    match symbol {
        "__RTS_FN_GL_URL_CAN_PARSE" => instance::__RTS_FN_GL_URL_CAN_PARSE as *const u8,
        "__RTS_FN_GL_URL_CAN_PARSE_BASE" => instance::__RTS_FN_GL_URL_CAN_PARSE_BASE as *const u8,
        "__RTS_FN_GL_URL_FREE" => instance::__RTS_FN_GL_URL_FREE as *const u8,
        "__RTS_FN_GL_URL_HASH" => instance::__RTS_FN_GL_URL_HASH as *const u8,
        "__RTS_FN_GL_URL_HOST" => instance::__RTS_FN_GL_URL_HOST as *const u8,
        "__RTS_FN_GL_URL_HOSTNAME" => instance::__RTS_FN_GL_URL_HOSTNAME as *const u8,
        "__RTS_FN_GL_URL_HREF" => instance::__RTS_FN_GL_URL_HREF as *const u8,
        "__RTS_FN_GL_URL_NEW" => instance::__RTS_FN_GL_URL_NEW as *const u8,
        "__RTS_FN_GL_URL_NEW_WITH_BASE" => instance::__RTS_FN_GL_URL_NEW_WITH_BASE as *const u8,
        "__RTS_FN_GL_URL_ORIGIN" => instance::__RTS_FN_GL_URL_ORIGIN as *const u8,
        "__RTS_FN_GL_URL_PASSWORD" => instance::__RTS_FN_GL_URL_PASSWORD as *const u8,
        "__RTS_FN_GL_URL_PATHNAME" => instance::__RTS_FN_GL_URL_PATHNAME as *const u8,
        "__RTS_FN_GL_URL_PORT" => instance::__RTS_FN_GL_URL_PORT as *const u8,
        "__RTS_FN_GL_URL_PROTOCOL" => instance::__RTS_FN_GL_URL_PROTOCOL as *const u8,
        "__RTS_FN_GL_URL_SEARCH" => instance::__RTS_FN_GL_URL_SEARCH as *const u8,
        "__RTS_FN_GL_URL_SEARCH_PARAMS" => instance::__RTS_FN_GL_URL_SEARCH_PARAMS as *const u8,
        "__RTS_FN_GL_URL_SET_PATHNAME" => instance::__RTS_FN_GL_URL_SET_PATHNAME as *const u8,
        "__RTS_FN_GL_URL_SET_PROTOCOL" => instance::__RTS_FN_GL_URL_SET_PROTOCOL as *const u8,
        "__RTS_FN_GL_URL_SET_HOSTNAME" => instance::__RTS_FN_GL_URL_SET_HOSTNAME as *const u8,
        "__RTS_FN_GL_URL_SET_PORT" => instance::__RTS_FN_GL_URL_SET_PORT as *const u8,
        "__RTS_FN_GL_URL_SET_HASH" => instance::__RTS_FN_GL_URL_SET_HASH as *const u8,
        "__RTS_FN_GL_URL_SET_SEARCH" => instance::__RTS_FN_GL_URL_SET_SEARCH as *const u8,
        "__RTS_FN_GL_URL_SET_USERNAME" => instance::__RTS_FN_GL_URL_SET_USERNAME as *const u8,
        "__RTS_FN_GL_URL_SET_PASSWORD" => instance::__RTS_FN_GL_URL_SET_PASSWORD as *const u8,
        "__RTS_FN_GL_URL_SET_HOST" => instance::__RTS_FN_GL_URL_SET_HOST as *const u8,
        "__RTS_FN_GL_URL_SET_HREF" => instance::__RTS_FN_GL_URL_SET_HREF as *const u8,
        "__RTS_FN_GL_URL_TO_STRING" => instance::__RTS_FN_GL_URL_TO_STRING as *const u8,
        "__RTS_FN_GL_URL_USERNAME" => instance::__RTS_FN_GL_URL_USERNAME as *const u8,
        // URLSearchParams: get/has/set/delete/append/sort/toString/size/new are
        // now `#[rtse::class]`-authored (search_params.rs) — their `fn_ptr` is
        // wired directly by the macro, not looked up here. Only the residual
        // hand-written members (array-return / callback-param macro gaps) still
        // route through `fp_for`.
        "__RTS_FN_GL_USP_GET_ALL" => search_params::__RTS_FN_GL_USP_GET_ALL as *const u8,
        "__RTS_FN_GL_USP_KEYS" => search_params::__RTS_FN_GL_USP_KEYS as *const u8,
        "__RTS_FN_GL_USP_VALUES" => search_params::__RTS_FN_GL_USP_VALUES as *const u8,
        "__RTS_FN_GL_USP_ENTRIES" => search_params::__RTS_FN_GL_USP_ENTRIES as *const u8,
        "__RTS_FN_GL_USP_FOR_EACH" => search_params::__RTS_FN_GL_USP_FOR_EACH as *const u8,
        _ => core::ptr::null(),
    }
}

/// Membro `external` (helper hand-written): o extern vive em `instance.rs`; o
/// `fn_ptr` real vem de [`fp_for`] (o motor instala o símbolo via harvest).
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
        fn_ptr: FnPtr(fp_for(symbol)),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure,
        emit: None,
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
            "toJSON",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_URL_TO_STRING",
            "toJSON(): string",
            "url.toJSON() — o href serializado (JSON.stringify(url) usa isto). Igual a toString().",
            false,
        ))
        // ---- property SETTERS (WHATWG URL is fully mutable) ----------------
        .member(m(
            "pathname",
            MemberKind::InstanceSetter,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Void),
            "__RTS_FN_GL_URL_SET_PATHNAME",
            "set pathname(value: string)",
            "url.pathname = value — percent-encode + normaliza leading /.",
            false,
        ))
        .member(m(
            "protocol",
            MemberKind::InstanceSetter,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Void),
            "__RTS_FN_GL_URL_SET_PROTOCOL",
            "set protocol(value: string)",
            "url.protocol = value.",
            false,
        ))
        .member(m(
            "hostname",
            MemberKind::InstanceSetter,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Void),
            "__RTS_FN_GL_URL_SET_HOSTNAME",
            "set hostname(value: string)",
            "url.hostname = value.",
            false,
        ))
        .member(m(
            "port",
            MemberKind::InstanceSetter,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Void),
            "__RTS_FN_GL_URL_SET_PORT",
            "set port(value: string)",
            "url.port = value.",
            false,
        ))
        .member(m(
            "hash",
            MemberKind::InstanceSetter,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Void),
            "__RTS_FN_GL_URL_SET_HASH",
            "set hash(value: string)",
            "url.hash = value — normaliza leading #.",
            false,
        ))
        .member(m(
            "search",
            MemberKind::InstanceSetter,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Void),
            "__RTS_FN_GL_URL_SET_SEARCH",
            "set search(value: string)",
            "url.search = value — normaliza leading ? + invalida cache de searchParams.",
            false,
        ))
        .member(m(
            "username",
            MemberKind::InstanceSetter,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Void),
            "__RTS_FN_GL_URL_SET_USERNAME",
            "set username(value: string)",
            "url.username = value.",
            false,
        ))
        .member(m(
            "password",
            MemberKind::InstanceSetter,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Void),
            "__RTS_FN_GL_URL_SET_PASSWORD",
            "set password(value: string)",
            "url.password = value.",
            false,
        ))
        .member(m(
            "host",
            MemberKind::InstanceSetter,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Void),
            "__RTS_FN_GL_URL_SET_HOST",
            "set host(value: string)",
            "url.host = value — split hostname:port.",
            false,
        ))
        .member(m(
            "href",
            MemberKind::InstanceSetter,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Void),
            "__RTS_FN_GL_URL_SET_HREF",
            "set href(value: string)",
            "url.href = value — re-parseia a URL inteira.",
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

/// Registra a classe global `URLSearchParams`. Ctor + get/has/set/delete/
/// append/sort/toString/size vêm do `#[rtse::class]` em `search_params.rs`
/// (autoria idiomática, macro G4+F1+F4). `getAll`/`keys`/`values`/`entries`/
/// `forEach` são gaps do macro hoje (retorno `string[]`/`[k,v][]` classificado
/// = F8; `forEach` recebe uma função-VALOR, sem suporte a `PolyValue` como
/// param) — ficam hand-written (mesmo `Entry::Rtse<UrlSearchParams>`), e são
/// ANEXADOS aqui porque `ClassBuilder::done()` SUBSTITUI (não funde) a entrada
/// anterior no Registry: lemos de volta os membros que o macro já registrou e
/// os re-emitimos junto dos residuais numa única `.done()`.
pub fn register_urlsp_class_spec(e: &mut Engine) {
    // 1) Macro-authored ctor/instance methods (search_params.rs::register).
    search_params::register(e);
    let macro_members: Vec<Member> = e
        .registry()
        .class("URLSearchParams")
        .map(|c| c.members.clone())
        .unwrap_or_default();
    // 2) Re-open the class, replay the macro members, then append the residual
    // hand-written ones.
    let mut cb = e
        .class("URLSearchParams")
        .doc("WHATWG URLSearchParams API — get/has/set/delete/append/sort/toString/size (rtse-authored) + getAll/keys/values/entries/forEach (hand-written: array-return/callback-param macro gaps).");
    for mem in macro_members {
        cb = cb.member(mem);
    }
    cb.member(m(
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
            "entries",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_USP_ENTRIES",
            "entries(): [string, string][]",
            "Array de pares [key, value] em ordem (com duplicatas).",
            true,
        ))
        .member(m(
            "forEach",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::PolyValue], AbiType::Handle),
            "__RTS_FN_GL_USP_FOR_EACH",
            "forEach(callback: (value: string, key: string, searchParams: URLSearchParams) => void): void",
            "Invoca callback(value, key, self) por par (snapshot).",
            false,
        ))
        .done();
}
