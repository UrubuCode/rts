//! `URL` — WHATWG URL, authored as a normal Rust struct + `#[rtse::class]`.
//!
//! Migrated (DRAIN_MOTOR) from the hand-written `Entry::Env` slot-array +
//! per-accessor `#[no_mangle] extern "C"` fns to the `#[rtse::class]` authoring
//! macro (like `Point`/`URLSearchParams`). The instance is a normal Rust struct
//! (`Url`, owned `String` components) stored via the generic `Entry::Rtse`.
//! Getters, setters, `toString`/`toJSON`, `searchParams`, and the `canParse`
//! statics are all macro-authored (idiomatic Rust; `&self`/`&mut self`).
//!
//! **Left hand-written** (appended by `url::register_url_class_spec` via the
//! class-reopen pattern): the two `new URL(...)` constructors. WHATWG `new URL`
//! is FALLIBLE — it must yield a null handle (`0`) on an unparseable input, and
//! the macro's `#[rtse::ctor] -> Self` always allocates a struct (no
//! null/`Option<Self>` return path). So the ctors stay ordinary
//! `#[no_mangle] extern "C"` fns that return `0` on a parse failure and
//! `alloc_rtse("URL", …)` on success — the SAME `Entry::Rtse<Url>` storage the
//! macro members read. (Macro gap: a fallible/nullable constructor.)

use rts_engine::abi::ty::Handle;
use rts_engine::heap::handles::{alloc_rtse, with_rtse};

use super::parse::{ParsedUrl, percent_encode_form, percent_encode_path, percent_encode_search};
use super::search_params::{UrlSearchParams, parse_query_pairs};

/// A parsed WHATWG URL. Normal Rust struct; `#[rtse::class("URL")]` exposes it as
/// the JS `URL`. All components are owned `String`s (no `#[rtse::variable]` — the
/// WHATWG properties are COMPUTED/normalized, authored as getter/setter methods).
/// `sp_cache` is the memoized `URLSearchParams` handle (`0` = none) so
/// `url.searchParams` returns a stable, live instance whose mutations reflect in
/// `url.search`/`url.href` (cross-runtime #55). Not a GC root (re-created if the
/// cached instance was collected — validated live on read).
#[rtse::class("URL")]
#[derive(Clone)]
pub struct Url {
    protocol: String,
    username: String,
    password: String,
    hostname: String,
    port: String,
    pathname: String,
    search: String,
    hash: String,
    sp_cache: u64,
}

impl Url {
    /// Build from a `ParsedUrl` (fresh, no cached searchParams).
    fn from_parsed(p: ParsedUrl) -> Self {
        Url {
            protocol: p.protocol,
            username: p.username,
            password: p.password,
            hostname: p.hostname,
            port: p.port,
            pathname: p.pathname,
            search: p.search,
            hash: p.hash,
            sp_cache: 0,
        }
    }

    /// `hostname[:port]`. (`host_str` not `host` — the `#[rtse::class]` impl has a
    /// `host` getter method; two inherent `host` fns would collide.)
    fn host_str(&self) -> String {
        if self.port.is_empty() {
            self.hostname.clone()
        } else {
            format!("{}:{}", self.hostname, self.port)
        }
    }

    /// The CURRENT `search` string (`"?a=1&b=2"` or `""`), reconstructed from the
    /// cached `URLSearchParams` when it exists and is still live (so `url.
    /// searchParams.append/set/…` mutations show up), else the parsed `search`.
    fn current_search(&self) -> String {
        if self.sp_cache != 0 {
            let pairs = with_rtse::<UrlSearchParams, _>(self.sp_cache, |s| {
                s.map(|s| s.pairs().to_vec())
            });
            if let Some(pairs) = pairs {
                if pairs.is_empty() {
                    return String::new();
                }
                let mut s = String::from("?");
                for (i, (k, v)) in pairs.iter().enumerate() {
                    if i > 0 {
                        s.push('&');
                    }
                    s.push_str(&percent_encode_form(k));
                    s.push('=');
                    s.push_str(&percent_encode_form(v));
                }
                return s;
            }
        }
        self.search.clone()
    }

    /// The CURRENT full href, reconstructed from the components + the live
    /// searchParams. Shared by `href`/`toString`/`toJSON`.
    fn current_href(&self) -> String {
        let host = self.host_str();
        let userinfo = if self.username.is_empty() && self.password.is_empty() {
            String::new()
        } else if self.password.is_empty() {
            format!("{}@", self.username)
        } else {
            format!("{}:{}@", self.username, self.password)
        };
        format!(
            "{}//{}{}{}{}{}",
            self.protocol,
            userinfo,
            host,
            self.pathname,
            self.current_search(),
            self.hash
        )
    }
}

#[rtse::class("URL")]
impl Url {
    // ---- readonly getters (0-arg instance methods, read as properties) --------

    /// `url.href` — the full URL serialized (reflects setters + searchParams).
    #[rtse::method]
    fn href(self: &Url) -> String {
        self.current_href()
    }

    /// `url.protocol` — scheme with trailing colon (`https:`).
    #[rtse::method]
    fn protocol(self: &Url) -> String {
        self.protocol.clone()
    }

    /// `url.host` — `hostname[:port]`.
    #[rtse::method]
    fn host(self: &Url) -> String {
        self.host_str()
    }

    /// `url.hostname` — host without port.
    #[rtse::method]
    fn hostname(self: &Url) -> String {
        self.hostname.clone()
    }

    /// `url.port` — port as a string (empty if default/absent).
    #[rtse::method]
    fn port(self: &Url) -> String {
        self.port.clone()
    }

    /// `url.pathname` — path (`/foo/bar`).
    #[rtse::method]
    fn pathname(self: &Url) -> String {
        self.pathname.clone()
    }

    /// `url.search` — query string with `?` (reflects searchParams mutations).
    #[rtse::method]
    fn search(self: &Url) -> String {
        self.current_search()
    }

    /// `url.hash` — fragment with `#` (empty if absent).
    #[rtse::method]
    fn hash(self: &Url) -> String {
        self.hash.clone()
    }

    /// `url.origin` — `scheme://host:port`.
    #[rtse::method]
    fn origin(self: &Url) -> String {
        format!("{}//{}", self.protocol, self.host_str())
    }

    /// `url.username` — userinfo before the `:` (empty if absent).
    #[rtse::method]
    fn username(self: &Url) -> String {
        self.username.clone()
    }

    /// `url.password` — userinfo after the `:` (empty if absent).
    #[rtse::method]
    fn password(self: &Url) -> String {
        self.password.clone()
    }

    // ---- toString / toJSON ----------------------------------------------------

    /// `url.toString()` — reconstructs the href, reflecting setters AND
    /// searchParams mutations.
    #[rtse::method(name = "toString")]
    fn to_string(self: &Url) -> String {
        self.current_href()
    }

    /// `url.toJSON()` — the serialized href (`JSON.stringify(url)` uses this).
    #[rtse::method(name = "toJSON")]
    fn to_json(self: &Url) -> String {
        self.current_href()
    }

    // ---- searchParams (live, memoized) ---------------------------------------

    /// `url.searchParams` — the parsed `URLSearchParams`, memoized so repeated
    /// reads return the SAME live instance and its mutations reflect back in
    /// `url.search`/`url.href`. `&mut self` so the cache persists into the struct.
    #[rtse::method(name = "searchParams", returns = "URLSearchParams")]
    fn search_params(self: &mut Url) -> Handle {
        if self.sp_cache != 0 {
            let still_valid = with_rtse::<UrlSearchParams, _>(self.sp_cache, |s| s.is_some());
            if still_valid {
                return self.sp_cache;
            }
        }
        let sp = alloc_rtse(
            "URLSearchParams",
            UrlSearchParams::from_pairs(parse_query_pairs(&self.search)),
        );
        self.sp_cache = sp;
        sp
    }

    // ---- property setters (WHATWG URL is fully mutable) -----------------------

    /// `url.pathname = value` — normalize leading `/` + percent-encode.
    #[rtse::setter]
    fn set_pathname(self: &mut Url, value: &str) {
        let normalized = if value.starts_with('/') {
            value.to_string()
        } else {
            format!("/{value}")
        };
        self.pathname = percent_encode_path(&normalized);
    }

    /// `url.protocol = value` — normalize to a trailing `:`; ignore empty.
    #[rtse::setter]
    fn set_protocol(self: &mut Url, value: &str) {
        let raw = value.trim_end_matches(':');
        if raw.is_empty() {
            return;
        }
        self.protocol = format!("{raw}:");
    }

    /// `url.hostname = value`.
    #[rtse::setter]
    fn set_hostname(self: &mut Url, value: &str) {
        self.hostname = value.to_string();
    }

    /// `url.port = value` — digits only; empty clears the port.
    #[rtse::setter]
    fn set_port(self: &mut Url, value: &str) {
        if !value.is_empty() && !value.chars().all(|c| c.is_ascii_digit()) {
            return; // invalid port → ignore (WHATWG)
        }
        self.port = value.to_string();
    }

    /// `url.hash = value` — normalize leading `#`; empty clears the hash.
    #[rtse::setter]
    fn set_hash(self: &mut Url, value: &str) {
        self.hash = if value.is_empty() {
            String::new()
        } else if value.starts_with('#') {
            percent_encode_search(value)
        } else {
            percent_encode_search(&format!("#{value}"))
        };
    }

    /// `url.search = value` — normalize leading `?` + invalidate the searchParams
    /// cache (the next `url.searchParams` re-parses the new query).
    #[rtse::setter]
    fn set_search(self: &mut Url, value: &str) {
        self.search = if value.is_empty() {
            String::new()
        } else if value.starts_with('?') {
            percent_encode_search(value)
        } else {
            percent_encode_search(&format!("?{value}"))
        };
        self.sp_cache = 0;
    }

    /// `url.username = value`.
    #[rtse::setter]
    fn set_username(self: &mut Url, value: &str) {
        self.username = value.to_string();
    }

    /// `url.password = value`.
    #[rtse::setter]
    fn set_password(self: &mut Url, value: &str) {
        self.password = value.to_string();
    }

    /// `url.host = value` — split into `hostname[:port]`.
    #[rtse::setter]
    fn set_host(self: &mut Url, value: &str) {
        let (hostname, port) = match value.rsplit_once(':') {
            Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => (h, p),
            _ => (value, ""),
        };
        self.hostname = hostname.to_string();
        self.port = port.to_string();
    }

    /// `url.href = value` — re-parse the whole URL, overwriting every component.
    /// Invalid href → no-op (Node throws a TypeError; here an honest no-op).
    #[rtse::setter]
    fn set_href(self: &mut Url, value: &str) {
        let Some(p) = ParsedUrl::parse(value) else {
            return;
        };
        self.protocol = p.protocol;
        self.username = p.username;
        self.password = p.password;
        self.hostname = p.hostname;
        self.port = p.port;
        self.pathname = p.pathname;
        self.search = p.search;
        self.hash = p.hash;
        self.sp_cache = 0;
    }

    // ---- statics --------------------------------------------------------------

    /// `URL.canParse(href)` — true if `href` parses as an absolute URL.
    #[rtse::statical(name = "canParse")]
    fn can_parse(href: &str) -> bool {
        ParsedUrl::parse(href).is_some()
    }

    /// `URL.canParse(href, base)` — true if `href` resolves against `base`.
    #[rtse::statical(name = "canParse")]
    fn can_parse_base(href: &str, base: &str) -> bool {
        if ParsedUrl::parse(href).is_some() {
            return true;
        }
        let Some(base_url) = ParsedUrl::parse(base) else {
            return false;
        };
        let combined = if href.starts_with('/') {
            format!(
                "{}://{}{}",
                base_url.protocol.trim_end_matches(':'),
                base_url.host(),
                href
            )
        } else {
            format!(
                "{}://{}/{}",
                base_url.protocol.trim_end_matches(':'),
                base_url.host(),
                href
            )
        };
        ParsedUrl::parse(&combined).is_some()
    }
}

// ---------------------------------------------------------------------------
// Hand-written constructors (macro gap: a FALLIBLE ctor returning `0`/null on an
// unparseable input — the macro's `-> Self` ctor always allocates). Both alloc
// the SAME `Entry::Rtse<Url>` the macro members read. Symbols/signatures are
// preserved verbatim: `rts-node` (words.rs/legacy.rs) links against
// `__RTS_FN_GL_URL_NEW` / `__RTS_FN_GL_URL_NEW_WITH_BASE` by name.
// ---------------------------------------------------------------------------

fn str_from_parts(ptr: i64, len: i64) -> &'static str {
    if ptr == 0 || len == 0 {
        return "";
    }
    unsafe {
        let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
        std::str::from_utf8_unchecked(slice)
    }
}

/// `new URL(url)` — parse a URL string. Returns the `URL` handle, or `0`
/// (null) on an invalid URL.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_NEW(ptr: i64, len: i64) -> u64 {
    match ParsedUrl::parse(str_from_parts(ptr, len)) {
        Some(p) => alloc_rtse("URL", Url::from_parsed(p)),
        None => 0,
    }
}

/// `new URL(relative, base)` — resolve a relative URL against a base. Returns
/// the `URL` handle, or `0` (null) when the base is unparseable.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_NEW_WITH_BASE(
    rel_ptr: i64,
    rel_len: i64,
    base_ptr: i64,
    base_len: i64,
) -> u64 {
    let rel = str_from_parts(rel_ptr, rel_len);
    let base = str_from_parts(base_ptr, base_len);
    // If the relative already has its own scheme (http://, …), use it directly.
    if rel.contains("://") {
        return __RTS_FN_GL_URL_NEW(rel_ptr, rel_len);
    }
    let Some(base_parsed) = ParsedUrl::parse(base) else {
        return 0;
    };
    // Resolve the path relative to base.pathname.
    let resolved_path = if rel.starts_with('/') {
        rel.to_string()
    } else {
        let base_dir = match base_parsed.pathname.rsplit_once('/') {
            Some((dir, _)) => format!("{dir}/"),
            None => "/".to_string(),
        };
        format!("{base_dir}{rel}")
    };
    // Reuse the parser to normalize segments + extract search/hash.
    let combined = format!(
        "{}//{}{}",
        base_parsed.protocol,
        base_parsed.host(),
        resolved_path
    );
    match ParsedUrl::parse(&combined) {
        Some(p) => alloc_rtse("URL", Url::from_parsed(p)),
        None => 0,
    }
}
