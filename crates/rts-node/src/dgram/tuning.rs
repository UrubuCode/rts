//! node:dgram — socket tuning: TTL / multicast TTL / multicast loopback /
//! broadcast, the send+receive buffer sizes, and the send-queue counters.
//!
//! Node's rules (dgram.md §4): these all need a BOUND socket. The TTL/broadcast/
//! loopback family throws `EBADF` when unbound; the buffer-size family throws
//! Node's own `ERR_SOCKET_BUFFER_SIZE` instead.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use super::errors;
use super::lifecycle::open;
use super::state::{self, SocketState};

/// A bound, open socket — or the throw Node documents for the op's family.
fn tunable(this: u64, buffer_family: bool) -> Option<Arc<SocketState>> {
    let st = open(this)?;
    if !st.is_bound() {
        if buffer_family {
            errors::throw(errors::BUFFER_SIZE, "Could not get or set buffer size");
        } else {
            errors::throw_unbound();
        }
        return None;
    }
    Some(st)
}

/// Node clamps nothing itself — the OS validates the range and reports EINVAL.
fn apply(result: std::io::Result<()>, op: &str) {
    if let Err(e) = result {
        errors::throw_io(&e, op);
    }
}

/// `socket.setTTL(ttl)` — the unicast hop limit (1–255).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_SET_TTL(this: u64, ttl: i64) {
    let Some(st) = tunable(this, false) else { return };
    let hops = ttl.clamp(0, u32::MAX as i64) as u32;
    let result = if st.v6 {
        st.sock.set_unicast_hops_v6(hops)
    } else {
        st.sock.set_ttl_v4(hops)
    };
    apply(result, "setTTL");
}

/// `socket.setMulticastTTL(ttl)` — the multicast hop limit (0–255).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_SET_MULTICAST_TTL(this: u64, ttl: i64) {
    let Some(st) = tunable(this, false) else { return };
    let hops = ttl.clamp(0, u32::MAX as i64) as u32;
    let result = if st.v6 {
        st.sock.set_multicast_hops_v6(hops)
    } else {
        st.sock.set_multicast_ttl_v4(hops)
    };
    apply(result, "setMulticastTTL");
}

/// `socket.setMulticastLoopback(flag)` — whether this host receives its own
/// multicast.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_SET_MULTICAST_LOOPBACK(this: u64, flag: i64) {
    let Some(st) = tunable(this, false) else { return };
    let on = flag != 0;
    let result = if st.v6 {
        st.sock.set_multicast_loop_v6(on)
    } else {
        st.sock.set_multicast_loop_v4(on)
    };
    apply(result, "setMulticastLoopback");
}

/// `socket.setBroadcast(flag)` — SO_BROADCAST.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_SET_BROADCAST(this: u64, flag: i64) {
    let Some(st) = tunable(this, false) else { return };
    apply(st.sock.set_broadcast(flag != 0), "setBroadcast");
}

/// A buffer-size setter (`SO_SNDBUF`/`SO_RCVBUF`). Node reports every failure
/// here as `ERR_SOCKET_BUFFER_SIZE`, not as a raw errno.
fn set_buffer(this: u64, size: i64, send: bool, op: &str) {
    let Some(st) = tunable(this, true) else { return };
    if size < 0 {
        errors::throw(
            errors::BUFFER_SIZE,
            &format!("Could not set buffer size: invalid size {size}"),
        );
        return;
    }
    let size = size as usize;
    let result = if send {
        st.sock.set_send_buffer_size(size)
    } else {
        st.sock.set_recv_buffer_size(size)
    };
    if let Err(e) = result {
        errors::throw(errors::BUFFER_SIZE, &format!("Could not set buffer size: {e}, {op}"));
    }
}

/// A buffer-size getter. The OS may report more than was set (it usually
/// doubles the request for bookkeeping) — that real value is what is returned.
fn get_buffer(this: u64, send: bool, op: &str) -> i64 {
    let Some(st) = tunable(this, true) else { return 0 };
    let result = if send {
        st.sock.send_buffer_size()
    } else {
        st.sock.recv_buffer_size()
    };
    match result {
        Ok(size) => size as i64,
        Err(e) => {
            errors::throw(errors::BUFFER_SIZE, &format!("Could not get buffer size: {e}, {op}"));
            0
        }
    }
}

/// `socket.setSendBufferSize(size)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_SET_SEND_BUFFER_SIZE(this: u64, size: i64) {
    set_buffer(this, size, true, "setSendBufferSize");
}

/// `socket.setRecvBufferSize(size)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_SET_RECV_BUFFER_SIZE(this: u64, size: i64) {
    set_buffer(this, size, false, "setRecvBufferSize");
}

/// `socket.getSendBufferSize()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_GET_SEND_BUFFER_SIZE(this: u64) -> i64 {
    get_buffer(this, true, "getSendBufferSize")
}

/// `socket.getRecvBufferSize()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_GET_RECV_BUFFER_SIZE(this: u64) -> i64 {
    get_buffer(this, false, "getRecvBufferSize")
}

/// `socket.getSendQueueSize()` — bytes of this socket's sends that have been
/// dispatched but have not yet left. Unlike the setters this needs no bound
/// socket (in Node it reports libuv's own write queue).
///
/// RTS sends a literal-address datagram with an inline `sendto`, which returns
/// once the bytes are handed to the kernel, so such a send is never "queued";
/// what IS counted is a hostname-addressed send while its resolution is still in
/// flight (send.rs). The number is therefore real, not an invented constant —
/// though it will read differently from Node under heavy async backpressure
/// (dgram.md §7).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_GET_SEND_QUEUE_SIZE(this: u64) -> i64 {
    state::get(this)
        .map(|st| st.queue_bytes.load(Ordering::Acquire))
        .unwrap_or(0)
}

/// `socket.getSendQueueCount()` — how many such sends are outstanding. Same
/// accounting as [`__RTS_FN_NODE_DGRAM_GET_SEND_QUEUE_SIZE`].
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_GET_SEND_QUEUE_COUNT(this: u64) -> i64 {
    state::get(this)
        .map(|st| st.queue_count.load(Ordering::Acquire))
        .unwrap_or(0)
}
