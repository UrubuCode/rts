//! node:http — the stream/server-independent surface: the `METHODS` list, the
//! `STATUS_CODES` registry, and the header validators. Real data + real
//! validation (RFC 7230 token / field-value rules) — no fabrication.

/// `http.METHODS` — the HTTP methods Node recognizes (sorted, as Node exposes).
pub const METHODS: &[&str] = &[
    "ACL", "BIND", "CHECKOUT", "CONNECT", "COPY", "DELETE", "GET", "HEAD", "LINK",
    "LOCK", "M-SEARCH", "MERGE", "MKACTIVITY", "MKCALENDAR", "MKCOL", "MOVE",
    "NOTIFY", "OPTIONS", "PATCH", "POST", "PROPFIND", "PROPPATCH", "PURGE", "PUT",
    "QUERY", "REBIND", "REPORT", "SEARCH", "SOURCE", "SUBSCRIBE", "TRACE",
    "UNBIND", "UNLINK", "UNLOCK", "UNSUBSCRIBE",
];

/// `http.STATUS_CODES` — `(code, reason-phrase)` pairs (IANA HTTP status registry).
pub const STATUS_CODES: &[(&str, &str)] = &[
    ("100", "Continue"),
    ("101", "Switching Protocols"),
    ("102", "Processing"),
    ("103", "Early Hints"),
    ("200", "OK"),
    ("201", "Created"),
    ("202", "Accepted"),
    ("203", "Non-Authoritative Information"),
    ("204", "No Content"),
    ("205", "Reset Content"),
    ("206", "Partial Content"),
    ("207", "Multi-Status"),
    ("208", "Already Reported"),
    ("226", "IM Used"),
    ("300", "Multiple Choices"),
    ("301", "Moved Permanently"),
    ("302", "Found"),
    ("303", "See Other"),
    ("304", "Not Modified"),
    ("305", "Use Proxy"),
    ("307", "Temporary Redirect"),
    ("308", "Permanent Redirect"),
    ("400", "Bad Request"),
    ("401", "Unauthorized"),
    ("402", "Payment Required"),
    ("403", "Forbidden"),
    ("404", "Not Found"),
    ("405", "Method Not Allowed"),
    ("406", "Not Acceptable"),
    ("407", "Proxy Authentication Required"),
    ("408", "Request Timeout"),
    ("409", "Conflict"),
    ("410", "Gone"),
    ("411", "Length Required"),
    ("412", "Precondition Failed"),
    ("413", "Payload Too Large"),
    ("414", "URI Too Long"),
    ("415", "Unsupported Media Type"),
    ("416", "Range Not Satisfiable"),
    ("417", "Expectation Failed"),
    ("418", "I'm a Teapot"),
    ("421", "Misdirected Request"),
    ("422", "Unprocessable Entity"),
    ("423", "Locked"),
    ("424", "Failed Dependency"),
    ("425", "Too Early"),
    ("426", "Upgrade Required"),
    ("428", "Precondition Required"),
    ("429", "Too Many Requests"),
    ("431", "Request Header Fields Too Large"),
    ("451", "Unavailable For Legal Reasons"),
    ("500", "Internal Server Error"),
    ("501", "Not Implemented"),
    ("502", "Bad Gateway"),
    ("503", "Service Unavailable"),
    ("504", "Gateway Timeout"),
    ("505", "HTTP Version Not Supported"),
    ("506", "Variant Also Negotiates"),
    ("507", "Insufficient Storage"),
    ("508", "Loop Detected"),
    ("509", "Bandwidth Limit Exceeded"),
    ("510", "Not Extended"),
    ("511", "Network Authentication Required"),
];

/// An RFC 7230 header name is one or more token characters.
pub fn valid_header_name(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(is_token_char)
}

fn is_token_char(b: u8) -> bool {
    matches!(b,
        b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-' | b'.' |
        b'^' | b'_' | b'`' | b'|' | b'~' |
        b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z'
    )
}

/// A header field-value must not contain a CR, LF, or NUL (and no other control
/// char except horizontal tab).
pub fn valid_header_value(value: &str) -> bool {
    value.bytes().all(|b| b == b'\t' || (b' '..=b'~').contains(&b) || b >= 0x80)
}
