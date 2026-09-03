//! Socket options that are not multicast: `setBroadcast`, `setTTL`, the
//! send/recv buffer size pair, the always-`0` send-queue readers, and
//! `ref`/`unref`.

use rts_core::entry;

use super::bufsize;
use super::common::{socket_id, with_socket};

/// `socket.setBroadcast(flag)`.
pub(super) extern "C" fn set_broadcast(_e: u64, this: u64, flag: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    with_socket(this, |socket| socket.set_broadcast(entry::to_boolean(flag)));
    entry::undefined_value()
}

/// `socket.setTTL(ttl)`.
pub(super) extern "C" fn set_ttl(_e: u64, this: u64, ttl: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let ttl = entry::number_of(ttl).unwrap_or(64.0) as u32;
    with_socket(this, |socket| socket.set_ttl(ttl));
    entry::undefined_value()
}

/// `socket.getRecvBufferSize()` — the real `SO_RCVBUF`, read back from the OS
/// (which commonly reports more than was ever set — its own bookkeeping, not
/// a bug here). `undefined` on an unbound socket or a failed syscall, Node's
/// own answer for "no socket to ask".
pub(super) extern "C" fn get_recv_buffer_size(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    buffer_size_of(this, bufsize::Which::Recv)
}

/// `socket.setRecvBufferSize(size)`.
pub(super) extern "C" fn set_recv_buffer_size(_e: u64, this: u64, size: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    set_buffer_size(this, bufsize::Which::Recv, size);
    entry::undefined_value()
}

/// `socket.getSendBufferSize()`.
pub(super) extern "C" fn get_send_buffer_size(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    buffer_size_of(this, bufsize::Which::Send)
}

/// `socket.setSendBufferSize(size)`.
pub(super) extern "C" fn set_send_buffer_size(_e: u64, this: u64, size: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    set_buffer_size(this, bufsize::Which::Send, size);
    entry::undefined_value()
}

fn buffer_size_of(this: u64, which: bufsize::Which) -> u64 {
    let Some(id) = socket_id(this) else { return entry::undefined_value() };
    let read = super::registry::with_sockets(|table| {
        table.get(&id).and_then(|entry| entry.socket.as_ref()).and_then(|socket| bufsize::get(socket, which))
    });
    match read {
        Some(size) => entry::make_number(f64::from(size)),
        None => entry::undefined_value(),
    }
}

fn set_buffer_size(this: u64, which: bufsize::Which, size: u64) {
    let Some(size) = entry::number_of(size) else { return };
    with_socket(this, |socket| {
        bufsize::set(socket, which, size as i32);
        Ok(())
    });
}

/// `socket.getSendQueueSize()` — always `0`; see the module doc for why that
/// is the real answer rather than an approximation of one.
pub(super) extern "C" fn get_send_queue_size(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::make_number(0.0)
}

/// `socket.getSendQueueCount()` — same reason as [`get_send_queue_size`].
pub(super) extern "C" fn get_send_queue_count(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::make_number(0.0)
}

/// `socket.ref()`/`socket.unref()` — see the module doc: recorded nowhere,
/// no event-loop keep-alive to affect; chainable, matching Node's return
/// type.
pub(super) extern "C" fn ref_unref(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    this
}
