//! node:net — the `Socket` class: `connect`, `write`, `end`, `destroy`, and the
//! read loop behind `'data'`/`'end'`.
//!
//! `connect` runs OFF the JS thread (resolution blocks, and so can the connect
//! itself), queueing `'lookup'`/`'connectionAttempt'`/`'connect'`/`'error'` as it
//! goes. When `autoSelectFamily` is on, the candidates are tried Happy-Eyeballs
//! style: AAAA-first-then-A, each attempt capped by
//! `autoSelectFamilyAttemptTimeout` — sequential-with-timeout, which is what
//! Node's own implementation does (net.md §5.1), not a concurrent fan-out.
//!
//! `write` runs the syscall inline on the JS thread and returns Node's
//! backpressure boolean; the read loop is a dedicated thread per connected
//! socket that only ever queues bytes.

use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use super::opts;
use super::state::{self, SockEvent, SocketOpts, SocketState};
use crate::values::{read, read_bytes, val, Val};

use rts_engine::gc_surface::{__RTS_FN_NS_GC_PIN_HANDLE, __RTS_FN_NS_GC_UNPIN_HANDLE};

pub const CLASS: &str = "Socket";

/// How long a read parks before re-checking `paused`/`destroyed`.
const POLL: Duration = Duration::from_millis(50);
/// The read buffer — one chunk per `'data'`.
const READ_CHUNK: usize = 64 * 1024;

/// The JS-visible `Socket` object.
pub fn alloc_object() -> u64 {
    use rts_engine::heap::handles::{alloc_entry, Entry};
    let mut m: indexmap::IndexMap<String, i64> = indexmap::IndexMap::new();
    m.insert(
        "__rts_class".to_string(),
        alloc_entry(Entry::String(CLASS.as_bytes().to_vec())) as i64,
    );
    alloc_entry(Entry::Map(Box::new(m)))
}

/// Register a socket state under a fresh object handle, pinned while it lives.
pub fn register(st: Arc<SocketState>) -> u64 {
    let handle = alloc_object();
    state::insert_socket(handle, st);
    unsafe { __RTS_FN_NS_GC_PIN_HANDLE(handle) };
    handle
}

/// `new Socket()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_NEW() -> u64 {
    register(Arc::new(SocketState::new(SocketOpts::default())))
}

/// `new Socket(options)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_NEW_OPTS(options: u64) -> u64 {
    if !opts::reject_unsupported(options) {
        return 0;
    }
    let block_list = match super::server::block_list_of(options) {
        Ok(list) => list,
        Err(()) => return 0,
    };
    let st = Arc::new(SocketState::new(opts::socket_opts(options)));
    *st.block_list.lock().unwrap() = block_list;
    register(st)
}

/// Resolve `host:port` to ONE address (the system resolver — what the default
/// `dns.lookup` is). A literal IP short-circuits.
pub fn resolve_host(host: &str, port: u16) -> std::io::Result<SocketAddr> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(SocketAddr::new(ip, port));
    }
    (host, port).to_socket_addrs()?.next().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::AddrNotAvailable,
            format!("getaddrinfo ENOTFOUND {host}"),
        )
    })
}

/// Every candidate for `host:port`, ordered Happy-Eyeballs style: the first
/// AAAA, then the first A, then the second AAAA, … (RFC 8305 §5's ordering,
/// which is what Node interleaves).
fn candidates(host: &str, port: u16) -> std::io::Result<Vec<SocketAddr>> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(vec![SocketAddr::new(ip, port)]);
    }
    let all: Vec<SocketAddr> = (host, port).to_socket_addrs()?.collect();
    let (v6, v4): (Vec<_>, Vec<_>) = all.into_iter().partition(|a| a.is_ipv6());
    let mut out = Vec::with_capacity(v6.len() + v4.len());
    let (mut i6, mut i4) = (v6.into_iter(), v4.into_iter());
    loop {
        match (i6.next(), i4.next()) {
            (None, None) => break,
            (a, b) => out.extend(a.into_iter().chain(b)),
        }
    }
    Ok(out)
}

/// The `connect` arguments after Node's normalization.
struct ConnectArgs {
    port: u16,
    host: String,
    cb: u64,
    auto_select_family: bool,
    attempt_timeout: u64,
}

/// The shared `connect` for every overload.
fn connect_impl(this: u64, words: &[u64]) -> u64 {
    let Some(st) = state::socket(this) else {
        opts::throw("ERR_SOCKET_CLOSED", "Socket is closed");
        return this;
    };
    let (mut port, mut host, mut cb) = (None, None, 0u64);
    let mut auto = opts::auto_select_family();
    let mut timeout = opts::auto_select_family_timeout() as u64;
    for &w in words {
        match val(w) {
            Val::Num(n) => match opts::port_of(n) {
                Ok(p) => port = Some(p),
                Err(msg) => {
                    opts::throw("ERR_SOCKET_BAD_PORT", &msg);
                    return this;
                }
            },
            Val::Str(s) => host = Some(s),
            Val::Func(f) => cb = f,
            Val::Obj(o) => {
                if !opts::reject_unsupported(o) {
                    return this;
                }
                if let Some(n) = crate::values::opt_num(o, "port") {
                    match opts::port_of(n) {
                        Ok(p) => port = Some(p),
                        Err(msg) => {
                            opts::throw("ERR_SOCKET_BAD_PORT", &msg);
                            return this;
                        }
                    }
                }
                if let Some(h) = crate::values::opt_str(o, "host") {
                    host = Some(h);
                }
                if crate::values::opt_has(o, "autoSelectFamily") {
                    auto = crate::values::opt_bool(o, "autoSelectFamily");
                }
                if let Some(t) = crate::values::opt_num(o, "autoSelectFamilyAttemptTimeout") {
                    timeout = (t as u64).max(10);
                }
            }
            _ => {}
        }
    }
    let Some(port) = port else {
        opts::throw("ERR_MISSING_ARGS", "The \"options\" or \"port\" argument must be specified");
        return this;
    };
    if cb != 0 {
        crate::emitter::add(this, "connect", cb, true, false);
    }
    let args = ConnectArgs {
        port,
        // Node's default host.
        host: host.unwrap_or_else(|| "localhost".to_string()),
        cb: 0,
        auto_select_family: auto,
        attempt_timeout: timeout,
    };
    st.connecting.store(true, Ordering::Release);
    super::pump::ensure_registered();
    spawn_connect(this, st, args);
    this
}

/// Resolution + the attempt loop, off the JS thread.
fn spawn_connect(this: u64, st: Arc<SocketState>, args: ConnectArgs) {
    let failed = st.clone();
    let spawned = std::thread::Builder::new()
        .name("rts-net-connect".to_string())
        .spawn(move || {
            let list = if args.auto_select_family {
                candidates(&args.host, args.port)
            } else {
                resolve_host(&args.host, args.port).map(|a| vec![a])
            };
            let list = match list {
                Ok(l) if !l.is_empty() => l,
                Ok(_) | Err(_) => {
                    let msg = format!("getaddrinfo ENOTFOUND {}", args.host);
                    st.push(SockEvent::Lookup {
                        err: Some(msg.clone()),
                        address: String::new(),
                        family: 0,
                        host: args.host.clone(),
                    });
                    st.connecting.store(false, Ordering::Release);
                    st.push_err(args.cb, "ENOTFOUND", &msg);
                    return;
                }
            };
            let first = list[0];
            st.push(SockEvent::Lookup {
                err: None,
                address: first.ip().to_string(),
                family: if first.is_ipv6() { 6 } else { 4 },
                host: args.host.clone(),
            });
            // `blockList` refuses the destination before dialing.
            if let Some(a) = list.iter().find(|a| state::blocked(&st.block_list, a.ip())) {
                st.connecting.store(false, Ordering::Release);
                st.push_err(
                    args.cb,
                    "ERR_IP_BLOCKED",
                    &format!("IP {} is blocked by net.BlockList", a.ip()),
                );
                return;
            }
            attempt_all(this, &st, &list, &args);
        });
    if spawned.is_err() {
        let st = failed;
        st.connecting.store(false, Ordering::Release);
        st.push_err(0, "EMFILE", "could not start the connect thread");
    }
}

/// Try each candidate in turn, capping every attempt but the last by
/// `attempt_timeout` — Node's real algorithm. Errors are swallowed while any
/// attempt may still succeed; if all fail, the LAST error surfaces.
fn attempt_all(this: u64, st: &Arc<SocketState>, list: &[SocketAddr], args: &ConnectArgs) {
    let multi = args.auto_select_family && list.len() > 1;
    let mut last: Option<std::io::Error> = None;
    for (i, addr) in list.iter().enumerate() {
        let family = if addr.is_ipv6() { 6 } else { 4 };
        if multi {
            st.attempted.lock().unwrap().push(format!("{}:{}", addr.ip(), addr.port()));
            st.push(SockEvent::Attempt {
                ip: addr.ip().to_string(),
                port: addr.port(),
                family,
                err: None,
            });
        }
        // The last attempt is not time-boxed the same way (net.md §4).
        let result = if multi && i + 1 < list.len() {
            TcpStream::connect_timeout(addr, Duration::from_millis(args.attempt_timeout))
        } else {
            TcpStream::connect(addr)
        };
        match result {
            Ok(stream) => {
                established(this, st, stream);
                return;
            }
            Err(e) => {
                if multi {
                    st.push(SockEvent::Attempt {
                        ip: addr.ip().to_string(),
                        port: addr.port(),
                        family,
                        err: Some(e.to_string()),
                    });
                }
                last = Some(e);
            }
        }
    }
    st.connecting.store(false, Ordering::Release);
    let e = last.unwrap_or_else(|| std::io::Error::other("connect failed"));
    let (code, msg) = opts::message_for(&e, "connect");
    st.push_err(args.cb, &code, &msg);
}

/// A connection is up (from `connect` or from a server `accept`): apply the
/// options, record the addresses, start the reader, queue `'connect'`.
pub fn established(this: u64, st: &Arc<SocketState>, stream: TcpStream) {
    let o = *st.opts.lock().unwrap();
    let _ = stream.set_nodelay(o.no_delay);
    if o.keep_alive {
        let sock = socket2::SockRef::from(&stream);
        let mut ka = socket2::TcpKeepalive::new();
        if o.keep_alive_initial_delay > 0 {
            ka = ka.with_time(Duration::from_millis(o.keep_alive_initial_delay));
        }
        let _ = sock.set_tcp_keepalive(&ka);
    }
    *st.local.lock().unwrap() = stream.local_addr().ok();
    *st.peer.lock().unwrap() = stream.peer_addr().ok();
    *st.stream.lock().unwrap() = Some(stream);
    st.connecting.store(false, Ordering::Release);
    st.readable.store(true, Ordering::Release);
    st.writable.store(true, Ordering::Release);
    if o.pause_on_connect {
        st.paused.store(true, Ordering::Release);
    }
    st.push(SockEvent::Connect);
    start_reader(this, st);
    keep_alive(st, true);
    super::pump::ensure_registered();
}

/// Add/remove this socket's keep-alive on the event loop.
pub fn keep_alive(st: &SocketState, want: bool) {
    let want = want && st.refd.load(Ordering::Acquire) && !st.is_destroyed();
    if want == st.counted.load(Ordering::Acquire) {
        return;
    }
    if want {
        rts_engine::loop_sources::inc_active();
    } else {
        rts_engine::loop_sources::dec_active();
    }
    st.counted.store(want, Ordering::Release);
}

/// The read thread: blocking reads with a short timeout, so `pause()`/`destroy()`
/// are observed. It only ever queues bytes — never a JS value.
fn start_reader(this: u64, st: &Arc<SocketState>) {
    let mut slot = st.read_thread.lock().unwrap();
    if slot.is_some() {
        return;
    }
    let Some(stream) = st.clone_stream() else { return };
    if stream.set_read_timeout(Some(POLL)).is_err() {
        return;
    }
    let state = st.clone();
    *slot = std::thread::Builder::new()
        .name(format!("rts-net-read-{this}"))
        .spawn(move || read_loop(state, stream))
        .ok();
}

fn read_loop(st: Arc<SocketState>, mut stream: TcpStream) {
    let mut buf = vec![0u8; READ_CHUNK];
    let mut idle = std::time::Instant::now();
    loop {
        if st.is_destroyed() || !st.readable.load(Ordering::Acquire) {
            return;
        }
        if st.paused.load(Ordering::Acquire) {
            std::thread::sleep(POLL);
            continue;
        }
        match stream.read(&mut buf) {
            // EOF: the peer sent FIN.
            Ok(0) => {
                st.readable.store(false, Ordering::Release);
                st.push(SockEvent::End);
                return;
            }
            Ok(n) => {
                idle = std::time::Instant::now();
                st.bytes_read.fetch_add(n as i64, Ordering::AcqRel);
                st.push(SockEvent::Data(buf[..n].to_vec()));
            }
            Err(e) if transient(&e) => {
                // `setTimeout(ms)`: an idle connection emits 'timeout' — which
                // does NOT destroy the socket (Node's contract).
                let t = st.timeout_ms.load(Ordering::Acquire);
                if t > 0 && idle.elapsed() >= Duration::from_millis(t as u64) {
                    idle = std::time::Instant::now();
                    st.push(SockEvent::Timeout);
                }
            }
            Err(e) => {
                if st.is_destroyed() {
                    return;
                }
                let (code, msg) = opts::message_for(&e, "read");
                st.push(SockEvent::Error(code, msg));
                return;
            }
        }
    }
}

fn transient(e: &std::io::Error) -> bool {
    use std::io::ErrorKind::*;
    matches!(e.kind(), WouldBlock | TimedOut | Interrupted)
}

/// `socket.connect(port)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_CONNECT1(this: u64, a0: u64) -> u64 {
    connect_impl(this, &[a0])
}

/// `socket.connect(port, hostOrCallback)` / `connect(options, callback)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_CONNECT2(this: u64, a0: u64, a1: u64) -> u64 {
    connect_impl(this, &[a0, a1])
}

/// `socket.connect(port, host, callback)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_CONNECT3(this: u64, a0: u64, a1: u64, a2: u64) -> u64 {
    connect_impl(this, &[a0, a1, a2])
}

/// The bytes a `write`/`end` data argument carries, honouring `encoding`.
fn payload(data: u64, encoding: Option<String>) -> Option<Vec<u8>> {
    match val(data) {
        Val::Str(s) => Some(super::encode(&s, encoding.as_deref().unwrap_or("utf8"))),
        Val::Obj(h) => Some(read_bytes(h)),
        _ => None,
    }
}

/// The shared `write`. Returns Node's backpressure boolean: `true` when the OS
/// took everything, `false` when some of it is still ours to flush.
fn write_impl(this: u64, data: u64, encoding: Option<String>, cb: u64) -> i64 {
    let Some(st) = state::socket(this) else {
        opts::throw("ERR_SOCKET_CLOSED", "Socket is closed");
        return 0;
    };
    if st.is_destroyed() {
        // Node: a write after destroy reports ERR_STREAM_DESTROYED via 'error',
        // it does not throw.
        st.push_err(cb, "ERR_STREAM_DESTROYED", "Cannot call write after a stream was destroyed");
        super::pump::ensure_registered();
        return 0;
    }
    let Some(bytes) = payload(data, encoding) else {
        opts::throw(
            "ERR_INVALID_ARG_TYPE",
            "The \"chunk\" argument must be of type string or an instance of Buffer or Uint8Array",
        );
        return 0;
    };
    let Some(mut stream) = st.clone_stream() else {
        st.push_err(cb, "ERR_SOCKET_CLOSED", "Socket is closed");
        super::pump::ensure_registered();
        return 0;
    };
    st.buffered.fetch_add(bytes.len() as i64, Ordering::AcqRel);
    let result = stream.write_all(&bytes).and_then(|()| stream.flush());
    st.buffered.fetch_sub(bytes.len() as i64, Ordering::AcqRel);
    super::pump::ensure_registered();
    match result {
        Ok(()) => {
            st.bytes_written.fetch_add(bytes.len() as i64, Ordering::AcqRel);
            if cb != 0 {
                st.push(SockEvent::Callback { cb, err: None });
            }
            // The kernel took it all: no backpressure, and Node still emits
            // 'drain' only after a false return — so none here.
            1
        }
        Err(e) => {
            let (code, msg) = opts::message_for(&e, "write");
            st.push_err(cb, &code, &msg);
            0
        }
    }
}

/// `socket.write(data)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_WRITE1(this: u64, data: u64) -> i64 {
    write_impl(this, data, None, 0)
}

/// `socket.write(data, encodingOrCallback)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_WRITE2(this: u64, data: u64, a1: u64) -> i64 {
    match val(a1) {
        Val::Str(enc) => write_impl(this, data, Some(enc), 0),
        Val::Func(cb) => write_impl(this, data, None, cb),
        _ => write_impl(this, data, None, 0),
    }
}

/// `socket.write(data, encoding, callback)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_WRITE3(this: u64, data: u64, enc: u64, cb: u64) -> i64 {
    let encoding = match val(enc) {
        Val::Str(s) => Some(s),
        _ => None,
    };
    write_impl(this, data, encoding, val(cb).as_func().unwrap_or(0))
}

/// Half-close: send FIN, keep reading. `'end'` on the peer follows.
fn end_impl(this: u64, data: u64, encoding: Option<String>, cb: u64) -> u64 {
    let Some(st) = state::socket(this) else { return this };
    if data != 0 && !val(data).is_nullish() {
        write_impl(this, data, encoding, 0);
    }
    st.writable.store(false, Ordering::Release);
    if let Some(stream) = st.clone_stream() {
        let _ = stream.shutdown(std::net::Shutdown::Write);
    }
    if cb != 0 {
        st.push(SockEvent::Callback { cb, err: None });
    }
    // With allowHalfOpen (the default false), the peer's FIN will close us; our
    // own FIN leaves the readable side alone until then.
    super::pump::ensure_registered();
    this
}

/// `socket.end()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_END0(this: u64) -> u64 {
    end_impl(this, 0, None, 0)
}

/// `socket.end(dataOrCallback)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_END1(this: u64, a0: u64) -> u64 {
    match val(a0) {
        Val::Func(cb) => end_impl(this, 0, None, cb),
        _ => end_impl(this, a0, None, 0),
    }
}

/// `socket.end(data, encodingOrCallback)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_END2(this: u64, a0: u64, a1: u64) -> u64 {
    match val(a1) {
        Val::Str(enc) => end_impl(this, a0, Some(enc), 0),
        Val::Func(cb) => end_impl(this, a0, None, cb),
        _ => end_impl(this, a0, None, 0),
    }
}

/// `socket.end(data, encoding, callback)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_END3(this: u64, a0: u64, enc: u64, cb: u64) -> u64 {
    let encoding = match val(enc) {
        Val::Str(s) => Some(s),
        _ => None,
    };
    end_impl(this, a0, encoding, val(cb).as_func().unwrap_or(0))
}

/// Tear the connection down. `had_error` drives `'close'`'s argument.
pub fn destroy(this: u64, st: &Arc<SocketState>, had_error: bool) {
    if st.is_destroyed() {
        return;
    }
    st.destroyed.store(true, Ordering::Release);
    st.readable.store(false, Ordering::Release);
    st.writable.store(false, Ordering::Release);
    if let Some(stream) = st.clone_stream() {
        let _ = stream.shutdown(std::net::Shutdown::Both);
    }
    *st.stream.lock().unwrap() = None;
    keep_alive(st, false);
    st.push(SockEvent::Close(had_error));
    let _ = this;
    super::pump::ensure_registered();
}

/// `socket.destroy()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_DESTROY0(this: u64) -> u64 {
    if let Some(st) = state::socket(this) {
        destroy(this, &st, false);
    }
    this
}

/// `socket.destroy(error)` — the error is emitted, then `'close'` with
/// `hadError = true` (Node always pairs them on a Socket).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_DESTROY1(this: u64, err: u64) -> u64 {
    let Some(st) = state::socket(this) else { return this };
    if val(err).is_nullish() {
        destroy(this, &st, false);
        return this;
    }
    crate::emitter::pin_word(err);
    st.push(SockEvent::Custom("error".to_string(), vec![err]));
    destroy(this, &st, true);
    this
}

/// `socket.destroySoon()` — end, then destroy once flushed. Writes here are
/// synchronous, so the flush is already done by the time `end` returns.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_DESTROY_SOON(this: u64) {
    end_impl(this, 0, None, 0);
    if let Some(st) = state::socket(this) {
        destroy(this, &st, false);
    }
}

/// `socket.resetAndDestroy()` — an RST, not a FIN (TCP only).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_RESET_AND_DESTROY(this: u64) -> u64 {
    let Some(st) = state::socket(this) else {
        opts::throw("ERR_SOCKET_CLOSED", "Socket is closed");
        return this;
    };
    if st.is_destroyed() {
        opts::throw("ERR_SOCKET_CLOSED", "Socket is closed");
        return this;
    }
    // SO_LINGER with a 0 timeout makes close() send RST instead of FIN.
    if let Some(stream) = st.clone_stream() {
        let _ = socket2::SockRef::from(&stream).set_linger(Some(Duration::ZERO));
    }
    destroy(this, &st, false);
    this
}

/// Release a destroyed socket: its listeners, its object pin and its state.
/// Called by the pump once `'close'` has been delivered.
pub fn finalize(this: u64) {
    if let Some(st) = state::remove_socket(this) {
        let thread = st.read_thread.lock().unwrap().take();
        if let Some(t) = thread {
            let _ = t.join();
        }
        crate::emitter::release_all(this);
        unsafe { __RTS_FN_NS_GC_UNPIN_HANDLE(this) };
    }
}

/// A user `socket.emit(event, ...args)`.
fn emit_words(this: u64, ep: *const u8, el: i64, args: Vec<u64>) -> i64 {
    let Some(st) = state::socket(this) else { return 0 };
    let event = read(ep, el);
    let had = crate::emitter::has(this, &event);
    for &a in &args {
        crate::emitter::pin_word(a);
    }
    st.push(SockEvent::Custom(event, args));
    super::pump::ensure_registered();
    i64::from(had)
}

/// `socket.emit(event)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_EMIT0(this: u64, ep: *const u8, el: i64) -> i64 {
    emit_words(this, ep, el, Vec::new())
}

/// `socket.emit(event, a0)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_EMIT1(this: u64, ep: *const u8, el: i64, a0: u64) -> i64 {
    emit_words(this, ep, el, vec![a0])
}
