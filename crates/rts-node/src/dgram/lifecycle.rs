//! node:dgram — socket lifecycle: `bind`, `close`, `connect`, `disconnect`, and
//! the address getters.
//!
//! Node's `bind([port][, address][, callback])` is overloaded by TYPE at every
//! arity, and the Registry keys overloads by arity, so each arity is ONE member
//! taking `PolyValue`s that [`args`] normalizes exactly as Node's JS does.
//!
//! Node's contract: binding is asynchronous — `'listening'`/the callback fire
//! after the call returns, never inline. The syscall runs here, the event is
//! QUEUED, and `pump.rs` delivers it on a later event-loop turn.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use socket2::SockAddr;

use super::emitter;
use super::errors;
use super::pump;
use super::reader;
use super::state::{self, SockEvent, SocketState};
use crate::values::{opt_has, opt_num, opt_str, val, Val};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_UNPIN_HANDLE(handle: u64);
}

/// The `bind` arguments after Node's normalization.
#[derive(Default)]
struct BindArgs {
    port: Option<u16>,
    address: Option<String>,
    cb: u64,
    bad: Option<(&'static str, String)>,
}

/// Normalize `bind`'s polymorphic argument list: each slot may be a port
/// number, an address string, a `BindOptions` object, or the callback.
fn bind_args(words: &[u64]) -> BindArgs {
    let mut out = BindArgs::default();
    for &w in words {
        match val(w) {
            Val::Num(n) => match port_of(n) {
                Ok(p) => out.port = Some(p),
                Err(msg) => out.bad = Some((errors::BAD_PORT, msg)),
            },
            Val::Str(s) => out.address = Some(s),
            Val::Func(cb) => out.cb = cb,
            Val::Obj(o) => read_bind_options(o, &mut out),
            _ => {}
        }
    }
    out
}

fn read_bind_options(o: u64, out: &mut BindArgs) {
    if let Some(n) = opt_num(o, "port") {
        match port_of(n) {
            Ok(p) => out.port = Some(p),
            Err(msg) => out.bad = Some((errors::BAD_PORT, msg)),
        }
    }
    if let Some(a) = opt_str(o, "address") {
        out.address = Some(a);
    }
    // `exclusive` selects whether CLUSTER WORKERS share the underlying handle.
    // A single-process runtime never shares one, so both values already behave
    // exactly as Node does here — nothing to apply, nothing being ignored.
    if opt_has(o, "fd") {
        out.bad = Some((
            "ERR_INVALID_ARG_VALUE",
            "bind: the 'fd' option is not implemented yet in RTS".to_string(),
        ));
    }
}

/// Node's port validation: `ERR_SOCKET_BAD_PORT` outside `[0, 65535]`.
fn port_of(n: f64) -> Result<u16, String> {
    if !n.is_finite() || n.fract() != 0.0 || !(0.0..=65535.0).contains(&n) {
        return Err(format!("Port should be >= 0 and < 65536. Received {n}."));
    }
    Ok(n as u16)
}

/// The default bind address for the socket's family: all interfaces.
fn any_addr(v6: bool) -> IpAddr {
    if v6 {
        IpAddr::V6(Ipv6Addr::UNSPECIFIED)
    } else {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    }
}

/// Resolve an address argument for THIS socket's family. A literal IP is used as
/// is; a hostname goes through the system resolver (`getaddrinfo`), the first
/// matching-family answer wins — the same choice `dns.lookup` makes.
pub fn resolve(host: &str, port: u16, v6: bool) -> std::io::Result<SocketAddr> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(SocketAddr::new(ip, port));
    }
    use std::net::ToSocketAddrs;
    let want_v6 = v6;
    (host, port)
        .to_socket_addrs()?
        .find(|a| a.is_ipv6() == want_v6)
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::AddrNotAvailable,
                format!("getaddrinfo ENOTFOUND {host}"),
            )
        })
}

/// The socket behind a `this` handle, if it is OPEN. Node raises
/// `ERR_SOCKET_DGRAM_NOT_RUNNING` for any operation on a closed socket — and a
/// finalized one is gone from the table entirely, which is the same answer.
pub fn open(this: u64) -> Option<Arc<SocketState>> {
    match state::get(this) {
        Some(st) if !st.is_closed() => Some(st),
        _ => {
            errors::throw_not_running();
            None
        }
    }
}

/// The shared `bind` implementation for every arity.
fn bind_impl(this: u64, words: &[u64]) {
    let Some(st) = open(this) else { return };
    let args = bind_args(words);
    if let Some((code, msg)) = args.bad {
        errors::throw(code, &msg);
        return;
    }
    if st.is_bound() {
        // Node: re-binding an already-bound socket is an ERR_SOCKET_ALREADY_BOUND.
        errors::throw("ERR_SOCKET_ALREADY_BOUND", "Socket is already bound");
        return;
    }
    if args.cb != 0 {
        emitter::add(this, "listening", args.cb, true, false);
    }

    let port = args.port.unwrap_or(0);
    let ip = match args.address.as_deref() {
        Some(a) => match resolve(a, port, st.v6) {
            Ok(sa) => sa.ip(),
            Err(e) => {
                let (code, msg) = errors::message_for(&e, "bind");
                st.push(SockEvent::Error(code, msg));
                return;
            }
        },
        None => any_addr(st.v6),
    };
    let addr = SockAddr::from(SocketAddr::new(ip, port));
    if let Err(e) = st.sock.bind(&addr) {
        // Node reports bind failures asynchronously, as an 'error' event.
        let (code, msg) = errors::message_for(&e, "bind");
        st.push(SockEvent::Error(code, msg));
        return;
    }
    st.bound.store(true, Ordering::Release);
    reader::start(this, &st);
    st.push(SockEvent::Listening);
    pump::ensure_registered();
    keep_alive(&st, true);
}

/// Bind implicitly (a `send`/membership call on an unbound socket), on a random
/// port + all interfaces — Node's auto-bind. Returns false if the bind failed.
pub fn ensure_bound(this: u64, st: &Arc<SocketState>) -> std::io::Result<()> {
    if st.is_bound() {
        return Ok(());
    }
    let addr = SockAddr::from(SocketAddr::new(any_addr(st.v6), 0));
    st.sock.bind(&addr)?;
    st.bound.store(true, Ordering::Release);
    reader::start(this, st);
    st.push(SockEvent::Listening);
    pump::ensure_registered();
    keep_alive(st, true);
    Ok(())
}

/// Add/remove this socket's keep-alive on the event loop. Idempotent: `counted`
/// tracks whether the count is currently held, so ref/unref/close stay balanced.
pub fn keep_alive(st: &SocketState, want: bool) {
    let want = want && st.refd.load(Ordering::Acquire) && !st.is_closed() && st.is_bound();
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

/// `socket.bind()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_BIND0(this: u64) {
    bind_impl(this, &[]);
}

/// `socket.bind(portOrOptionsOrCallback)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_BIND1(this: u64, a0: u64) {
    bind_impl(this, &[a0]);
}

/// `socket.bind(port, addressOrCallback)` / `bind(options, callback)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_BIND2(this: u64, a0: u64, a1: u64) {
    bind_impl(this, &[a0, a1]);
}

/// `socket.bind(port, address, callback)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_BIND3(this: u64, a0: u64, a1: u64, a2: u64) {
    bind_impl(this, &[a0, a1, a2]);
}

/// The shared `close` implementation. Stops the reader thread, releases the
/// keep-alive, and queues `'close'` (delivered on a later turn, per Node).
fn close_impl(this: u64, cb: u64) {
    // Node: closing an already-closed socket raises ERR_SOCKET_DGRAM_NOT_RUNNING.
    let Some(st) = open(this) else { return };
    if cb != 0 {
        emitter::add(this, "close", cb, true, false);
    }
    st.closed.store(true, Ordering::Release);
    reader::stop(&st);
    keep_alive(&st, false);
    st.push(SockEvent::Close);
    // The pump releases the state + the object pin once 'close' is delivered —
    // the listeners must still be reachable for that delivery.
    pump::ensure_registered();
}

/// `socket.close()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_CLOSE0(this: u64) {
    close_impl(this, 0);
}

/// `socket.close(callback)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_CLOSE1(this: u64, cb: u64) {
    let cb = val(cb).as_func().unwrap_or(0);
    close_impl(this, cb);
}

/// Drop a closed socket's state: unpin its listeners and its object, and forget
/// it. Called by the pump after `'close'` has been delivered.
pub fn finalize(this: u64) {
    if let Some(st) = state::remove(this) {
        emitter::release_all(&st);
        unsafe { __RTS_FN_NS_GC_UNPIN_HANDLE(this) };
    }
}

/// The shared `connect` implementation.
fn connect_impl(this: u64, words: &[u64]) {
    let Some(st) = open(this) else { return };
    if st.peer_addr().is_some() {
        errors::throw(errors::IS_CONNECTED, "Already connected");
        return;
    }
    let (mut port, mut address, mut cb) = (None, None, 0u64);
    for &w in words {
        match val(w) {
            Val::Num(n) => match port_of(n) {
                Ok(p) => port = Some(p),
                Err(msg) => {
                    errors::throw(errors::BAD_PORT, &msg);
                    return;
                }
            },
            Val::Str(s) => address = Some(s),
            Val::Func(f) => cb = f,
            _ => {}
        }
    }
    let Some(port) = port else {
        errors::throw(errors::BAD_PORT, "Port should be >= 0 and < 65536. Received undefined.");
        return;
    };
    if cb != 0 {
        emitter::add(this, "connect", cb, true, false);
    }
    // An unbound socket auto-binds before connecting, like Node.
    if let Err(e) = ensure_bound(this, &st) {
        let (code, msg) = errors::message_for(&e, "bind");
        st.push_err(cb, true, &code, &msg);
        return;
    }
    let host = address.unwrap_or_else(|| default_peer(st.v6).to_string());
    let target = match resolve(&host, port, st.v6) {
        Ok(a) => a,
        Err(e) => {
            let (code, msg) = errors::message_for(&e, "connect");
            st.push_err(cb, true, &code, &msg);
            return;
        }
    };
    if let Err(e) = st.sock.connect(&SockAddr::from(target)) {
        let (code, msg) = errors::message_for(&e, "connect");
        st.push_err(cb, true, &code, &msg);
        return;
    }
    *st.peer.lock().unwrap() = Some(target);
    st.push(SockEvent::Connect);
    pump::ensure_registered();
}

/// Node's default peer address when `send`/`connect` omits one.
pub fn default_peer(v6: bool) -> IpAddr {
    if v6 {
        IpAddr::V6(Ipv6Addr::LOCALHOST)
    } else {
        IpAddr::V4(Ipv4Addr::LOCALHOST)
    }
}

/// `socket.connect(port)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_CONNECT1(this: u64, a0: u64) {
    connect_impl(this, &[a0]);
}

/// `socket.connect(port, addressOrCallback)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_CONNECT2(this: u64, a0: u64, a1: u64) {
    connect_impl(this, &[a0, a1]);
}

/// `socket.connect(port, address, callback)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_CONNECT3(this: u64, a0: u64, a1: u64, a2: u64) {
    connect_impl(this, &[a0, a1, a2]);
}

/// `socket.disconnect()` — back to an unconnected (but still bound) socket.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_DISCONNECT(this: u64) {
    let Some(st) = open(this) else { return };
    if st.peer_addr().is_none() {
        errors::throw(errors::NOT_CONNECTED, "Not connected");
        return;
    }
    // Dissolving a UDP association is `connect()` to an AF_UNSPEC address.
    match disconnect_os(&st) {
        Ok(()) => *st.peer.lock().unwrap() = None,
        Err(e) => errors::throw_io(&e, "disconnect"),
    }
}

/// `connect(AF_UNSPEC)` — the portable way to dissolve a UDP association.
fn disconnect_os(st: &SocketState) -> std::io::Result<()> {
    // `AF_UNSPEC` is family 0, so an all-zero `sockaddr_storage` IS the address.
    let storage = socket2::SockAddrStorage::zeroed();
    let len = storage.size_of();
    // SAFETY: a zeroed storage of the full `sockaddr_storage` length is a valid
    // AF_UNSPEC address — the one `connect(2)` reads to drop the association.
    let unspec = unsafe { SockAddr::new(storage, len) };
    match st.sock.connect(&unspec) {
        Ok(()) => Ok(()),
        // A successful AF_UNSPEC disconnect still reports EAFNOSUPPORT on Linux
        // (97), macOS (47) and Windows (WSAEAFNOSUPPORT 10047) — the association
        // IS dissolved, so this is the success path, not an error.
        Err(e) if errors::code_for(&e) == "EAFNOSUPPORT" => Ok(()),
        Err(e) => Err(e),
    }
}

/// `"IPv4"`/`"IPv6"` — Node reports `family` as a string (it was briefly a
/// number in v18.0.0 and reverted in v18.4.0; only the string form exists here).
fn family_of(addr: &SocketAddr) -> &'static str {
    if addr.is_ipv6() {
        "IPv6"
    } else {
        "IPv4"
    }
}

/// `{ address, family, port }` — the shape `address()`/`remoteAddress()` return.
pub fn address_info(addr: &SocketAddr) -> u64 {
    use rts_engine::heap::shapes::{alloc_shaped_object, string_word};
    alloc_shaped_object(
        &["address", "family", "port"],
        &[
            string_word(addr.ip().to_string().as_bytes()) as i64,
            string_word(family_of(addr).as_bytes()) as i64,
            f64::from(addr.port()).to_bits() as i64,
        ],
    )
}

/// `socket.address()` — throws `EBADF` on an unbound socket.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_ADDRESS(this: u64) -> u64 {
    let Some(st) = open(this) else { return 0 };
    if !st.is_bound() {
        errors::throw_unbound();
        return 0;
    }
    match st.sock.local_addr().ok().and_then(|a| a.as_socket()) {
        Some(addr) => address_info(&addr),
        None => {
            errors::throw_unbound();
            0
        }
    }
}

/// `socket.remoteAddress()` — throws `ERR_SOCKET_DGRAM_NOT_CONNECTED` unless
/// `connect()`ed.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_REMOTE_ADDRESS(this: u64) -> u64 {
    let Some(st) = open(this) else { return 0 };
    match st.peer_addr() {
        Some(addr) => address_info(&addr),
        None => {
            errors::throw(errors::NOT_CONNECTED, "Not connected");
            0
        }
    }
}

/// `socket.ref()` — restore the default "an open socket keeps the process
/// alive" accounting. Chainable.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_REF(this: u64) -> u64 {
    if let Some(st) = state::get(this) {
        st.refd.store(true, Ordering::Release);
        keep_alive(&st, true);
    }
    this
}

/// `socket.unref()` — exclude this socket from the loop's keep-alive
/// accounting, so the process may exit with it still open. Chainable.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_UNREF(this: u64) -> u64 {
    if let Some(st) = state::get(this) {
        st.refd.store(false, Ordering::Release);
        keep_alive(&st, false);
    }
    this
}
