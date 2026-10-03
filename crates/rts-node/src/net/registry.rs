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
    /// O que foi escrito ENQUANTO o socket ainda conectava. O Node enfileira
    /// nesse intervalo em vez de errar; sem isto, o primeiro `write` de um
    /// cliente que acabou de chamar `connect` emitia `'error'`.
    pub(super) pending: Vec<u8>,
    pub(super) closed: bool,
    /// Whether this socket may hold the program open — `ref()`/`unref()`.
    ///
    /// Read by [`source`] and nowhere else. Before 2026-10-03 the two JS
    /// methods were `noop_self`, which was defensible only while NOTHING here
    /// held a program open; an open socket has answered [`entry::Pending::In`]
    /// since before that, so `socket.unref()` was already a method that
    /// returned `this` and did not do what its name means.
    pub(super) refed: bool,
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
    /// Whether this server may hold the program open — `ref()`/`unref()`.
    ///
    /// Read by [`source`] and nowhere else. It exists because making a
    /// listening server hold the program open is what turns `unref()` from a
    /// harmless no-op into a false one: in Node an unrefed listener is exactly
    /// a listener that does not keep the process alive, and that is the only
    /// thing it means.
    pub(super) refed: bool,
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

thread_local! {
    /// Já estamos a entregar eventos NESTA thread?
    ///
    /// O `pump` é reentrante por construção: entregar um `'data'` chama um
    /// listener, o listener escreve (é o que o `node:tls` faz para responder), e
    /// o `write` chama `pump` outra vez. O interno drenava a fila que o externo
    /// ainda estava a processar, e os chunks chegavam ao programa FORA DE ORDEM
    /// — numa página de 590 KB isso lê-se como bytes perdidos a meio, num ponto
    /// que muda a cada corrida, porque o que muda é o entrelaçamento.
    static ENTREGANDO: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Delivers every queued socket AND server event, oldest first per entry,
/// then drops both table locks before calling anything — see
/// [`fs::watch::pump`](crate::fs::watch) for why a listener that calls back
/// into this module (writes on `'connection'`, say) must not deadlock on a
/// lock this function still holds.
pub(super) fn pump() {
    if ENTREGANDO.with(|f| f.replace(true)) {
        return;
    }
    // Servers first, and the order is measured rather than chosen: a client
    // connecting to a server in the SAME program has both events queued by the
    // time one pass runs, and Node delivers `'connection'` before the client's
    // `'connect'` — three runs out of three of `tests/claude-net-socket-events`'s
    // program under Node 22. Sockets first gave the opposite order, which a
    // program that answers on `'connection'` can observe.
    pump_servers();
    pump_sockets();
    ENTREGANDO.with(|f| f.set(false));
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
        let mut events = events.into_iter();
        while let Some(event) = events.next() {
            match event {
                SocketEvent::Connected { local, remote } => {
                    // Descarrega o que foi escrito ENQUANTO conectava, na ordem.
                    let pendente = with_sockets(|table| {
                        table.get_mut(&id).map(|e| std::mem::take(&mut e.pending))
                    });
                    if let Some(bytes) = pendente {
                        if !bytes.is_empty() {
                            let _ = write_now(id, &bytes);
                        }
                    }
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
                    // A socket that never connected has nothing left to deliver,
                    // and since an open socket now holds the program open
                    // (`source`), one that is not marked closed holds it open
                    // FOREVER. Marked here rather than on the connect thread
                    // because `pump_sockets` skips a closed entry, and skipping
                    // it would mean never emitting the `'error'` itself.
                    with_sockets(|table| {
                        if let Some(entry) = table.get_mut(&id) {
                            entry.closed = true;
                        }
                    });
                    // `connect` set this true and nothing ever set it back, so a
                    // program that polled `socket.connecting` after a refused
                    // connection waited on a flag that could not change. Node
                    // clears it before the `'error'` fires.
                    entry::with_runtime(|context| {
                        super::common::set_bool(context, instance, "connecting", false)
                    });
                    let error = error_value(&message, "ECONNREFUSED");
                    super::common::emit(instance, "error", error, absent, absent);
                    super::common::emit(instance, "close", entry::boolean_value(true), absent, absent);
                }
                SocketEvent::Data(bytes) => {
                    let push_fn = entry::with_runtime(|context| entry::get_member(context, instance, "push"));
                    if push_fn == absent {
                        continue;
                    }
                    // BUFFER e não `Uint8Array`: é o que o Node entrega num
                    // `'data'`, e a diferença aparece no `toString()` — o do
                    // Uint8Array responde "72,84,84,80,…" (os bytes como lista)
                    // onde o do Buffer decodifica o texto. Um programa que
                    // concatena `chunk.toString()` recebia a página inteira em
                    // números.
                    let chunk = entry::with_runtime(|context| entry::make_buffer(context, &bytes));
                    entry::call(push_fn, instance, chunk, absent, absent, absent);
                }
                SocketEvent::End => {
                    let push_fn = entry::with_runtime(|context| entry::get_member(context, instance, "push"));
                    if push_fn != absent {
                        let null = entry::null_value();
                        entry::call(push_fn, instance, null, absent, absent, absent);
                    }
                    // `push(null)` does not emit `'end'` — `stream::flowing`
                    // SCHEDULES it and its own loop source delivers it, for the
                    // reason that module's doc gives. That source is pumped after
                    // this one, so draining the `Closed` that follows in this
                    // same pass emitted `'close'` BEFORE `'end'`; Node emits
                    // `'end'` first, three runs out of three.
                    //
                    // So the rest of this socket's queue goes back and the next
                    // pass delivers it — a millisecond away, since an open socket
                    // answers `In(POLL)`. The rejected alternative was calling
                    // `flowing::pump` from here, which puts this module in charge
                    // of another's delivery order and emits `'end'` inline, the
                    // one thing that module exists to avoid.
                    let rest: Vec<SocketEvent> = events.collect();
                    if !rest.is_empty() {
                        with_sockets(|table| {
                            if let Some(entry) = table.get_mut(&id) {
                                for event in rest.into_iter().rev() {
                                    entry.queue.push_front(event);
                                }
                            }
                        });
                    }
                    break;
                }
                SocketEvent::Error(message) => {
                    let error = error_value(&message, "ECONNRESET");
                    super::common::emit(instance, "error", error, absent, absent);
                }
                SocketEvent::Closed { had_error } => {
                    // `'close'` is the last event a socket has, so the entry is
                    // done — and an entry that is not closed keeps the program
                    // open now that `source` answers `In` for an open socket.
                    // Nothing used to set this: `destroy()` was the only path to
                    // `closed`, so a socket the PEER closed stayed live in the
                    // table and was rescanned on every pass forever.
                    with_sockets(|table| {
                        if let Some(entry) = table.get_mut(&id) {
                            entry.closed = true;
                            // And the OS socket goes with it. `'close'` means
                            // the descriptor is gone in Node, and leaving it
                            // open left the PEER reading a socket nobody would
                            // ever write to or close: the server side of
                            // `tests/claude-net-socket-events`'s exchange got an
                            // ECONNRESET (os 10060) where Node gives it `'end'`
                            // then `'close'`. Node does this because a socket
                            // with the default `allowHalfOpen: false` ends its
                            // writable half when its readable half ends.
                            if let Some(stream) = entry.stream.take() {
                                let _ = stream.shutdown(std::net::Shutdown::Both);
                            }
                        }
                    });
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
                    // A bind that failed has nothing further to deliver, and a
                    // server that is neither listening nor closed is what
                    // `source` now answers `In` for — so leaving it would poll
                    // forever rather than letting the program end.
                    with_servers(|table| {
                        if let Some(entry) = table.get_mut(&id) {
                            entry.closed = true;
                        }
                    });
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
            // AINDA CONECTANDO: o Node enfileira e envia quando o socket abre —
            // ele não erra. Errar aqui quebrava todo `http.get`: o cliente
            // escreve um buffer VAZIO em laço para bombear este módulo enquanto
            // espera a conexão, e o primeiro desses writes emitia um `'error'`
            // que o próprio cliente lia como "conexão recusada".
            if entry.closed {
                return Err(std::io::Error::other("socket closed"));
            }
            entry.pending.extend_from_slice(bytes);
            return Ok(());
        };
        stream.write_all(bytes)
    })
}

/// How long the host may wait while this thread still owns an open socket.
///
/// A socket's events arrive on a reader thread with no deadline to report, and
/// nothing in this engine can be waited on by the OS: `entry::loops` hands the
/// host a DURATION, so the only way a socket's event reaches the program is for
/// the host to come back and ask. One millisecond is short enough that a round
/// trip over loopback is not noticeably slower than Node's and long enough that
/// the cost is a sleep rather than a spin — a pump is a scan of two small maps.
///
/// The alternative, and the reason it is not here: a condition variable the
/// reader threads notify and the host waits on. That is the right shape and it
/// puts the WAITING in `rts-core`, whose `entry::loops` doc states the opposite
/// rule — `std::thread::sleep` is kept out of that crate because its membership
/// test is "exists on every target, wasm included". Changing where the waiting
/// lives is a change to that rule, so it is not smuggled in here.
const POLL: std::time::Duration = std::time::Duration::from_millis(1);

/// This module as a loop source: deliver what its background threads queued,
/// then say whether any is still live.
///
/// # Why an open SOCKET and a LISTENING server both answer `In`
///
/// Both answered `Blocked`, and by `entry::loops`' contract that neither holds
/// the program open nor bounds the host's sleep. For a client socket it made
/// the module unusable: a
/// program that called `net.connect` and waited exited before the connect thread
/// had queued anything, and one kept alive by an unrelated `setTimeout` slept
/// straight to that timer's deadline, ran the timer, and exited from inside it
/// without `net` ever being pumped. Neither `'connect'` nor `'error'` was ever
/// delivered to a program that only waited, which is Node's entire contract for
/// this module.
///
/// A socket is therefore `In(POLL)` while it is open, which is also what Node
/// does — a connected socket refs the loop. What makes that terminate rather
/// than hang is the other half of that change: a socket whose reader reached
/// EOF now closes, so a completed exchange stops holding the program open.
///
/// # And why a LISTENING server stopped answering `Blocked` (#2893)
///
/// It answered `Blocked`, and the reason written here was "a suite where one
/// unclosed listener hangs every fixture is worse". That was the wrong trade,
/// and the measurement that settles it is not about fixtures:
///
/// ```text
/// const s = http.createServer(handler);
/// s.listen(8160, () => console.log("listening"));
/// ```
///
/// printed `listening` and the process exited, rc=0, immediately — so every
/// connection after it was refused and **no server program could be written in
/// this engine at all** without a `setTimeout` chain to hold the process up.
/// One red fixture is a smaller price than the whole class of server programs,
/// and the hazard the divergence was defending against is already handled a
/// level up rather than here: `rts test` runs each file as a child with a 30 s
/// budget, kills it, and reports it as a FAILURE — "a test that never finishes
/// has not passed", in its own words. A listener nothing closes therefore costs
/// one red file, which is what it should cost.
///
/// The second reader of that answer is what makes this more than a loop bug.
/// `promise_await` treats `pump_sources() == None` as a deadlock and raises
/// "this promise cannot settle", so `await new Promise(() => {})` beside a
/// listening server — the ordinary way a server program parks — threw instead
/// of waiting. One accounting, two readers; both were wrong because the
/// accounting was.
///
/// **What this costs, measured rather than assumed — and it is not what it
/// looks like.** `POLL` is 1 ms, so the obvious reading is that an idle
/// listener now wakes the host a thousand times a second where the `setTimeout`
/// workaround it replaces woke it once a second. That reading is wrong, because
/// the workaround was ALREADY paying it: `entry::loops::BLOCKED_CAP` caps the
/// host's sleep at 1 ms as soon as any source answers `Blocked`, so a listening
/// server beside a 1 s timer already forced a 1 ms loop. Moving the listener
/// from `Blocked` to `In(POLL)` keeps the same wake rate and changes who asks
/// for it.
///
/// Measured 2026-10-03, CPU time over a 6 s idle window, three runs each, same
/// machine: workaround on the old binary 141/203/250 ms, workaround on this one
/// 156/203/219 ms, plain listener on this one 203/219/312 ms. The spread inside
/// one configuration is as wide as the spread between them, so this cannot
/// distinguish them — which is the answer the mechanism above predicts. The one
/// genuinely new case is a program whose ONLY work is a listener, and that
/// program did not run at all before, so there is no cost to compare it to.
///
/// What remains true is that 1 ms of polling is the wrong mechanism, for the
/// reason named under `POLL` — nothing here can be notified by a background
/// thread. That is #2868, and it is fixed in that one place rather than here.
pub fn source() -> entry::Pending {
    pump();
    let mine = std::thread::current().id();
    // Both tables in one shape: whether anything of mine is still live AND
    // counts (`holding`), and whether anything of mine is still live but was
    // unrefed (`waiting`). The second is what tells `Blocked` from `Idle`.
    let (mut holding, mut waiting) = with_sockets(|table| {
        let mut holding = false;
        let mut waiting = false;
        for entry in table.values().filter(|entry| entry.owner == mine && !entry.closed) {
            match entry.refed {
                true => holding = true,
                false => waiting = true,
            }
        }
        (holding, waiting)
    });
    with_servers(|table| {
        for entry in table.values().filter(|entry| entry.owner == mine && !entry.closed) {
            // A server that has not bound YET holds the program open whatever
            // its ref flag says: its accept thread is about to queue
            // `'listening'` or `'error'`, that is a bounded wait, and in Node
            // the `listen` callback runs before `unref()` can have any meaning.
            // It used to be `Blocked`, which meant a program whose only pending
            // work was `server.listen(0, host, callback)` ended before the bind
            // completed and the callback never ran at all.
            match entry.refed || !entry.listening {
                true => holding = true,
                // Unrefed and listening: still pumped every pass, so it still
                // accepts and emits while anything else keeps the loop turning,
                // and contributes no deadline of its own. That is the whole of
                // what `unref()` means.
                false => waiting = true,
            }
        }
    });
    if holding {
        return entry::Pending::In(POLL);
    }
    // `Blocked` is "pumped, does not hold open", which is the honest answer for
    // a handle a program asked not to count: an unrefed socket or listener is
    // still delivered to while anything else turns the loop.
    match waiting {
        true => entry::Pending::Blocked,
        false => entry::Pending::Idle,
    }
}
