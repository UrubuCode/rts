//! node:dgram — the live state of a `Socket`, its event queue and its listener
//! table.
//!
//! A `dgram.Socket` is an object-backed Registry class: the JS-visible value is
//! an `Entry::Map` tagged `__rts_class = "Socket"`, and THAT HANDLE is the key
//! into this module's side table, where the OS socket and everything that cannot
//! cross the ABI (the `socket2::Socket`, the reader thread, the listener
//! closures, the pending-event queue) lives. The handle is GC-pinned while the
//! socket is open — Node keeps an open socket (and its object) alive.
//!
//! Threading: the OS socket is read by ONE dedicated reader thread per bound
//! socket (blocking `recv_from` with a short timeout so `close()` is observed).
//! That thread NEVER touches the JS heap: it pushes plain bytes + the sender
//! address into [`SocketState::events`], and the event loop's pump (`pump.rs`)
//! drains the queue on the JS thread. See docs/node-implementation/dgram.md §5.3.

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use socket2::Socket;

use crate::net::blocklist::rules::Rule;

/// One received datagram: the payload bytes and the sender's address.
pub struct Datagram {
    pub bytes: Vec<u8>,
    pub from: SocketAddr,
}

/// A queued event, delivered to JS by the pump on the event-loop thread.
///
/// Node's contract is that `bind`/`connect`/`send` callbacks and the
/// `'listening'`/`'connect'` events never fire in the same tick as the call, so
/// even the events whose syscall already completed synchronously are queued here
/// rather than invoked inline.
pub enum SockEvent {
    /// `'listening'` — the socket is bound and addressable.
    Listening,
    /// `'connect'` — a `connect()` completed.
    Connect,
    /// `'close'` — the handle was released.
    Close,
    /// `'message'` — an inbound datagram.
    Message(Datagram),
    /// `'error'` — `(code, message)`. Emitted when no more specific callback
    /// took the error.
    Error(String, String),
    /// A user `socket.emit(event, ...args)` — queued through the same path as
    /// the OS-produced events so ordering is preserved. The arg WORDS are
    /// GC-pinned while queued (nothing on the JS stack keeps them alive).
    Custom(String, Vec<u64>),
    /// A one-shot callback (bind/connect/send/close), invoked with the error
    /// (Node's err-first shape) or with `null`/no argument.
    Callback {
        /// The listener's Function HANDLE (already normalized + GC-pinned).
        cb: u64,
        /// `Some((code, message))` → the callback gets an `Error`; `None` → it
        /// gets `null` (`send`) or no argument (`bind`/`connect`/`close`).
        err: Option<(String, String)>,
        /// Whether the callback takes an err-first argument at all
        /// (`send`/`connect` do; `bind`/`close` are `() => void`).
        err_first: bool,
    },
}

/// Everything about a live `dgram.Socket` that cannot cross the ABI.
pub struct SocketState {
    /// The OS socket. `socket2` gives one type for every option dgram exposes.
    pub sock: Socket,
    /// `'udp6'` (an AF_INET6 socket) vs `'udp4'`.
    pub v6: bool,
    /// Bound (explicitly or implicitly by a `send`/membership call).
    pub bound: AtomicBool,
    /// The connected peer (`connect()`), if any.
    pub peer: Mutex<Option<SocketAddr>>,
    /// `close()` was called — the reader thread exits on its next timeout.
    pub closed: AtomicBool,
    /// `ref()`ed (default) vs `unref()`ed — whether this socket holds a
    /// keep-alive on the event loop.
    pub refd: AtomicBool,
    /// Whether this socket currently HOLDS a `loop_sources` keep-alive count
    /// (bound + ref'd + open). Keeps inc/dec balanced across ref/unref/close.
    pub counted: AtomicBool,
    /// Pending events for the JS thread.
    pub events: Mutex<VecDeque<SockEvent>>,
    /// `event → listeners`, in insertion order.
    pub listeners: Mutex<Listeners>,
    /// Bytes queued in sends this process has dispatched but not yet completed
    /// (`getSendQueueSize`) and how many such sends (`getSendQueueCount`). Real
    /// counters over the sends that are genuinely still in flight — RTS resolves
    /// + sends a hostname-addressed datagram off-thread, so these are non-zero
    /// exactly while such a send is outstanding.
    pub queue_bytes: AtomicI64,
    pub queue_count: AtomicI64,
    /// The reader thread, joined by `close()`.
    pub reader: Mutex<Option<std::thread::JoinHandle<()>>>,
    /// `createSocket({ receiveBlockList })` — a snapshot of the list's rules at
    /// creation. An inbound datagram whose SENDER matches is dropped before it
    /// ever reaches a `'message'` listener.
    pub receive_block_list: Option<Vec<Rule>>,
    /// `createSocket({ sendBlockList })` — an outbound datagram whose
    /// DESTINATION matches is refused instead of sent.
    pub send_block_list: Option<Vec<Rule>>,
}

/// Whether `addr` is covered by an optional block list (absent list = allow).
pub fn blocked(list: &Option<Vec<Rule>>, addr: std::net::IpAddr) -> bool {
    match list {
        Some(rules) => rules.iter().any(|r| r.matches(addr)),
        None => false,
    }
}

impl SocketState {
    pub fn new(sock: Socket, v6: bool) -> Self {
        Self {
            sock,
            v6,
            bound: AtomicBool::new(false),
            peer: Mutex::new(None),
            closed: AtomicBool::new(false),
            refd: AtomicBool::new(true),
            counted: AtomicBool::new(false),
            events: Mutex::new(VecDeque::new()),
            listeners: Mutex::new(Listeners::default()),
            queue_bytes: AtomicI64::new(0),
            queue_count: AtomicI64::new(0),
            reader: Mutex::new(None),
            receive_block_list: None,
            send_block_list: None,
        }
    }

    /// Queue an event for the JS thread.
    pub fn push(&self, ev: SockEvent) {
        self.events.lock().unwrap().push_back(ev);
    }

    /// Queue `'error'` — or hand the error to `cb` instead when the caller
    /// supplied one (Node: "errors go to the callback if present, else the
    /// `'error'` event").
    pub fn push_err(&self, cb: u64, err_first: bool, code: &str, message: &str) {
        if cb != 0 {
            self.push(SockEvent::Callback {
                cb,
                err: Some((code.to_string(), message.to_string())),
                err_first,
            });
        } else {
            self.push(SockEvent::Error(code.to_string(), message.to_string()));
        }
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    pub fn is_bound(&self) -> bool {
        self.bound.load(Ordering::Acquire)
    }

    pub fn peer_addr(&self) -> Option<SocketAddr> {
        *self.peer.lock().unwrap()
    }
}

/// A registered listener: the Function HANDLE (normalized off the PolyValue word
/// and GC-pinned for as long as it is registered) plus `once` semantics.
#[derive(Clone, Copy)]
pub struct Listener {
    pub cb: u64,
    pub once: bool,
}

/// The `Socket`'s own EventEmitter state.
///
/// `rts-node` is independent of `rts-std` (where the canonical `EventEmitter`
/// class lives), so a node class that IS an emitter carries its own table — as
/// dgram.md §5.6 prescribes. It is the same model (ordered per-event listener
/// list + `once` + a max-listeners setting), owned here.
#[derive(Default)]
pub struct Listeners {
    pub map: Vec<(String, Vec<Listener>)>,
    pub max: i64,
}

impl Listeners {
    pub fn slot(&mut self, event: &str) -> &mut Vec<Listener> {
        if let Some(i) = self.map.iter().position(|(k, _)| k == event) {
            return &mut self.map[i].1;
        }
        self.map.push((event.to_string(), Vec::new()));
        let last = self.map.len() - 1;
        &mut self.map[last].1
    }

    pub fn get(&self, event: &str) -> &[Listener] {
        self.map
            .iter()
            .find(|(k, _)| k == event)
            .map(|(_, v)| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn max_listeners(&self) -> i64 {
        if self.max == 0 {
            DEFAULT_MAX_LISTENERS
        } else {
            self.max
        }
    }
}

/// Node's `EventEmitter.defaultMaxListeners`.
pub const DEFAULT_MAX_LISTENERS: i64 = 10;

/// Sockets keyed by their JS object handle, in CREATION order — an `IndexMap`,
/// not a `HashMap`, so the pump delivers across sockets in a deterministic order
/// (a hash order would shuffle two sockets' events run to run).
type Table = indexmap::IndexMap<u64, Arc<SocketState>>;

fn table() -> &'static Mutex<Table> {
    static T: OnceLock<Mutex<Table>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(Table::new()))
}

pub fn sockets() -> MutexGuard<'static, Table> {
    table().lock().unwrap()
}

/// Register a freshly created socket under its JS object handle.
pub fn insert(handle: u64, state: Arc<SocketState>) {
    sockets().insert(handle, state);
}

/// The state behind a `this` handle, if it is still open.
pub fn get(handle: u64) -> Option<Arc<SocketState>> {
    sockets().get(&handle).cloned()
}

/// Remove a closed socket's state from the table. `shift_remove` keeps the
/// creation order of the survivors (`swap_remove` would reshuffle it).
pub fn remove(handle: u64) -> Option<Arc<SocketState>> {
    sockets().shift_remove(&handle)
}

/// A snapshot of `(handle, state)` for the pump — taken without holding the
/// table lock while listeners run (a listener may `close()` its socket).
pub fn snapshot() -> Vec<(u64, Arc<SocketState>)> {
    sockets().iter().map(|(h, s)| (*h, s.clone())).collect()
}
