//! `http.request`/`http.get`/`ClientRequest` — built the same way
//! [`super::server`] is: entirely through `net`'s public JS surface (a
//! `net.Socket`, connected then written to and read from via its own
//! `write`/`on('data', …)`), never through `net`'s private internals.
//!
//! # It no longer blocks, and that is the whole of this module's history
//!
//! `request()` used to connect synchronously and `end()` used to read the entire
//! response before returning, each by spinning on `socket.write(empty)` to force
//! `net::registry::pump` with a 4 ms sleep between attempts. The doc here called
//! that "the one real divergence from Node", and it was worse than a divergence:
//! over loopback, with the server in the SAME program, the exchange could not
//! complete at all. `net::registry::pump` is guarded against reentrancy, a
//! same-program request starts inside a pump (`server.listen`'s callback runs
//! from one), and so every pump the spin asked for returned without delivering
//! the `'connection'` the server was waiting for. Measured: `listening` and then
//! a real `ETIMEDOUT` ten seconds later.
//!
//! `request()` and `end()` now return immediately and the response arrives
//! through `'data'` listeners — `super::response_reader`, which is `server.rs`'
//! own reading discipline applied to the client half. That module's doc carries
//! the measurement, the Node 22 event order beside ours, and the two contracts
//! this deliberately changed.
//!
//! **One "later turn" this module has**, and it predates the above:
//! `node:timers`' `setTimeout(fn, 0)`, which [`emit_later`] rides — enough for
//! `'socket'` and for a deferred `'error'`, neither of which a caller could
//! observe if emitted from inside `build_request`.
//!
//! # Two bugs fixed 2026-09, both shared with `https::client`'s twin copy
//!
//! `apply_options` took `context` (meaning its caller already held the
//! runtime borrow) yet called the ambient `entry::own_keys` directly to walk
//! `headers`' keys, so `http.request({ headers: {...} })` aborted the
//! process — found by a crate-wide sweep, not a fixture, and fixed by
//! pulling that walk into its own pass, [`read_headers`] (see its doc for
//! why it cannot be one more field `apply_options` fills in). Separately,
//! `build_request` used to `emit` a failed connection's `'error'`
//! SYNCHRONOUSLY, before the instance it just built was even returned —
//! so `req.on('error', cb)` on the next line could never run in time, and an
//! `'error'` with no listener kills the process. [`emit_error_later`] defers
//! it through `setTimeout(fn, 0)` instead. `https::client`'s copy of this
//! file has the fuller account of both; they were found and fixed together.

use rts_core::entry::{self, Provided};
use super::common::*;
use super::{outgoing, parser, response_reader};

pub(super) const CLIENT_METHODS: &[(&str, Provided)] = outgoing::OUTGOING_METHODS;

pub(super) fn prototype(context: &mut entry::Context) -> u64 {
    // `ClientRequest` needs `end` to mean "frame the whole request and start
    // reading the response" rather than `OutgoingMessage`'s plain "flush and
    // finish", and `write` to BUFFER rather than send — so its own `end`/`write`
    // shadow the shared list rather than reusing it verbatim.
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
    let (host, port, path, method) = entry::with_runtime(|context| read_request_options(context, url_or_options, options));
    // Read OUTSIDE the borrow above, and as its OWN pass over both option
    // sources — see `read_headers`'s own doc for why a `headers` object walk
    // cannot share `read_request_options`'s borrow the way the four scalar
    // fields do.
    let mut headers = read_headers(url_or_options, options);
    if !headers.iter().any(|(n, _)| n.eq_ignore_ascii_case("host")) {
        headers.push(("Host".to_owned(), host.clone()));
    }
    let net_ns = entry::with_runtime(super::net_namespace);
    let absent = entry::undefined_value();
    let socket_ctor = entry::with_runtime(|context| entry::get_member(context, net_ns, "Socket"));
    let socket = entry::call(socket_ctor, absent, absent, absent, absent, absent);
    // Before `connect`, because the first `socket.write` pumps `node:net` and is
    // therefore the first thing that can deliver a refusal — see
    // `crate::owned_socket`.
    crate::owned_socket::absorb_errors(socket);

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

    // The socket's failure, relayed onto the request a program actually holds —
    // `connect_blocking` used to ask for it, and nothing asks now. Installed
    // before `connect`, for the reason `crate::owned_socket` states: the first
    // pump can deliver a refusal.
    relay_socket_error(instance, socket);

    let connect_fn = entry::with_runtime(|context| entry::get_member(context, socket, "connect"));
    let port_v = entry::make_number(port as f64);
    let host_v = entry::with_runtime(|context| entry::make_string(context, &host));
    entry::call(connect_fn, socket, port_v, host_v, absent, absent);
    // No wait for `'connect'`. `net::registry::write_now` queues what is written
    // while a socket is still connecting and flushes it on `'connect'` — which is
    // Node's own behaviour — so `end()` below can frame the request immediately,
    // and `'socket'` is reported on a later turn the way Node reports it.
    emit_socket_later(instance, socket);

    if auto_end {
        client_end(0, instance, absent, absent, absent, 0);
    }
    instance
}

/// Emits `'error'` on `instance` on a LATER turn instead of synchronously —
/// see `https::client`'s copy of this function for the full account (the two
/// were found and fixed together, the same failed-connection shape). Short
/// version: emitting inside `build_request` itself, before the
/// value it just built was even returned to the caller, made
/// `req.on('error', cb)` — the ordinary Node idiom — impossible to run in
/// time, and an `'error'` with no listener kills the process (`common::emit`'s
/// own doc), unrecoverably even from a `try`/`catch` wrapping the whole
/// `http.request(...)` call. `setTimeout(fn, 0)` gives the caller's own
/// synchronous statements a turn to run first, the same "later turn"
/// `docs/reference/node/STATUS.md`'s fixed `setTimeout(f, 0)` defect
/// describes pumping into — reused rather than building a second
/// queue/table/loop-source the way `node:net`'s own (threaded) `connect`
/// needs one for.
fn emit_error_later(instance: u64, error: u64) {
    emit_later(instance, "error", error);
}

/// `'socket'` on a later turn — Node reports it once the request has a socket,
/// and a program's `req.on('socket', …)` is written on the line AFTER
/// `http.request(...)` returns, so emitting it from inside `build_request` is the
/// same unobservable-by-construction event `emit_error_later` exists for.
fn emit_socket_later(instance: u64, socket: u64) {
    emit_later(instance, "socket", socket);
}

/// Relays the owned socket's `'error'` onto the request, deferred.
///
/// Deferred and not direct: `_write` pumps `node:net` before it writes, so a
/// refusal already queued is delivered from INSIDE `end()` — before the caller's
/// own `req.on('error', …)` for `http.get`, which ends the request itself. The
/// socket keeps `crate::owned_socket`'s recording listener too; that one absorbs,
/// this one reports.
fn relay_socket_error(instance: u64, socket: u64) {
    let absent = entry::undefined_value();
    let on_fn = entry::with_runtime(|context| entry::get_member(context, socket, "on"));
    if on_fn == absent {
        return;
    }
    // The request is reached through a PROPERTY of the socket and the listener is
    // a plain callable, which is the form every other relay in this module takes
    // (`server.rs`'s four `__httpServer__` relays). A closure over the request
    // was tried first and never fired.
    entry::with_runtime(|context| set_value(context, socket, "__clientRequest__", instance));
    let listener = entry::with_runtime(|context| entry::make_callable(context, relay_error));
    entry::call(on_fn, socket, key("error"), listener, absent, absent);
}

/// The socket's `'error'`, reported on the `ClientRequest` a program holds.
extern "C" fn relay_error(_e: u64, socket: u64, error: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let instance = get_value(socket, "__clientRequest__");
    if instance == absent {
        return absent;
    }
    // Once. A refused connection is followed by a reset and then a close, and
    // `node:net` emits `'error'` for more than one of them; Node reports the
    // first on the request and nothing after it.
    if get_value(instance, "__errored__") != entry::boolean_value(true) {
        entry::with_runtime(|context| set_bool(context, instance, "__errored__", true));
        emit_error_later(instance, error);
    }
    absent
}

/// Emits `event` on `instance` from a `setTimeout(fn, 0)` turn.
fn emit_later(instance: u64, event: &str, payload: u64) {
    let state = entry::with_runtime(|context| {
        let state = entry::make_object(context);
        entry::put_member(context, state, "instance", instance);
        entry::put_member(context, state, "error", payload);
        let name = entry::make_string(context, event);
        entry::put_member(context, state, "event", name);
        state
    });
    // Minted OUTSIDE the borrow above — `entry::closure_new` takes the
    // runtime borrow itself.
    let closure = entry::closure_new(deliver_deferred_error as *const () as usize as i64, state);
    let (timers_ns, absent) = entry::with_runtime(|context| (entry::module_at_name(context, "node:timers"), entry::undefined_in(context)));
    let set_timeout = entry::with_runtime(|context| entry::get_member(context, timers_ns, "setTimeout"));
    let delay = entry::make_number(0.0);
    entry::call(set_timeout, absent, closure, delay, absent, absent);
}

/// The `setTimeout` callback [`emit_later`] schedules.
extern "C" fn deliver_deferred_error(state: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let (instance, error, event) = entry::with_runtime(|context| {
        (
            entry::get_member(context, state, "instance"),
            entry::get_member(context, state, "error"),
            entry::get_member(context, state, "event"),
        )
    });
    let absent = entry::undefined_value();
    let name = entry::text_of(event).unwrap_or_else(|| "error".to_owned());
    emit(instance, &name, error, absent, absent);
    absent
}

/// `(host, port, path, method)` off either a URL string or an options
/// object — `docs/reference/node/http.md` §3's `RequestOptions`, reduced to
/// the fields this client acts on. Unrecognized fields (`agent`, `signal`,
/// `family`, `lookup`, …) are read never; see the module's own "not
/// implemented" list. `headers` used to be a fifth field here — see
/// [`read_headers`] for why it was pulled out into its own pass rather than
/// staying alongside these four.
fn read_request_options(context: &mut entry::Context, url_or_options: u64, options: u64) -> (String, u16, String, String) {
    let mut host = "localhost".to_owned();
    let mut port = 80u16;
    let mut path = "/".to_owned();
    let mut method = "GET".to_owned();
    // `string_in` and not `text_in`: this decides WHICH OVERLOAD was written,
    // and a conversion cannot. `http.request(undefined, opts, cb)` answered the
    // text "undefined" here and parsed it as a URL, so `opts` went unread — the
    // same mistake `common::option_text` documents, one argument earlier.
    if let Some(text) = entry::string_in(context, url_or_options) {
        parse_url_into(&text, &mut host, &mut port, &mut path);
    } else {
        apply_options(context, url_or_options, &mut host, &mut port, &mut path, &mut method);
    }
    if options != entry::undefined_in(context) {
        apply_options(context, options, &mut host, &mut port, &mut path, &mut method);
    }
    (host, port, path, method)
}

fn apply_options(context: &mut entry::Context, options: u64, host: &mut String, port: &mut u16, path: &mut String, method: &mut String) {
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
}

/// `options.headers` off either option source, merged in the same order
/// [`read_request_options`] applies `url_or_options` then `options` — a
/// `string[]`-per-name shape is not read; only the plain `{name: value}`
/// object form is, matching this client's other reduced option handling.
///
/// # Why this is not a fifth field `apply_options` fills in
///
/// `apply_options` takes `context: &mut Context` — by this crate's own
/// convention (`docs/reference/node/STATUS.md`'s "the rule every module here
/// pays") that means its CALLER already holds the runtime borrow, so nothing
/// inside it may grab a second one. Walking `headers`' keys needs
/// `entry::own_keys`, which is AMBIENT — it takes the borrow itself, on
/// purpose, so a `headers` getter can run with none held (its own doc says
/// so) — so it cannot be one more step inside `apply_options` the way
/// `hostname`/`port`/`path`/`method` are. It used to be, and every
/// `http.request({..., headers: {...}})` aborted the process with `[RTS
/// PANIC] RefCell already borrowed` before a single byte went out — the same
/// shape `https::client`'s sibling copy of this file had, found together.
/// See `wasi::mod::read_string_map` for the identical open-and-close-per-step
/// discipline applied to a different options object.
fn read_headers(url_or_options: u64, options: u64) -> Vec<(String, String)> {
    let mut headers = Vec::new();
    // The same overload test `read_request_options` makes, and it has to ask
    // the same question: a conversion here would look for headers on a URL.
    let is_url_string = entry::with_runtime(|context| entry::string_in(context, url_or_options).is_some());
    if !is_url_string {
        collect_headers(url_or_options, &mut headers);
    }
    let has_options = entry::with_runtime(|context| options != entry::undefined_in(context));
    if has_options {
        collect_headers(options, &mut headers);
    }
    headers
}

/// One options object's `headers`, appended in enumeration order — the
/// ambient half of [`read_headers`]. Each step opens and closes its own
/// borrow rather than sharing one across the whole walk, because
/// `entry::own_keys`/`entry::get_indexed`-backed `entry::get_member` both
/// grab it themselves.
fn collect_headers(options: u64, headers: &mut Vec<(String, String)>) {
    let headers_value = entry::with_runtime(|context| {
        let absent = entry::undefined_in(context);
        let value = option_member(context, options, "headers");
        (value != absent).then_some(value)
    });
    let Some(headers_value) = headers_value else { return };
    let names = entry::own_keys(headers_value);
    let absent = entry::undefined_value();
    let mut index = 0.0;
    loop {
        let name_v = entry::get_indexed(names, entry::make_number(index));
        if name_v == absent {
            break;
        }
        let Some(name) = entry::text_of(name_v) else { break };
        let value_v = entry::with_runtime(|context| entry::get_member(context, headers_value, &name));
        if let Some(value) = entry::text_of(value_v) {
            headers.push((name, value));
        }
        index += 1.0;
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
/// headers and body, then hands the socket to [`response_reader`] and RETURNS.
/// `'response'` is emitted later, from a pump, with an `IncomingMessage` that
/// streams its body as it arrives.
extern "C" fn client_end(_e: u64, this: u64, chunk: u64, _encoding: u64, callback: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    // The ABSENCE test and not a conversion: `entry::text_of(undefined)` answers
    // `Some("undefined")`, so a plain `req.end()` appended the nine characters
    // `undefined` to the body and framed them in `Content-Length`. Measured: a
    // `POST` of `hello` arrived as `helloundefined`, 14 bytes, and the server read
    // it as the body it was told to expect — self-consistent and wrong. It was
    // invisible until this client stopped blocking, because the exchange it
    // corrupted never completed. Same mistake `read_request_options` documents one
    // argument earlier: a conversion cannot answer "was anything written".
    if chunk != absent && chunk != entry::null_value() {
        client_write(0, this, chunk, absent, absent, 0);
    }
    let socket = get_value(this, "socket");
    let method = get_text(this, "method").unwrap_or_else(|| "GET".to_owned());
    let path = get_text(this, "path").unwrap_or_else(|| "/".to_owned());
    let body = get_text(this, "__body__").unwrap_or_default();
    let mut head = format!("{method} {path} HTTP/1.1\r\n");
    // `header_order` is ambient now (it reads array elements through
    // `entry::get_indexed`, which opens its own borrow) — called bare rather
    // than wrapped in `with_runtime`, which would nest them. See its own doc.
    let order = outgoing::header_order(this);
    let has_length = order.iter().any(|n| n.eq_ignore_ascii_case("content-length"));
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

    // Start LISTENING for the response instead of reading it here. The loop that
    // used to be on this line could not complete at all when the server was in
    // the same program — `response_reader`'s module doc has the measurement and
    // the reason — and a reset or a refusal now reaches the program through
    // [`relay_socket_error`] rather than through this function's return path.
    response_reader::begin(this, socket);
    if callback != absent {
        entry::call(callback, absent, absent, absent, absent, absent);
    }
    this
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

extern "C" fn client_destroy(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let socket = get_value(this, "socket");
    // Before the socket goes: an aborted exchange leaves a half-parsed response
    // in `response_reader`'s table, and nothing would ever complete it.
    response_reader::forget(socket);
    call_method(socket, "destroy", entry::undefined_value(), entry::undefined_value(), entry::undefined_value());
    entry::with_runtime(|context| set_bool(context, this, "destroyed", true));
    this
}
