//! `url.parse(urlString)` — the legacy (pre-WHATWG) Node `url.parse` result
//! object: `{ href, protocol, host, hostname, port, pathname, search, hash,
//! query }`. Self-contained string logic (`scheme://host:port/path?query#hash`)
//! — no dependency on the WHATWG `URL` global class or any other rts-node
//! module, matching the isolation convention of the sibling parent module.
//!
//! Every field is a real Rust `Option<String>` (`None` -> the `null` word,
//! `Some(s)` -> a real interned string word); there is no placeholder/fake
//! value substituted for a field this simple scanner cannot determine — an
//! absent component is genuinely `null`, exactly like real Node's legacy
//! parser returns for a relative reference (`url.parse("/a/b")` has `host`,
//! `protocol`, etc. all `null`, not `""`).
//!
//! `query` is always the raw (un-decoded) query string — this one-argument
//! `parse(urlString)` has no `parseQueryString` parameter, so it mirrors
//! Node's own default (`parseQueryString: false`) where `query` is a string,
//! not a parsed object.

use rts_engine::abi::str_abi::from_abi;
use rts_engine::heap::shapes::{alloc_shaped_object, null_word, string_word};

/// The 9 fields of a legacy-parsed URL, each present (`Some`) or absent
/// (`None` -> `null`) independently. `href` is always present (reconstructed
/// from whatever components were found).
struct LegacyUrl {
    href: String,
    protocol: Option<String>,
    host: Option<String>,
    hostname: Option<String>,
    port: Option<String>,
    pathname: Option<String>,
    search: Option<String>,
    hash: Option<String>,
    query: Option<String>,
}

/// Recognizes a leading URL scheme (`^[a-zA-Z][a-zA-Z0-9+.-]*:`), the same
/// production the WHATWG/legacy parsers use. Returns `(scheme_without_colon,
/// remainder_after_colon)`. A `/` (or any other non-scheme byte) before the
/// `:` correctly falls through to `None` — e.g. a bare path `"/a:b"` is not
/// mistaken for a scheme, since `/` breaks the scan before any `:` is seen.
fn parse_scheme(s: &str) -> Option<(&str, &str)> {
    let bytes = s.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_alphabetic() {
        return None;
    }
    let mut i = 1;
    while i < bytes.len() {
        match bytes[i] {
            b':' => return Some((&s[..i], &s[i + 1..])),
            b if b.is_ascii_alphanumeric() || b == b'+' || b == b'-' || b == b'.' => i += 1,
            _ => break,
        }
    }
    None
}

/// Splits a bracket-aware `host[:port]` authority (minus any userinfo) into
/// `(hostname, port)`. IPv6 literals (`[::1]`) keep their brackets in
/// `hostname` (matches Node). A trailing `:digits` is treated as a port only
/// when every byte after the colon is an ASCII digit — a colon that's part of
/// a non-numeric host token (rare, but self-contained scanners must stay
/// honest) leaves the whole thing as `hostname` with no `port`.
fn split_host_port(host_part: &str) -> (Option<String>, Option<String>) {
    if host_part.is_empty() {
        return (None, None);
    }
    if let Some(rest) = host_part.strip_prefix('[') {
        return match rest.find(']') {
            Some(close) => {
                let hostname = format!("[{}]", &rest[..close]);
                let after = &rest[close + 1..];
                let port = after
                    .strip_prefix(':')
                    .filter(|p| !p.is_empty())
                    .map(|p| p.to_string());
                (Some(hostname), port)
            }
            None => (Some(host_part.to_string()), None),
        };
    }
    if let Some(idx) = host_part.rfind(':') {
        let (h, p) = (&host_part[..idx], &host_part[idx + 1..]);
        if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) {
            let hostname = if h.is_empty() { None } else { Some(h.to_string()) };
            return (hostname, Some(p.to_string()));
        }
    }
    (Some(host_part.to_string()), None)
}

/// The real parse: splits hash, then search/query, then protocol, then
/// (when `//` follows the protocol, or opens the whole string — a
/// protocol-relative URL) the authority into host/hostname/port, with
/// whatever's left as `pathname`. Reconstructs `href` from the components
/// actually found — never a copy of the raw input (so a URL with e.g. extra
/// whitespace-equivalent quirks still round-trips through what was parsed).
fn legacy_parse(input: &str) -> LegacyUrl {
    let (before_hash, hash) = match input.find('#') {
        Some(idx) => (&input[..idx], Some(input[idx..].to_string())),
        None => (input, None),
    };
    let (before_search, search, query) = match before_hash.find('?') {
        Some(idx) => (
            &before_hash[..idx],
            Some(before_hash[idx..].to_string()),
            Some(before_hash[idx + 1..].to_string()),
        ),
        None => (before_hash, None, None),
    };

    let (protocol, after_protocol) = match parse_scheme(before_search) {
        Some((scheme, remainder)) => (Some(format!("{scheme}:")), remainder),
        None => (None, before_search),
    };
    let (slashes, after_slashes) = match after_protocol.strip_prefix("//") {
        Some(rest) => (true, rest),
        None => (false, after_protocol),
    };

    let (host, hostname, port, pathname) = if slashes {
        let authority_end = after_slashes.find('/').unwrap_or(after_slashes.len());
        let authority = &after_slashes[..authority_end];
        let path_part = &after_slashes[authority_end..];
        // Drop userinfo (`user:pass@`) — not part of this object's field set.
        let host_part = match authority.rfind('@') {
            Some(idx) => &authority[idx + 1..],
            None => authority,
        };
        let host = if host_part.is_empty() {
            None
        } else {
            Some(host_part.to_string())
        };
        let (hostname, port) = split_host_port(host_part);
        // Node defaults pathname to "/" whenever an authority section was
        // present (slashes=true) and no explicit path followed it.
        let pathname = Some(if path_part.is_empty() {
            "/".to_string()
        } else {
            path_part.to_string()
        });
        (host, hostname, port, pathname)
    } else {
        let pathname = if after_slashes.is_empty() {
            None
        } else {
            Some(after_slashes.to_string())
        };
        (None, None, None, pathname)
    };

    let mut href = String::new();
    if let Some(p) = &protocol {
        href.push_str(p);
    }
    if slashes {
        href.push_str("//");
    }
    if let Some(h) = &host {
        href.push_str(h);
    }
    if let Some(p) = &pathname {
        href.push_str(p);
    }
    if let Some(s) = &search {
        href.push_str(s);
    }
    if let Some(h) = &hash {
        href.push_str(h);
    }

    LegacyUrl {
        href,
        protocol,
        host,
        hostname,
        port,
        pathname,
        search,
        hash,
        query,
    }
}

fn opt_word(v: &Option<String>) -> i64 {
    match v {
        Some(s) => string_word(s.as_bytes()) as i64,
        None => null_word() as i64,
    }
}

/// `url.parse(urlString)` — returns the legacy result object described in the
/// module doc. On a null/invalid-UTF-8 arg, returns `0` (no handle) like the
/// rest of this namespace's string-arg functions.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_URL_PARSE(ptr: *const u8, len: i64) -> u64 {
    let Some(url_str) = (unsafe { from_abi(ptr, len) }) else {
        return 0;
    };
    let parsed = legacy_parse(url_str);
    let keys: &[&str] = &[
        "href", "protocol", "host", "hostname", "port", "pathname", "search", "hash", "query",
    ];
    let values: [i64; 9] = [
        string_word(parsed.href.as_bytes()) as i64,
        opt_word(&parsed.protocol),
        opt_word(&parsed.host),
        opt_word(&parsed.hostname),
        opt_word(&parsed.port),
        opt_word(&parsed.pathname),
        opt_word(&parsed.search),
        opt_word(&parsed.hash),
        opt_word(&parsed.query),
    ];
    alloc_shaped_object(keys, &values)
}
