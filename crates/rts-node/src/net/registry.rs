//! The native-thread ↔ JS-thread handoff every class in this module needs —
//! the same shape `fs/watch.rs` worked out, generalised to two tables (one
//! per class that owns a background thread) sharing one pump.
//!
//! # Why this exists at all
//!
//! A socket's bytes and a server's accepted connections both arrive on a
//! background thread — `std::net`'s blocking calls give no other option, and
//! this crate has no async runtime dependency to await instead (see the
//! module doc's "why blocking, not tokio" note). That thread is not the JS
//! thread, and this engine's context is thread-local: calling a listener
//! from it aborts on the first event, exactly as `fs/watch.rs` documents.
//! So neither a socket's reader thread nor a server's accept thread ever
//! calls into JS — each only ever pushes a native record into the matching
//! table here, and [`pump`] is what turns a queued record into a call.

use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rts_core::entry;

/// One observation off a socket's reader thread. Native data only.
pub(super) enum SocketEvent {
    Connected { local: String, remote: String },
    ConnectFailed(String),
    Data(Vec<u8>),
    End,
    Error(String),
    Closed { had_error: bool },
}

pub(super) struct SocketEntry {
    /// The thread that made it.
    ///
    /// This table is process-wide and every thread running a program pumps it,
    /// so without this one thread delivers another thread's event: it emits
    /// onto a JS instance naming cells in a region it does not have. Found in
    /// `node:worker_threads` first, where two parallel tests were doing it to
    /// each other. Every table of this shape needs it.
    pub(super) owner: std::thread::ThreadId,
    /// The `Socket` JS instance — see the module doc's last section for the
    /// one assumption holding it across calls adds.
    pub(super) instance: u64,
    pub(super) queue: VecDeque<SocketEvent>,
    /// The live stream, once connected — `write`/`end`/`destroy` reach the OS
    /// socket through this; the reader thread never touches it, it owns its
    /// own `try_clone()`.
    pub(super) stream: Option<TcpStream>,
    pub(super) closed: bool,
}

/// One observation off a server's accept thread.
pub(super) enum ServerEvent {
    Listening { local: String },
    ListenFailed(String),
    Accepted(TcpStream, String),
    Error(String),
}

pub(super) struct ServerEntry {
    /// The thread that made it.
    ///
    /// This table is process-wide and every thread running a program pumps it,
    /// so without this one thread delivers another thread's event: it emits
    /// onto a JS instance naming cells in a region it does not have. Found in
    /// `node:worker_threads` first, where two parallel tests were doing it to
    /// each other. Every table of this shape needs it.
    pub(super) owner: std::thread::ThreadId,
    pub(super) instance: u64,
    pub(super) queue: VecDeque<ServerEvent>,
    pub(super) listening: bool,
    pub(super) closed: bool,
    /// Told to the accept thread to stop; the thread notices it between
    /// `accept()` calls, which is why `close()` does not return instantly —
    /// named, not hidden, in `server.rs`'s own doc.
    pub(super) stop: Arc<AtomicBool>,
    pub(super) local_addr: Option<String>,
}

static SOCKETS: Mutex<Option<HashMap<u64, SocketEntry>>> = Mutex::new(None);
static SERVERS: Mutex<Option<HashMap<u64, ServerEntry>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub(super) fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::SeqCst)
}

pub(super) fn with_sockets<T>(body: impl FnOnce(&mut HashMap<u64, SocketEntry>) -> T) -> T {
    let mut guard = SOCKETS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    body(guard.get_or_insert_with(HashMap::new))
}

pub(super) fn with_servers<T>(body: impl FnOnce(&mut HashMap<u64, ServerEntry>) -> T) -> T {
    let mut guard = SERVERS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    body(guard.get_or_insert_with(HashMap::new))
}

fn error_value(message: &str, code: &str) -> u64 {
    // No `new Error(...)` reaches this crate's entry surface (see the module
    // doc's "what this refuses" section) — a plain `{message, code}` object
    // carries what a listener actually reads (`err.message`, `err.code`),
    // named as the divergence rather than left silent.
    entry::with_runtime(|context| {
        let object = entry::make_object(context);
        let message = entry::make_string(context, message);
        let code = entry::make_string(context, code);
        entry::put_member(context, object, "message", message);
        entry::put_member(context, object, "code", code);
        object
    })
}

/// Delivers every queued socket AND server event, oldest first per entry,
/// then drops both table locks before calling anything — see
/// [`fs::watch::pump`](crate::fs::watch) for why a listener that calls back
/// into this module (writes on `'connection'`, say) must not deadlock on a
/// lock this function still holds.
pub(super) fn pump() {
    pump_sockets();
    pump_servers();
}

fn pump_sockets() {
    let due: Vec<(u64, Vec<SocketEvent>)> = with_sockets(|table| {
        table
            .iter_mut()
            .filter(|(_, entry)| entry.owner == std::thread::current().id() && !entry.closed && !entry.queue.is_empty())
            .map(|(&id, entry)| (id, entry.queue.drain(..).collect()))
            .collect()
    });
    let absent = entry::undefined_value();
    for (id, events) in due {
        let Some(instance) = with_sockets(|table| table.get(&id).map(|e| e.instance)) else { continue };
        for event in events {
            match event {
                SocketEvent::Connected { local, remote } => {
                    entry::with_runtime(|context| {
                        let local_v = entry::make_string(context, &local);
                        let remote_v = entry::make_string(context, &remote);
                        entry::put_member(context, instance, "localAddress", local_v);
                        entry::put_member(context, instance, "remoteAddress", remote_v);
                        let connecting = entry::boolean_value(false);
                        entry::put_member(context, instance, "connecting", connecting);
                    });
                    super::common::emit(instance, "connect", absent, absent, absent);
                    super::common::emit(instance, "ready", absent, absent, absent);
                }
                SocketEvent::ConnectFailed(message) => {
                    let error = error_value(&message, "ECONNREFUSED");
                    super::common::emit(instance, "error", error, absent, absent);
                    super::common::emit(instance, "close", entry::boolean_value(true), absent, absent);
                }
                SocketEvent::Data(bytes) => {
                    let push_fn = entry::with_runtime(|context| entry::get_member(context, instance, "push"));
                    if push_fn == absent {
                        continue;
                    }
                    let chunk = entry::with_runtime(|context| entry::make_bytes(context, &bytes));
                    entry::call(push_fn, instance, chunk, absent, absent, absent);
                }
                SocketEvent::End => {
                    let push_fn = entry::with_runtime(|context| entry::get_member(context, instance, "push"));
                    if push_fn != absent {
                        let null = entry::null_value();
                        entry::call(push_fn, instance, null, absent, absent, absent);
                    }
                }
                SocketEvent::Error(message) => {
                    let error = error_value(&message, "ECONNRESET");
                    super::common::emit(instance, "error", error, absent, absent);
                }
                SocketEvent::Closed { had_error } => {
                    super::common::emit(instance, "close", entry::boolean_value(had_error), absent, absent);
                }
            }
        }
    }
}

fn pump_servers() {
    let due: Vec<(u64, Vec<ServerEvent>)> = with_servers(|table| {
        table
            .iter_mut()
            .filter(|(_, entry)| entry.owner == std::thread::current().id() && !entry.closed && !entry.queue.is_empty())
            .map(|(&id, entry)| (id, entry.queue.drain(..).collect()))
            .collect()
    });
    let absent = entry::undefined_value();
    for (id, events) in due {
        let Some(instance) = with_servers(|table| table.get(&id).map(|e| e.instance)) else { continue };
        for event in events {
            match event {
                ServerEvent::Listening { local } => {
                    with_servers(|table| {
                        if let Some(entry) = table.get_mut(&id) {
                            entry.listening = true;
                            entry.local_addr = Some(local);
                        }
                    });
                    // The JS-visible `server.listening` data property — this
                    // used to only flip the Rust-side `ServerEntry::listening`
                    // flag, which nothing reads back through the JS surface,
                    // so a program checking `server.listening` after the
                    // `'listening'` event still saw the constructor's `false`.
                    entry::with_runtime(|context| super::common::set_bool(context, instance, "listening", true));
                    super::common::emit(instance, "listening", absent, absent, absent);
                }
                ServerEvent::ListenFailed(message) => {
                    let error = error_value(&message, "EADDRINUSE");
                    super::common::emit(instance, "error", error, absent, absent);
                }
                ServerEvent::Accepted(stream, remote) => {
                    let socket = super::socket::adopt(stream, remote);
                    super::common::emit(instance, "connection", socket, absent, absent);
                }
                ServerEvent::Error(message) => {
                    let error = error_value(&message, "ECONNABORTED");
                    super::common::emit(instance, "error", error, absent, absent);
                }
            }
        }
    }
}

/// Writes `bytes` straight to the OS socket, synchronously — the shape
/// `socket.rs`'s `_write` hook needs, factored out so a background thread is
/// never involved in a write (there is nothing to queue: `TcpStream::write`
/// blocks until the kernel accepts the bytes, same as Node's own libuv path
/// accepting into its kernel buffer).
pub(super) fn write_now(id: u64, bytes: &[u8]) -> std::io::Result<()> {
    with_sockets(|table| {
        let Some(entry) = table.get_mut(&id) else {
            return Err(std::io::Error::other("socket closed"));
        };
        let Some(stream) = entry.stream.as_mut() else {
            return Err(std::io::Error::other("socket not connected"));
        };
        stream.write_all(bytes)
    })
}

/// This module as a loop source: deliver what its background threads queued,
/// then say whether any is still live.
///
/// `Blocked` and never `In`, because what this waits on is the outside world
/// and that has no deadline to report. A `Blocked` source is pumped on every
/// pass and does NOT hold the program open — `entry::loops` says why. It means
/// a program whose last act is to start one of these still ends, where Node
/// would keep running; the alternative is every fixture hanging on a listener
/// nothing closes.
pub fn source() -> entry::Pending {
    pump();
    let mine = std::thread::current().id();
    let sockets = with_sockets(|table| {
        table.values().any(|entry| entry.owner == mine && !entry.closed)
    });
    let servers = with_servers(|table| {
        table.values().any(|entry| entry.owner == mine && !entry.closed)
    });
    match sockets || servers {
        true => entry::Pending::Blocked,
        false => entry::Pending::Idle,
    }
}
