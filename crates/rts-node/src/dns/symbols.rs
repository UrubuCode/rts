//! node:dns — name resolution over `std::net` (getaddrinfo), delivering results
//! through the codegen callback bridge (the same one timers/events use). The
//! resolution is synchronous; the callback is invoked with Node's
//! `(err, ...)`-first shape. Real DNS — no stubs, no fabricated addresses.

use std::net::ToSocketAddrs;

use rts_engine::heap::handles::{alloc_entry, Entry};
use rts_engine::heap::poly::POLY_UNDEFINED;
use rts_engine::heap::shapes::{alloc_shaped_object, handle_word_auto, null_word, string_word};

unsafe extern "C" {
    fn __rtsadp_fn_invoke(f: u64, a0: u64, a1: u64, a2: u64, a3: u64, this: u64) -> u64;
}

fn read(ptr: *const u8, len: i64) -> String {
    unsafe { rts_engine::abi::str_abi::from_abi(ptr, len) }.unwrap_or("").to_string()
}

fn num(v: f64) -> u64 {
    v.to_bits()
}

/// A Node-style error object `{ message, code }` (truthy → the `if (err)` path).
fn err_object(message: &str, code: &str) -> u64 {
    let h = alloc_shaped_object(
        &["message", "code"],
        &[string_word(message.as_bytes()) as i64, string_word(code.as_bytes()) as i64],
    );
    handle_word_auto(h)
}

fn invoke(cb: u64, a0: u64, a1: u64, a2: u64) {
    unsafe { __rtsadp_fn_invoke(cb, a0, a1, a2, POLY_UNDEFINED, POLY_UNDEFINED) };
}

/// Resolve `host` to socket addresses (port 0), or an error message.
fn resolve(host: &str) -> Result<Vec<std::net::IpAddr>, String> {
    match (host, 0u16).to_socket_addrs() {
        Ok(iter) => Ok(iter.map(|sa| sa.ip()).collect()),
        Err(e) => Err(e.to_string()),
    }
}

/// `dns.lookup(hostname, callback)` → `callback(err, address, family)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DNS_LOOKUP(hp: *const u8, hl: i64, callback: u64) {
    let host = read(hp, hl);
    match resolve(&host).ok().and_then(|v| v.into_iter().next()) {
        Some(ip) => {
            let family = if ip.is_ipv4() { 4.0 } else { 6.0 };
            invoke(callback, null_word(), string_word(ip.to_string().as_bytes()) as u64, num(family));
        }
        None => invoke(
            callback,
            err_object(&format!("getaddrinfo ENOTFOUND {host}"), "ENOTFOUND"),
            POLY_UNDEFINED,
            POLY_UNDEFINED,
        ),
    }
}

/// `dns.resolve4(hostname, callback)` → `callback(err, addresses)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DNS_RESOLVE4(hp: *const u8, hl: i64, callback: u64) {
    resolve_family(read(hp, hl), callback, true);
}

/// `dns.resolve6(hostname, callback)` → `callback(err, addresses)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DNS_RESOLVE6(hp: *const u8, hl: i64, callback: u64) {
    resolve_family(read(hp, hl), callback, false);
}

fn resolve_family(host: String, callback: u64, v4: bool) {
    match resolve(&host) {
        Ok(ips) => {
            let words: Vec<i64> = ips
                .iter()
                .filter(|ip| ip.is_ipv4() == v4)
                .map(|ip| string_word(ip.to_string().as_bytes()) as i64)
                .collect();
            let arr = handle_word_auto(alloc_entry(Entry::Vec(Box::new(words))));
            invoke(callback, null_word(), arr, POLY_UNDEFINED);
        }
        Err(_) => invoke(
            callback,
            err_object(&format!("queryA ENOTFOUND {host}"), "ENOTFOUND"),
            POLY_UNDEFINED,
            POLY_UNDEFINED,
        ),
    }
}

