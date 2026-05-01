//! UDP socket — std::net.

use std::net::UdpSocket;

use super::super::gc::handles::{Entry, UdpEntry, alloc_entry, free_handle, with_entry, with_entry_mut};

fn str_from_abi<'a>(ptr: *const u8, len: i64) -> Option<&'a str> {
    if ptr.is_null() || len < 0 {
        return None;
    }
    // SAFETY: caller contract.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).ok()
}

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn clone_socket(handle: u64) -> Option<UdpSocket> {
    with_entry(handle, |entry| match entry {
        Some(Entry::UdpSocket(e)) => e.socket.try_clone().ok(),
        _ => None,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_UDP_BIND(addr_ptr: *const u8, addr_len: i64) -> u64 {
    let Some(addr) = str_from_abi(addr_ptr, addr_len) else {
        return 0;
    };
    match UdpSocket::bind(addr) {
        Ok(s) => alloc_entry(Entry::UdpSocket(Box::new(UdpEntry {
            socket: s,
            last_peer: None,
        }))),
        Err(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_UDP_SEND_TO(
    sock: u64,
    dest_ptr: *const u8,
    dest_len: i64,
    data_ptr: *const u8,
    data_len: i64,
) -> i64 {
    let Some(dest) = str_from_abi(dest_ptr, dest_len) else {
        return -1;
    };
    if data_len < 0 || data_ptr.is_null() {
        return -1;
    }
    let Some(s) = clone_socket(sock) else {
        return -1;
    };
    // SAFETY: caller contract.
    let payload = unsafe { std::slice::from_raw_parts(data_ptr, data_len as usize) };
    match s.send_to(payload, dest) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_UDP_RECV_FROM(sock: u64, buf_ptr: u64, len: i64) -> i64 {
    if len < 0 || buf_ptr == 0 {
        return -1;
    }
    let Some(s) = clone_socket(sock) else {
        return -1;
    };
    // SAFETY: caller passou ponteiro raw valido.
    let dst = unsafe { std::slice::from_raw_parts_mut(buf_ptr as *mut u8, len as usize) };
    let (n, peer) = match s.recv_from(dst) {
        Ok(p) => p,
        Err(_) => return -1,
    };
    // Atualiza last_peer no socket original (clone released before this point).
    with_entry_mut(sock, |entry| {
        if let Some(Entry::UdpSocket(e)) = entry {
            e.last_peer = Some(peer);
        }
    });
    n as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_UDP_LAST_PEER(sock: u64) -> u64 {
    let addr: Option<String> = with_entry(sock, |entry| match entry {
        Some(Entry::UdpSocket(e)) => e.last_peer.map(|p| p.to_string()),
        _ => None,
    });
    match addr {
        Some(a) => unsafe { __RTS_FN_NS_GC_STRING_NEW(a.as_ptr(), a.len() as i64) },
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_UDP_LOCAL_ADDR(sock: u64) -> u64 {
    let addr: Option<String> = with_entry(sock, |entry| match entry {
        Some(Entry::UdpSocket(e)) => e.socket.local_addr().ok().map(|a| a.to_string()),
        _ => None,
    });
    match addr {
        Some(a) => unsafe { __RTS_FN_NS_GC_STRING_NEW(a.as_ptr(), a.len() as i64) },
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_UDP_CLOSE(handle: u64) {
    free_handle(handle);
}
