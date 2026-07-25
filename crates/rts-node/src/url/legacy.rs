//! node:url — the legacy (pre-WHATWG) `parse`/`format`/`resolve` API. Deprecated
//! in Node but part of the surface. `parse` returns the legacy urlObject shape;
//! `format` recomposes it; `resolve` uses the real WHATWG base-resolution.

use super::words::{null_w, object, str_word};

unsafe extern "C" {
    fn __RTS_FN_GL_URL_new_with_base(
        rel_ptr: i64,
        rel_len: i64,
        base_ptr: i64,
        base_len: i64,
    ) -> u64;
    fn __RTS_FN_GL_URL_href(h: u64) -> u64;
}

/// A parsed legacy urlObject (fields are `None` when absent).
#[derive(Default)]
struct Parsed {
    protocol: Option<String>,
    slashes: bool,
    auth: Option<String>,
    host: Option<String>,
    port: Option<String>,
    hostname: Option<String>,
    hash: Option<String>,
    search: Option<String>,
    pathname: Option<String>,
}

/// `url.parse(urlString[, parseQueryString[, slashesDenoteHost]])`.
pub fn parse(url: &str, parse_query: bool, slashes_denote_host: bool) -> u64 {
    let p = parse_components(url, slashes_denote_host);
    let path = match (&p.pathname, &p.search) {
        (Some(pn), Some(s)) => Some(format!("{pn}{s}")),
        (Some(pn), None) => Some(pn.clone()),
        (None, Some(s)) => Some(s.clone()),
        (None, None) => None,
    };
    let href = format_parsed(&p);
    let query_word = match &p.search {
        Some(s) => {
            let q = s.strip_prefix('?').unwrap_or(s);
            if parse_query {
                // A parsed object would require the querystring parser; the
                // string form is the default (parseQueryString defaults false).
                str_word(q)
            } else {
                str_word(q)
            }
        }
        None => null_w(),
    };

    let keys: &[&str] = &[
        "protocol", "slashes", "auth", "host", "port", "hostname", "hash", "search", "query",
        "pathname", "path", "href",
    ];
    let values: [i64; 12] = [
        opt_str(&p.protocol),
        if p.slashes { bool_true() } else { null_w() },
        opt_str(&p.auth),
        opt_str(&p.host),
        opt_str(&p.port),
        opt_str(&p.hostname),
        opt_str(&p.hash),
        opt_str(&p.search),
        query_word,
        opt_str(&p.pathname),
        opt_str(&path),
        str_word(&href),
    ];
    object(keys, &values)
}

/// `url.resolve(from, to)` — WHATWG base resolution (`new URL(to, from).href`).
pub fn resolve(from: &str, to: &str) -> String {
    let h = unsafe {
        __RTS_FN_GL_URL_new_with_base(
            to.as_ptr() as i64,
            to.len() as i64,
            from.as_ptr() as i64,
            from.len() as i64,
        )
    };
    let sh = unsafe { __RTS_FN_GL_URL_href(h) };
    rts_engine::heap::handles::read_string_handle(sh).unwrap_or_else(|| to.to_string())
}

/// `url.format(urlObject)` — recompose from the legacy fields (already read into
/// `Parsed` by the caller). Exposed as `format_from_fields`.
pub fn format_from_fields(
    protocol: Option<String>,
    slashes: bool,
    auth: Option<String>,
    host: Option<String>,
    hostname: Option<String>,
    port: Option<String>,
    pathname: Option<String>,
    search: Option<String>,
    hash: Option<String>,
) -> String {
    let host_final = host.or_else(|| {
        hostname.map(|hn| match &port {
            Some(p) if !p.is_empty() => format!("{hn}:{p}"),
            _ => hn,
        })
    });
    let p = Parsed {
        protocol,
        slashes: slashes || host_final.is_some(),
        auth,
        host: host_final,
        port: None,
        hostname: None,
        hash,
        search,
        pathname,
    };
    format_parsed(&p)
}

fn format_parsed(p: &Parsed) -> String {
    let mut out = String::new();
    if let Some(proto) = &p.protocol {
        out.push_str(proto);
        if !proto.ends_with(':') {
            out.push(':');
        }
    }
    if p.slashes || (p.host.is_some() && p.protocol.is_some()) {
        out.push_str("//");
    }
    if let Some(auth) = &p.auth {
        out.push_str(auth);
        out.push('@');
    }
    if let Some(host) = &p.host {
        out.push_str(host);
    }
    if let Some(pathname) = &p.pathname {
        out.push_str(pathname);
    }
    if let Some(search) = &p.search {
        out.push_str(search);
    }
    if let Some(hash) = &p.hash {
        out.push_str(hash);
    }
    out
}

/// Manual legacy-shape parse (Node's `url.parse` splits hash → search →
/// protocol → `//`authority → pathname).
fn parse_components(url: &str, slashes_denote_host: bool) -> Parsed {
    let mut p = Parsed::default();
    let mut rest = url.trim();

    if let Some(idx) = rest.find('#') {
        p.hash = Some(rest[idx..].to_string());
        rest = &rest[..idx];
    }
    if let Some(idx) = rest.find('?') {
        p.search = Some(rest[idx..].to_string());
        rest = &rest[..idx];
    }

    // protocol: scheme ':'
    let scheme_end = rest
        .find(':')
        .filter(|&i| rest[..i].chars().enumerate().all(|(j, c)| {
            if j == 0 {
                c.is_ascii_alphabetic()
            } else {
                c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-')
            }
        }) && i > 0);
    let mut had_protocol = false;
    if let Some(i) = scheme_end {
        p.protocol = Some(rest[..=i].to_lowercase());
        rest = &rest[i + 1..];
        had_protocol = true;
    }

    let has_slashes = rest.starts_with("//");
    if has_slashes && (had_protocol || slashes_denote_host) {
        p.slashes = true;
        rest = &rest[2..];
        // authority = up to next '/', '?' already stripped
        let auth_end = rest.find('/').unwrap_or(rest.len());
        let authority = &rest[..auth_end];
        rest = &rest[auth_end..];

        let host_part = if let Some(at) = authority.rfind('@') {
            p.auth = Some(authority[..at].to_string());
            &authority[at + 1..]
        } else {
            authority
        };
        if let Some(colon) = host_part.rfind(':') {
            if host_part[colon + 1..].chars().all(|c| c.is_ascii_digit()) {
                p.port = Some(host_part[colon + 1..].to_string());
                p.hostname = Some(host_part[..colon].to_lowercase());
            } else {
                p.hostname = Some(host_part.to_lowercase());
            }
        } else {
            p.hostname = Some(host_part.to_lowercase());
        }
        p.host = Some(host_part.to_lowercase());
    }

    if !rest.is_empty() {
        p.pathname = Some(rest.to_string());
    } else if p.hostname.is_none() && p.search.is_none() {
        // A bare path input with nothing else keeps an empty pathname absent.
    }
    p
}

fn opt_str(s: &Option<String>) -> i64 {
    match s {
        Some(v) => str_word(v),
        None => null_w(),
    }
}

fn bool_true() -> i64 {
    rts_engine::heap::shapes::bool_word(true) as i64
}
