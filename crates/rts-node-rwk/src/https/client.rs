//! `https.request`/`https.get`/`https.Agent`/`https.globalAgent` — a
//! `ClientRequest` built over a `tls.TLSSocket` instead of a `net.Socket`.
//!
//! # How this reuses `http`'s request-line writer and response parser
//!
//! `http::client`'s `write`/`end` methods (`client_write`/`client_end`,
//! installed on `http.ClientRequest.prototype`) read and act on nothing but
//! ordinary instance properties — `socket`, `method`, `path`, `__body__`,
//! `__headers__`, `__headerOrder__` — and call `socket.write`/`socket.read`
//! generically (`http/client.rs`'s own doc: it reaches `net` only through
//! `net`'s public JS surface, the same rule this module pays). Nothing in
//! either method assumes `socket` is a `net.Socket` rather than any other
//! object with `write`/`read`, and a `TLSSocket` has both (`tls/socket.rs`:
//! it is a `Duplex` chained onto `net`'s own `"Socket"` prototype).
//!
//! So this module builds a `ClientRequest` instance whose prototype IS
//! `http.ClientRequest.prototype` (fetched by name off `crate::http::namespace`,
//! never rebuilt) and whose `socket` is a `TLSSocket`. Calling `.end()` /
//! `.write()` on it runs `http`'s own compiled `client_end`/`client_write` —
//! including `parser::parse_response_head`/the chunked decoder inside it —
//! unmodified. **No HTTP request/response format code exists in this
//! file.** What this file adds is the one piece that legitimately differs
//! from plain `http`: opening the connection through `tls.connect` instead
//! of a bare `net.Socket`, and the request-options reader (`hostname`/
//! `port`/`path`/`method`/`headers`), which `http::client`'s own copy is
//! private to that crate module and is small enough (reads five fields off
//! a JS object) that duplicating it is the cost every module here pays for
//! its own options object, not a second parser.

use rts_core_rwk::entry;
use std::time::{Duration, Instant};

use super::common::*;

const CONNECT_TIMEOUT_MS: u64 = 5000;

/// `https.request(url|options[, options][, callback])`.
pub(super) extern "C" fn request(_e: u64, _this: u64, a: u64, b: u64, c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let (options, callback) = if is_callable(b) { (absent, b) } else { (b, c) };
    build_request(a, options, callback, false)
}

/// `https.get(url|options[, options][, callback])` — [`request`] plus an
/// implicit `.end()`, the same collapse `http.get` makes.
pub(super) extern "C" fn get(_e: u64, _this: u64, a: u64, b: u64, c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let (options, callback) = if is_callable(b) { (absent, b) } else { (b, c) };
    build_request(a, options, callback, true)
}

fn build_request(url_or_options: u64, options: u64, callback: u64, auto_end: bool) -> u64 {
    let absent = entry::undefined_value();
    let (host, port, path, method, headers) = entry::with_runtime(|context| read_request_options(context, url_or_options, options));

    let socket = tls_connect(&host, port);
    if !connect_blocking(socket) {
        let error_instance = error_object("ECONNREFUSED", "connect failed");
        // Still hand back a real `ClientRequest`-shaped object so a
        // program's `.on('error', ...)` has something to have registered
        // on, matching `http::client::build_request`'s own shape for the
        // same failure.
        let instance = entry::with_runtime(|context| {
            let prototype = http_member(context, "ClientRequest");
            let prototype = entry::get_member(context, prototype, "prototype");
            let instance = entry::make_instance(context, prototype);
            init_emitter(context, instance);
            instance
        });
        emit(instance, "error", error_instance, absent, absent);
        return instance;
    }

    let instance = entry::with_runtime(|context| {
        let ctor = http_member(context, "ClientRequest");
        let prototype = entry::get_member(context, ctor, "prototype");
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

    // `setHeader` — the same `OutgoingMessage` method every `http` request
    // writes through, reused rather than poking `__headers__` by hand.
    for (name, value) in &headers {
        let (name_v, value_v) = entry::with_runtime(|context| (entry::make_string(context, name), entry::make_string(context, value)));
        call_method(instance, "setHeader", name_v, value_v, absent);
    }

    if callback != absent {
        let once_fn = entry::with_runtime(|context| entry::get_member(context, instance, "once"));
        entry::call(once_fn, instance, key("response"), callback, absent, absent);
    }

    if auto_end {
        call_method(instance, "end", absent, absent, absent);
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

/// Opens the `TLSSocket` this request runs over, through `tls`'s own public
/// `connect` — never a bare `net.Socket` or a `TcpStream` opened here.
fn tls_connect(host: &str, port: u16) -> u64 {
    let absent = entry::undefined_value();
    let connect_fn = entry::with_runtime(|context| tls_member(context, "connect"));
    let options = entry::with_runtime(|context| {
        let options = entry::make_object(context);
        let host_v = entry::make_string(context, host);
        let port_v = entry::make_number(port as f64);
        entry::put_member(context, options, "host", host_v);
        entry::put_member(context, options, "port", port_v);
        let servername_v = entry::make_string(context, host);
        entry::put_member(context, options, "servername", servername_v);
        options
    });
    entry::call(connect_fn, absent, options, absent, absent, absent)
}

/// Spins on `tlsSocket.write(empty)` (the only reliable way to force the
/// underlying `net` registry to pump — the same technique
/// `http::client::connect_blocking` uses on a plain socket) until
/// `getProtocol()` reports the handshake done, or the timeout elapses.
fn connect_blocking(socket: u64) -> bool {
    let start = Instant::now();
    let absent = entry::undefined_value();
    loop {
        let empty = entry::with_runtime(|context| entry::make_bytes(context, &[]));
        call_method(socket, "write", empty, absent, absent);
        let protocol = call_method(socket, "getProtocol", absent, absent, absent);
        if protocol != entry::null_value() && protocol != absent {
            return true;
        }
        if start.elapsed() > Duration::from_millis(CONNECT_TIMEOUT_MS) {
            return false;
        }
        std::thread::sleep(Duration::from_millis(4));
    }
}

/// `(host, port, path, method, headers)` off either a URL string or an
/// options object — `docs/reference/node/https.md`'s reduced
/// `RequestOptions`, the same fields `http::client`'s own (private) reader
/// takes; see the module doc for why this is a small duplicate rather than a
/// reused import.
fn read_request_options(context: &mut entry::Context, url_or_options: u64, options: u64) -> (String, u16, String, String, Vec<(String, String)>) {
    let mut host = "localhost".to_owned();
    let mut port = 443u16;
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

/// A minimal `https://host[:port]/path` split — the same reduced form
/// `http::client`'s own copy makes for `http://`, ported to the default
/// port `443` makes.
fn parse_url_into(text: &str, host: &mut String, port: &mut u16, path: &mut String) {
    let rest = text.strip_prefix("https://").unwrap_or(text);
    let (authority, rest_path) = rest.split_once('/').map(|(a, p)| (a, format!("/{p}"))).unwrap_or_else(|| (rest, "/".to_owned()));
    *path = rest_path;
    match authority.split_once(':') {
        Some((h, p)) => {
            *host = h.to_owned();
            *port = p.parse().unwrap_or(443);
        }
        None => *host = authority.to_owned(),
    }
}
