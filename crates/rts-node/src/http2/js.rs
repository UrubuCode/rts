//! The JavaScript surface: `connect`, `createServer`, and the session and
//! stream objects a program actually holds.
//!
//! # What this is and is not
//!
//! Everything protocol-shaped is in [`super::session`] and everything
//! socket-shaped is in [`super::registry`]. This file only turns one into the
//! other: it reads arguments, calls the connection, and delivers queued records
//! as events. Nothing here decides anything about HTTP/2.
//!
//! # `h2c` only, said out loud
//!
//! `connect("http://host:port")` speaks cleartext HTTP/2 with prior knowledge.
//! An `https:` authority is REFUSED by name rather than attempted: HTTP/2 over
//! TLS is chosen by ALPN, `node:tls` here has no ALPN, and a client that opened
//! a TLS socket and started sending frames would be talking to a server that
//! believes it agreed to HTTP/1.1.
//!
//! # Not implemented, by name
//!
//! `PUSH_PROMISE` and every `pushStream`, `Http2ServerRequest`/
//! `Http2ServerResponse` (the compatibility API — `createServer` here raises
//! `'stream'` and nothing else), `session.settings()` after the opening
//! exchange, `session.ping()` with a callback, trailers, `options` of any kind,
//! and `stream.pause()`/`resume()`.

use rts_core::entry::{self, Context, Provided};

use super::registry::{self, Queued};
use super::session::{CANCEL, Side};

/// The methods on `Http2Session.prototype`.
const SESSION_METHODS: &[(&str, Provided)] = &[
    ("request", session_request),
    ("close", session_close),
    ("destroy", session_close),
];

/// The methods on `Http2Stream.prototype`.
const STREAM_METHODS: &[(&str, Provided)] = &[
    ("respond", stream_respond),
    ("write", stream_write),
    ("end", stream_end),
    ("close", stream_close),
];

/// The methods on `Http2Server.prototype`.
const SERVER_METHODS: &[(&str, Provided)] = &[("listen", server_listen), ("close", server_close)];

/// Adds the lifecycle members to the namespace `mod.rs` already built.
pub(super) fn extend(context: &mut Context, namespace: u64) {
    let connect = entry::make_callable(context, connect);
    entry::put_member(context, namespace, "connect", connect);
    let create_server = entry::make_callable(context, create_server);
    entry::put_member(context, namespace, "createServer", create_server);
    // `createSecureServer` is NOT registered. Node's is HTTP/2 over TLS chosen
    // by ALPN, and answering with a cleartext server under that name would be a
    // security claim this cannot keep.
    entry::declare_loop_source(context, "node:http2", source);
}

fn emitter(context: &mut Context, name: &'static str, methods: &[(&str, Provided)]) -> u64 {
    let parent = entry::make_prototype(context, "EventEmitter", &[]);
    let made = entry::make_prototype(context, name, methods);
    entry::set_prototype_in(context, made, parent);
    made
}

fn instance_of(context: &mut Context, prototype: u64) -> u64 {
    let made = entry::make_instance(context, prototype);
    let listeners = entry::make_object(context);
    entry::put_member(context, made, "__events__", listeners);
    made
}

fn number_member(context: &mut Context, object: u64, name: &str, value: u64) {
    let held = entry::make_number(value as f64);
    entry::put_member(context, object, name, held);
}

fn id_member(object: u64, name: &str) -> Option<u64> {
    let held = entry::with_runtime(|context| entry::get_member(context, object, name));
    entry::number_of(held).map(|value| value as u64)
}

/// `http2.connect(authority)`.
extern "C" fn connect(_e: u64, _this: u64, authority: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let text = entry::text_of(authority).unwrap_or_default();
    let (instance, failure) = entry::with_runtime(|context| {
        let prototype = emitter(context, "Http2Session", SESSION_METHODS);
        let instance = instance_of(context, prototype);
        let failure = match address_of(&text) {
            Ok(address) => match std::net::TcpStream::connect(&address) {
                Ok(stream) => {
                    let id = registry::adopt(instance, stream, Side::Client);
                    number_member(context, instance, "__sessionId", id);
                    None
                }
                Err(error) => Some(format!("{address}: {error}")),
            },
            Err(reason) => Some(reason),
        };
        (instance, failure)
    });
    if let Some(reason) = failure {
        // Queued rather than emitted: a listener is attached after this returns,
        // so emitting now would reach nobody. `registry` already exists for
        // exactly this ordering.
        let id = registry::orphan(instance);
        registry::deposit(id, Queued::Closed(Some(reason)));
    }
    instance
}

/// `"http://host:port"` as something `TcpStream::connect` takes.
///
/// `https:` is refused here rather than deeper down, so the message names the
/// reason instead of a socket error.
fn address_of(authority: &str) -> Result<String, String> {
    if authority.starts_with("https://") {
        return Err(
            "node:http2 here speaks h2c (cleartext) only: HTTP/2 over TLS is chosen by ALPN, \
             which node:tls does not have"
                .to_owned(),
        );
    }
    let rest = authority.strip_prefix("http://").unwrap_or(authority);
    let host = rest.split('/').next().unwrap_or(rest);
    match host.contains(':') {
        true => Ok(host.to_owned()),
        false => Ok(format!("{host}:80")),
    }
}

/// `session.request(headers)` — answers an `Http2Stream`.
extern "C" fn session_request(_e: u64, this: u64, headers: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(session) = id_member(this, "__sessionId") else {
        return entry::undefined_value();
    };
    let (fields, stream, prototype) = entry::with_runtime(|context| {
        let fields = fields_of(context, headers);
        let prototype = emitter(context, "Http2Stream", STREAM_METHODS);
        let stream = instance_of(context, prototype);
        (fields, stream, prototype)
    });
    let _ = prototype;
    // A request with no body ends its stream here. `stream.end(body)` is how a
    // caller says otherwise, and it reopens nothing — so this is the common
    // case made right rather than a shortcut.
    let stream_id = registry::with_sessions(|table| {
        let id = table
            .get_mut(&session)
            .and_then(|entry| entry.connection.as_mut())
            .map(|connection| connection.send_headers(&fields, true));
        registry::flush(table, session);
        if let (Some(stream_id), Some(entry)) = (id, table.get_mut(&session)) {
            entry.streams.insert(stream_id, stream);
        }
        id
    });
    entry::with_runtime(|context| {
        number_member(context, stream, "__sessionId", session);
        if let Some(stream_id) = stream_id {
            number_member(context, stream, "__streamId", u64::from(stream_id));
        }
        let held = entry::make_number(stream_id.unwrap_or(0) as f64);
        entry::put_member(context, stream, "id", held);
    });
    stream
}

/// The header fields of a plain object, in the order it holds them.
fn fields_of(context: &mut Context, headers: u64) -> Vec<(String, String)> {
    entry::member_names(context, headers)
        .into_iter()
        .filter_map(|name| {
            let held = entry::get_member(context, headers, &name);
            let value = entry::text_in(context, held)?;
            Some((name, value))
        })
        .collect()
}

extern "C" fn session_close(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    if let Some(session) = id_member(this, "__sessionId") {
        registry::shutdown(session);
    }
    entry::undefined_value()
}

/// `stream.respond(headers)` — a server answering on a stream it was given.
pub(super) extern "C" fn stream_respond(_e: u64, this: u64, headers: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let (Some(session), Some(stream_id)) = (
        id_member(this, "__sessionId"),
        id_member(this, "__streamId"),
    ) else {
        return entry::undefined_value();
    };
    let fields = entry::with_runtime(|context| fields_of(context, headers));
    registry::with_sessions(|table| {
        if let Some(connection) = table
            .get_mut(&session)
            .and_then(|entry| entry.connection.as_mut())
        {
            connection.respond(stream_id as u32, &fields, false);
        }
        registry::flush(table, session);
    });
    entry::undefined_value()
}

pub(super) extern "C" fn stream_write(_e: u64, this: u64, chunk: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    write_body(this, chunk, false);
    entry::undefined_value()
}

pub(super) extern "C" fn stream_end(_e: u64, this: u64, chunk: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    write_body(this, chunk, true);
    entry::undefined_value()
}

fn write_body(this: u64, chunk: u64, end: bool) {
    let (Some(session), Some(stream_id)) = (
        id_member(this, "__sessionId"),
        id_member(this, "__streamId"),
    ) else {
        return;
    };
    let bytes = entry::with_runtime(|context| {
        let absent = entry::undefined_in(context);
        match chunk == absent {
            true => Vec::new(),
            false => match entry::bytes_of(context, chunk) {
                Some(bytes) => bytes,
                None => entry::text_in(context, chunk)
                    .unwrap_or_default()
                    .into_bytes(),
            },
        }
    });
    registry::with_sessions(|table| {
        if let Some(connection) = table
            .get_mut(&session)
            .and_then(|entry| entry.connection.as_mut())
        {
            connection.send_data(stream_id as u32, &bytes, end);
        }
        registry::flush(table, session);
        // A stream this side has finished writing is no longer outstanding
        // work. `source` reports a session with a live stream as work the loop
        // must keep turning for, so a server that answered and kept the stream
        // in its table would hold the program open forever — which is what the
        // end-to-end fixture did until this line existed.
        if end && let Some(entry) = table.get_mut(&session) {
            entry.streams.remove(&(stream_id as u32));
        }
    });
}

pub(super) extern "C" fn stream_close(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let (Some(session), Some(stream_id)) = (
        id_member(this, "__sessionId"),
        id_member(this, "__streamId"),
    ) else {
        return entry::undefined_value();
    };
    registry::with_sessions(|table| {
        if let Some(connection) = table
            .get_mut(&session)
            .and_then(|entry| entry.connection.as_mut())
        {
            connection.send_reset(stream_id as u32, CANCEL);
        }
        registry::flush(table, session);
    });
    entry::undefined_value()
}

/// `http2.createServer()` — raises `'stream'` and nothing else; see the module
/// doc for the compatibility API this does not have.
extern "C" fn create_server(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let prototype = emitter(context, "Http2Server", SERVER_METHODS);
        instance_of(context, prototype)
    })
}

/// `server.listen(port[, callback])`.
///
/// The callback runs SYNCHRONOUSLY once the socket is bound, which diverges from
/// Node (there it is a `'listening'` listener). A callback deferred to the loop
/// would be the Node shape, and a program that then connects in its next
/// statement would find nothing listening — so binding first and calling second
/// is the shape that works here.
extern "C" fn server_listen(_e: u64, this: u64, port: u64, callback: u64, _c: u64, _d: u64) -> u64 {
    let wanted = entry::number_of(port).unwrap_or(0.0) as u16;
    let id = registry::listen(this, &format!("127.0.0.1:{wanted}"));
    if let Some(id) = id {
        let bound = registry::local_port(id).unwrap_or(wanted);
        entry::with_runtime(|context| {
            number_member(context, this, "__serverId", id);
            let held = entry::make_number(f64::from(bound));
            entry::put_member(context, this, "port", held);
        });
    }
    let absent = entry::undefined_value();
    if callback != absent {
        entry::call(callback, this, absent, absent, absent, absent);
    }
    this
}

extern "C" fn server_close(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    if let Some(id) = id_member(this, "__serverId") {
        registry::shutdown(id);
    }
    entry::undefined_value()
}

/// This module as a loop source.
///
/// # Why an open stream answers `In` and a bare session answers `Blocked`
///
/// They are different kinds of waiting. A session with a stream still open has
/// OUTSTANDING WORK — a response is coming and the program asked for it — so it
/// holds the loop open, the same way an unfinished worker does. A listening
/// server with no live stream is waiting on the outside world, which may never
/// arrive, so it does not.
///
/// This was `Blocked` for both at first and the end-to-end fixture caught it
/// immediately: nothing else held the loop, so a client got exactly one pass and
/// its response — which the server had already sent — was never pumped. A
/// request that is answered but never delivered is the failure mode this whole
/// module exists to avoid.
pub(super) fn source() -> entry::Pending {
    super::delivery::pump();
    let mine = std::thread::current().id();
    let (waiting, live) = registry::with_sessions(|table| {
        let mine_only = || table.values().filter(|entry| entry.owner == mine);
        let waiting = mine_only().any(|entry| !entry.closed && !entry.streams.is_empty());
        let live = mine_only().any(|entry| !entry.closed);
        (waiting, live)
    });
    match (waiting, live) {
        (true, _) => entry::Pending::In(std::time::Duration::from_micros(200)),
        (false, true) => entry::Pending::Blocked,
        (false, false) => entry::Pending::Idle,
    }
}

/// The `Http2Session` prototype, for a session `delivery` had to build an
/// object for on the JS thread.
pub(super) fn session_prototype(context: &mut Context) -> u64 {
    emitter(context, "Http2Session", SESSION_METHODS)
}
