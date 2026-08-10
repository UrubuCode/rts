//! `node:http` — `createServer`/`Server`/`IncomingMessage`/`ServerResponse`,
//! `request`/`get`/`ClientRequest`, `Agent`, `STATUS_CODES`/`METHODS`, against
//! `docs/reference/node/http.md`.
//!
//! # Reuse-check, before anything here was written
//!
//! `.claude/skills/reuse-check/SKILL.md`'s search: `rts-cranelift` has
//! nothing to say about a socket or an HTTP parser (its table is value
//! encodings, shapes, ABI — none of which this module touches). Inside this
//! crate, the answer was "most of it, reuse rather than re-derive": `net`
//! already IS the TCP layer (`net::Socket`/`net::Server` over a background
//! thread + this crate's native↔JS handoff recipe) and `stream` already IS
//! `Readable`/`Writable` with correct backpressure — this module never opens
//! a socket and never reimplements push/write/backpressure; see
//! `server.rs`'s and `message.rs`'s own docs for exactly how each is reached
//! (through `net`'s and `stream`'s public JS surface, since both crate
//! modules' internals are private to them — the same constraint
//! `docs/reference/node/STATUS.md` names for every module here). The one
//! piece nothing in this crate or `rts-cranelift` had: an HTTP/1.1 head
//! parser and a chunked/`Content-Length` body decoder. That is `parser.rs`,
//! and it is the real work this module adds.
//!
//! # The rule this module pays like every other one here
//!
//! `with_runtime` holds a `RefCell` borrow for its body; a call into a
//! JavaScript listener inside one aborts the process (`RefCell` borrows
//! don't unwind an `extern "C"` frame). Every native here collects what it
//! needs from JS values, drops any borrow, THEN calls a listener — see
//! `server.rs`'s `drain`/`advance` split for the shape this takes when the
//! parsing itself must also not hold this module's OWN lock across a call
//! into JS (a `std::sync::Mutex` is no more reentrant than a `RefCell`).
//!
//! # Not implemented, by name
//!
//! **Keep-alive and pipelining** — every response sends `Connection: close`
//! and the socket is torn down after one request/response; a second request
//! line on the same socket is never read (`registry.rs`/`server.rs`).
//! **`100-continue`** (`'checkContinue'`, `writeContinue`, client
//! `'continue'`) — bodies are always read in full, `Expect` is not
//! inspected. **`CONNECT`/`Upgrade`** — no `'connect'`/`'upgrade'` event,
//! no raw-socket hand-off. **Trailers on the WRITE side** — `addTrailers` is
//! a no-op; the READ side (`parser::ChunkedDecoder`) consumes and discards
//! them. **`AbortSignal`** (`options.signal`) is not read. **Unix domain
//! sockets / `socketPath`** — `net`'s own module doc already refuses these;
//! this module inherits that. **`insecureHTTPParser`, `maxHeaderSize`
//! enforcement, `uniqueHeaders`** — the parser is already permissive and
//! nothing bounds header-block size. **`http.setMaxIdleHTTPParsers`**,
//! **`http.setGlobalProxyFromEnv`/proxy tunneling** — accepted and ignored
//! or not exposed. **`writeHead`'s headers-as-iterable-of-pairs form** — only
//! a plain `{name: value}` object is read (`outgoing::apply_headers_object`).
//! **A streaming `ClientRequest`** — see `client.rs`'s own doc for why this
//! client blocks through the whole exchange instead: there is no event loop
//! a background thread can post a later turn into, the same limit `net`'s
//! own doc names.

mod agent;
mod client;
mod common;
mod incoming;
mod outgoing;
mod parser;
mod registry;
mod server;
mod status;

use rts_core::entry::{self, Context, Provided};

/// The one place this module reaches `net`'s public constructor table —
/// `crate::net::namespace` is `pub fn` on a `pub mod`, so calling it here is
/// calling a sibling crate module's real API, not its internals. Building it
/// again per call costs a namespace object and (idempotently, by name) the
/// `Socket`/`Server` prototypes — see `net::common::chained_prototype`'s own
/// doc for why a second call never gets a second prototype.
pub(super) fn net_namespace(context: &mut Context) -> u64 {
    crate::net::namespace(context)
}

/// The namespace `node:http` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("createServer", create_server),
        ("request", request),
        ("get", get),
        ("validateHeaderName", validate_header_name),
        ("validateHeaderValue", validate_header_value),
        ("setMaxIdleHTTPParsers", no_op),
        ("setGlobalProxyFromEnv", no_op),
    ];
    let namespace = entry::make_namespace(context, members);

    let server_ctor = entry::make_callable(context, server::construct);
    let server_prototype = server::prototype(context);
    entry::put_member(context, server_ctor, "prototype", server_prototype);
    entry::put_member(context, namespace, "Server", server_ctor);

    let agent_ctor = entry::make_callable(context, agent::construct);
    let agent_prototype = agent::prototype(context);
    entry::put_member(context, agent_ctor, "prototype", agent_prototype);
    entry::put_member(context, namespace, "Agent", agent_ctor);
    let global_agent = entry::make_instance(context, agent_prototype);
    entry::put_member(context, namespace, "globalAgent", global_agent);

    // `IncomingMessage`/`ServerResponse`/`ClientRequest`/`OutgoingMessage`
    // are exported as names (Node's own docs list them as importable) but
    // never constructed directly by a program — `message.rs`'s own doc
    // states why: they carry state (`socket`, `req`) only this module's own
    // construction sites can supply.
    let incoming_prototype = incoming::incoming_prototype(context);
    let incoming_ctor = entry::make_callable(context, refuse_construct);
    entry::put_member(context, incoming_ctor, "prototype", incoming_prototype);
    entry::put_member(context, namespace, "IncomingMessage", incoming_ctor);

    let response_prototype = outgoing::server_response_prototype(context);
    let response_ctor = entry::make_callable(context, refuse_construct);
    entry::put_member(context, response_ctor, "prototype", response_prototype);
    entry::put_member(context, namespace, "ServerResponse", response_ctor);

    let outgoing_prototype = common::chained_prototype(context, "Writable", "OutgoingMessage", outgoing::OUTGOING_METHODS);
    let outgoing_ctor = entry::make_callable(context, refuse_construct);
    entry::put_member(context, outgoing_ctor, "prototype", outgoing_prototype);
    entry::put_member(context, namespace, "OutgoingMessage", outgoing_ctor);

    let client_prototype = client::prototype(context);
    let client_ctor = entry::make_callable(context, refuse_construct);
    entry::put_member(context, client_ctor, "prototype", client_prototype);
    entry::put_member(context, namespace, "ClientRequest", client_ctor);

    let status_codes = status::status_codes_object(context);
    entry::put_member(context, namespace, "STATUS_CODES", status_codes);
    let methods = status::methods_array(context);
    entry::put_member(context, namespace, "METHODS", methods);
    let max_header_size = entry::make_number(16384.0);
    entry::put_member(context, namespace, "maxHeaderSize", max_header_size);
    namespace
}

extern "C" fn refuse_construct(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    this
}

extern "C" fn create_server(e: u64, _this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    server::construct(e, 0, a, b, c, d)
}

/// `http.request(url|options[, options][, callback])`.
extern "C" fn request(_e: u64, _this: u64, a: u64, b: u64, c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let (options, callback) = if common::is_callable(b) { (absent, b) } else { (b, c) };
    client::build_request(a, options, callback, false)
}

/// `http.get(url|options[, options][, callback])` — `request` plus an
/// implicit `.end()`.
extern "C" fn get(_e: u64, _this: u64, a: u64, b: u64, c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let (options, callback) = if common::is_callable(b) { (absent, b) } else { (b, c) };
    client::build_request(a, options, callback, true)
}

/// `http.validateHeaderName(name[, label])` — Node THROWS
/// `ERR_INVALID_HTTP_TOKEN` on an invalid name rather than answering `false`;
/// this crate's only public raise is `throw_type_error` (rule 8,
/// `rts-core`'s README), so the class is `TypeError` rather than Node's
/// own, named here rather than silently substituted. On a valid name this
/// still answers `undefined` — Node's own success return — never `true`.
extern "C" fn validate_header_name(_e: u64, _this: u64, name: u64, _label: u64, _c: u64, _d: u64) -> u64 {
    let valid = entry::text_of(name).is_some_and(|text| parser::valid_header_name(&text));
    if !valid {
        entry::throw_type_error("Header name must be a valid HTTP token");
    }
    entry::undefined_value()
}

/// `http.validateHeaderValue(name, value)` — throws
/// `ERR_INVALID_CHAR`/`ERR_HTTP_INVALID_HEADER_VALUE` on an invalid value,
/// same reasoning as [`validate_header_name`].
extern "C" fn validate_header_value(_e: u64, _this: u64, _name: u64, value: u64, _c: u64, _d: u64) -> u64 {
    let valid = entry::text_of(value).is_some_and(|text| parser::valid_header_value(&text));
    if !valid {
        entry::throw_type_error("Invalid character in header content");
    }
    entry::undefined_value()
}

extern "C" fn no_op(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::undefined_value()
}
