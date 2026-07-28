//! node:net — the `Server` class: `createServer`, `listen`, `close`, the
//! `'connection'`/`'listening'`/`'close'`/`'error'`/`'drop'` events.
//!
//! `listen` binds through `socket2` (SO_REUSEADDR like Node/libuv, an explicit
//! `listen(backlog)`, `IPV6_V6ONLY`, `SO_REUSEPORT`), then hands the listener to
//! a dedicated ACCEPT THREAD. That thread queues the accepted stream as plain
//! data; `pump.rs` builds the JS `Socket` on the JS thread and emits
//! `'connection'`.
//!
//! Node's asymmetry is preserved exactly: a `Server`'s `'error'` is NOT followed
//! by an automatic `'close'` (a `Socket`'s always is).

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use socket2::{Domain, Protocol, SockAddr, Socket, Type};

use super::opts::{self, ListenArgs};
use super::state::{self, ServerEvent, ServerState, SocketOpts};
use crate::values::{opt_word, val, Val};

use rts_engine::gc_surface::{__RTS_FN_NS_GC_PIN_HANDLE, __RTS_FN_NS_GC_UNPIN_HANDLE};

pub const CLASS: &str = "Server";

/// The JS-visible `Server` object.
fn alloc_object() -> u64 {
    use rts_engine::heap::handles::{alloc_entry, Entry};
    let mut m: indexmap::IndexMap<String, i64> = indexmap::IndexMap::new();
    m.insert(
        "__rts_class".to_string(),
        alloc_entry(Entry::String(CLASS.as_bytes().to_vec())) as i64,
    );
    alloc_entry(Entry::Map(Box::new(m)))
}

/// Build a server from `options` (a `ServerOptions` object or nothing) and an
/// optional `connectionListener`.
fn build(options: u64, listener: u64) -> u64 {
    let mut sock_opts = SocketOpts::default();
    let mut block_list = None;
    if options != 0 {
        if !opts::reject_unsupported(options) {
            return 0;
        }
        sock_opts = opts::socket_opts(options);
        match block_list_of(options) {
            Ok(list) => block_list = list,
            Err(()) => return 0,
        }
    }
    let handle = alloc_object();
    let mut state = ServerState::new(sock_opts);
    // The accept thread reads `maxConnections`/`dropMaxConnection` off the JS
    // object, where Node keeps them as plain properties.
    state.handle = handle;
    let st = Arc::new(state);
    *st.block_list.lock().unwrap() = block_list;
    state::insert_server(handle, st);
    // An open server (and therefore its object) stays alive until close().
    unsafe { __RTS_FN_NS_GC_PIN_HANDLE(handle) };
    if let Val::Func(cb) = val(listener) {
        crate::emitter::add(handle, "connection", cb, false, false);
    }
    handle
}

/// `net.createServer()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_CREATE_SERVER() -> u64 {
    build(0, 0)
}

/// `net.createServer(options | connectionListener)` — overloaded BY VALUE, the
/// same branch Node's own JS takes.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_CREATE_SERVER1(a0: u64) -> u64 {
    match val(a0) {
        Val::Func(_) => build(0, a0),
        Val::Obj(o) => build(o, 0),
        _ => build(0, 0),
    }
}

/// `net.createServer(options, connectionListener)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_CREATE_SERVER2(options: u64, listener: u64) -> u64 {
    match val(options) {
        Val::Obj(o) => build(o, listener),
        _ => build(0, listener),
    }
}

/// The default listen address: `::` when IPv6 is available (dual-stack accepts
/// IPv4 too), else `0.0.0.0` — Node's own rule.
fn default_host(v6_ok: bool) -> IpAddr {
    if v6_ok {
        IpAddr::V6(Ipv6Addr::UNSPECIFIED)
    } else {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    }
}

/// Bind + listen, returning the real listener (the OS resolves port 0).
fn bind_listener(a: &ListenArgs) -> std::io::Result<TcpListener> {
    let host = match a.host.as_deref() {
        Some(h) => super::socket::resolve_host(h, a.port)?.ip(),
        // Try IPv6 dual-stack first; fall back to IPv4 where v6 is unavailable.
        None => default_host(Socket::new(Domain::IPV6, Type::STREAM, Some(Protocol::TCP)).is_ok()),
    };
    let addr = SocketAddr::new(host, a.port);
    let domain = if addr.is_ipv6() { Domain::IPV6 } else { Domain::IPV4 };
    let sock = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
    // Node/libuv set SO_REUSEADDR on every listening socket.
    sock.set_reuse_address(true)?;
    if addr.is_ipv6() && a.ipv6_only {
        sock.set_only_v6(true)?;
    }
    if a.reuse_port {
        set_reuse_port(&sock)?;
    }
    sock.bind(&SockAddr::from(addr))?;
    sock.listen(a.backlog)?;
    Ok(sock.into())
}

/// SO_REUSEPORT where the platform has it. Node documents `reusePort` as Linux
/// 3.9+/DragonFly/FreeBSD/Solaris/AIX only and says an unsupported platform must
/// RAISE rather than ignore it — so this reports the real errno.
#[cfg(unix)]
fn set_reuse_port(sock: &Socket) -> std::io::Result<()> {
    sock.set_reuse_port(true)
}

#[cfg(not(unix))]
fn set_reuse_port(_sock: &Socket) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "SO_REUSEPORT is not supported on this platform",
    ))
}

/// The shared `listen` for every overload.
fn listen_impl(this: u64, args: ListenArgs, cb: u64) -> u64 {
    let Some(st) = state::server(this) else {
        opts::throw("ERR_SERVER_NOT_RUNNING", "Server is not running");
        return this;
    };
    if st.listening.load(Ordering::Acquire) {
        // Node: listen() may only be called again after an error or a close().
        opts::throw("ERR_SERVER_ALREADY_LISTEN", "Listen method has been called more than once without closing");
        return this;
    }
    if cb != 0 {
        crate::emitter::add(this, "listening", cb, true, false);
    }
    match bind_listener(&args) {
        Ok(listener) => {
            let bound = listener.local_addr().ok();
            *st.bound.lock().unwrap() = bound;
            *st.listener.lock().unwrap() = listener.try_clone().ok();
            st.listening.store(true, Ordering::Release);
            start_accept(this, &st, listener);
            st.push(ServerEvent::Listening);
            super::pump::ensure_registered();
            keep_alive(&st, true);
        }
        Err(e) => {
            // Node reports listen failures as an 'error' EVENT, and does NOT
            // auto-close the server after one.
            let (code, msg) = opts::message_for(&e, "listen");
            st.push(ServerEvent::Error(code, msg));
            super::pump::ensure_registered();
        }
    }
    this
}

/// Add/remove the server's keep-alive on the event loop.
pub fn keep_alive(st: &ServerState, want: bool) {
    let want = want
        && st.refd.load(Ordering::Acquire)
        && !st.is_closed()
        && st.listening.load(Ordering::Acquire);
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

/// The accept thread: blocks in `accept()`, queues each connection as plain
/// data. `close()` drops the listener, which fails the blocking accept and ends
/// the loop.
fn start_accept(this: u64, st: &Arc<ServerState>, listener: TcpListener) {
    // NON-BLOCKING + a short park, so `close()` is observed and the thread
    // exits: a thread parked inside a blocking `accept()` would never see the
    // flag (dropping the state's clone of the listener does not wake the
    // thread's own), and `finalize`'s join would hang the event loop forever.
    if listener.set_nonblocking(true).is_err() {
        return;
    }
    let state = st.clone();
    let handle = std::thread::Builder::new()
        .name(format!("rts-net-accept-{this}"))
        .spawn(move || {
            const PARK: std::time::Duration = std::time::Duration::from_millis(5);
            while !state.is_closed() {
                match listener.accept() {
                    Ok((stream, _)) => {
                        // The accepted socket goes back to blocking: its own read
                        // thread wants a timeout, not a spin.
                        let _ = stream.set_nonblocking(false);
                        accept_one(&state, stream);
                    }
                    Err(e)
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                        ) =>
                    {
                        std::thread::sleep(PARK);
                    }
                    Err(e) => {
                        if state.is_closed() {
                            break;
                        }
                        let (code, msg) = opts::message_for(&e, "accept");
                        state.push(ServerEvent::Error(code, msg));
                        break;
                    }
                }
            }
        })
        .ok();
    *st.accept_thread.lock().unwrap() = handle;
}

/// One accepted connection: the block list and `maxConnections` are applied HERE
/// (before the connection is ever visible to JS), exactly as Node does.
fn accept_one(st: &Arc<ServerState>, stream: std::net::TcpStream) {
    let Ok(remote) = stream.peer_addr() else { return };
    if state::blocked(&st.block_list, remote.ip()) {
        // Refused before 'connection' fires. (Node's doc: not a security
        // boundary behind a proxy/NAT — the address checked is the peer's.)
        return;
    }
    if st.max_connections().is_some_and(|max| st.connections.load(Ordering::Acquire) >= max) {
        // Node ≥ v21: maxConnections 0 drops EVERY connection (not Infinity).
        // Non-cluster mode closes the connection and emits 'drop'.
        let local = st.bound.lock().unwrap().unwrap_or(remote);
        st.push(ServerEvent::Drop { local, remote });
        drop(stream);
        return;
    }
    st.connections.fetch_add(1, Ordering::AcqRel);
    st.push(ServerEvent::Connection(stream));
}

/// `server.listen()` — an OS-assigned port on the default host.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SERVER_LISTEN0(this: u64) -> u64 {
    listen_impl(this, ListenArgs::default(), 0)
}

/// Normalize `listen`'s polymorphic argument list: a number is the port, a
/// string the host, an object the `ListenOptions`, a function the callback.
/// (A `path` string would be IPC — refused by `reject_unsupported`.)
fn listen_args(this: u64, words: &[u64]) -> u64 {
    let mut a = ListenArgs::default();
    let mut cb = 0u64;
    let mut numbers = Vec::new();
    for &w in words {
        match val(w) {
            Val::Num(n) => numbers.push(n),
            Val::Str(s) => a.host = Some(s),
            Val::Func(f) => cb = f,
            Val::Obj(o) => match opts::listen_options(o) {
                Ok(parsed) => a = parsed,
                Err(()) => return this,
            },
            _ => {}
        }
    }
    // `listen(port[, host][, backlog])`: the first number is the port, a second
    // is the backlog.
    if let Some(&p) = numbers.first() {
        match opts::port_of(p) {
            Ok(port) => a.port = port,
            Err(msg) => {
                opts::throw("ERR_SOCKET_BAD_PORT", &msg);
                return this;
            }
        }
    }
    if let Some(&b) = numbers.get(1) {
        a.backlog = b.max(0.0) as i32;
    }
    listen_impl(this, a, cb)
}

/// `server.listen(portOrOptionsOrCallback)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SERVER_LISTEN1(this: u64, a0: u64) -> u64 {
    listen_args(this, &[a0])
}

/// `server.listen(port, hostOrBacklogOrCallback)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SERVER_LISTEN2(this: u64, a0: u64, a1: u64) -> u64 {
    listen_args(this, &[a0, a1])
}

/// `server.listen(port, host, backlogOrCallback)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SERVER_LISTEN3(this: u64, a0: u64, a1: u64, a2: u64) -> u64 {
    listen_args(this, &[a0, a1, a2])
}

/// `server.listen(port, host, backlog, callback)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SERVER_LISTEN4(this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    listen_args(this, &[a0, a1, a2, a3])
}

/// The shared `close`. Node: the callback gets `ERR_SERVER_NOT_RUNNING` when the
/// server was not listening; `'close'` fires once it is fully closed.
fn close_impl(this: u64, cb: u64) -> u64 {
    let Some(st) = state::server(this) else {
        if cb != 0 {
            crate::emitter::add(this, "close", cb, true, false);
        }
        opts::throw("ERR_SERVER_NOT_RUNNING", "Server is not running");
        return this;
    };
    let was_listening = st.listening.load(Ordering::Acquire);
    if cb != 0 {
        st.push(ServerEvent::Callback {
            cb,
            err: (!was_listening)
                .then(|| ("ERR_SERVER_NOT_RUNNING".to_string(), "Server is not running".to_string())),
        });
    }
    if !was_listening {
        super::pump::ensure_registered();
        return this;
    }
    st.closed.store(true, Ordering::Release);
    st.listening.store(false, Ordering::Release);
    // Dropping the listener unblocks the accept thread.
    *st.listener.lock().unwrap() = None;
    *st.bound.lock().unwrap() = None;
    keep_alive(&st, false);
    st.push(ServerEvent::Close);
    super::pump::ensure_registered();
    this
}

/// `server.close()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SERVER_CLOSE0(this: u64) -> u64 {
    close_impl(this, 0)
}

/// `server.close(callback)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SERVER_CLOSE1(this: u64, cb: u64) -> u64 {
    close_impl(this, val(cb).as_func().unwrap_or(0))
}

/// Release a closed server: its listeners, its object pin and its state. Called
/// by the pump once `'close'` has been delivered.
pub fn finalize(this: u64) {
    if let Some(st) = state::remove_server(this) {
        let thread = st.accept_thread.lock().unwrap().take();
        if let Some(t) = thread {
            let _ = t.join();
        }
        crate::emitter::release_all(this);
        unsafe { __RTS_FN_NS_GC_UNPIN_HANDLE(this) };
    }
}

/// `server.address()` — `null` before `'listening'` and after `close()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SERVER_ADDRESS(this: u64) -> u64 {
    let Some(st) = state::server(this) else { return 0 };
    match *st.bound.lock().unwrap() {
        // A 0 handle boxes as null on a nullable `): object` return.
        None => 0,
        Some(addr) => super::super::address_info(&addr),
    }
}

/// `server.getConnections(callback)` — Node documents it as asynchronous, so the
/// count is read now and the callback fires on a later turn.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SERVER_GET_CONNECTIONS(this: u64, cb: u64) -> u64 {
    let Some(st) = state::server(this) else { return this };
    if let Val::Func(cb) = val(cb) {
        st.push(ServerEvent::Connections { cb, count: st.connections.load(Ordering::Acquire) });
        super::pump::ensure_registered();
    }
    this
}

/// `server.listening`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SERVER_LISTENING(this: u64) -> i64 {
    state::server(this)
        .map(|st| i64::from(st.listening.load(Ordering::Acquire)))
        .unwrap_or(0)
}





/// `server.ref()` — chainable.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SERVER_REF(this: u64) -> u64 {
    if let Some(st) = state::server(this) {
        st.refd.store(true, Ordering::Release);
        keep_alive(&st, true);
    }
    this
}

/// `server.unref()` — chainable.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SERVER_UNREF(this: u64) -> u64 {
    if let Some(st) = state::server(this) {
        st.refd.store(false, Ordering::Release);
        keep_alive(&st, false);
    }
    this
}

/// A user `server.emit(event, ...args)` — queued through the same path as the
/// accept thread's events, so ordering holds.
fn emit_words(this: u64, ep: *const u8, el: i64, args: Vec<u64>) -> i64 {
    let Some(st) = state::server(this) else { return 0 };
    let event = crate::values::read(ep, el);
    let had = crate::emitter::has(this, &event);
    for &a in &args {
        crate::emitter::pin_word(a);
    }
    st.push(ServerEvent::Custom(event, args));
    super::pump::ensure_registered();
    i64::from(had)
}

/// `server.emit(event)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SERVER_EMIT0(this: u64, ep: *const u8, el: i64) -> i64 {
    emit_words(this, ep, el, Vec::new())
}

/// `server.emit(event, a0)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SERVER_EMIT1(this: u64, ep: *const u8, el: i64, a0: u64) -> i64 {
    emit_words(this, ep, el, vec![a0])
}

/// Read a `blockList` option off an options object — shared with `Socket`.
pub fn block_list_of(options: u64) -> Result<Option<Vec<super::super::blocklist::rules::Rule>>, ()> {
    let Some(word) = opt_word(options, "blockList") else {
        return Ok(None);
    };
    if val(word).is_nullish() {
        return Ok(None);
    }
    match val(word) {
        Val::Obj(h) => match crate::net::blocklist::rules_of(h) {
            Some(rules) => Ok(Some(rules)),
            None => {
                opts::throw(
                    "ERR_INVALID_ARG_TYPE",
                    "The \"options.blockList\" property must be an instance of net.BlockList",
                );
                Err(())
            }
        },
        _ => {
            opts::throw(
                "ERR_INVALID_ARG_TYPE",
                "The \"options.blockList\" property must be an instance of net.BlockList",
            );
            Err(())
        }
    }
}
