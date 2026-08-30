//! The real work of this module: a byte-level HTTP/1.1 head parser and a
//! chunked/`Content-Length` body decoder, with no engine dependency at all —
//! everything here takes and returns plain Rust data, so it is unit-testable
//! without a `Context` and reusable identically for a request head (server
//! side) and a response head (client side).
//!
//! # The two things a naive parser gets wrong, named because they are the
//! reason this file exists rather than three lines of `String::find`
//!
//! - **Header names are case-insensitive.** `Content-Length`, `content-length`
//!   and `CONTENT-LENGTH` name the same header. [`header_find`] compares with
//!   [`str::eq_ignore_ascii_case`] rather than an exact match, and every
//!   caller reaches headers through it — never a direct `==` against a raw
//!   header name.
//! - **A chunked body ends with a zero-length chunk**, not at end-of-stream.
//!   [`ChunkedDecoder::step`] only reports [`ChunkOutcome::Done`] on that
//!   sentinel; a connection that closes mid-chunk is a truncation, reported
//!   as [`ChunkOutcome::NeedMore`] forever, which is what "the body never
//!   completed" should look like rather than a silently-short body.

/// One parsed header, in the case and order the wire sent it — folding into
/// Node's `headers`/`headersDistinct` split is [`super::message`]'s, not this
/// file's; a parser that folds cannot also hand back `rawHeaders`.
pub(super) type RawHeaders = Vec<(String, String)>;

pub(crate) struct RequestHead {
    pub method: String,
    pub target: String,
    pub version: String,
    pub headers: RawHeaders,
}

pub(crate) struct ResponseHead {
    pub version: String,
    pub status: u16,
    pub reason: String,
    pub headers: RawHeaders,
}

fn find_double_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

fn find_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\r\n")
}

fn split_head_lines(head: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(head).split("\r\n").map(str::to_owned).collect()
}

fn parse_headers(lines: &[String]) -> RawHeaders {
    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some(idx) = line.find(':') {
            let name = line[..idx].trim().to_owned();
            let value = line[idx + 1..].trim().to_owned();
            if !name.is_empty() {
                headers.push((name, value));
            }
        }
    }
    headers
}

/// A request head, if `buf` holds a complete `\r\n\r\n`-terminated one.
/// `Some((head, consumed))` — `consumed` is how many leading bytes of `buf`
/// the caller should drop; whatever follows is body, not head.
pub(super) fn parse_request_head(buf: &[u8]) -> Option<(RequestHead, usize)> {
    let end = find_double_crlf(buf)?;
    let lines = split_head_lines(&buf[..end]);
    let request_line = lines.first()?;
    let mut parts = request_line.splitn(3, ' ');
    let method = parts.next()?.to_owned();
    let target = parts.next()?.to_owned();
    let version = parts
        .next()
        .unwrap_or("HTTP/1.1")
        .trim_start_matches("HTTP/")
        .to_owned();
    let headers = parse_headers(&lines[1..]);
    Some((RequestHead { method, target, version, headers }, end + 4))
}

/// The response-side pair of [`parse_request_head`].
pub(crate) fn parse_response_head(buf: &[u8]) -> Option<(ResponseHead, usize)> {
    let end = find_double_crlf(buf)?;
    let lines = split_head_lines(&buf[..end]);
    let status_line = lines.first()?;
    let mut parts = status_line.splitn(3, ' ');
    let version = parts.next()?.trim_start_matches("HTTP/").to_owned();
    let status: u16 = parts.next()?.trim().parse().ok()?;
    let reason = parts.next().unwrap_or("").to_owned();
    let headers = parse_headers(&lines[1..]);
    Some((ResponseHead { version, status, reason, headers }, end + 4))
}

/// A header's value, found case-insensitively — see the module doc's first
/// named mistake.
pub(super) fn header_find<'a>(headers: &'a RawHeaders, name: &str) -> Option<&'a str> {
    headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

/// How a body following this head is delimited — RFC 9112 §6.3's order:
/// `Transfer-Encoding: chunked` wins over `Content-Length` when both are
/// present (a smuggling-prone combination real servers reject; this parser
/// just picks the framing Node's own `http` would use for a well-formed
/// message rather than adding a rejection path this crate has no error
/// channel to report through).
#[derive(Clone, Copy)]
pub(crate) enum Framing {
    None,
    Length(usize),
    Chunked,
}

pub(crate) fn framing_of(headers: &RawHeaders) -> Framing {
    if let Some(te) = header_find(headers, "transfer-encoding")
        && te.to_ascii_lowercase().contains("chunked")
    {
        return Framing::Chunked;
    }
    if let Some(cl) = header_find(headers, "content-length")
        && let Ok(n) = cl.trim().parse::<usize>()
    {
        return Framing::Length(n);
    }
    Framing::None
}

/// One decode step's answer.
pub(crate) enum ChunkOutcome {
    /// Not enough bytes yet for the next chunk-size line or chunk body.
    NeedMore,
    /// One decoded chunk, with its bytes already stripped of the
    /// size-line/CRLF framing.
    Body(Vec<u8>),
    /// The zero-length terminator chunk was seen — see the module doc's
    /// second named mistake. Trailers, if any, are consumed but discarded;
    /// `addTrailers`/`rawTrailers` are not implemented (see the module doc).
    Done,
}

/// Incremental chunked-body decoder — holds just enough state (whether it is
/// mid-chunk, and how many bytes are left in the current one) to be fed
/// arbitrary byte spans as they arrive off a socket, one `'data'` event at a
/// time.
pub(crate) struct ChunkedDecoder {
    remaining: Option<usize>,
    trailers_done: bool,
}

impl ChunkedDecoder {
    pub(super) fn new() -> Self {
        Self { remaining: None, trailers_done: false }
    }

    /// Consumes as much of `buf` as a complete chunk-size line or chunk body
    /// needs, draining what it consumes. Call repeatedly (the caller loops
    /// while it gets `Body`) until `NeedMore` or `Done`.
    pub(super) fn step(&mut self, buf: &mut Vec<u8>) -> ChunkOutcome {
        match self.remaining {
            None => {
                let Some(pos) = find_crlf(buf) else { return ChunkOutcome::NeedMore };
                let line = String::from_utf8_lossy(&buf[..pos]).into_owned();
                let size_text = line.split(';').next().unwrap_or("").trim();
                let Ok(size) = usize::from_str_radix(size_text, 16) else {
                    // Malformed size line — no error channel to report through
                    // (see the module doc's framing note); treat as the
                    // stream ending rather than looping forever.
                    buf.drain(..(pos + 2).min(buf.len()));
                    return ChunkOutcome::Done;
                };
                buf.drain(..pos + 2);
                if size == 0 {
                    return self.finish_trailers(buf);
                }
                self.remaining = Some(size);
                self.step(buf)
            }
            Some(size) => {
                if buf.len() < size + 2 {
                    return ChunkOutcome::NeedMore;
                }
                let data = buf[..size].to_vec();
                buf.drain(..size + 2);
                self.remaining = None;
                ChunkOutcome::Body(data)
            }
        }
    }

    /// After the zero-length chunk: zero or more trailer header lines, then a
    /// blank line. Discarded rather than surfaced — `addTrailers`'s read side
    /// is not implemented (module doc).
    fn finish_trailers(&mut self, buf: &mut Vec<u8>) -> ChunkOutcome {
        if self.trailers_done {
            return ChunkOutcome::Done;
        }
        let Some(end) = find_double_crlf(buf) else {
            // No trailers at all is the common case: the terminator is
            // immediately followed by a single CRLF, which does not contain
            // a second CRLF pair to find — accept that shape too.
            if buf.len() >= 2 && &buf[..2] == b"\r\n" {
                buf.drain(..2);
                self.trailers_done = true;
                return ChunkOutcome::Done;
            }
            return ChunkOutcome::NeedMore;
        };
        buf.drain(..end + 4);
        self.trailers_done = true;
        ChunkOutcome::Done
    }
}

/// A status/reason/method table lookup, shared by [`super::status`].
pub(super) fn is_token_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&c)
}

/// Whether `name` is a valid HTTP header-name token — RFC 9110 §5.1, used by
/// `http.validateHeaderName`.
pub(super) fn valid_header_name(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(is_token_char)
}

/// Whether `value` is free of characters RFC 9110 §5.5 forbids in a header
/// field value (no CR/LF, no bare NUL) — used by `http.validateHeaderValue`.
pub(super) fn valid_header_value(value: &str) -> bool {
    value.bytes().all(|c| c != b'\r' && c != b'\n' && c != 0)
}
