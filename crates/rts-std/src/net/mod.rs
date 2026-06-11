//! `net` namespace — TCP/UDP (sync) + DNS via std::net (issue #16).
//!
//! `send` takes a UTF-8 string; `recv` takes a raw writable buffer pointer
//! (U64 cast to *mut u8) + length; `close` frees the handle. Handle 0 = error.
//!
//! Migrado do `#[rts_namespace]` pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`; ver pilotos hint/hash/ptr/mem/runtime).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs, UdpSocket};

use rts_engine::abi::ty::{Handle, I64, U64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

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

/// Bind a TCP listener to `addr`. Handle, 0 on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_TCP_LISTEN(addr_ptr: *const u8, addr_len: i64) -> Handle {
    let addr = match unsafe { rts_engine::abi::str_abi::from_abi(addr_ptr, addr_len) } {
        Some(s) => s,
        None => return 0,
    };
    match TcpListener::bind(addr) {
        Ok(l) => alloc_entry(Entry::TcpListener(Box::new(l))),
        Err(_) => 0,
    }
}

/// Accept a connection on `listener`. Stream handle, 0 on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_TCP_ACCEPT(listener: U64) -> Handle {
    let Some(l) = clone_listener(listener) else {
        return 0;
    };
    match l.accept() {
        Ok((stream, _peer)) => alloc_entry(Entry::TcpStream(Box::new(stream))),
        Err(_) => 0,
    }
}

/// Connect a TCP stream to `addr`. Handle, 0 on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_TCP_CONNECT(addr_ptr: *const u8, addr_len: i64) -> Handle {
    let addr = match unsafe { rts_engine::abi::str_abi::from_abi(addr_ptr, addr_len) } {
        Some(s) => s,
        None => return 0,
    };
    match TcpStream::connect(addr) {
        Ok(s) => alloc_entry(Entry::TcpStream(Box::new(s))),
        Err(_) => 0,
    }
}

/// Write `data` bytes to `stream`. Bytes written, or -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_TCP_SEND(stream: U64, data_ptr: *const u8, data_len: i64) -> I64 {
    let data = match unsafe { rts_engine::abi::str_abi::from_abi(data_ptr, data_len) } {
        Some(s) => s,
        None => return 0,
    };
    let Some(mut s) = clone_stream(stream) else {
        return -1;
    };
    match s.write(data.as_bytes()) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

/// Read up to `len` bytes from `stream` into a raw buffer. Count, or -1.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_TCP_RECV(stream: U64, buf_ptr: U64, len: I64) -> I64 {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_TCP_LOCAL_ADDR(handle: U64) -> Handle {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_TCP_CLOSE(handle: U64) {
    free_handle(handle);
}

/// Bind a UDP socket to `addr`. Handle, 0 on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_UDP_BIND(addr_ptr: *const u8, addr_len: i64) -> Handle {
    let addr = match unsafe { rts_engine::abi::str_abi::from_abi(addr_ptr, addr_len) } {
        Some(s) => s,
        None => return 0,
    };
    match UdpSocket::bind(addr) {
        Ok(s) => alloc_entry(Entry::UdpSocket(Box::new(UdpEntry {
            socket: s,
            last_peer: None,
        }))),
        Err(_) => 0,
    }
}

/// Send `data` to `dest`. Bytes sent, or -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_UDP_SEND_TO(
    sock: U64,
    dest_ptr: *const u8,
    dest_len: i64,
    data_ptr: *const u8,
    data_len: i64,
) -> I64 {
    let dest = match unsafe { rts_engine::abi::str_abi::from_abi(dest_ptr, dest_len) } {
        Some(s) => s,
        None => return 0,
    };
    let data = match unsafe { rts_engine::abi::str_abi::from_abi(data_ptr, data_len) } {
        Some(s) => s,
        None => return 0,
    };
    let Some(s) = clone_socket(sock) else {
        return -1;
    };
    match s.send_to(data.as_bytes(), dest) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

/// Receive into a raw buffer; records the peer. Count, or -1.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_UDP_RECV_FROM(sock: U64, buf_ptr: U64, len: I64) -> I64 {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_UDP_LAST_PEER(sock: U64) -> Handle {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_UDP_LOCAL_ADDR(sock: U64) -> Handle {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_UDP_CLOSE(sock: U64) {
    free_handle(sock);
}

/// Resolve `host` (or `host:port`) to its first IP. String handle, 0 on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_RESOLVE(host_ptr: *const u8, host_len: i64) -> Handle {
    let host = match unsafe { rts_engine::abi::str_abi::from_abi(host_ptr, host_len) } {
        Some(s) => s,
        None => return 0,
    };
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

/// Função `net.f(args)`.
fn func(name: &str, symbol: &str, sig: Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: false,
        intrinsic: None,
    }
}

/// Registra a namespace `net` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("net")
        .doc("TCP/UDP sockets (sync, std::net) + DNS resolution.")
        .member(func(
            "tcp_listen",
            "__RTS_FN_NS_NET_TCP_LISTEN",
            Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
            "tcp_listen(addr: string): number",
            "Bind a TCP listener to `addr`. Handle, 0 on error.",
            __RTS_FN_NS_NET_TCP_LISTEN as *const u8,
        ))
        .member(func(
            "tcp_accept",
            "__RTS_FN_NS_NET_TCP_ACCEPT",
            Sig::new(vec![AbiType::U64], AbiType::Handle),
            "tcp_accept(listener: number): number",
            "Accept a connection on `listener`. Stream handle, 0 on error.",
            __RTS_FN_NS_NET_TCP_ACCEPT as *const u8,
        ))
        .member(func(
            "tcp_connect",
            "__RTS_FN_NS_NET_TCP_CONNECT",
            Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
            "tcp_connect(addr: string): number",
            "Connect a TCP stream to `addr`. Handle, 0 on error.",
            __RTS_FN_NS_NET_TCP_CONNECT as *const u8,
        ))
        .member(func(
            "tcp_send",
            "__RTS_FN_NS_NET_TCP_SEND",
            Sig::new(vec![AbiType::U64, AbiType::StrPtr], AbiType::I64),
            "tcp_send(stream: number, data: string): number",
            "Write `data` bytes to `stream`. Bytes written, or -1 on error.",
            __RTS_FN_NS_NET_TCP_SEND as *const u8,
        ))
        .member(func(
            "tcp_recv",
            "__RTS_FN_NS_NET_TCP_RECV",
            Sig::new(vec![AbiType::U64, AbiType::U64, AbiType::I64], AbiType::I64),
            "tcp_recv(stream: number, bufPtr: number, len: number): number",
            "Read up to `len` bytes from `stream` into a raw buffer. Count, or -1.",
            __RTS_FN_NS_NET_TCP_RECV as *const u8,
        ))
        .member(func(
            "tcp_local_addr",
            "__RTS_FN_NS_NET_TCP_LOCAL_ADDR",
            Sig::new(vec![AbiType::U64], AbiType::Handle),
            "tcp_local_addr(handle: number): string",
            "Local address of a stream/listener handle. String handle, 0 on error.",
            __RTS_FN_NS_NET_TCP_LOCAL_ADDR as *const u8,
        ))
        .member(func(
            "tcp_close",
            "__RTS_FN_NS_NET_TCP_CLOSE",
            Sig::new(vec![AbiType::U64], AbiType::Void),
            "tcp_close(handle: number): void",
            "Closes (frees) a TCP handle.",
            __RTS_FN_NS_NET_TCP_CLOSE as *const u8,
        ))
        .member(func(
            "udp_bind",
            "__RTS_FN_NS_NET_UDP_BIND",
            Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
            "udp_bind(addr: string): number",
            "Bind a UDP socket to `addr`. Handle, 0 on error.",
            __RTS_FN_NS_NET_UDP_BIND as *const u8,
        ))
        .member(func(
            "udp_send_to",
            "__RTS_FN_NS_NET_UDP_SEND_TO",
            Sig::new(vec![AbiType::U64, AbiType::StrPtr, AbiType::StrPtr], AbiType::I64),
            "udp_send_to(sock: number, dest: string, data: string): number",
            "Send `data` to `dest`. Bytes sent, or -1 on error.",
            __RTS_FN_NS_NET_UDP_SEND_TO as *const u8,
        ))
        .member(func(
            "udp_recv_from",
            "__RTS_FN_NS_NET_UDP_RECV_FROM",
            Sig::new(vec![AbiType::U64, AbiType::U64, AbiType::I64], AbiType::I64),
            "udp_recv_from(sock: number, bufPtr: number, len: number): number",
            "Receive into a raw buffer; records the peer. Count, or -1.",
            __RTS_FN_NS_NET_UDP_RECV_FROM as *const u8,
        ))
        .member(func(
            "udp_last_peer",
            "__RTS_FN_NS_NET_UDP_LAST_PEER",
            Sig::new(vec![AbiType::U64], AbiType::Handle),
            "udp_last_peer(sock: number): string",
            "Address of the last peer seen by `recv_from`. String handle, 0 if none.",
            __RTS_FN_NS_NET_UDP_LAST_PEER as *const u8,
        ))
        .member(func(
            "udp_local_addr",
            "__RTS_FN_NS_NET_UDP_LOCAL_ADDR",
            Sig::new(vec![AbiType::U64], AbiType::Handle),
            "udp_local_addr(sock: number): string",
            "Local address of a UDP socket. String handle, 0 on error.",
            __RTS_FN_NS_NET_UDP_LOCAL_ADDR as *const u8,
        ))
        .member(func(
            "udp_close",
            "__RTS_FN_NS_NET_UDP_CLOSE",
            Sig::new(vec![AbiType::U64], AbiType::Void),
            "udp_close(sock: number): void",
            "Closes (frees) a UDP socket handle.",
            __RTS_FN_NS_NET_UDP_CLOSE as *const u8,
        ))
        .member(func(
            "resolve",
            "__RTS_FN_NS_NET_RESOLVE",
            Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
            "resolve(host: string): string",
            "Resolve `host` (or `host:port`) to its first IP. String handle, 0 on error.",
            __RTS_FN_NS_NET_RESOLVE as *const u8,
        ))
        .done();
}
