//! `http.Server` — an `EventEmitter` (per the task's chaining rule; real
//! Node's `Server extends net.Server` is not reproduced as prototype
//! inheritance here) that HOLDS a real `net.Server`, reached only through
//! `net`'s own public JS surface (`crate::net::namespace`, `pub mod net` in
//! `lib.rs`) — never through `net`'s private `socket`/`server`/`registry`
//! submodules, which this crate module cannot see. `listen`/`close` forward
//! to the held `net.Server`; its `'connection'` events are where an accepted
//! `net.Socket` — already a real `Duplex`/`EventEmitter`, built and owned
//! entirely by `net` — is handed to this module to read HTTP off of.
//!
//! # How a request is read off a `net.Socket`
//!
//! [`on_connection`] calls `socket.resume()` once (switching it to flowing
//! mode — see `stream/mod.rs`'s doc for why merely attaching a `'data'`
//! listener does not) and `socket.on('data', ...)`/`socket.on('end', ...)`.
//! Each `'data'` chunk's bytes are appended to a per-connection buffer in
//! [`super::registry`] and [`advance`] runs the pure `parser.rs` state
//! machine over it — never touching the engine while doing so. Only once a
//! step is fully decided (a head is complete, a body chunk decoded, the body
//! ended) does this module drop back to calling `entry::with_runtime`/
//! `entry::call`, and only AFTER the per-connection table lock is released —
//! the same lock-then-collect-then-call discipline `net::registry::pump`
//! documents, for the same reason: a listener this module calls (the
//! program's own `'request'` handler, say) can — and in every serious
//! program does — call back into this module (`res.write`, `res.end`), and a
//! `std::sync::Mutex` is not reentrant.
//!
//! # No keep-alive, no pipelining, no `Expect: 100-continue`
//!
//! Every response sends `Connection: close` and the socket is `end()`-ed
//! after `res.end()` (module doc, `registry.rs`). A second request line
//! arriving on the same socket before this module tears it down is never
//! read. See the module-level "Not implemented" section for the rest.

use rts_core_rwk::entry::{self, Provided};

use super::common::*;
use super::registry::{self, ServerConn, Stage};
use super::{incoming, outgoing, parser};

const METHODS: &[(&str, Provided)] = &[
    ("listen", listen),
    ("close", close),
    ("closeAllConnections", noop_self),
    ("closeIdleConnections", noop_self),
    ("setTimeout", set_timeout),
];

pub(super) fn prototype(context: &mut entry::Context) -> u64 {
    chained_prototype(context, "EventEmitter", "Server", METHODS)
}

/// `http.createServer([options][, requestListener])` / `new http.Server(...)`.
pub(super) extern "C" fn construct(_e: u64, this: u64, a: u64, b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let (listener,) = if is_callable(a) { (a,) } else { (b,) };
    let net_ns = entry::with_runtime(super::net_namespace);
    let create_server = entry::with_runtime(|context| entry::get_member(context, net_ns, "createServer"));
    let net_server = entry::call(create_server, absent, absent, absent, absent, absent);

    let instance = entry::with_runtime(|context| {
        let prototype = prototype(context);
        let instance = self_or_new(context, this, prototype);
        init_emitter(context, instance);
        set_bool(context, instance, "listening", false);
        set_num(context, instance, "maxHeadersCount", 2000.0);
        set_num(context, instance, "timeout", 0.0);
        set_num(context, instance, "keepAliveTimeout", 5000.0);
        set_value(context, instance, "__netServer__", net_server);
        set_value(context, net_server, "__httpServer__", instance);
        instance
    });

    let relays: [(&str, Provided); 4] =
        [("connection", on_connection), ("listening", relay_listening), ("close", relay_close), ("error", relay_error)];
    for (event, relay) in relays {
        let on_fn = entry::with_runtime(|context| entry::get_member(context, net_server, "on"));
        let callable = entry::with_runtime(|context| entry::make_callable(context, relay));
        entry::call(on_fn, net_server, key(event), callable, absent, absent);
    }

    if listener != absent {
        let on_fn = entry::with_runtime(|context| entry::get_member(context, instance, "on"));
        entry::call(on_fn, instance, key("request"), listener, absent, absent);
    }
    instance
}

/// `server.listen([port[, host]][, callback])` — forwarded verbatim to the
/// held `net.Server`; that class's own doc states which overloads it reads.
extern "C" fn listen(_e: u64, this: u64, a: u64, b: u64, c: u64, _d: u64) -> u64 {
    let net_server = get_value(this, "__netServer__");
    call_method(net_server, "listen", a, b, c);
    entry::with_runtime(|context| set_bool(context, this, "listening", true));
    this
}

/// `server.close(callback?)`.
extern "C" fn close(_e: u64, this: u64, callback: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let net_server = get_value(this, "__netServer__");
    let absent = entry::undefined_value();
    call_method(net_server, "close", absent, absent, absent);
    entry::with_runtime(|context| set_bool(context, this, "listening", false));
    if callback != absent {
        let once_fn = entry::with_runtime(|context| entry::get_member(context, this, "once"));
        entry::call(once_fn, this, key("close"), callback, absent, absent);
    }
    this
}

extern "C" fn noop_self(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    this
}

/// `server.setTimeout(msecs?, callback?)` — recorded on the instance, never
/// enforced; see `net::socket::set_timeout`'s own note for why (no idle-timer
/// mechanism exists in this crate).
extern "C" fn set_timeout(_e: u64, this: u64, msecs: u64, callback: u64, _c: u64, _d: u64) -> u64 {
    if let Some(ms) = entry::number_of(msecs) {
        entry::with_runtime(|context| set_num(context, this, "timeout", ms));
    }
    let absent = entry::undefined_value();
    if callback != absent {
        let once_fn = entry::with_runtime(|context| entry::get_member(context, this, "once"));
        entry::call(once_fn, this, key("timeout"), callback, absent, absent);
    }
    this
}

extern "C" fn relay_listening(_e: u64, net_server: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let http_server = get_value(net_server, "__httpServer__");
    let absent = entry::undefined_value();
    emit(http_server, "listening", absent, absent, absent);
    absent
}

extern "C" fn relay_close(_e: u64, net_server: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let http_server = get_value(net_server, "__httpServer__");
    let absent = entry::undefined_value();
    emit(http_server, "close", absent, absent, absent);
    absent
}

extern "C" fn relay_error(_e: u64, net_server: u64, error: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let http_server = get_value(net_server, "__httpServer__");
    let absent = entry::undefined_value();
    emit(http_server, "error", error, absent, absent);
    absent
}

/// `net.Server`'s `'connection'` listener — the point a fresh, already-built
/// `net.Socket` is handed to this module.
extern "C" fn on_connection(_e: u64, net_server: u64, socket: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let http_server = get_value(net_server, "__httpServer__");
    let id = socket_id(socket);
    registry::with_server_conns(|table| {
        table.insert(id, ServerConn { socket, http_server, buf: Vec::new(), stage: Stage::Head });
    });
    let absent = entry::undefined_value();
    let data_fn = entry::with_runtime(|context| entry::make_callable(context, on_data));
    let end_fn = entry::with_runtime(|context| entry::make_callable(context, on_end));
    call_method(socket, "on", key("data"), data_fn, absent);
    call_method(socket, "on", key("end"), end_fn, absent);
    call_method(socket, "resume", absent, absent, absent);
    absent
}

fn socket_id(socket: u64) -> u64 {
    entry::number_of(get_value(socket, "__socketId")).unwrap_or(0.0) as u64
}

extern "C" fn on_data(_e: u64, socket: u64, chunk: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let bytes = entry::with_runtime(|context| entry::bytes_of(context, chunk)).unwrap_or_default();
    let id = socket_id(socket);
    registry::with_server_conns(|table| {
        if let Some(conn) = table.get_mut(&id) {
            conn.buf.extend_from_slice(&bytes);
        }
    });
    drain(id);
    entry::undefined_value()
}

extern "C" fn on_end(_e: u64, socket: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let id = socket_id(socket);
    // A body still `Length`/`Chunked`-incomplete when the peer closes is a
    // truncation this parser reports by simply never completing (module
    // doc's "not implemented" list); dropping the connection here at least
    // stops it leaking in this table forever.
    registry::with_server_conns(|table| {
        table.remove(&id);
    });
    entry::undefined_value()
}

enum Effect {
    Request { message: u64, response: u64, http_server: u64 },
    Body { message: u64, bytes: Vec<u8> },
    End { message: u64 },
}

/// Runs [`advance`] with the connection removed from the table (so the pure
/// parsing/object-building work never holds the lock across a user callback),
/// then performs the collected effects — which DO call into JS — only after
/// the connection is back in the table. See the module doc.
fn drain(id: u64) {
    let Some(mut conn) = registry::with_server_conns(|table| table.remove(&id)) else { return };
    let mut effects = Vec::new();
    advance(&mut conn, &mut effects);
    let done = matches!(conn.stage, Stage::Done);
    registry::with_server_conns(|table| {
        table.insert(id, conn);
    });
    let absent = entry::undefined_value();
    for effect in effects {
        match effect {
            Effect::Request { message, response, http_server } => {
                emit(http_server, "request", message, response, absent);
            }
            Effect::Body { message, bytes } => {
                let push_fn = entry::with_runtime(|context| entry::get_member(context, message, "push"));
                let chunk = entry::with_runtime(|context| entry::make_bytes(context, &bytes));
                entry::call(push_fn, message, chunk, absent, absent, absent);
            }
            Effect::End { message } => {
                let push_fn = entry::with_runtime(|context| entry::get_member(context, message, "push"));
                let null = entry::null_value();
                entry::call(push_fn, message, null, absent, absent, absent);
                entry::with_runtime(|context| set_bool(context, message, "complete", true));
            }
        }
    }
    if done {
        registry::with_server_conns(|table| {
            table.remove(&id);
        });
    }
}

/// The pure-parsing step: consumes as much of `conn.buf` as is currently
/// decidable, mutating `conn.stage` and appending every JS-visible
/// consequence to `effects` rather than acting on it immediately.
fn advance(conn: &mut ServerConn, effects: &mut Vec<Effect>) {
    loop {
        match &mut conn.stage {
            Stage::Head => {
                let Some((head, consumed)) = parser::parse_request_head(&conn.buf) else { return };
                conn.buf.drain(..consumed);
                let socket = conn.socket;
                let (message, response) = entry::with_runtime(|context| {
                    let message = incoming::build_incoming(context, socket, &head.headers, &head.version, Some((head.method.as_str(), head.target.as_str())), None);
                    let response = outgoing::build_server_response(context, socket, message);
                    (message, response)
                });
                let framing = match parser::framing_of(&head.headers) {
                    parser::Framing::None => registry::Framing::None,
                    parser::Framing::Length(n) => registry::Framing::Length(n),
                    parser::Framing::Chunked => registry::Framing::Chunked(parser::ChunkedDecoder::new()),
                };
                effects.push(Effect::Request { message, response, http_server: conn.http_server });
                conn.stage = Stage::Body { message, framing };
            }
            Stage::Body { message, framing } => match framing {
                registry::Framing::None => {
                    effects.push(Effect::End { message: *message });
                    conn.stage = Stage::Done;
                    return;
                }
                registry::Framing::Length(remaining) => {
                    if *remaining == 0 {
                        effects.push(Effect::End { message: *message });
                        conn.stage = Stage::Done;
                        return;
                    }
                    if conn.buf.is_empty() {
                        return;
                    }
                    let take = (*remaining).min(conn.buf.len());
                    let bytes: Vec<u8> = conn.buf.drain(..take).collect();
                    *remaining -= take;
                    let msg = *message;
                    effects.push(Effect::Body { message: msg, bytes });
                    if *remaining == 0 {
                        effects.push(Effect::End { message: msg });
                        conn.stage = Stage::Done;
                    }
                    return;
                }
                registry::Framing::Chunked(decoder) => loop {
                    match decoder.step(&mut conn.buf) {
                        parser::ChunkOutcome::NeedMore => return,
                        parser::ChunkOutcome::Body(bytes) => effects.push(Effect::Body { message: *message, bytes }),
                        parser::ChunkOutcome::Done => {
                            effects.push(Effect::End { message: *message });
                            conn.stage = Stage::Done;
                            return;
                        }
                    }
                },
            },
            Stage::Done => return,
        }
    }
}
