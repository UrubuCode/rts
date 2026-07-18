//! node:net — the live state of a `Server` and a `Socket`, their event queues
//! and their OS threads.
//!
//! Same model `node:dgram` proved: the JS-visible value is an object-backed
//! Registry class (an `Entry::Map` tagged `__rts_class`), and THAT HANDLE keys
//! the side tables here, where everything that cannot cross the ABI lives (the
//! OS socket, the accept/read threads, the pending events). The listener table
//! is the crate-shared [`crate::emitter`], keyed by the same handle.
//!
//! Threading: an accept thread per listening server and a read thread per
//! connected socket. Neither touches the JS heap — they queue plain data, and
//! `pump.rs` turns it into JS values ON THE JS THREAD. (Dispatch stays there
//! until the threading model's blockers #2/#5 land — T1 shared gcells is in,
//! the pending-error slot is still thread-local. See docs/specs/
//! rts-threading-model.md.)

use std::collections::VecDeque;
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use crate::net::blocklist::rules::Rule;

/// A queued `Server` event, delivered to JS by the pump.
pub enum ServerEvent {
    /// `'listening'`.
    Listening,
    /// `'connection'` — an accepted connection, not yet a JS `Socket`: the pump
    /// builds one on the JS thread.
    Connection(TcpStream),
    /// `'drop'` — `maxConnections` was reached; carries the peer that was
    /// refused (Node's `DropArgument`).
    Drop { local: SocketAddr, remote: SocketAddr },
    /// `'close'`.
    Close,
    /// `'error'` — `(code, message)`.
    Error(String, String),
    /// A one-shot callback (`listen`'s / `close`'s).
    Callback { cb: u64, err: Option<(String, String)> },
    /// `getConnections(cb)` — Node documents it as asynchronous, so the count is
    /// read at call time and delivered on a later turn.
    Connections { cb: u64, count: i64 },
    /// A user `server.emit(event, ...args)` — same queue, so ordering holds.
    Custom(String, Vec<u64>),
}

/// A queued `Socket` event.
pub enum SockEvent {
    /// `'connect'`, immediately followed by `'ready'`.
    Connect,
    /// `'data'` — an inbound chunk.
    Data(Vec<u8>),
    /// `'end'` — the peer sent FIN.
    End,
    /// `'drain'` — the write queue emptied.
    Drain,
    /// `'timeout'` — the idle timer elapsed (does NOT destroy the socket).
    Timeout,
    /// `'close'` — `hadError`.
    Close(bool),
    /// `'error'` — `(code, message)`; always followed by `'close'`.
    Error(String, String),
    /// `'lookup'` — after DNS resolution, before connecting.
    Lookup { err: Option<String>, address: String, family: i64, host: String },
    /// `'connectionAttempt'` / `'connectionAttemptFailed'` — one per candidate
    /// under `autoSelectFamily`.
    Attempt { ip: String, port: u16, family: i64, err: Option<String> },
    /// A one-shot callback (`connect`'s / `write`'s / `end`'s).
    Callback { cb: u64, err: Option<(String, String)> },
    /// A user `socket.emit(event, ...args)`.
    Custom(String, Vec<u64>),
}

/// Everything about a live `net.Server` that cannot cross the ABI.
pub struct ServerState {
    /// The listening socket, until `close()` drops it (which also wakes the
    /// accept thread out of its blocking `accept`).
    pub listener: Mutex<Option<std::net::TcpListener>>,
    pub bound: Mutex<Option<SocketAddr>>,
    pub listening: AtomicBool,
    pub closed: AtomicBool,
    pub refd: AtomicBool,
    pub counted: AtomicBool,
    /// Live connections — `getConnections`, and the `maxConnections` gate.
    pub connections: AtomicI64,
    /// The JS object this state backs. `maxConnections`/`dropMaxConnection` are
    /// PLAIN PROPERTIES in Node (`server.maxConnections = 1`), not accessors — so
    /// they live on the object itself and the accept thread reads them from
    /// there, which is both simpler and what Node actually exposes.
    ///
    /// Only `maxConnections` is read back natively: `dropMaxConnection` decides,
    /// in Node, whether a connection over the limit is closed outright or handed
    /// to another CLUSTER worker. A single-process runtime has no worker to hand
    /// it to, so it always closes — reading the flag could not change the
    /// outcome. It stays a plain readable/writable property, which is exactly
    /// its Node shape.
    pub handle: u64,
    /// `new Server({ blockList })` — an inbound peer that matches is refused
    /// before `'connection'` fires.
    pub block_list: Mutex<Option<Vec<Rule>>>,
    /// Inherited by every accepted socket (Node's `ServerOptions`).
    pub opts: SocketOpts,
    pub events: Mutex<VecDeque<ServerEvent>>,
    pub accept_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

/// The socket options a `Server` passes to the sockets it accepts, and that
/// `new Socket(options)` takes.
#[derive(Clone, Copy, Default)]
pub struct SocketOpts {
    pub allow_half_open: bool,
    pub keep_alive: bool,
    pub keep_alive_initial_delay: u64,
    pub no_delay: bool,
    pub pause_on_connect: bool,
}

impl ServerState {
    /// `server.maxConnections`, read off the JS object where the user set it.
    /// Node ≥ v21: `0` DROPS every connection (it is NOT Infinity); unset means
    /// no limit.
    pub fn max_connections(&self) -> Option<i64> {
        let w = crate::values::opt_word(self.handle, "maxConnections")?;
        match crate::values::val(w) {
            crate::values::Val::Num(n) if n >= 0.0 => Some(n as i64),
            _ => None,
        }
    }


    pub fn new(opts: SocketOpts) -> Self {
        Self {
            listener: Mutex::new(None),
            bound: Mutex::new(None),
            listening: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            refd: AtomicBool::new(true),
            counted: AtomicBool::new(false),
            connections: AtomicI64::new(0),
            handle: 0,
            block_list: Mutex::new(None),
            opts,
            events: Mutex::new(VecDeque::new()),
            accept_thread: Mutex::new(None),
        }
    }

    pub fn push(&self, ev: ServerEvent) {
        self.events.lock().unwrap().push_back(ev);
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
}

/// Everything about a live `net.Socket` that cannot cross the ABI.
pub struct SocketState {
    /// The connected stream. `None` while connecting / after destroy.
    pub stream: Mutex<Option<TcpStream>>,
    pub local: Mutex<Option<SocketAddr>>,
    pub peer: Mutex<Option<SocketAddr>>,
    /// A `connect()` is in flight (Node's `socket.connecting`).
    pub connecting: AtomicBool,
    /// `destroy()`ed (Node's `socket.destroyed`).
    pub destroyed: AtomicBool,
    /// The readable side saw FIN, or the writable side was `end()`ed — the two
    /// halves behind `readyState`.
    pub readable: AtomicBool,
    pub writable: AtomicBool,
    /// `pause()`/`resume()` — the read thread parks while paused.
    pub paused: AtomicBool,
    pub refd: AtomicBool,
    pub counted: AtomicBool,
    /// `setEncoding(enc)` — `'data'` then delivers strings, not Buffers.
    pub encoding: Mutex<Option<String>>,
    /// `setTimeout(ms)` — 0 disables. The read thread arms it.
    pub timeout_ms: AtomicI64,
    pub bytes_read: AtomicI64,
    pub bytes_written: AtomicI64,
    /// Bytes handed to `write()` that the OS has not taken yet — Node's
    /// (deprecated) `bufferSize` / `writableLength`.
    pub buffered: AtomicI64,
    /// `new Socket({ allowHalfOpen })` and friends.
    pub opts: Mutex<SocketOpts>,
    /// `{ blockList }` — a blocked destination is refused before dialing.
    pub block_list: Mutex<Option<Vec<Rule>>>,
    /// The addresses `autoSelectFamily` actually tried.
    pub attempted: Mutex<Vec<String>>,
    pub events: Mutex<VecDeque<SockEvent>>,
    pub read_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl SocketState {
    pub fn new(opts: SocketOpts) -> Self {
        Self {
            stream: Mutex::new(None),
            local: Mutex::new(None),
            peer: Mutex::new(None),
            connecting: AtomicBool::new(false),
            destroyed: AtomicBool::new(false),
            readable: AtomicBool::new(false),
            writable: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            refd: AtomicBool::new(true),
            counted: AtomicBool::new(false),
            encoding: Mutex::new(None),
            timeout_ms: AtomicI64::new(0),
            bytes_read: AtomicI64::new(0),
            bytes_written: AtomicI64::new(0),
            buffered: AtomicI64::new(0),
            opts: Mutex::new(opts),
            block_list: Mutex::new(None),
            attempted: Mutex::new(Vec::new()),
            events: Mutex::new(VecDeque::new()),
            read_thread: Mutex::new(None),
        }
    }

    pub fn push(&self, ev: SockEvent) {
        self.events.lock().unwrap().push_back(ev);
    }

    /// Queue an error for `cb` if there is one, else as `'error'` — Node's rule.
    /// A `Socket`'s `'error'` is ALWAYS followed by `'close'` (unlike `Server`).
    pub fn push_err(&self, cb: u64, code: &str, message: &str) {
        if cb != 0 {
            self.push(SockEvent::Callback {
                cb,
                err: Some((code.to_string(), message.to_string())),
            });
        } else {
            self.push(SockEvent::Error(code.to_string(), message.to_string()));
        }
    }

    pub fn is_destroyed(&self) -> bool {
        self.destroyed.load(Ordering::Acquire)
    }

    pub fn peer_addr(&self) -> Option<SocketAddr> {
        *self.peer.lock().unwrap()
    }

    /// Node's derived `readyState`.
    pub fn ready_state(&self) -> &'static str {
        if self.connecting.load(Ordering::Acquire) {
            return "opening";
        }
        match (
            self.readable.load(Ordering::Acquire),
            self.writable.load(Ordering::Acquire),
        ) {
            (true, true) => "open",
            (true, false) => "readOnly",
            (false, true) => "writeOnly",
            (false, false) => "closed",
        }
    }

    /// A clone of the stream for a background thread (`try_clone` dups the fd,
    /// so the reader and the JS-thread writer share one connection).
    pub fn clone_stream(&self) -> Option<TcpStream> {
        self.stream.lock().unwrap().as_ref().and_then(|s| s.try_clone().ok())
    }
}

type Servers = indexmap::IndexMap<u64, Arc<ServerState>>;
type Sockets = indexmap::IndexMap<u64, Arc<SocketState>>;

fn servers_table() -> MutexGuard<'static, Servers> {
    static T: OnceLock<Mutex<Servers>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(Servers::new())).lock().unwrap()
}

fn sockets_table() -> MutexGuard<'static, Sockets> {
    static T: OnceLock<Mutex<Sockets>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(Sockets::new())).lock().unwrap()
}

pub fn insert_server(handle: u64, st: Arc<ServerState>) {
    servers_table().insert(handle, st);
}

pub fn server(handle: u64) -> Option<Arc<ServerState>> {
    servers_table().get(&handle).cloned()
}

pub fn remove_server(handle: u64) -> Option<Arc<ServerState>> {
    servers_table().shift_remove(&handle)
}

pub fn insert_socket(handle: u64, st: Arc<SocketState>) {
    sockets_table().insert(handle, st);
}

pub fn socket(handle: u64) -> Option<Arc<SocketState>> {
    sockets_table().get(&handle).cloned()
}

pub fn remove_socket(handle: u64) -> Option<Arc<SocketState>> {
    sockets_table().shift_remove(&handle)
}

/// Snapshots for the pump — taken without holding a table lock while listeners
/// run (a listener may close its server / destroy its socket).
pub fn server_snapshot() -> Vec<(u64, Arc<ServerState>)> {
    servers_table().iter().map(|(h, s)| (*h, s.clone())).collect()
}

pub fn socket_snapshot() -> Vec<(u64, Arc<SocketState>)> {
    sockets_table().iter().map(|(h, s)| (*h, s.clone())).collect()
}

/// Whether `addr` is covered by an optional block list (absent = allow).
pub fn blocked(list: &Mutex<Option<Vec<Rule>>>, addr: std::net::IpAddr) -> bool {
    match &*list.lock().unwrap() {
        Some(rules) => rules.iter().any(|r| r.matches(addr)),
        None => false,
    }
}
