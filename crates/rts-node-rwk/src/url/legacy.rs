//! `url.parse`/`url.format`/`url.resolve` — Stability 0, Legacy. Node's own
//! docs call the real algorithm "lenient, non-standard" string splitting,
//! historically its own multi-hundred-line state machine; this is a
//! best-effort approximation built on the `url` crate's real parser rather
//! than a reimplementation of that state machine, and it is NOT verified
//! byte-for-byte against Node — the same caveat the module doc states for
//! this whole approximation.

use rts_core_rwk::entry::{self, Context};

/// `url.parse(urlString, parseQueryString?, slashesDenoteHost?)`.
///
/// `slashesDenoteHost` is accepted and ignored: `url::Url` already treats
/// `//host/path` as host-bearing whenever the surrounding scheme is one it
/// knows to be "special" (`http`, `file`, …), which is the common case this
/// flag exists for; a scheme this crate's parser treats as opaque (an
/// unrecognized custom scheme) is not retried under the flag.
pub(super) extern "C" fn parse(_e: u64, _this: u64, url_string: u64, parse_query: u64, _slashes: u64, _d: u64) -> u64 {
    let Some(text) = super::text(url_string) else {
        return entry::with_runtime(legacy_object_empty);
    };
    let parse_query = entry::to_boolean(parse_query);
    let parsed = url::Url::parse(&text);
    entry::with_runtime(|context| match parsed {
        Ok(parsed) => legacy_object(context, &parsed, parse_query),
        // Not a real URL — Node's lenient parser still answers a partial
        // object rather than throwing in the common case (an unparseable
        // relative string). `href`/`path`/`pathname` carry the raw text;
        // every other field is `null`, which is the honest "did not parse
        // as a component" answer rather than a guess.
        Err(_) => {
            let object = entry::make_object(context);
            for name in ["auth", "hash", "host", "hostname", "port", "protocol", "query", "search"] {
                let null = entry::null_in(context);
                entry::put_member(context, object, name, null);
            }
            let text_value = super::string_in(context, &text);
            entry::put_member(context, object, "href", text_value);
            let path_value = super::string_in(context, &text);
            entry::put_member(context, object, "path", path_value);
            let pathname_value = super::string_in(context, &text);
            entry::put_member(context, object, "pathname", pathname_value);
            let slashes = entry::boolean_value(text.contains("//"));
            entry::put_member(context, object, "slashes", slashes);
            object
        }
    })
}

fn legacy_object_empty(context: &mut Context) -> u64 {
    entry::make_object(context)
}

/// The `LegacyUrlObject` a successfully parsed `url::Url` reduces to.
fn legacy_object(context: &mut Context, parsed: &url::Url, parse_query: bool) -> u64 {
    let object = entry::make_object(context);
    let auth = legacy_auth(parsed);
    set_opt(context, object, "auth", auth.as_deref());
    let hash = parsed.fragment().map(|fragment| format!("#{fragment}"));
    set_opt(context, object, "hash", hash.as_deref());
    let host = parsed.host_str().map(|host| super::class::host_with_port(parsed, host));
    set_opt(context, object, "host", host.as_deref());
    set_opt(context, object, "hostname", parsed.host_str());
    let href = super::string_in(context, parsed.as_str());
    entry::put_member(context, object, "href", href);
    let path = format!("{}{}", parsed.path(), parsed.query().map(|q| format!("?{q}")).unwrap_or_default());
    let path_value = super::string_in(context, &path);
    entry::put_member(context, object, "path", path_value);
    let pathname_value = super::string_in(context, parsed.path());
    entry::put_member(context, object, "pathname", pathname_value);
    let port = parsed.port().map(|port| port.to_string());
    set_opt(context, object, "port", port.as_deref());
    let protocol = format!("{}:", parsed.scheme());
    let protocol_value = super::string_in(context, &protocol);
    entry::put_member(context, object, "protocol", protocol_value);
    let query_value = query_value(context, parsed, parse_query);
    entry::put_member(context, object, "query", query_value);
    let search = parsed.query().map(|query| format!("?{query}"));
    set_opt(context, object, "search", search.as_deref());
    let slashes = entry::boolean_value(parsed.as_str().contains("://"));
    entry::put_member(context, object, "slashes", slashes);
    object
}

fn legacy_auth(parsed: &url::Url) -> Option<String> {
    if parsed.username().is_empty() && parsed.password().is_none() {
        return None;
    }
    match parsed.password() {
        Some(password) => Some(format!("{}:{password}", parsed.username())),
        None => Some(parsed.username().to_owned()),
    }
}

fn query_value(context: &mut Context, parsed: &url::Url, parse_query: bool) -> u64 {
    let Some(query) = parsed.query() else {
        return entry::null_in(context);
    };
    if !parse_query {
        return super::string_in(context, query);
    }
    let object = entry::make_object(context);
    for (name, value) in form_urlencoded::parse(query.as_bytes()) {
        let held = super::string_in(context, &value);
        entry::put_member(context, object, &name, held);
    }
    object
}

fn set_opt(context: &mut Context, object: u64, name: &str, value: Option<&str>) {
    let held = match value {
        Some(text) => super::string_in(context, text),
        None => entry::null_in(context),
    };
    entry::put_member(context, object, name, held);
}

/// `url.format(urlObjectOrString, options?)` — three overloads: a legacy
/// `LegacyUrlObject`-shaped plain object, a `URL` instance (with
/// `UrlFormatOptions`), or a bare string (`new URL(s).toString()`, DEP0149).
pub(super) extern "C" fn format(_e: u64, _this: u64, value: u64, options: u64, _c: u64, _d: u64) -> u64 {
    if let Some(text) = super::text(value) {
        let formatted = url::Url::parse(&text).map(|parsed| parsed.as_str().to_owned()).unwrap_or(text);
        return super::string(&formatted);
    }
    if let Some(mut parsed) = super::class::stored(value) {
        format_url(&mut parsed, options);
        return super::string(parsed.as_str());
    }
    // A legacy plain object: `protocol + (slashes ? '//' : '') + auth@ +
    // host + pathname + search + hash`, the historical composition rule.
    entry::with_runtime(|context| {
        let protocol = entry::get_member(context, value, "protocol");
        let protocol = entry::text_in(context, protocol).unwrap_or_default();
        let slashes_raw = entry::get_member(context, value, "slashes");
        let slashes = entry::to_boolean(slashes_raw);
        let auth_raw = entry::get_member(context, value, "auth");
        let auth = entry::text_in(context, auth_raw);
        let host_raw = entry::get_member(context, value, "host");
        let host = entry::text_in(context, host_raw);
        let pathname_raw = entry::get_member(context, value, "pathname");
        let pathname = entry::text_in(context, pathname_raw).unwrap_or_default();
        let search_raw = entry::get_member(context, value, "search");
        let search = entry::text_in(context, search_raw).unwrap_or_default();
        let hash_raw = entry::get_member(context, value, "hash");
        let hash = entry::text_in(context, hash_raw).unwrap_or_default();
        let mut out = String::new();
        if !protocol.is_empty() {
            out.push_str(&protocol);
        }
        if slashes || host.is_some() {
            out.push_str("//");
        }
        if let Some(auth) = auth {
            out.push_str(&auth);
            out.push('@');
        }
        if let Some(host) = host {
            out.push_str(&host);
        }
        out.push_str(&pathname);
        out.push_str(&search);
        out.push_str(&hash);
        entry::make_string(context, &out)
    })
}

/// Applies `UrlFormatOptions` (`auth`/`fragment`/`search`/`unicode`,
/// default `true`/`true`/`true`/`false`) in place.
fn format_url(parsed: &mut url::Url, options: u64) {
    let auth = option_bool(options, "auth", true);
    let fragment = option_bool(options, "fragment", true);
    let search = option_bool(options, "search", true);
    let unicode = option_bool(options, "unicode", false);
    if !auth {
        let _ = parsed.set_username("");
        let _ = parsed.set_password(None);
    }
    if !fragment {
        parsed.set_fragment(None);
    }
    if !search {
        parsed.set_query(None);
    }
    if unicode && let Some(host) = parsed.host_str() {
        let (unicode_host, _) = idna::domain_to_unicode(host);
        let _ = parsed.set_host(Some(&unicode_host));
    }
}

fn option_bool(options: u64, name: &str, default: bool) -> bool {
    let absent = entry::undefined_value();
    if options == absent {
        return default;
    }
    let value = entry::with_runtime(|context| entry::get_member(context, options, name));
    if value == absent { default } else { entry::to_boolean(value) }
}

/// `url.resolve(from, to)` — DEP0169. `to` resolved against `from`: through
/// `url::Url::join` when `from` parses as an absolute URL, else a plain
/// path-segment merge (Node's own canonical examples — `'/one/two/three' +
/// 'four'` → `'/one/two/four'` — are exactly this case, with no scheme at
/// all).
pub(super) extern "C" fn resolve(_e: u64, _this: u64, from: u64, to: u64, _c: u64, _d: u64) -> u64 {
    let from = super::text(from).unwrap_or_default();
    let to = super::text(to).unwrap_or_default();
    if let Ok(base) = url::Url::parse(&from)
        && let Ok(joined) = base.join(&to)
    {
        return super::string(joined.as_str());
    }
    super::string(&path_merge(&from, &to))
}

/// A path-only (no-scheme) resolve: `to` starting with `/` replaces the
/// whole path; otherwise `to` replaces the last segment of `from`'s
/// directory. `.`/`..` are not collapsed — this crate's URL parser owns no
/// path-normalization helper, and `path.rs`'s is private to that module.
fn path_merge(from: &str, to: &str) -> String {
    if to.starts_with('/') || from.is_empty() {
        return to.to_owned();
    }
    match from.rfind('/') {
        Some(at) => format!("{}/{to}", &from[..at]),
        None => to.to_owned(),
    }
}
