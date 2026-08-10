//! `http.STATUS_CODES` / `http.METHODS` — static tables,
//! `docs/reference/node/http.md` §2.3, and the reason-phrase lookup
//! [`message::send_head_if_needed`](super::message) falls back to when a
//! caller never set `statusMessage`.

const CODES: &[(u16, &str)] = &[
    (100, "Continue"), (101, "Switching Protocols"), (102, "Processing"), (103, "Early Hints"),
    (200, "OK"), (201, "Created"), (202, "Accepted"), (203, "Non-Authoritative Information"),
    (204, "No Content"), (205, "Reset Content"), (206, "Partial Content"), (207, "Multi-Status"),
    (208, "Already Reported"), (226, "IM Used"),
    (300, "Multiple Choices"), (301, "Moved Permanently"), (302, "Found"), (303, "See Other"),
    (304, "Not Modified"), (305, "Use Proxy"), (307, "Temporary Redirect"), (308, "Permanent Redirect"),
    (400, "Bad Request"), (401, "Unauthorized"), (402, "Payment Required"), (403, "Forbidden"),
    (404, "Not Found"), (405, "Method Not Allowed"), (406, "Not Acceptable"),
    (407, "Proxy Authentication Required"), (408, "Request Timeout"), (409, "Conflict"),
    (410, "Gone"), (411, "Length Required"), (412, "Precondition Failed"),
    (413, "Payload Too Large"), (414, "URI Too Long"), (415, "Unsupported Media Type"),
    (416, "Range Not Satisfiable"), (417, "Expectation Failed"), (418, "I'm a Teapot"),
    (421, "Misdirected Request"), (422, "Unprocessable Entity"), (423, "Locked"),
    (424, "Failed Dependency"), (425, "Too Early"), (426, "Upgrade Required"),
    (428, "Precondition Required"), (429, "Too Many Requests"),
    (431, "Request Header Fields Too Large"), (451, "Unavailable For Legal Reasons"),
    (500, "Internal Server Error"), (501, "Not Implemented"), (502, "Bad Gateway"),
    (503, "Service Unavailable"), (504, "Gateway Timeout"), (505, "HTTP Version Not Supported"),
    (506, "Variant Also Negotiates"), (507, "Insufficient Storage"), (508, "Loop Detected"),
    (509, "Bandwidth Limit Exceeded"), (510, "Not Extended"), (511, "Network Authentication Required"),
];

pub(super) const METHODS: &[&str] = &[
    "ACL", "BIND", "CHECKOUT", "CONNECT", "COPY", "DELETE", "GET", "HEAD", "LINK", "LOCK",
    "M-SEARCH", "MERGE", "MKACTIVITY", "MKCALENDAR", "MKCOL", "MOVE", "NOTIFY", "OPTIONS",
    "PATCH", "POST", "PROPFIND", "PROPPATCH", "PURGE", "PUT", "REBIND", "REPORT", "SEARCH",
    "SOURCE", "SUBSCRIBE", "TRACE", "UNBIND", "UNLINK", "UNLOCK", "UNSUBSCRIBE",
];

/// The reason phrase for a status code — `""` for one this table does not
/// carry (Node itself leaves `statusMessage` empty in that case too).
pub(super) fn reason_phrase(code: u16) -> &'static str {
    CODES.iter().find(|(c, _)| *c == code).map(|(_, phrase)| *phrase).unwrap_or("")
}

pub(super) fn status_codes_object(context: &mut rts_core::entry::Context) -> u64 {
    let object = rts_core::entry::make_object(context);
    for (code, phrase) in CODES {
        let value = rts_core::entry::make_string(context, phrase);
        rts_core::entry::put_member(context, object, &code.to_string(), value);
    }
    object
}

pub(super) fn methods_array(context: &mut rts_core::entry::Context) -> u64 {
    let values: Vec<u64> = METHODS.iter().map(|m| rts_core::entry::make_string(context, m)).collect();
    rts_core::entry::make_array_in(context, values)
}
