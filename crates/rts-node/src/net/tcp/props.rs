//! node:net — the `Socket`'s tuning setters and its read-only properties.
//!
//! Every getter here reports what the OS/state actually holds — none of them
//! fabricates a value. Where Node's property is `undefined` (a not-yet-connected
//! socket has no `remoteAddress`), the accessor returns the empty string / -1,
//! which the member's nullable return boxes back to `null`.

use std::sync::atomic::Ordering;
use std::time::Duration;

use super::opts;
use super::state;
use crate::values::{intern, read, string_array};

/// `socket.setNoDelay([noDelay])` — Nagle off (Node's default arg is `true`).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_SET_NO_DELAY(this: u64, on: i64) -> u64 {
    if let Some(st) = state::socket(this) {
        st.opts.lock().unwrap().no_delay = on != 0;
        if let Some(s) = st.clone_stream() {
            let _ = s.set_nodelay(on != 0);
        }
    }
    this
}

/// `socket.setKeepAlive([enable][, initialDelay])`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_SET_KEEP_ALIVE(this: u64, enable: i64, delay_ms: f64) -> u64 {
    let Some(st) = state::socket(this) else { return this };
    {
        let mut o = st.opts.lock().unwrap();
        o.keep_alive = enable != 0;
        o.keep_alive_initial_delay = delay_ms.max(0.0) as u64;
    }
    if let Some(s) = st.clone_stream() {
        let sock = socket2::SockRef::from(&s);
        if enable != 0 {
            let mut ka = socket2::TcpKeepalive::new();
            if delay_ms > 0.0 {
                ka = ka.with_time(Duration::from_millis(delay_ms as u64));
            }
            let _ = sock.set_tcp_keepalive(&ka);
        } else {
            let _ = sock.set_keepalive(false);
        }
    }
    this
}

/// `socket.setTimeout(timeout[, callback])` — 0 disables. The idle timer lives in
/// the read loop; `'timeout'` does NOT destroy the socket.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_SET_TIMEOUT(this: u64, ms: f64) -> u64 {
    if let Some(st) = state::socket(this) {
        st.timeout_ms.store(ms.max(0.0) as i64, Ordering::Release);
    }
    this
}

/// `socket.timeout` — `-1` where Node reports `undefined`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_GET_TIMEOUT(this: u64) -> f64 {
    state::socket(this)
        .map(|st| match st.timeout_ms.load(Ordering::Acquire) {
            0 => -1.0,
            t => t as f64,
        })
        .unwrap_or(-1.0)
}

/// `socket.setEncoding([encoding])` — `'data'` then delivers strings.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_SET_ENCODING(this: u64, p: *const u8, l: i64) -> u64 {
    if let Some(st) = state::socket(this) {
        let enc = read(p, l);
        *st.encoding.lock().unwrap() = (!enc.is_empty()).then_some(enc);
    }
    this
}

/// `socket.pause()` — the read loop parks; no `'data'` until `resume()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_PAUSE(this: u64) -> u64 {
    if let Some(st) = state::socket(this) {
        st.paused.store(true, Ordering::Release);
    }
    this
}

/// `socket.resume()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_RESUME(this: u64) -> u64 {
    if let Some(st) = state::socket(this) {
        st.paused.store(false, Ordering::Release);
    }
    this
}

/// `socket.ref()` — chainable.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_REF(this: u64) -> u64 {
    if let Some(st) = state::socket(this) {
        st.refd.store(true, Ordering::Release);
        super::socket::keep_alive(&st, true);
    }
    this
}

/// `socket.unref()` — chainable.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_UNREF(this: u64) -> u64 {
    if let Some(st) = state::socket(this) {
        st.refd.store(false, Ordering::Release);
        super::socket::keep_alive(&st, false);
    }
    this
}

/// `IPV6_TCLASS` — the IPv6 twin of IP_TOS. `socket2` exposes only the v4 side,
/// and the option number is per-OS ABI (`<linux/in6.h>` vs the BSD/Darwin
/// `<netinet/in.h>`). Windows has no `IPV6_TCLASS` at all (it routes QoS through
/// its own API), so there the caller gets a real "not supported" error rather
/// than a silently-ignored setter.
#[cfg(any(target_os = "linux", target_os = "android"))]
const IPV6_TCLASS: std::os::raw::c_int = 67;
#[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd", target_os = "dragonfly"))]
const IPV6_TCLASS: std::os::raw::c_int = 36;

#[cfg(unix)]
fn tclass_v6(stream: &std::net::TcpStream) -> std::io::Result<u32> {
    use std::os::fd::AsRawFd;
    let mut value: libc::c_int = 0;
    let mut len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
    // SAFETY: `value`/`len` describe a live c_int for the duration of the call.
    let rc = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            41, // IPPROTO_IPV6
            IPV6_TCLASS,
            (&raw mut value).cast(),
            &raw mut len,
        )
    };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(value as u32)
}

#[cfg(unix)]
fn set_tclass_v6(stream: &std::net::TcpStream, tos: u32) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    let value = tos as libc::c_int;
    // SAFETY: as above.
    let rc = unsafe {
        libc::setsockopt(
            stream.as_raw_fd(),
            41, // IPPROTO_IPV6
            IPV6_TCLASS,
            (&raw const value).cast(),
            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
        )
    };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(unix))]
fn tclass_v6(_stream: &std::net::TcpStream) -> std::io::Result<u32> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "IPV6_TCLASS is not available on this platform",
    ))
}

#[cfg(not(unix))]
fn set_tclass_v6(_stream: &std::net::TcpStream, _tos: u32) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "IPV6_TCLASS is not available on this platform",
    ))
}

/// `socket.getTypeOfService()` — the IP TOS byte (IPv6: the Traffic Class).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_GET_TOS(this: u64) -> f64 {
    let Some(st) = state::socket(this) else {
        opts::throw("ERR_SOCKET_CLOSED", "Socket is closed");
        return 0.0;
    };
    let Some(s) = st.clone_stream() else {
        opts::throw("ERR_SOCKET_CLOSED", "Socket is closed");
        return 0.0;
    };
    let got = if st.peer_addr().is_some_and(|a| a.is_ipv6()) {
        tclass_v6(&s)
    } else {
        socket2::SockRef::from(&s).tos_v4()
    };
    match got {
        Ok(v) => v as f64,
        Err(e) => {
            let (code, msg) = opts::message_for(&e, "getTypeOfService");
            opts::throw(&code, &msg);
            0.0
        }
    }
}

/// `socket.setTypeOfService(tos)` — 0..255.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_SET_TOS(this: u64, tos: f64) -> u64 {
    let Some(st) = state::socket(this) else {
        opts::throw("ERR_SOCKET_CLOSED", "Socket is closed");
        return this;
    };
    if !tos.is_finite() || tos.fract() != 0.0 || !(0.0..=255.0).contains(&tos) {
        opts::throw(
            "ERR_OUT_OF_RANGE",
            &format!("The value of \"tos\" is out of range. It must be >= 0 && <= 255. Received {tos}"),
        );
        return this;
    }
    let Some(s) = st.clone_stream() else {
        opts::throw("ERR_SOCKET_CLOSED", "Socket is closed");
        return this;
    };
    let set = if st.peer_addr().is_some_and(|a| a.is_ipv6()) {
        set_tclass_v6(&s, tos as u32)
    } else {
        socket2::SockRef::from(&s).set_tos_v4(tos as u32)
    };
    if let Err(e) = set {
        let (code, msg) = opts::message_for(&e, "setTypeOfService");
        opts::throw(&code, &msg);
    }
    this
}

/// `socket.address()` — `{}` before the socket is connected/bound.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_ADDRESS(this: u64) -> u64 {
    let Some(st) = state::socket(this) else { return 0 };
    match *st.local.lock().unwrap() {
        Some(addr) => super::super::address_info(&addr),
        None => 0,
    }
}

/// An empty string where Node reports `undefined` — the member's nullable
/// return boxes it back to `null`.
fn str_or_empty(s: Option<String>) -> u64 {
    match s {
        Some(s) => intern(&s),
        None => 0,
    }
}

/// `socket.remoteAddress`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_REMOTE_ADDRESS(this: u64) -> u64 {
    str_or_empty(state::socket(this).and_then(|st| st.peer_addr()).map(|a| a.ip().to_string()))
}

/// `socket.remoteFamily`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_REMOTE_FAMILY(this: u64) -> u64 {
    str_or_empty(
        state::socket(this)
            .and_then(|st| st.peer_addr())
            .map(|a| super::super::family_name(&a).to_string()),
    )
}

/// `socket.remotePort` — `-1` where Node reports `undefined`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_REMOTE_PORT(this: u64) -> f64 {
    state::socket(this)
        .and_then(|st| st.peer_addr())
        .map(|a| f64::from(a.port()))
        .unwrap_or(-1.0)
}

/// `socket.localAddress`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_LOCAL_ADDRESS(this: u64) -> u64 {
    str_or_empty(
        state::socket(this)
            .and_then(|st| *st.local.lock().unwrap())
            .map(|a| a.ip().to_string()),
    )
}

/// `socket.localFamily`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_LOCAL_FAMILY(this: u64) -> u64 {
    str_or_empty(
        state::socket(this)
            .and_then(|st| *st.local.lock().unwrap())
            .map(|a| super::super::family_name(&a).to_string()),
    )
}

/// `socket.localPort`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_LOCAL_PORT(this: u64) -> f64 {
    state::socket(this)
        .and_then(|st| *st.local.lock().unwrap())
        .map(|a| f64::from(a.port()))
        .unwrap_or(-1.0)
}

/// `socket.bytesRead`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_BYTES_READ(this: u64) -> f64 {
    state::socket(this)
        .map(|st| st.bytes_read.load(Ordering::Acquire) as f64)
        .unwrap_or(0.0)
}

/// `socket.bytesWritten`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_BYTES_WRITTEN(this: u64) -> f64 {
    state::socket(this)
        .map(|st| st.bytes_written.load(Ordering::Acquire) as f64)
        .unwrap_or(0.0)
}

/// `socket.bufferSize` — DEPRECATED since v14.6.0 in favour of
/// `writable.writableLength`; Node itself documents it as an approximation, and
/// this is the same number: the bytes handed to `write()` the OS has not taken.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_BUFFER_SIZE(this: u64) -> f64 {
    state::socket(this)
        .map(|st| st.buffered.load(Ordering::Acquire) as f64)
        .unwrap_or(0.0)
}

/// `socket.connecting`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_CONNECTING(this: u64) -> i64 {
    state::socket(this)
        .map(|st| i64::from(st.connecting.load(Ordering::Acquire)))
        .unwrap_or(0)
}

/// `socket.pending` — created but not yet connecting/connected.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_PENDING(this: u64) -> i64 {
    state::socket(this)
        .map(|st| {
            i64::from(st.peer_addr().is_none() && !st.connecting.load(Ordering::Acquire))
        })
        .unwrap_or(0)
}

/// `socket.destroyed` — a finalized socket is gone from the table, which is the
/// same answer.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_DESTROYED(this: u64) -> i64 {
    state::socket(this).map(|st| i64::from(st.is_destroyed())).unwrap_or(1)
}

/// `socket.readyState` — derived, never stored.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_READY_STATE(this: u64) -> u64 {
    let s = state::socket(this).map(|st| st.ready_state()).unwrap_or("closed");
    intern(s)
}

/// `socket.autoSelectFamilyAttemptedAddresses` — the `"$IP:$PORT"` list the
/// racer actually tried (empty when it never ran).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_ATTEMPTED(this: u64) -> u64 {
    let list = state::socket(this)
        .map(|st| st.attempted.lock().unwrap().clone())
        .unwrap_or_default();
    string_array(&list)
}
