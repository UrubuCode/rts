//! A socket's life cycle: `bind`, `connect`, `disconnect`, `close`,
//! `address`, `remoteAddress`.

use rts_core::entry;
use std::net::UdpSocket;

use super::common::{emit, get_bool, get_value, once, option_num, option_text, set_bool, set_num, socket_id};
use super::registry::{self, DgramEvent, SocketEntry};

/// `socket.bind(port?, address?, callback?)` / `bind(options, callback?)`.
///
/// Binding here is synchronous at the syscall level — only `'listening'`
/// itself is queued through [`registry::pump`], the same divergence
/// `net.md`'s own "landed" section names for TCP's `connect`/`accept`.
pub(super) extern "C" fn bind(_e: u64, this: u64, a: u64, b: u64, c: u64, _d: u64) -> u64 {
    registry::pump();
    let absent = entry::undefined_value();
    let (port, address, callback) = entry::with_runtime(|context| match entry::number_of(a) {
        // `string_in` and not `text_in`: the second is `ToString`, so an ABSENT
        // address arrived as the literal host name `"undefined"`, went to the
        // resolver, and came back as WSAHOST_NOT_FOUND — an `'error'` event
        // nothing handled, which ended the program. `bind(0)` is the common
        // spelling and it could not work.
        //
        // `modules::string_in` says so in its own doc: a coercion that can be
        // mistaken for a test will be.
        Some(port) => (
            port as u16,
            entry::string_in(context, b).unwrap_or_default(),
            c,
        ),
        None => {
            let port = option_num(context, a, "port").unwrap_or(0.0) as u16;
            let address = option_text(context, a, "address").unwrap_or_default();
            (port, address, b)
        }
    });
    let is_udp6 = get_bool(this, "__udp6");
    let bind_addr = if address.is_empty() {
        if is_udp6 { format!("[::]:{port}") } else { format!("0.0.0.0:{port}") }
    } else if is_udp6 {
        format!("[{address}]:{port}")
    } else {
        format!("{address}:{port}")
    };
    if callback != absent {
        once(this, "listening", callback);
    }
    match UdpSocket::bind(&bind_addr) {
        Ok(socket) => {
            let id = registry::next_id();
            let reader = socket.try_clone().ok();
            entry::with_runtime(|context| {
                set_num(context, this, "__socketId", id as f64);
                set_bool(context, this, "__bound", true);
            });
            registry::with_sockets(|table| {
                table.insert(
                    id,
                    SocketEntry { owner: std::thread::current().id(), instance: this, queue: Default::default(), socket: Some(socket), closed: false },
                );
                if let Some(entry) = table.get_mut(&id) {
                    entry.queue.push_back(DgramEvent::Listening);
                }
            });
            if let Some(reader) = reader {
                registry::spawn_reader(id, reader);
            }
            let (want_recv, want_send) = (
                entry::number_of(get_value(this, "__wantRecvBuf")),
                entry::number_of(get_value(this, "__wantSendBuf")),
            );
            if want_recv.is_some() || want_send.is_some() {
                registry::with_sockets(|table| {
                    if let Some(socket) = table.get(&id).and_then(|entry| entry.socket.as_ref()) {
                        if let Some(size) = want_recv {
                            super::bufsize::set(socket, super::bufsize::Which::Recv, size as i32);
                        }
                        if let Some(size) = want_send {
                            super::bufsize::set(socket, super::bufsize::Which::Send, size as i32);
                        }
                    }
                });
            }
        }
        Err(error) => {
            let id = registry::next_id();
            registry::with_sockets(|table| {
                table.insert(id, SocketEntry { owner: std::thread::current().id(), instance: this, queue: Default::default(), socket: None, closed: false });
                if let Some(entry) = table.get_mut(&id) {
                    entry.queue.push_back(DgramEvent::BindFailed(error.to_string()));
                }
            });
            entry::with_runtime(|context| set_num(context, this, "__socketId", id as f64));
        }
    }
    absent
}

/// `socket.connect(port, address?, callback?)` — narrows the socket to one
/// peer via `UdpSocket::connect`; unlike TCP this is a local-only OS call, no
/// background thread involved, so it completes before returning and needs no
/// registry entry of its own.
pub(super) extern "C" fn connect(_e: u64, this: u64, port: u64, address: u64, callback: u64, _d: u64) -> u64 {
    registry::pump();
    let absent = entry::undefined_value();
    // A second `connect()` on an already-connected socket — `UdpSocket::
    // connect` would happily re-narrow to a new peer, which is not Node's
    // answer: Node fixes a socket to ONE peer per its lifetime and refuses a
    // second attempt outright.
    if get_bool(this, "__connected") {
        crate::errors::socket_dgram_is_connected();
        return absent;
    }
    if !get_bool(this, "__bound") {
        bind(0, this, entry::make_number(0.0), absent, absent, 0);
    }
    let Some(id) = socket_id(this) else { return absent };
    let port = entry::number_of(port).unwrap_or(0.0) as u16;
    let host = entry::text_of(address).unwrap_or_else(|| "127.0.0.1".to_owned());
    let result = registry::with_sockets(|table| match table.get(&id).and_then(|entry| entry.socket.as_ref()) {
        Some(socket) => socket.connect((host.as_str(), port)).map_err(|error| error.to_string()),
        None => Err("socket not bound".to_owned()),
    });
    match result {
        Ok(()) => {
            entry::with_runtime(|context| set_bool(context, this, "__connected", true));
            if callback != absent {
                entry::call(callback, absent, absent, absent, absent, absent);
            }
            emit(this, "connect", absent, absent, absent);
        }
        Err(message) => {
            let error = entry::with_runtime(|context| {
                let object = entry::make_object(context);
                let message_v = entry::make_string(context, &message);
                entry::put_member(context, object, "message", message_v);
                object
            });
            if callback != absent {
                entry::call(callback, absent, error, absent, absent, absent);
            } else {
                emit(this, "error", error, absent, absent);
            }
        }
    }
    absent
}

/// `socket.disconnect()` — the inverse of [`connect`]; `UdpSocket` has no
/// "unconnect" call, so this reconnects to the OS-assigned wildcard, matching
/// what an unconnected socket accepts datagrams from.
pub(super) extern "C" fn disconnect(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    if !get_bool(this, "__connected") {
        crate::errors::socket_dgram_not_connected();
        return absent;
    }
    let Some(id) = socket_id(this) else { return absent };
    registry::with_sockets(|table| {
        if let Some(entry) = table.get(&id)
            && let Some(socket) = &entry.socket
        {
            let wildcard = if get_bool(this, "__udp6") { "[::]:0" } else { "0.0.0.0:0" };
            let _ = socket.connect(wildcard);
        }
    });
    entry::with_runtime(|context| set_bool(context, this, "__connected", false));
    absent
}

/// `socket.close(callback?)`. A socket never bound is a silent no-op (Node's
/// own answer for a handle that was never created); one already closed is
/// `ERR_SOCKET_DGRAM_NOT_RUNNING`, checked AFTER the callback is registered —
/// Node registers the listener before its own `handle === null` check, so a
/// caller of a doomed `close(cb)` still gets `cb` wired to the event.
pub(super) extern "C" fn close(_e: u64, this: u64, callback: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    if callback != absent {
        once(this, "close", callback);
    }
    let Some(id) = socket_id(this) else { return absent };
    let already_closed = registry::with_sockets(|table| table.get(&id).map(|entry| entry.closed).unwrap_or(true));
    if already_closed {
        crate::errors::socket_dgram_not_running();
        return absent;
    }
    registry::with_sockets(|table| {
        if let Some(entry) = table.get_mut(&id) {
            entry.closed = true;
            entry.queue.push_back(DgramEvent::Closed);
        }
    });
    registry::pump();
    absent
}

/// `socket.address()` — `EBADF` for a socket never bound (no entry to ask at
/// all), `ERR_SOCKET_DGRAM_NOT_RUNNING` for one already [`close`]d (an entry
/// that answers `closed`), distinguished because the two are different
/// mistakes with different Node codes.
pub(super) extern "C" fn address(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let Some(id) = socket_id(this) else {
        crate::errors::system_error("getsockname", "EBADF");
        return absent;
    };
    let found = registry::with_sockets(|table| {
        table.get(&id).map(|entry| (entry.closed, entry.socket.as_ref().and_then(|socket| socket.local_addr().ok())))
    });
    match found {
        Some((false, Some(addr))) => address_info(addr.ip().to_string(), addr.port(), get_bool(this, "__udp6")),
        Some((true, _)) => {
            crate::errors::socket_dgram_not_running();
            absent
        }
        _ => {
            crate::errors::system_error("getsockname", "EBADF");
            absent
        }
    }
}

/// `socket.remoteAddress()` — see the module doc: implemented via
/// `UdpSocket::peer_addr`, valid only after [`connect`]; a socket that never
/// has is `ERR_SOCKET_DGRAM_NOT_CONNECTED`.
pub(super) extern "C" fn remote_address(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    if !get_bool(this, "__connected") {
        crate::errors::socket_dgram_not_connected();
        return absent;
    }
    let Some(id) = socket_id(this) else {
        crate::errors::socket_dgram_not_connected();
        return absent;
    };
    let peer = registry::with_sockets(|table| {
        table.get(&id).and_then(|entry| entry.socket.as_ref()).and_then(|socket| socket.peer_addr().ok())
    });
    match peer {
        Some(addr) => address_info(addr.ip().to_string(), addr.port(), get_bool(this, "__udp6")),
        None => {
            crate::errors::socket_dgram_not_connected();
            absent
        }
    }
}

fn address_info(ip: String, port: u16, is_udp6: bool) -> u64 {
    entry::with_runtime(|context| {
        let object = entry::make_object(context);
        let addr_v = entry::make_string(context, &ip);
        let family_v = entry::make_string(context, if is_udp6 { "IPv6" } else { "IPv4" });
        let port_v = entry::make_number(f64::from(port));
        entry::put_member(context, object, "address", addr_v);
        entry::put_member(context, object, "family", family_v);
        entry::put_member(context, object, "port", port_v);
        object
    })
}
