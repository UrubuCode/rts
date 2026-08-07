//! Sessions over real sockets, and the handoff between their reader threads and
//! the thread running JavaScript.
//!
//! # Reuse-check
//!
//! The recipe is `net/registry.rs`'s and nothing here re-derives it: a reader
//! thread that never calls into JS, a table of native records, and delivery on
//! the program's own thread. What is different is that the protocol state lives
//! beside the socket rather than in the JS object — a `Connection`
//! ([`super::session`]) which knows nothing about the engine — so the reader
//! thread can advance the whole state machine without a heap.
//!
//! Registered as a loop source (`entry::declare_loop_source`), which is what
//! makes a response arrive without the program calling back into `node:http2`.
//! Before that existed this module could not have worked at all.
//!
//! # Why the connection is behind the table's lock and not its own
//!
//! Two threads touch it — the reader, and whichever one a program calls
//! `request`/`respond` on — and a second lock would be a second order to
//! acquire them in. One lock, held only across protocol work and never across a
//! call into JS, is the arrangement that cannot deadlock.

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use super::session::{Connection, Event, Side};

/// One thing a session's reader thread observed, in native data only.
pub(super) enum Queued {
    /// A peer opened a stream and sent headers — a request at a server, a
    /// response at a client.
    Headers {
        stream_id: u32,
        fields: Vec<(String, String)>,
        end_stream: bool,
    },
    Data {
        stream_id: u32,
        bytes: Vec<u8>,
        end_stream: bool,
    },
    Reset {
        stream_id: u32,
        error_code: u32,
    },
    Goaway {
        error_code: u32,
    },
    /// The socket ended, or the protocol did.
    Closed(Option<String>),
    /// A connection this server accepted, already running.
    Accepted(u64),
}

pub(super) struct SessionEntry {
    /// The thread that made it; see `net/registry.rs` for why every table of
    /// this shape has one.
    pub(super) owner: std::thread::ThreadId,
    /// The `Http2Session` JS instance.
    pub(super) instance: u64,
    /// The protocol state. `None` once the session is gone.
    pub(super) connection: Option<Connection>,
    /// The write half. The reader thread owns its own `try_clone`.
    pub(super) socket: Option<TcpStream>,
    pub(super) queue: VecDeque<Queued>,
    /// The JS stream object for each live stream id.
    pub(super) streams: HashMap<u32, u64>,
    pub(super) closed: bool,
    /// The port a listening entry bound to. A session has none, which is also
    /// how the two are told apart — a separate `is_server` flag was a second
    /// answer to one question.
    pub(super) port: Option<u16>,
}

static SESSIONS: Mutex<Option<HashMap<u64, SessionEntry>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub(super) fn with_sessions<T>(body: impl FnOnce(&mut HashMap<u64, SessionEntry>) -> T) -> T {
    let mut guard = SESSIONS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    body(guard.get_or_insert_with(HashMap::new))
}

fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::SeqCst)
}

/// Writes whatever the connection has queued, under the table's lock.
///
/// Called after every protocol operation. A caller that forgets leaves frames
/// in a buffer and a peer waiting for them, which looks like a hang rather than
/// like a missing call — so nothing outside this module ever has to remember.
pub(super) fn flush(table: &mut HashMap<u64, SessionEntry>, id: u64) {
    let Some(entry) = table.get_mut(&id) else {
        return;
    };
    let Some(connection) = entry.connection.as_mut() else {
        return;
    };
    let bytes = connection.take_outbound();
    if bytes.is_empty() {
        return;
    }
    if let Some(socket) = entry.socket.as_mut() {
        let _ = socket.write_all(&bytes);
        let _ = socket.flush();
    }
}

/// Starts a session over `stream`, answering its id.
pub(super) fn adopt(instance: u64, stream: TcpStream, side: Side) -> u64 {
    let id = next_id();
    let reader = stream.try_clone().ok();
    with_sessions(|table| {
        table.insert(
            id,
            SessionEntry {
                owner: std::thread::current().id(),
                instance,
                connection: Some(Connection::new(side)),
                socket: Some(stream),
                queue: VecDeque::new(),
                streams: HashMap::new(),
                closed: false,
                port: None,
            },
        );
        // The preface and opening SETTINGS are already in the connection's
        // buffer — `Connection::new` puts them there so a caller cannot forget
        // them — and this is what actually sends them.
        flush(table, id);
    });
    if let Some(mut reader) = reader {
        std::thread::Builder::new()
            .name(format!("rts-http2-{id}"))
            .spawn(move || {
                let mut buffer = [0u8; 16384];
                loop {
                    let read = match reader.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(count) => count,
                        Err(error) => {
                            deposit(id, Queued::Closed(Some(error.to_string())));
                            return;
                        }
                    };
                    let events = with_sessions(|table| {
                        let Some(entry) = table.get_mut(&id) else {
                            return Vec::new();
                        };
                        let Some(connection) = entry.connection.as_mut() else {
                            return Vec::new();
                        };
                        let events = connection.receive(&buffer[..read]);
                        // Answering frames — a SETTINGS acknowledgement, a
                        // WINDOW_UPDATE, a PING echo — are written from HERE
                        // rather than waiting for the program to do something.
                        // A peer that never hears its SETTINGS acknowledged
                        // closes the connection, and a program has no reason to
                        // call anything in the meantime.
                        flush(table, id);
                        events
                    });
                    let mut stop = false;
                    for event in events {
                        if let Some(queued) = translate(event, &mut stop) {
                            deposit(id, queued);
                        }
                    }
                    if stop {
                        break;
                    }
                }
                deposit(id, Queued::Closed(None));
            })
            .ok();
    }
    id
}

/// One protocol event as the native record the JS thread will deliver.
///
/// `Settings` and `PingAck` are dropped deliberately: neither has a Node event
/// this module raises, and queuing a record nothing consumes is how a table
/// grows without bound on a busy connection.
fn translate(event: Event, stop: &mut bool) -> Option<Queued> {
    match event {
        Event::Headers { stream_id, fields, end_stream } => {
            Some(Queued::Headers { stream_id, fields, end_stream })
        }
        Event::Data { stream_id, bytes, end_stream } => {
            Some(Queued::Data { stream_id, bytes, end_stream })
        }
        Event::Reset { stream_id, error_code } => Some(Queued::Reset { stream_id, error_code }),
        Event::Goaway { error_code, .. } => {
            *stop = true;
            Some(Queued::Goaway { error_code })
        }
        Event::Failed(reason) => {
            *stop = true;
            Some(Queued::Closed(Some(reason.to_owned())))
        }
        Event::Settings(_) | Event::PingAck(_) => None,
    }
}

pub(super) fn deposit(id: u64, queued: Queued) {
    with_sessions(|table| {
        if let Some(entry) = table.get_mut(&id) {
            entry.queue.push_back(queued);
        }
    });
}

/// Starts a server on `address`, answering its id.
///
/// The listening entry is a session entry with no connection: it holds the JS
/// `Http2Server` and the queue its accept thread posts into, which keeps one
/// table and one pump rather than a second of each for a second lifetime.
pub(super) fn listen(instance: u64, address: &str) -> Option<u64> {
    let listener = TcpListener::bind(address).ok()?;
    // Read BEFORE the listener moves into its thread, and kept: `listen(0)` is
    // how a test asks for any free port, and the number it got is the only way
    // anything can then connect to it.
    let port = listener.local_addr().ok().map(|address| address.port());
    let id = next_id();
    with_sessions(|table| {
        table.insert(
            id,
            SessionEntry {
                owner: std::thread::current().id(),
                instance,
                connection: None,
                socket: None,
                queue: VecDeque::new(),
                streams: HashMap::new(),
                closed: false,
                port,
            },
        );
    });
    let owner = std::thread::current().id();
    std::thread::Builder::new()
        .name(format!("rts-http2-listen-{id}"))
        .spawn(move || {
            for accepted in listener.incoming() {
                let Ok(stream) = accepted else { break };
                // The accepted session is adopted HERE, on the accept thread,
                // and its owner is fixed to the thread that called `listen` —
                // otherwise the pump on the program's thread would skip it as
                // another thread's, which is the same fault every table of this
                // shape has.
                let session = adopt_with_owner(0, stream, Side::Server, owner);
                deposit(id, Queued::Accepted(session));
            }
        })
        .ok()?;
    Some(id)
}

/// [`adopt`], with the owning thread stated rather than taken from the caller.
fn adopt_with_owner(
    instance: u64,
    stream: TcpStream,
    side: Side,
    owner: std::thread::ThreadId,
) -> u64 {
    let id = adopt(instance, stream, side);
    with_sessions(|table| {
        if let Some(entry) = table.get_mut(&id) {
            entry.owner = owner;
        }
    });
    id
}

/// The port a listening entry ended up on.
pub(super) fn local_port(id: u64) -> Option<u16> {
    with_sessions(|table| table.get(&id).and_then(|entry| entry.port))
}

/// Closes a session and stops its reader.
pub(super) fn shutdown(id: u64) {
    with_sessions(|table| {
        if let Some(entry) = table.get_mut(&id) {
            if let Some(connection) = entry.connection.as_mut() {
                connection.send_goaway(super::session::NO_ERROR);
            }
            entry.closed = true;
        }
        flush(table, id);
        if let Some(entry) = table.get_mut(&id)
            && let Some(socket) = entry.socket.as_ref()
        {
            let _ = socket.shutdown(std::net::Shutdown::Both);
        }
    });
}

/// An entry with no socket, for a session that never connected.
///
/// A failure has to reach a listener the caller attaches AFTER the constructor
/// returns, so it is queued like every other record rather than emitted at the
/// point of failure — where it would reach nobody.
pub(super) fn orphan(instance: u64) -> u64 {
    let id = next_id();
    with_sessions(|table| {
        table.insert(
            id,
            SessionEntry {
                owner: std::thread::current().id(),
                instance,
                connection: None,
                socket: None,
                queue: VecDeque::new(),
                streams: HashMap::new(),
                closed: false,
                port: None,
            },
        );
    });
    id
}
