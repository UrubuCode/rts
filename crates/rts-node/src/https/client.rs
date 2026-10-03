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
//! # It no longer blocks on the handshake, and that is the last of three loops
//!
//! `build_request` used to call a `connect_blocking` that spun on
//! `tlsSocket.write(empty)` + `getProtocol()` with a 4 ms sleep until the
//! handshake reported done. It was the third of three blocking loops on the
//! `http`/`https` path; the other two went with `crate::http::client`'s
//! own rewrite, and `http::response_reader`'s module doc carries the
//! measurement that condemns all three: `net::registry::pump` has a
//! reentrancy guard, so every nested pump such a loop asks for returns
//! WITHOUT delivering anything. That makes it a deadlock rather than
//! slowness, and spinning harder cannot help.
//!
//! Measured here, 2026-10-03, against `@whiskeysockets/baileys`: the socket
//! emitted one `connection.update` with `connecting` and then nothing at all
//! for 45 s — no QR, no second update, no error.
//!
//! Nothing waits now. `tls.connect` returns a `TLSSocket` immediately and
//! rustls' own writer BUFFERS the plaintext this request writes until the
//! handshake finishes (`tls::conn::Driver::send` is `writer().write_all`), so
//! `http`'s `client_end` can frame the request straight away — exactly as
//! `http::client` writes into a `net.Socket` that is still connecting.
//! `http::response_reader` reads the answer off the `TLSSocket` by listening,
//! which it was already written to do (see its `ID` constant's doc).
//!
//! # Two process-killing bugs here, both fixed 2026-09
//!
//! **`connect_blocking` (since deleted) against a real server aborted the process** with
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
//! not assumed. `crate::owned_socket::report_error_later` defers the emit
//! through a real `setTimeout(fn, 0)`, the same "later turn" a caller's own
//! synchronous statements now get to run ahead of, matching Node's own
//! behavior (a connection attempt there is never synchronous either). See
//! that function's own doc for why this reuses `node:timers` rather than
//! building `node:net`'s queue-and-pump shape a second time — and
//! `crate::owned_socket`'s own doc for why both the recorder and the relay
//! now live there instead of once here and once in `http::client`.

use rts_core::entry;

use super::common::*;

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
    let target = entry::with_runtime(|context| read_request_options(context, url_or_options, options));
    let Target { host, port, path, method, ca, servername } = target;
    // Read OUTSIDE the borrow above, and as its OWN pass over both option
    // sources — see `read_headers`'s own doc for why a `headers` object walk
    // cannot share `read_request_options`'s borrow the way the four scalar
    // fields do.
    let mut headers = read_headers(url_or_options, options);
    if !headers.iter().any(|(n, _)| n.eq_ignore_ascii_case("host")) {
        headers.push(("Host".to_owned(), host.clone()));
    }

    let socket = tls_connect(&host, port, ca.as_deref(), servername.as_deref());
    // The `TLSSocket` is this module's, not the program's — see
    // `crate::owned_socket`. `tls::socket::on_underlying_error` already relays
    // the inner `net.Socket`'s failure onto it, so without a listener HERE that
    // relay was the thing that killed the process.
    crate::owned_socket::absorb_errors(socket);

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

    // The socket's failure, reported on the request a program actually holds.
    // `connect_blocking` used to ask the socket for it and emit here; nothing
    // asks now, so the relay is what reports — including a refusal already
    // recorded, which for `https` is possible because `tls.connect` both
    // connects and writes the ClientHello before this line is reached.
    crate::owned_socket::relay_errors(instance, socket);

    if auto_end {
        call_method(instance, "end", absent, absent, absent);
    }
    instance
}

/// Opens the `TLSSocket` this request runs over, through `tls`'s own public
/// `connect` — never a bare `net.Socket` or a `TcpStream` opened here.
///
/// `ca` and `servername` are forwarded; everything else a caller put in its
/// options object is not, which is the divergence worth naming: `ws` sets
/// `options.createConnection` and expects its own socket to be used, and this
/// ignores it. Harmless for `ws` specifically — its `tlsConnect` is
/// `tls.connect` with the same `host`/`port`/`servername` this builds — and
/// still a divergence for anyone who supplies a different one.
fn tls_connect(host: &str, port: u16, ca: Option<&str>, servername: Option<&str>) -> u64 {
    let absent = entry::undefined_value();
    let connect_fn = entry::with_runtime(|context| tls_member(context, "connect"));
    let options = entry::with_runtime(|context| {
        let options = entry::make_object(context);
        let host_v = entry::make_string(context, host);
        let port_v = entry::make_number(port as f64);
        entry::put_member(context, options, "host", host_v);
        entry::put_member(context, options, "port", port_v);
        // The address dialled unless the caller said otherwise: a certificate
        // names a host, and an IP literal matches none.
        let servername_v = entry::make_string(context, servername.unwrap_or(host));
        entry::put_member(context, options, "servername", servername_v);
        if let Some(ca) = ca {
            let ca_v = entry::make_string(context, ca);
            entry::put_member(context, options, "ca", ca_v);
        }
        options
    });
    entry::call(connect_fn, absent, options, absent, absent, absent)
}

/// What a request needs off either a URL string or an options object —
/// `docs/reference/node/https.md`'s reduced `RequestOptions`, plus the two
/// fields that are `https`'s own rather than `http`'s. See the module doc for
/// why this is a small duplicate of `http::client`'s own (private) reader
/// rather than a reused import. `headers` is NOT among them — see
/// [`read_headers`] for why it cannot share this borrow.
struct Target {
    host: String,
    port: u16,
    path: String,
    method: String,
    /// `options.ca`, forwarded to `tls.connect` as the extra trust anchor.
    ///
    /// Read because without it a server whose certificate is not in
    /// `webpki_roots` cannot be reached at all — which includes every local
    /// one, so it is also what makes a ruler for this module possible without
    /// the internet. `rejectUnauthorized` is NOT read: `tls::context` has no
    /// way to turn verification off, so accepting the option would be a name
    /// that does not do what it means.
    ca: Option<String>,
    /// `options.servername`, forwarded so SNI and the certificate's name can
    /// differ from the address dialled — `localhost` against `127.0.0.1` is
    /// the ordinary case, and `ws` sets it for exactly that reason.
    servername: Option<String>,
}

fn read_request_options(context: &mut entry::Context, url_or_options: u64, options: u64) -> Target {
    let mut target = Target {
        host: "localhost".to_owned(),
        port: 443,
        path: "/".to_owned(),
        method: "GET".to_owned(),
        ca: None,
        servername: None,
    };
    // The overload test, not a conversion — see `http::client`'s copy of this
    // line for the account.
    if let Some(text) = entry::string_in(context, url_or_options) {
        parse_url_into(&text, &mut target.host, &mut target.port, &mut target.path);
    } else {
        apply_options(context, url_or_options, &mut target);
    }
    if options != entry::undefined_in(context) {
        apply_options(context, options, &mut target);
    }
    target
}

fn apply_options(context: &mut entry::Context, options: u64, target: &mut Target) {
    if let Some(h) = option_text(context, options, "hostname").or_else(|| option_text(context, options, "host")) {
        target.host = h;
    }
    if let Some(p) = option_num(context, options, "port") {
        target.port = p as u16;
    }
    if let Some(p) = option_text(context, options, "path") {
        target.path = p;
    }
    if let Some(m) = option_text(context, options, "method") {
        target.method = m.to_ascii_uppercase();
    }
    if let Some(ca) = option_text(context, options, "ca") {
        target.ca = Some(ca);
    }
    if let Some(name) = option_text(context, options, "servername") {
        target.servername = Some(name);
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
