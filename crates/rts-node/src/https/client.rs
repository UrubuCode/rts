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
//!
//! # Two process-killing bugs here, both fixed 2026-09
//!
//! **`connect_blocking` against a real server aborted the process** with
//! `[RTS PANIC] RefCell already borrowed`, before a single request byte went
//! out. The panic was IN this file's own call chain but not this file's own
//! bug: this module's tight `write`/`getProtocol` spin ends up pumping the
//! accepted-server side's queue re-entrantly on the same thread, and the
//! actual nested borrow was in `tls::server::on_connection` — see that
//! function's own doc for the fix. `tests/claude-node-https-crash.test.ts`
//! has the full backtrace this was traced from.
//!
//! **`https.request({ headers: {...} })` aborted the process too**, a
//! second and unrelated defect in [`apply_options`] itself: it took
//! `context` — meaning its caller already held the runtime borrow — and
//! called the ambient `entry::own_keys` directly to walk the headers
//! object's keys. Fixed by pulling that walk into its own pass,
//! [`read_headers`], which is not one more field `apply_options` fills in —
//! see that function's own doc for why. Neither killer call above was
//! reachable through a header option, so neither fixture caught this one; a
//! crate-wide sweep for the same shape (any `context`-taking function
//! calling an ambient `entry::*` directly) did, and found the identical bug
//! in `http::client`'s own copy of this file at the same time.
//!
//! # A third bug, a different class: `'error'` emitted before a listener
//! could exist
//!
//! `build_request` used to call `common::emit` for a failed connection
//! SYNCHRONOUSLY, inside the same native call that had just built the
//! `ClientRequest` instance — before the value was even returned to the
//! caller, so `req.on('error', cb)` (the ordinary Node idiom, written on the
//! line after `https.request(...)` returns) could never run in time. An
//! `'error'` with no listener kills the process (`common::emit`'s own doc),
//! and not even a `try`/`catch` wrapping the whole call saves it — checked,
//! not assumed. [`emit_error_later`] defers the emit through a real
//! `setTimeout(fn, 0)`, the same "later turn" a caller's own synchronous
//! statements now get to run ahead of, matching Node's own behavior (a
//! connection attempt there is never synchronous either). See that
//! function's own doc for why this reuses `node:timers` rather than
//! building `node:net`'s queue-and-pump shape a second time.

use rts_core::entry;
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
    let (host, port, path, method) = entry::with_runtime(|context| read_request_options(context, url_or_options, options));
    // Read OUTSIDE the borrow above, and as its OWN pass over both option
    // sources — see `read_headers`'s own doc for why a `headers` object walk
    // cannot share `read_request_options`'s borrow the way the four scalar
    // fields do.
    let mut headers = read_headers(url_or_options, options);
    if !headers.iter().any(|(n, _)| n.eq_ignore_ascii_case("host")) {
        headers.push(("Host".to_owned(), host.clone()));
    }

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
        emit_error_later(instance, error_instance);
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

/// Emits `'error'` on `instance` on a LATER turn instead of synchronously.
///
/// # The bug this replaces
///
/// `build_request` used to call `common::emit` directly, inside the same
/// native call that just built `instance` a line above — before the value
/// had even been returned to the caller, let alone before a caller's next
/// statement could run `req.on('error', cb)`. Real Node's `http(s).request()`
/// NEVER emits `'error'` synchronously during construction for exactly this
/// reason (a connection attempt is always asynchronous there), so the
/// ordinary, universally-documented idiom — `const req = https.request(opts);
/// req.on('error', cb); req.end();` — has no possible way to have attached a
/// listener first under a synchronous emit. `common::emit`'s own doc: an
/// `'error'` with none attached ends the process, and a native cannot raise
/// something a caller's `try`/`catch` around the whole `https.request(...)`
/// call would see — that path was checked and does not save it either.
///
/// # Why `setTimeout(fn, 0)` rather than `node:net`'s own queue-and-pump
///
/// `node:net`'s `Socket::connect` solves the identical ordering problem by
/// spawning a background OS thread and delivering the failure through
/// `net::registry`'s queue, pumped on a LATER native call. This client has no
/// background thread — `connect_blocking`/[`build_request`] already know the
/// outcome by the time this runs, synchronously, on the calling thread — so
/// there is nothing to poll for. What is missing is only a LATER TURN to
/// deliver on, and `node:timers`' zero-delay `setTimeout` already IS that:
/// `docs/reference/node/STATUS.md`'s fixed defect describes the same
/// end-of-turn pump this rides. Building a second queue/table/loop-source
/// registration to get the same "later" would be the class
/// `docs/reference/node/STATUS.md`'s "one source, generated views" section
/// warns against — reusing a mechanism this crate already has and already
/// tests, rather than adding a second one that does the same thing.
fn emit_error_later(instance: u64, error: u64) {
    let state = entry::with_runtime(|context| {
        let state = entry::make_object(context);
        entry::put_member(context, state, "instance", instance);
        entry::put_member(context, state, "error", error);
        state
    });
    // Minted OUTSIDE the borrow above, like every closure/call in this crate
    // — `entry::closure_new` takes the runtime borrow itself.
    let closure = entry::closure_new(deliver_deferred_error as *const () as usize as i64, state);
    let (timers_ns, absent) = entry::with_runtime(|context| (entry::module_at_name(context, "node:timers"), entry::undefined_in(context)));
    let set_timeout = entry::with_runtime(|context| entry::get_member(context, timers_ns, "setTimeout"));
    let delay = entry::make_number(0.0);
    entry::call(set_timeout, absent, closure, delay, absent, absent);
}

/// The `setTimeout` callback [`emit_error_later`] schedules — reads
/// `instance`/`error` back off its closure state and emits, now on a turn a
/// caller's own synchronous statements (`req.on('error', cb)`) have already
/// run past.
extern "C" fn deliver_deferred_error(state: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let (instance, error) =
        entry::with_runtime(|context| (entry::get_member(context, state, "instance"), entry::get_member(context, state, "error")));
    let absent = entry::undefined_value();
    emit(instance, "error", error, absent, absent);
    absent
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

/// `(host, port, path, method)` off either a URL string or an options
/// object — `docs/reference/node/https.md`'s reduced `RequestOptions`, the
/// same fields `http::client`'s own (private) reader takes; see the module
/// doc for why this is a small duplicate rather than a reused import.
/// `headers` used to be a fifth field here — see [`read_headers`] for why it
/// was pulled out into its own pass rather than staying alongside these
/// four.
fn read_request_options(context: &mut entry::Context, url_or_options: u64, options: u64) -> (String, u16, String, String) {
    let mut host = "localhost".to_owned();
    let mut port = 443u16;
    let mut path = "/".to_owned();
    let mut method = "GET".to_owned();
    if let Some(text) = entry::text_in(context, url_or_options) {
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
/// `https.request({..., headers: {...}})` aborted the process with `[RTS
/// PANIC] RefCell already borrowed` before a single byte went out — the same
/// shape `http::client`'s sibling copy of this file had, found together. See
/// `wasi::mod::read_string_map` for the identical open-and-close-per-step
/// discipline applied to a different options object.
fn read_headers(url_or_options: u64, options: u64) -> Vec<(String, String)> {
    let mut headers = Vec::new();
    let is_url_string = entry::with_runtime(|context| entry::text_in(context, url_or_options).is_some());
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
