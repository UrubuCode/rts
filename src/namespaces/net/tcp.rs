//! TCP listener/stream — std::net.
//!
//! Convencoes da issue #16:
//! - send recebe StrPtr (data UTF-8 bytes)
//! - recv recebe ponteiro raw (u64 cast pra *mut u8) + len
//! - close retorna void e libera o handle
//! - handle 0 = invalido/erro

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

use super::super::gc::handles::{Entry, alloc_entry, free_handle, shard_for_handle};

fn str_from_abi<'a>(ptr: *const u8, len: i64) -> Option<&'a str> {
    if ptr.is_null() || len < 0 {
        return None;
    }
    // SAFETY: caller contract.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).ok()
}

fn clone_stream(handle: u64) -> Option<TcpStream> {
    let guard = shard_for_handle(handle).lock().unwrap();
    match guard.get(handle) {
        Some(Entry::TcpStream(s)) => s.try_clone().ok(),
        _ => None,
    }
}

fn clone_listener(handle: u64) -> Option<TcpListener> {
    let guard = shard_for_handle(handle).lock().unwrap();
    match guard.get(handle) {
        Some(Entry::TcpListener(l)) => l.try_clone().ok(),
        _ => None,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_TCP_LISTEN(addr_ptr: *const u8, addr_len: i64) -> u64 {
    let Some(addr) = str_from_abi(addr_ptr, addr_len) else {
        return 0;
    };
    match TcpListener::bind(addr) {
        Ok(l) => alloc_entry(Entry::TcpListener(Box::new(l))),
        Err(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_TCP_ACCEPT(listener: u64) -> u64 {
    let Some(l) = clone_listener(listener) else {
        return 0;
    };
    match l.accept() {
        Ok((stream, _peer)) => alloc_entry(Entry::TcpStream(Box::new(stream))),
        Err(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_TCP_CONNECT(addr_ptr: *const u8, addr_len: i64) -> u64 {
    let Some(addr) = str_from_abi(addr_ptr, addr_len) else {
        return 0;
    };
    match TcpStream::connect(addr) {
        Ok(s) => alloc_entry(Entry::TcpStream(Box::new(s))),
        Err(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_TCP_SEND(
    stream: u64,
    data_ptr: *const u8,
    data_len: i64,
) -> i64 {
    if data_len < 0 || data_ptr.is_null() {
        return -1;
    }
    let Some(mut s) = clone_stream(stream) else {
        return -1;
    };
    // SAFETY: caller contract — ptr/len descrevem buffer valido.
    let slice = unsafe { std::slice::from_raw_parts(data_ptr, data_len as usize) };
    match s.write(slice) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_TCP_RECV(stream: u64, buf_ptr: u64, len: i64) -> i64 {
    if len < 0 || buf_ptr == 0 {
        return -1;
    }
    let Some(mut s) = clone_stream(stream) else {
        return -1;
    };
    // SAFETY: caller passou ponteiro raw valido (geralmente buffer.ptr()).
    let dst = unsafe { std::slice::from_raw_parts_mut(buf_ptr as *mut u8, len as usize) };
    match s.read(dst) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_TCP_LOCAL_ADDR(handle: u64) -> u64 {
    let guard = shard_for_handle(handle).lock().unwrap();
    let addr = match guard.get(handle) {
        Some(Entry::TcpStream(s)) => match s.local_addr() {
            Ok(a) => a.to_string(),
            Err(_) => return 0,
        },
        Some(Entry::TcpListener(l)) => match l.local_addr() {
            Ok(a) => a.to_string(),
            Err(_) => return 0,
        },
        _ => return 0,
    };
    drop(guard);
    unsafe { __RTS_FN_NS_GC_STRING_NEW(addr.as_ptr(), addr.len() as i64) }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_TCP_CLOSE(handle: u64) {
    free_handle(handle);
}
