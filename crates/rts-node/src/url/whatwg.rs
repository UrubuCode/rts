//! node:url — WHATWG helpers: `domainToASCII`/`domainToUnicode` (real UTS-46
//! IDNA via the `idna` crate — the same algorithm Node's own binding uses) and
//! `urlToHttpOptions` (builds the Node HTTP request-options object from a live
//! `URL`, read through its component getters).

use super::words::{self, num_word, null_w, object, str_word};

/// `url.domainToASCII(domain)` — UTS-46 to-ASCII; `''` on an invalid domain
/// (matching Node's behavior of returning the empty string on failure).
pub fn domain_to_ascii(domain: &str) -> String {
    idna::domain_to_ascii(domain).unwrap_or_default()
}

/// `url.domainToUnicode(domain)` — UTS-46 to-Unicode (never throws; returns the
/// best-effort decoded form).
pub fn domain_to_unicode(domain: &str) -> String {
    idna::domain_to_unicode(domain).0
}

/// `url.urlToHttpOptions(url)` — the `{ protocol, hostname, hash, search,
/// pathname, path, href, port, auth }` request-options object from a live URL.
pub fn url_to_http_options(url_handle: u64) -> u64 {
    let protocol = words::url_protocol(url_handle);
    let hostname = words::url_hostname(url_handle);
    let hash = words::url_hash(url_handle);
    let search = words::url_search(url_handle);
    let pathname = words::url_pathname(url_handle);
    let href = words::url_href(url_handle);
    let port = words::url_port(url_handle);
    let user = words::url_username(url_handle);
    let pass = words::url_password(url_handle);
    let path = format!("{pathname}{search}");

    // `port`: numeric when present, else the empty string (Node's shape).
    let port_word = match port.parse::<f64>() {
        Ok(p) => num_word(p),
        Err(_) => str_word(""),
    };
    // `auth`: "user:pass" when userinfo present, else null.
    let auth_word = if user.is_empty() && pass.is_empty() {
        null_w()
    } else {
        str_word(&format!("{user}:{pass}"))
    };

    let keys: &[&str] = &[
        "protocol", "hostname", "hash", "search", "pathname", "path", "href", "port", "auth",
    ];
    let values: [i64; 9] = [
        str_word(&protocol),
        str_word(&hostname),
        str_word(&hash),
        str_word(&search),
        str_word(&pathname),
        str_word(&path),
        str_word(&href),
        port_word,
        auth_word,
    ];
    object(keys, &values)
}
