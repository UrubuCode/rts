//! `net` namespace — TCP/UDP (sync) + DNS via std::net (issue #16).
//!
//! `send` takes a UTF-8 string; `recv` takes a raw writable buffer pointer
//! (U64 cast to *mut u8) + length; `close` frees the handle. Handle 0 = error.
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs, UdpSocket};

use rts_abi::ty::{Handle, I64, U64};
use rts_macro::rts_namespace;

use crate::namespaces::gc::handles::{
    Entry, UdpEntry, alloc_entry, free_handle, with_entry, with_entry_mut,
};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

fn clone_stream(handle: u64) -> Option<TcpStream> {
    with_entry(handle, |entry| match entry {
        Some(Entry::TcpStream(s)) => s.try_clone().ok(),
        _ => None,
    })
}

fn clone_listener(handle: u64) -> Option<TcpListener> {
    with_entry(handle, |entry| match entry {
        Some(Entry::TcpListener(l)) => l.try_clone().ok(),
        _ => None,
    })
}

fn clone_socket(handle: u64) -> Option<UdpSocket> {
    with_entry(handle, |entry| match entry {
        Some(Entry::UdpSocket(e)) => e.socket.try_clone().ok(),
        _ => None,
    })
}

/// TCP/UDP sockets (sync, std::net) + DNS resolution.
#[rts_namespace(net)]
impl NetNs {
    /// Bind a TCP listener to `addr`. Handle, 0 on error.
    #[rts_fn]
    pub fn tcp_listen(addr: Str) -> Handle {
        match TcpListener::bind(addr) {
            Ok(l) => alloc_entry(Entry::TcpListener(Box::new(l))),
            Err(_) => 0,
        }
    }

    /// Accept a connection on `listener`. Stream handle, 0 on error.
    #[rts_fn]
    pub fn tcp_accept(listener: U64) -> Handle {
        let Some(l) = clone_listener(listener) else {
            return 0;
        };
        match l.accept() {
            Ok((stream, _peer)) => alloc_entry(Entry::TcpStream(Box::new(stream))),
            Err(_) => 0,
        }
    }

    /// Connect a TCP stream to `addr`. Handle, 0 on error.
    #[rts_fn]
    pub fn tcp_connect(addr: Str) -> Handle {
        match TcpStream::connect(addr) {
            Ok(s) => alloc_entry(Entry::TcpStream(Box::new(s))),
            Err(_) => 0,
        }
    }

    /// Write `data` bytes to `stream`. Bytes written, or -1 on error.
    #[rts_fn]
    pub fn tcp_send(stream: U64, data: Str) -> I64 {
        let Some(mut s) = clone_stream(stream) else {
            return -1;
        };
        match s.write(data.as_bytes()) {
            Ok(n) => n as i64,
            Err(_) => -1,
        }
    }

    /// Read up to `len` bytes from `stream` into a raw buffer. Count, or -1.
    #[rts_fn(ts = "tcp_recv(stream: number, bufPtr: number, len: number): number")]
    pub fn tcp_recv(stream: U64, buf_ptr: U64, len: I64) -> I64 {
        if len < 0 || buf_ptr == 0 {
            return -1;
        }
        let Some(mut s) = clone_stream(stream) else {
            return -1;
        };
        // SAFETY: caller passes a valid raw pointer (usually buffer.ptr()).
        let dst = unsafe { std::slice::from_raw_parts_mut(buf_ptr as *mut u8, len as usize) };
        match s.read(dst) {
            Ok(n) => n as i64,
            Err(_) => -1,
        }
    }

    /// Local address of a stream/listener handle. String handle, 0 on error.
    #[rts_fn(ts = "tcp_local_addr(handle: number): string")]
    pub fn tcp_local_addr(handle: U64) -> Handle {
        let addr: Option<String> = with_entry(handle, |entry| match entry {
            Some(Entry::TcpStream(s)) => s.local_addr().ok().map(|a| a.to_string()),
            Some(Entry::TcpListener(l)) => l.local_addr().ok().map(|a| a.to_string()),
            _ => None,
        });
        match addr {
            Some(a) => intern(&a),
            None => 0,
        }
    }

    /// Closes (frees) a TCP handle.
    #[rts_fn]
    pub fn tcp_close(handle: U64) {
        free_handle(handle);
    }

    /// Bind a UDP socket to `addr`. Handle, 0 on error.
    #[rts_fn]
    pub fn udp_bind(addr: Str) -> Handle {
        match UdpSocket::bind(addr) {
            Ok(s) => alloc_entry(Entry::UdpSocket(Box::new(UdpEntry {
                socket: s,
                last_peer: None,
            }))),
            Err(_) => 0,
        }
    }

    /// Send `data` to `dest`. Bytes sent, or -1 on error.
    #[rts_fn]
    pub fn udp_send_to(sock: U64, dest: Str, data: Str) -> I64 {
        let Some(s) = clone_socket(sock) else {
            return -1;
        };
        match s.send_to(data.as_bytes(), dest) {
            Ok(n) => n as i64,
            Err(_) => -1,
        }
    }

    /// Receive into a raw buffer; records the peer. Count, or -1.
    #[rts_fn(ts = "udp_recv_from(sock: number, bufPtr: number, len: number): number")]
    pub fn udp_recv_from(sock: U64, buf_ptr: U64, len: I64) -> I64 {
        if len < 0 || buf_ptr == 0 {
            return -1;
        }
        let Some(s) = clone_socket(sock) else {
            return -1;
        };
        // SAFETY: caller passes a valid raw pointer.
        let dst = unsafe { std::slice::from_raw_parts_mut(buf_ptr as *mut u8, len as usize) };
        let (n, peer) = match s.recv_from(dst) {
            Ok(p) => p,
            Err(_) => return -1,
        };
        with_entry_mut(sock, |entry| {
            if let Some(Entry::UdpSocket(e)) = entry {
                e.last_peer = Some(peer);
            }
        });
        n as i64
    }

    /// Address of the last peer seen by `recv_from`. String handle, 0 if none.
    #[rts_fn(ts = "udp_last_peer(sock: number): string")]
    pub fn udp_last_peer(sock: U64) -> Handle {
        let addr: Option<String> = with_entry(sock, |entry| match entry {
            Some(Entry::UdpSocket(e)) => e.last_peer.map(|p| p.to_string()),
            _ => None,
        });
        match addr {
            Some(a) => intern(&a),
            None => 0,
        }
    }

    /// Local address of a UDP socket. String handle, 0 on error.
    #[rts_fn(ts = "udp_local_addr(sock: number): string")]
    pub fn udp_local_addr(sock: U64) -> Handle {
        let addr: Option<String> = with_entry(sock, |entry| match entry {
            Some(Entry::UdpSocket(e)) => e.socket.local_addr().ok().map(|a| a.to_string()),
            _ => None,
        });
        match addr {
            Some(a) => intern(&a),
            None => 0,
        }
    }

    /// Closes (frees) a UDP socket handle.
    #[rts_fn]
    pub fn udp_close(sock: U64) {
        free_handle(sock);
    }

    /// Resolve `host` (or `host:port`) to its first IP. String handle, 0 on error.
    #[rts_fn(ts = "resolve(host: string): string")]
    pub fn resolve(host: Str) -> Handle {
        // ToSocketAddrs needs "host:port"; append :0 when only a host is given.
        let target = if host.contains(':') {
            host.to_string()
        } else {
            format!("{host}:0")
        };
        let mut iter = match target.to_socket_addrs() {
            Ok(it) => it,
            Err(_) => return 0,
        };
        let Some(addr) = iter.next() else {
            return 0;
        };
        intern(&addr.ip().to_string())
    }
}
