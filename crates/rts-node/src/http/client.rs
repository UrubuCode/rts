//! `http.request`/`http.get`/`ClientRequest` — built the same way
//! [`super::server`] is: entirely through `net`'s public JS surface (a
//! `net.Socket`, connected then written to and read from via its own
//! `write`/`on('data', …)`), never through `net`'s private internals.
//!
//! # The one real divergence from Node here, named rather than hidden
//!
//! Node's `ClientRequest` is asynchronous: `request()` returns before the
//! socket connects, and `'response'` arrives later, on its own turn. This
//! engine has no event loop a background thread can post into — the same
//! limit `net`'s own module doc states for `'connection'`/`'data'` — so
//! there is no "later turn" for a native to defer to. [`connect_blocking`]
//! and [`read_response_blocking`] instead SPIN: they call a real `net`
//! native in a short loop (the only way to force `net::registry::pump` to
//! run, since that function is private) with a small sleep between
//! attempts, until the socket connects / the response head and body are
//! fully read, or a bounded timeout elapses. **The request only returns
//! once the whole exchange is done** — `request()` connects synchronously
//! before handing back the `ClientRequest`, and `end()` sends the body and
//! reads the whole response before returning, then fires `'response'`
//! synchronously with a complete, non-streaming `IncomingMessage`. A
//! program that never calls `.end()` (relying on `.write()` alone plus a
//! timer to flush later, a legal but rare shape) never gets a response —
//! named, not silently different.

use rts_core::entry::{self, Provided};
use std::time::{Duration, Instant};

use super::common::*;
use super::{incoming, outgoing, parser};

const CONNECT_TIMEOUT_MS: u64 = 5000;
const RESPONSE_TIMEOUT_MS: u64 = 10000;

pub(super) const CLIENT_METHODS: &[(&str, Provided)] = outgoing::OUTGOING_METHODS;

pub(super) fn prototype(context: &mut entry::Context) -> u64 {
    // `ClientRequest` needs `end` to mean "send, then block for the whole
    // response" rather than `OutgoingMessage`'s plain "flush and finish" —
    // so its own `end`/`write` shadow the shared list rather than reusing it
    // verbatim.
    let mut methods: Vec<(&str, Provided)> = CLIENT_METHODS.to_vec();
    methods.retain(|(name, _)| *name != "end" && *name != "write");
    methods.push(("write", client_write));
    methods.push(("end", client_end));
    methods.push(("abort", client_destroy));
    chained_prototype(context, "Writable", "ClientRequest", &methods)
}

/// `http.request(options|url[, options][, callback])` / `http.get(...)`.
/// `auto_end` is `true` for `get` (Node calls `.end()` for the caller).
pub(super) fn build_request(url_or_options: u64, options: u64, callback: u64, auto_end: bool) -> u64 {
    let (host, port, path, method, headers) = entry::with_runtime(|context| read_request_options(context, url_or_options, options));
    let net_ns = entry::with_runtime(super::net_namespace);
    let absent = entry::undefined_value();
    let socket_ctor = entry::with_runtime(|context| entry::get_member(context, net_ns, "Socket"));
    let socket = entry::call(socket_ctor, absent, absent, absent, absent, absent);

    let instance = entry::with_runtime(|context| {
        let prototype = prototype(context);
        let instance = entry::make_instance(context, prototype);
        init_emitter(context, instance);
        set_value(context, instance, "socket", socket);
        set_bool(context, instance, "destroyed", false);
        set_bool(context, instance, "writableEnded", false);
        set_text(context, instance, "method", &method);
        set_text(context, instance, "path", &path);
        set_text(context, instance, "host", &host);
        let headers_obj = entry::make_object(context);
        set_value(context, instance, "__headers__", headers_obj);
        let order = entry::make_array_in(context, Vec::new());
        set_value(context, instance, "__headerOrder__", order);
        let empty_body = entry::make_string(context, "");
        set_value(context, instance, "__body__", empty_body);
        instance
    });
    for (name, value) in &headers {
        let value_v = entry::with_runtime(|context| entry::make_string(context, value));
        let name_v = entry::with_runtime(|context| entry::make_string(context, name));
        outgoing::set_header_pub(instance, name_v, value_v);
    }
    if callback != absent {
        let once_fn = entry::with_runtime(|context| entry::get_member(context, instance, "once"));
        entry::call(once_fn, instance, key("response"), callback, absent, absent);
    }

    let connect_fn = entry::with_runtime(|context| entry::get_member(context, socket, "connect"));
    let port_v = entry::make_number(port as f64);
    let host_v = entry::with_runtime(|context| entry::make_string(context, &host));
    entry::call(connect_fn, socket, port_v, host_v, absent, absent);
    if !connect_blocking(socket) {
        emit(instance, "error", error_object("ECONNREFUSED", "connect failed"), absent, absent);
        return instance;
    }

    if auto_end {
        client_end(0, instance, absent, absent, absent, 0);
    }
    instance
}

fn error_object(code: &str, message: &str) -> u64 {
    entry::with_runtime(|context| {
        let object = entry::make_object(context);
        let message_v = entry::make_string(context, message);
        let code_v = entry::make_string(context, code);
        entry::put_member(context, object, "message", message_v);
        entry::put_member(context, object, "code", code_v);
        object
    })
}

/// `(host, port, path, method, headers)` off either a URL string or an
/// options object — `docs/reference/node/http.md` §3's `RequestOptions`,
/// reduced to the fields this client acts on. Unrecognized fields
/// (`agent`, `signal`, `family`, `lookup`, …) are read never; see the
/// module's own "not implemented" list.
fn read_request_options(context: &mut entry::Context, url_or_options: u64, options: u64) -> (String, u16, String, String, Vec<(String, String)>) {
    let mut host = "localhost".to_owned();
    let mut port = 80u16;
    let mut path = "/".to_owned();
    let mut method = "GET".to_owned();
    let mut headers = Vec::new();
    if let Some(text) = entry::text_in(context, url_or_options) {
        parse_url_into(&text, &mut host, &mut port, &mut path);
    } else {
        apply_options(context, url_or_options, &mut host, &mut port, &mut path, &mut method, &mut headers);
    }
    if options != entry::undefined_in(context) {
        apply_options(context, options, &mut host, &mut port, &mut path, &mut method, &mut headers);
    }
    if !headers.iter().any(|(n, _)| n.eq_ignore_ascii_case("host")) {
        headers.push(("Host".to_owned(), host.clone()));
    }
    (host, port, path, method, headers)
}

fn apply_options(
    context: &mut entry::Context,
    options: u64,
    host: &mut String,
    port: &mut u16,
    path: &mut String,
    method: &mut String,
    headers: &mut Vec<(String, String)>,
) {
    if let Some(h) = option_text(context, options, "hostname").or_else(|| option_text(context, options, "host")) {
        *host = h;
    }
    if let Some(p) = option_num(context, options, "port") {
        *port = p as u16;
    }
    if let Some(p) = option_text(context, options, "path") {
        *path = p;
    }
    if let Some(m) = option_text(context, options, "method") {
        *method = m.to_ascii_uppercase();
    }
    let headers_value = option_member(context, options, "headers");
    if headers_value != entry::undefined_in(context) {
        let names = entry::own_keys(headers_value);
        let mut index = 0.0;
        loop {
            let name_v = entry::get_member(context, names, &index.to_string());
            if name_v == entry::undefined_in(context) {
                break;
            }
            let Some(name) = entry::text_in(context, name_v) else { break };
            let value_v = entry::get_member(context, headers_value, &name);
            if let Some(value) = entry::text_in(context, value_v) {
                headers.push((name, value));
            }
            index += 1.0;
        }
    }
}

/// A minimal `http://host[:port]/path` split — enough for `request`/`get`'s
/// string-URL form; no query-string canonicalization, no `URL`-object form.
fn parse_url_into(text: &str, host: &mut String, port: &mut u16, path: &mut String) {
    let rest = text.strip_prefix("http://").unwrap_or(text);
    let (authority, rest_path) = rest.split_once('/').map(|(a, p)| (a, format!("/{p}"))).unwrap_or_else(|| (rest, "/".to_owned()));
    *path = rest_path;
    match authority.split_once(':') {
        Some((h, p)) => {
            *host = h.to_owned();
            *port = p.parse().unwrap_or(80);
        }
        None => *host = authority.to_owned(),
    }
}

/// Spins on a real `net` native (`socket.write`, the only one guaranteed to
/// call `net::registry::pump` — see the module doc) until `connecting` flips
/// false, meaning `net::registry::pump` delivered either `'connect'` or a
/// connect failure. Answers whether it connected.
fn connect_blocking(socket: u64) -> bool {
    let start = Instant::now();
    loop {
        let empty = entry::with_runtime(|context| entry::make_bytes(context, &[]));
        let absent = entry::undefined_value();
        call_method(socket, "write", empty, absent, absent);
        let connecting = get_value(socket, "connecting") == entry::boolean_value(true);
        if !connecting {
            return get_text(socket, "remoteAddress").is_some();
        }
        if start.elapsed() > Duration::from_millis(CONNECT_TIMEOUT_MS) {
            return false;
        }
        std::thread::sleep(Duration::from_millis(4));
    }
}

/// `request.write(chunk, encoding?, callback?)` — buffered in `__body__`
/// rather than sent immediately: this client sends one framed request (see
/// [`client_end`]), not an incrementally streamed one.
extern "C" fn client_write(_e: u64, this: u64, chunk: u64, _encoding: u64, callback: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let text = entry::text_of(chunk).unwrap_or_default();
    let mut body = get_text(this, "__body__").unwrap_or_default();
    body.push_str(&text);
    entry::with_runtime(|context| set_text(context, this, "__body__", &body));
    if callback != absent {
        entry::call(callback, absent, absent, absent, absent, absent);
    }
    entry::boolean_value(true)
}

/// `request.end(chunk?, encoding?, callback?)` — sends the request line,
/// headers and body, then blocks for the whole response (see the module
/// doc) and emits `'response'` with a complete `IncomingMessage`.
extern "C" fn client_end(_e: u64, this: u64, chunk: u64, _encoding: u64, callback: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    if entry::text_of(chunk).is_some() {
        client_write(0, this, chunk, absent, absent, 0);
    }
    let socket = get_value(this, "socket");
    let method = get_text(this, "method").unwrap_or_else(|| "GET".to_owned());
    let path = get_text(this, "path").unwrap_or_else(|| "/".to_owned());
    let body = get_text(this, "__body__").unwrap_or_default();
    let mut head = format!("{method} {path} HTTP/1.1\r\n");
    let (order, has_length) = entry::with_runtime(|context| {
        let order = outgoing::header_order(context, this);
        let has_length = order.iter().any(|n| n.eq_ignore_ascii_case("content-length"));
        (order, has_length)
    });
    let headers_obj = get_value(this, "__headers__");
    for name in &order {
        let lower = name.to_ascii_lowercase();
        let value = entry::with_runtime(|context| entry::get_member(context, headers_obj, &lower));
        if let Some(text) = entry::text_of(value) {
            head.push_str(&format!("{name}: {text}\r\n"));
        }
    }
    if !has_length && !body.is_empty() {
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    head.push_str("Connection: close\r\n\r\n");
    head.push_str(&body);
    let head_v = entry::with_runtime(|context| entry::make_string(context, &head));
    call_method(socket, "write", head_v, absent, absent);
    entry::with_runtime(|context| set_bool(context, this, "writableEnded", true));
    emit(this, "finish", absent, absent, absent);

    match read_response_blocking(socket) {
        Some(message) => {
            emit(this, "response", message, absent, absent);
        }
        None => {
            emit(this, "error", error_object("ETIMEDOUT", "response timed out"), absent, absent);
        }
    }
    if callback != absent {
        entry::call(callback, absent, absent, absent, absent, absent);
    }
    this
}

/// Spins reading `socket`'s buffered bytes (forced the same way
/// [`connect_blocking`] is) until a full response head and body have
/// arrived, building the `IncomingMessage` [`client_end`] emits. `None` on
/// timeout.
fn read_response_blocking(socket: u64) -> Option<u64> {
    let start = Instant::now();
    let mut buf: Vec<u8> = Vec::new();
    loop {
        let chunk_text = drain_socket_buffer(socket);
        buf.extend_from_slice(&chunk_text);
        if let Some((head, consumed)) = parser::parse_response_head(&buf) {
            buf.drain(..consumed);
            let framing = parser::framing_of(&head.headers);
            let target_len = match framing {
                parser::Framing::Length(n) => Some(n),
                parser::Framing::None => Some(0),
                parser::Framing::Chunked => None,
            };
            loop {
                if let Some(n) = target_len
                    && buf.len() >= n
                {
                    break;
                }
                if start.elapsed() > Duration::from_millis(RESPONSE_TIMEOUT_MS) {
                    return None;
                }
                if let parser::Framing::Chunked = framing
                    && buf.windows(5).any(|w| w == b"0\r\n\r\n")
                {
                    break;
                }
                let more = drain_socket_buffer(socket);
                if more.is_empty() {
                    std::thread::sleep(Duration::from_millis(4));
                } else {
                    buf.extend_from_slice(&more);
                }
            }
            let body = decode_body(&buf, framing);
            let message = entry::with_runtime(|context| {
                incoming::build_incoming(context, socket, &head.headers, &head.version, None, Some((head.status, head.reason.as_str())))
            });
            let absent = entry::undefined_value();
            let push_fn = entry::with_runtime(|context| entry::get_member(context, message, "push"));
            if !body.is_empty() {
                let chunk = entry::with_runtime(|context| entry::make_bytes(context, &body));
                entry::call(push_fn, message, chunk, absent, absent, absent);
            }
            let null = entry::null_value();
            entry::call(push_fn, message, null, absent, absent, absent);
            entry::with_runtime(|context| set_bool(context, message, "complete", true));
            return Some(message);
        }
        if start.elapsed() > Duration::from_millis(RESPONSE_TIMEOUT_MS) {
            return None;
        }
        std::thread::sleep(Duration::from_millis(4));
    }
}

pub(crate) fn decode_body(buf: &[u8], framing: parser::Framing) -> Vec<u8> {
    match framing {
        parser::Framing::None => Vec::new(),
        parser::Framing::Length(n) => buf[..n.min(buf.len())].to_vec(),
        parser::Framing::Chunked => {
            let mut remaining = buf.to_vec();
            let mut decoder = parser::ChunkedDecoder::new();
            let mut out = Vec::new();
            loop {
                match decoder.step(&mut remaining) {
                    parser::ChunkOutcome::Body(bytes) => out.extend_from_slice(&bytes),
                    parser::ChunkOutcome::Done | parser::ChunkOutcome::NeedMore => break,
                }
            }
            out
        }
    }
}

/// Pulls whatever `net` has already pushed into `socket`'s Readable buffer
/// (via `net::registry::pump`, forced the same way [`connect_blocking`]
/// forces it) and returns it as raw bytes, leaving the stream's own
/// bookkeeping untouched — this client reads the wire directly rather than
/// going through `read()`/`'data'`, since it needs the raw framing bytes,
/// not decoded chunks.
fn drain_socket_buffer(socket: u64) -> Vec<u8> {
    let empty = entry::with_runtime(|context| entry::make_bytes(context, &[]));
    let absent = entry::undefined_value();
    call_method(socket, "write", empty, absent, absent);
    let mut out = Vec::new();
    loop {
        let read_fn = entry::with_runtime(|context| entry::get_member(context, socket, "read"));
        let chunk = entry::call(read_fn, socket, absent, absent, absent, absent);
        if chunk == entry::null_value() || chunk == absent {
            break;
        }
        if let Some(bytes) = entry::with_runtime(|context| entry::bytes_of(context, chunk)) {
            out.extend_from_slice(&bytes);
        }
    }
    out
}

extern "C" fn client_destroy(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let socket = get_value(this, "socket");
    call_method(socket, "destroy", entry::undefined_value(), entry::undefined_value(), entry::undefined_value());
    entry::with_runtime(|context| set_bool(context, this, "destroyed", true));
    this
}
