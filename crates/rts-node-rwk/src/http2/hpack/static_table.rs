//! RFC 7541 Appendix A — the 61-entry static table, fixed by the spec and
//! never modified at run time. Indices are 1-based on the wire; entry `N`
//! here lives at `STATIC_TABLE[N - 1]`.

/// `(name, value)` — `value` is `""` for names the table lists with no
/// paired value (the peer supplies one when it indexes only the name).
pub const STATIC_TABLE: [(&str, &str); 61] = [
    (":authority", ""),
    (":method", "GET"),
    (":method", "POST"),
    (":path", "/"),
    (":path", "/index.html"),
    (":scheme", "http"),
    (":scheme", "https"),
    (":status", "200"),
    (":status", "204"),
    (":status", "206"),
    (":status", "304"),
    (":status", "400"),
    (":status", "404"),
    (":status", "500"),
    ("accept-charset", ""),
    ("accept-encoding", "gzip, deflate"),
    ("accept-language", ""),
    ("accept-ranges", ""),
    ("accept", ""),
    ("access-control-allow-origin", ""),
    ("age", ""),
    ("allow", ""),
    ("authorization", ""),
    ("cache-control", ""),
    ("content-disposition", ""),
    ("content-encoding", ""),
    ("content-language", ""),
    ("content-length", ""),
    ("content-location", ""),
    ("content-range", ""),
    ("content-type", ""),
    ("cookie", ""),
    ("date", ""),
    ("etag", ""),
    ("expect", ""),
    ("expires", ""),
    ("from", ""),
    ("host", ""),
    ("if-match", ""),
    ("if-modified-since", ""),
    ("if-none-match", ""),
    ("if-range", ""),
    ("if-unmodified-since", ""),
    ("last-modified", ""),
    ("link", ""),
    ("location", ""),
    ("max-forwards", ""),
    ("proxy-authenticate", ""),
    ("proxy-authorization", ""),
    ("range", ""),
    ("referer", ""),
    ("refresh", ""),
    ("retry-after", ""),
    ("server", ""),
    ("set-cookie", ""),
    ("strict-transport-security", ""),
    ("transfer-encoding", ""),
    ("user-agent", ""),
    ("vary", ""),
    ("via", ""),
    ("www-authenticate", ""),
];

/// The 1-based static index for an exact `(name, value)` pair, if one exists.
pub fn find_exact(name: &str, value: &str) -> Option<usize> {
    STATIC_TABLE.iter().position(|&(n, v)| n == name && v == value).map(|i| i + 1)
}

/// The 1-based static index for `name` alone (first match, matching real
/// encoders' tie-break), if one exists.
pub fn find_name(name: &str) -> Option<usize> {
    STATIC_TABLE.iter().position(|&(n, _)| n == name).map(|i| i + 1)
}

/// The `(name, value)` pair a 1-based static index names, if `index` falls
/// in the static table's range (1..=61).
pub fn at(index: usize) -> Option<(&'static str, &'static str)> {
    index.checked_sub(1).and_then(|i| STATIC_TABLE.get(i)).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_has_61_entries() {
        assert_eq!(STATIC_TABLE.len(), 61);
    }

    #[test]
    fn known_indices_match_rfc_appendix_a() {
        assert_eq!(at(1), Some((":authority", "")));
        assert_eq!(at(2), Some((":method", "GET")));
        assert_eq!(at(8), Some((":status", "200")));
        assert_eq!(at(61), Some(("www-authenticate", "")));
        assert_eq!(at(62), None);
    }

    #[test]
    fn find_exact_and_name() {
        assert_eq!(find_exact(":method", "POST"), Some(3));
        assert_eq!(find_name("content-type"), Some(31));
        assert_eq!(find_exact("content-type", "text/html"), None);
    }
}
