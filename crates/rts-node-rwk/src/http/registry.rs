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
//! native-only state, generalised to one table instead of two because this
//! module has exactly one kind of connection (server-accepted; a
//! client-initiated socket reuses the same struct, one request at a time).
//!
//! # No keep-alive, no pipelining — named here because this table is what
//! enforces it
//!
//! [`ServerConn::stage`] has no "await the next request line" state: once a
//! request's body completes, the connection is dropped from this table and
//! its socket is `end()`-ed. A client that sends a second request on the
//! same socket gets silence, not a second `'request'` event — see the
//! module-level "Not implemented" section for why (no expectation of
//! `Connection: keep-alive` is read either direction).

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

// A client-initiated request (`client.rs`) reads its response by blocking
// synchronously (see that file's own doc for why) rather than through this
// table — there is no per-connection state to hold between calls, so there
// is no client-side counterpart here.

static SERVER_CONNS: Mutex<Option<HashMap<u64, ServerConn>>> = Mutex::new(None);

pub(super) fn with_server_conns<T>(body: impl FnOnce(&mut HashMap<u64, ServerConn>) -> T) -> T {
    let mut guard = SERVER_CONNS.lock().unwrap_or_else(|p| p.into_inner());
    body(guard.get_or_insert_with(HashMap::new))
}
