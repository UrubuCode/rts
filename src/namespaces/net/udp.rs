//! UDP socket — std::net.

use std::net::UdpSocket;

use super::super::gc::handles::{Entry, UdpEntry, alloc_entry, free_handle, shard_for_handle};

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
    let guard = shard_for_handle(handle).lock().unwrap();
    match guard.get(handle) {
        Some(Entry::UdpSocket(e)) => e.socket.try_clone().ok(),
        _ => None,
    }
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
    // Atualiza last_peer no socket original.
    {
        let mut guard = shard_for_handle(sock).lock().unwrap();
        if let Some(Entry::UdpSocket(e)) = guard.get_mut(sock) {
            e.last_peer = Some(peer);
        }
    }
    n as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_UDP_LAST_PEER(sock: u64) -> u64 {
    let guard = shard_for_handle(sock).lock().unwrap();
    let addr = match guard.get(sock) {
        Some(Entry::UdpSocket(e)) => match e.last_peer {
            Some(p) => p.to_string(),
            None => return 0,
        },
        _ => return 0,
    };
    drop(guard);
    unsafe { __RTS_FN_NS_GC_STRING_NEW(addr.as_ptr(), addr.len() as i64) }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_UDP_LOCAL_ADDR(sock: u64) -> u64 {
    let guard = shard_for_handle(sock).lock().unwrap();
    let addr = match guard.get(sock) {
        Some(Entry::UdpSocket(e)) => match e.socket.local_addr() {
            Ok(a) => a.to_string(),
            Err(_) => return 0,
        },
        _ => return 0,
    };
    drop(guard);
    unsafe { __RTS_FN_NS_GC_STRING_NEW(addr.as_ptr(), addr.len() as i64) }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_UDP_CLOSE(handle: u64) {
    free_handle(handle);
}
