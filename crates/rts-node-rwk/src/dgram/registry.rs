//! The native-thread ↔ JS-thread handoff `dgram.Socket` needs — the same
//! recipe `net/registry.rs` worked out (itself following `fs/watch.rs`),
//! narrowed to one table because `dgram` has no server half to share it with.
//!
//! # Why this exists at all
//!
//! A datagram arrives on a background thread — `std::net::UdpSocket::recv_from`
//! blocks, and this crate has no async runtime to await instead — and that
//! thread is not the JS thread: this engine's context is thread-local, and
//! calling a listener from a foreign thread aborts on the first event (see
//! `net/registry.rs`'s own doc for why — `with_runtime` holds a `RefCell`
//! borrow and an `extern "C"` frame cannot unwind past a panic). So the
//! reader thread never calls into JS; it only ever pushes a native record
//! into the table here, and [`pump`] is what turns a queued record into a
//! call, on the JS thread, at the start of the next `node:dgram` call the
//! program makes.

use std::collections::{HashMap, VecDeque};
use std::net::UdpSocket;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use rts_core_rwk::entry;

/// One observation off a socket's reader thread. Native data only.
pub(super) enum DgramEvent {
    Listening,
    BindFailed(String),
    Message { data: Vec<u8>, address: String, port: u16 },
    Error(String),
    Closed,
}

pub(super) struct SocketEntry {
    /// The `Socket` JS instance.
    pub(super) instance: u64,
    pub(super) queue: VecDeque<DgramEvent>,
    /// The live OS socket, once bound — `send`/tuning reach it through here;
    /// the reader thread owns its own `try_clone()`.
    pub(super) socket: Option<UdpSocket>,
    pub(super) closed: bool,
}

static SOCKETS: Mutex<Option<HashMap<u64, SocketEntry>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub(super) fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::SeqCst)
}

pub(super) fn with_sockets<T>(body: impl FnOnce(&mut HashMap<u64, SocketEntry>) -> T) -> T {
    let mut guard = SOCKETS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    body(guard.get_or_insert_with(HashMap::new))
}

fn error_value(message: &str, code: &str) -> u64 {
    // No `new Error(...)` reaches this crate's entry surface — a plain
    // `{message, code}` object carries what a listener reads (`err.message`,
    // `err.code`), the same divergence `net/registry.rs` names.
    entry::with_runtime(|context| {
        let object = entry::make_object(context);
        let message = entry::make_string(context, message);
        let code = entry::make_string(context, code);
        entry::put_member(context, object, "message", message);
        entry::put_member(context, object, "code", code);
        object
    })
}

/// Delivers every queued event, oldest first per socket, then drops the
/// table lock before calling anything — a listener that calls back into
/// `node:dgram` (sending from a `'message'` handler, say) must not deadlock
/// on a lock this function still holds.
pub(super) fn pump() {
    let due: Vec<(u64, Vec<DgramEvent>)> = with_sockets(|table| {
        table
            .iter_mut()
            .filter(|(_, entry)| !entry.closed && !entry.queue.is_empty())
            .map(|(&id, entry)| (id, entry.queue.drain(..).collect()))
            .collect()
    });
    let absent = entry::undefined_value();
    for (id, events) in due {
        let Some(instance) = with_sockets(|table| table.get(&id).map(|e| e.instance)) else { continue };
        for event in events {
            match event {
                DgramEvent::Listening => {
                    super::emit(instance, "listening", absent, absent, absent);
                }
                DgramEvent::BindFailed(message) => {
                    let error = error_value(&message, "EADDRINUSE");
                    super::emit(instance, "error", error, absent, absent);
                }
                DgramEvent::Message { data, address, port } => {
                    let (msg, rinfo) = entry::with_runtime(|context| {
                        let msg = entry::make_bytes(context, &data);
                        let rinfo = entry::make_object(context);
                        let addr_v = entry::make_string(context, &address);
                        let family = if address.contains(':') { "IPv6" } else { "IPv4" };
                        let family_v = entry::make_string(context, family);
                        let port_v = entry::make_number(f64::from(port));
                        let size_v = entry::make_number(data.len() as f64);
                        entry::put_member(context, rinfo, "address", addr_v);
                        entry::put_member(context, rinfo, "family", family_v);
                        entry::put_member(context, rinfo, "port", port_v);
                        entry::put_member(context, rinfo, "size", size_v);
                        (msg, rinfo)
                    });
                    super::emit(instance, "message", msg, rinfo, absent);
                }
                DgramEvent::Error(message) => {
                    let error = error_value(&message, "ERR_SOCKET_DGRAM");
                    super::emit(instance, "error", error, absent, absent);
                }
                DgramEvent::Closed => {
                    super::emit(instance, "close", absent, absent, absent);
                }
            }
        }
    }
}

/// Starts the background reader for a freshly bound socket — a short read
/// timeout so the thread notices `closed` soon after `close()` sets it,
/// rather than blocking in `recv_from` forever with no way to wake it (UDP
/// has no `shutdown()` the way TCP does).
pub(super) fn spawn_reader(id: u64, socket: UdpSocket) {
    std::thread::spawn(move || {
        let _ = socket.set_read_timeout(Some(std::time::Duration::from_millis(200)));
        let mut buffer = [0u8; 65535];
        loop {
            let stop = with_sockets(|table| table.get(&id).map(|e| e.closed).unwrap_or(true));
            if stop {
                return;
            }
            match socket.recv_from(&mut buffer) {
                Ok((count, from)) => with_sockets(|table| {
                    if let Some(entry) = table.get_mut(&id)
                        && !entry.closed
                    {
                        entry.queue.push_back(DgramEvent::Message {
                            data: buffer[..count].to_vec(),
                            address: from.ip().to_string(),
                            port: from.port(),
                        });
                    }
                }),
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        || error.kind() == std::io::ErrorKind::TimedOut =>
                {
                    continue;
                }
                Err(error) => {
                    with_sockets(|table| {
                        if let Some(entry) = table.get_mut(&id)
                            && !entry.closed
                        {
                            entry.queue.push_back(DgramEvent::Error(error.to_string()));
                        }
                    });
                    return;
                }
            }
        }
    });
}
