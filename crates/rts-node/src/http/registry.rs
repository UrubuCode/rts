//! Per-connection HTTP parsing state, keyed by the numeric `__socketId` the
//! underlying `net.Socket` already carries (a public property that module
//! sets on every instance it builds — reading it costs nothing this module
//! would otherwise have to invent a second identity scheme for).
//!
//! # Why a table here rather than a field on the JS objects
//!
//! [`parser::ChunkedDecoder`](super::parser) and the raw byte buffer are pure
//! Rust, not JS values — there is nowhere on a JS object to hang them. This
//! is the same shape `net::registry` and `fs::watch` use for their own
//! native-only state, and there are TWO tables because this module reads in two
//! directions: a connection a server accepted, and a response a client is
//! receiving. The second one is new — `client.rs` used to read its response by
//! blocking, which had no state to keep between calls because there were no
//! calls; see `super::response_reader` for why that could not work at all.
//!
//! # No keep-alive, no pipelining — named here because this table is what
//! enforces it
//!
//! [`Stage`] has no "await the next request line" state, and nor does
//! [`ClientStage`]: once a
//! request's body completes, the connection is dropped from this table and
//! its socket is `end()`-ed. A client that sends a second request on the
//! same socket gets silence, not a second `'request'` event — see the
//! module-level "Not implemented" section for why (no expectation of
//! `Connection: keep-alive` is read either direction) — and the client sends
//! `Connection: close` on every request for the same reason.

use std::collections::HashMap;
use std::sync::Mutex;

use super::parser::ChunkedDecoder;

pub(super) enum Framing {
    None,
    Length(usize),
    Chunked(ChunkedDecoder),
}

/// What a connection accepted by an `http.Server` is doing right now.
pub(super) enum Stage {
    /// Accumulating bytes toward a complete request head.
    Head,
    /// Head parsed, `IncomingMessage`/`ServerResponse` built and `'request'`
    /// emitted; now streaming the body in as it decodes. `Framing::Length`
    /// holds the byte count still owed.
    Body { message: u64, framing: Framing },
    /// The request (headers-only, or body fully delivered) is done; further
    /// bytes on this socket are not read into a second request (module doc).
    Done,
}

pub(super) struct ServerConn {
    pub(super) socket: u64,
    pub(super) http_server: u64,
    pub(super) buf: Vec<u8>,
    pub(super) stage: Stage,
}

/// What a response a `ClientRequest` is reading is doing right now.
///
/// The client half of [`Stage`], and it exists because the client stopped
/// blocking: this comment used to say there was no client-side counterpart here
/// because `client.rs` read its response in a loop on the JavaScript thread —
/// which over loopback could not complete at all (`response_reader`'s module doc
/// has the measurement). The two stages differ in one place, `Framing::None`,
/// and `response_reader::advance` says why they must.
pub(super) enum ClientStage {
    /// Accumulating bytes toward a complete response head.
    Head,
    /// Head parsed, `IncomingMessage` built and `'response'` emitted.
    Body { message: u64, framing: Framing },
    /// The whole response has been delivered.
    Done,
}

pub(super) struct ClientConn {
    pub(super) request: u64,
    pub(super) socket: u64,
    pub(super) buf: Vec<u8>,
    pub(super) stage: ClientStage,
}

static SERVER_CONNS: Mutex<Option<HashMap<u64, ServerConn>>> = Mutex::new(None);
static CLIENT_CONNS: Mutex<Option<HashMap<u64, ClientConn>>> = Mutex::new(None);

pub(super) fn with_server_conns<T>(body: impl FnOnce(&mut HashMap<u64, ServerConn>) -> T) -> T {
    let mut guard = SERVER_CONNS.lock().unwrap_or_else(|p| p.into_inner());
    body(guard.get_or_insert_with(HashMap::new))
}

/// Two tables and not one map with a tagged stage: a socket this process
/// ACCEPTED and a socket it DIALLED are numbered from the same
/// `net::registry::next_id` space, so one map would be correct — but a server
/// connection and a client response share no state and no step, and the one
/// thing a single map would buy (a collision that cannot happen) is not worth a
/// `match` in every accessor.
pub(super) fn with_client_conns<T>(body: impl FnOnce(&mut HashMap<u64, ClientConn>) -> T) -> T {
    let mut guard = CLIENT_CONNS.lock().unwrap_or_else(|p| p.into_inner());
    body(guard.get_or_insert_with(HashMap::new))
}
