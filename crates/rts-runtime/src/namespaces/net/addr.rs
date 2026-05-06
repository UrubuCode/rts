//! DNS resolution — std::net::ToSocketAddrs.
//!
//! `resolve(host)` aceita "host" puro (porta dummy 0 adicionada) ou
//! "host:port". Retorna o primeiro endereco como string handle.

use std::net::ToSocketAddrs;

use super::super::gc::handles::{Entry, alloc_entry};

fn str_from_abi<'a>(ptr: *const u8, len: i64) -> Option<&'a str> {
    if ptr.is_null() || len < 0 {
        return None;
    }
    // SAFETY: caller contract.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).ok()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NET_RESOLVE(host_ptr: *const u8, host_len: i64) -> u64 {
    let Some(host) = str_from_abi(host_ptr, host_len) else {
        return 0;
    };
    // ToSocketAddrs requer "host:port"; se vier so host, anexamos :0.
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
    let s = addr.ip().to_string();
    let bytes = s.into_bytes();
    alloc_entry(Entry::String(bytes))
}
