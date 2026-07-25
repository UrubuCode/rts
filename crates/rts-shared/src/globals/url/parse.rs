//! URL parsing + percent-encoding helpers, shared by `instance.rs` (the `Url`
//! `#[rtse::class]`) and `search_params.rs`. `ParsedUrl` is the pure Rust parser
//! (no external deps) — the `Url` struct is built from it.

/// (cross-runtime #746) Resolve `.` and `..` in path segments, matching the
/// WHATWG URL spec.
fn normalize_path(p: &str) -> String {
    if p.is_empty() {
        return "/".to_string();
    }
    let trailing_slash = p.ends_with('/');
    let mut out: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => continue,
            ".." => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    let mut result = String::from("/");
    result.push_str(&out.join("/"));
    if trailing_slash && !result.ends_with('/') {
        result.push('/');
    }
    result
}

/// (#67) Percent-encode chars unsafe in a URL search/hash component. Preserves
/// ASCII-safe chars (alfanum + `- _ . ~ ! $ & ' ( ) * + , ; = : @ / ?` and `#`).
/// Encodes space, `< > " { } | \ ^ \`` etc.
pub(super) fn percent_encode_search(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        let safe = matches!(b,
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9'
            | b'-' | b'_' | b'.' | b'~'
            | b'!' | b'$' | b'&' | b'\'' | b'(' | b')'
            | b'*' | b'+' | b',' | b';' | b'='
            | b':' | b'@' | b'/' | b'?' | b'#'
            | b'%'
        );
        if safe {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

/// (#67) Encode a pathname segment (RFC 3986 path char set). Space -> %20, does
/// not touch `/` and unreserved chars.
pub(super) fn percent_encode_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        let safe = matches!(b,
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9'
            | b'-' | b'_' | b'.' | b'~'
            | b'!' | b'$' | b'&' | b'\'' | b'(' | b')'
            | b'*' | b'+' | b',' | b';' | b'='
            | b':' | b'@' | b'/'
            | b'%'
        );
        if safe {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

/// (#67) URLSearchParams form encoding: space -> '+', unsafe chars -> %XX.
/// Different from path encoding (which uses %20 for space). Shared with
/// `search_params.rs`'s `UrlSearchParams::to_string` and `instance.rs`'s
/// `current_search`.
pub(super) fn percent_encode_form(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// A parsed WHATWG URL, decomposed into its components. Pure Rust, no deps.
pub(super) struct ParsedUrl {
    pub(super) protocol: String,
    pub(super) username: String,
    pub(super) password: String,
    pub(super) hostname: String,
    pub(super) port: String,
    pub(super) pathname: String,
    pub(super) search: String,
    pub(super) hash: String,
}

impl ParsedUrl {
    pub(super) fn parse(raw: &str) -> Option<Self> {
        // Minimal URL parser without external deps.
        let (raw, hash_raw) = match raw.split_once('#') {
            Some((before, after)) => (before, format!("#{after}")),
            None => (raw, String::new()),
        };
        let (raw, search_raw) = match raw.split_once('?') {
            Some((before, after)) => (before, format!("?{after}")),
            None => (raw, String::new()),
        };
        // (#67) Percent-encode chars unsafe in search/hash. JS spec encodes space
        // as %20 (not + in URL.search; URLSearchParams uses + but URL.search does not).
        let search = percent_encode_search(&search_raw);
        let hash = percent_encode_search(&hash_raw);
        let (scheme, rest) = raw.split_once("://")?;
        let protocol = format!("{scheme}:");
        let (authority, pathname_raw) = match rest.split_once('/') {
            Some((auth, path)) => (auth, format!("/{path}")),
            None => (rest, "/".to_owned()),
        };
        // (cross-runtime #746) Extract userinfo (`user:pass@`) from the authority.
        let (userinfo, host_part) = match authority.rsplit_once('@') {
            Some((ui, host)) => (Some(ui), host),
            None => (None, authority),
        };
        let (username, password) = match userinfo {
            Some(ui) => match ui.split_once(':') {
                Some((u, p)) => (u.to_owned(), p.to_owned()),
                None => (ui.to_owned(), String::new()),
            },
            None => (String::new(), String::new()),
        };
        // (cross-runtime #287) IPv6 literals: `[addr]` or `[addr]:port`. Hostname
        // keeps the brackets. Rejects invalid IPv6 (>1 `::` or non-hex/colon inside).
        let (hostname, port) = if let Some(rest_after_lb) = host_part.strip_prefix('[') {
            let (ipv6, after_rb) = match rest_after_lb.split_once(']') {
                Some(parts) => parts,
                None => return None, // unclosed bracket
            };
            // Validate IPv6 content: only hex digits and ':'; at most 1 `::`.
            if ipv6.is_empty() {
                return None;
            }
            if !ipv6.chars().all(|c| c.is_ascii_hexdigit() || c == ':') {
                return None;
            }
            if ipv6.matches("::").count() > 1 {
                return None;
            }
            // Additionally reject 3+ consecutive colons (eg `:::1`).
            if ipv6.contains(":::") {
                return None;
            }
            let host_with_brackets = format!("[{ipv6}]");
            let port = if let Some(p) = after_rb.strip_prefix(':') {
                if !p.chars().all(|c| c.is_ascii_digit()) {
                    return None;
                }
                p.to_owned()
            } else if after_rb.is_empty() {
                String::new()
            } else {
                return None;
            };
            (host_with_brackets, port)
        } else {
            match host_part.rsplit_once(':') {
                Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => {
                    (h.to_owned(), p.to_owned())
                }
                _ => (host_part.to_owned(), String::new()),
            }
        };
        let pathname = normalize_path(&pathname_raw);
        Some(ParsedUrl {
            protocol,
            username,
            password,
            hostname,
            port,
            pathname,
            search,
            hash,
        })
    }

    /// `hostname[:port]`.
    pub(super) fn host(&self) -> String {
        if self.port.is_empty() {
            self.hostname.clone()
        } else {
            format!("{}:{}", self.hostname, self.port)
        }
    }
}
