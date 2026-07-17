//! node:net — the `SocketAddress` class: an immutable `{address, family, port,
//! flowlabel}` value.
//!
//! It has no state beyond those four fields and no mutation, so — unlike
//! `BlockList` — it needs no side table: the object-backed instance (an
//! `Entry::Map` tagged `__rts_class = "SocketAddress"`) CARRIES the fields, and
//! the getters read them back. Read-only follows for free: the class registers
//! getters and no setters.

use std::net::IpAddr;

use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};

use super::blocklist::rules::{family_of_type, parse_addr, AddressValue};
use crate::values::{handle_string, opt_num, opt_str, read};

unsafe extern "C" {
    fn __rtsadp_throw_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64);
}

/// The JS class name — also the `__rts_class` tag every instance carries.
pub const CLASS: &str = "SocketAddress";

fn throw(code: &str, class: &str, message: &str) {
    let msg = format!("{code}: {message}");
    unsafe {
        __rtsadp_throw_js_error(class.as_ptr(), class.len() as i64, msg.as_ptr(), msg.len() as i64);
    }
}

fn str_field(s: &str) -> i64 {
    alloc_entry(Entry::String(s.as_bytes().to_vec())) as i64
}

fn num_field(n: f64) -> i64 {
    n.to_bits() as i64
}

/// Build the instance. `flowlabel` is meaningful only for IPv6 (Node keeps the
/// field on both, defaulting to 0).
fn build(address: IpAddr, port: u16, flowlabel: u32) -> u64 {
    let family = if address.is_ipv6() { "ipv6" } else { "ipv4" };
    let mut m: indexmap::IndexMap<String, i64> = indexmap::IndexMap::new();
    m.insert("__rts_class".to_string(), str_field(CLASS));
    m.insert("__address".to_string(), str_field(&address.to_string()));
    m.insert("__family".to_string(), str_field(family));
    m.insert("__port".to_string(), num_field(f64::from(port)));
    m.insert("__flowlabel".to_string(), num_field(f64::from(flowlabel)));
    alloc_entry(Entry::Map(Box::new(m)))
}

/// A field word off an instance.
fn field(handle: u64, key: &str) -> Option<i64> {
    with_entry(handle, |e| match e {
        Some(Entry::Map(m)) => m.get(key).copied(),
        _ => None,
    })
}

/// Whether `handle` is a `SocketAddress` instance (its `__rts_class` tag).
fn is_socket_address(handle: u64) -> bool {
    match field(handle, "__rts_class") {
        Some(w) => handle_string(w as u64) == CLASS,
        None => false,
    }
}

/// The `address` of a `SocketAddress` instance — how another class reads one
/// passed where Node accepts `string | SocketAddress`.
pub fn address_of(handle: u64) -> Option<String> {
    if !is_socket_address(handle) {
        return None;
    }
    field(handle, "__address").map(|w| handle_string(w as u64))
}

/// `new SocketAddress()` — Node's defaults: `127.0.0.1`, family `ipv4`, port 0.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_ADDRESS_NEW() -> u64 {
    build(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 0, 0)
}

/// `new SocketAddress(options)` — `{address, family, port, flowlabel}`. The
/// address default follows the family: `127.0.0.1` (ipv4) / `::` (ipv6).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_ADDRESS_NEW_OPTS(options: u64) -> u64 {
    let family = opt_str(options, "family").unwrap_or_else(|| "ipv4".to_string());
    let Some(v6) = family_of_type(&family) else {
        throw(
            "ERR_INVALID_ARG_VALUE",
            "TypeError",
            &format!("The argument 'family' is invalid. Received '{family}'"),
        );
        return 0;
    };
    let text = opt_str(options, "address")
        .unwrap_or_else(|| if v6 { "::".to_string() } else { "127.0.0.1".to_string() });
    let Some(address) = parse_addr(&text, v6) else {
        throw(
            "ERR_INVALID_ADDRESS",
            "TypeError",
            &format!("Invalid address: '{text}'"),
        );
        return 0;
    };
    let port = match port_of(opt_num(options, "port").unwrap_or(0.0)) {
        Some(p) => p,
        None => return 0,
    };
    let flowlabel = opt_num(options, "flowlabel").unwrap_or(0.0).max(0.0) as u32;
    build(address, port, flowlabel)
}

/// A port option must be an integer in `[0, 65535]`.
fn port_of(n: f64) -> Option<u16> {
    if !n.is_finite() || n.fract() != 0.0 || !(0.0..=65535.0).contains(&n) {
        throw(
            "ERR_SOCKET_BAD_PORT",
            "RangeError",
            &format!("Port should be >= 0 and < 65536. Received {n}."),
        );
        return None;
    }
    Some(n as u16)
}

/// `SocketAddress.parse(input)` (static) — `'1.2.3.4:80'` / `'[::1]:80'`.
/// Returns `undefined` on any parse failure; never throws.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_ADDRESS_PARSE(p: *const u8, l: i64) -> u64 {
    match AddressValue::parse(&read(p, l)) {
        // A 0 handle boxes as `null`/`undefined` on the nullable-object return
        // path — the engine's own null-guard for a `): object` member.
        None => 0,
        Some(v) => build(v.address, v.port, v.flowlabel),
    }
}

/// `socketaddress.address`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_ADDRESS_ADDRESS(this: u64) -> u64 {
    field(this, "__address").unwrap_or(0) as u64
}

/// `socketaddress.family`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_ADDRESS_FAMILY(this: u64) -> u64 {
    field(this, "__family").unwrap_or(0) as u64
}

/// `socketaddress.port`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_ADDRESS_PORT(this: u64) -> f64 {
    field(this, "__port").map(|w| f64::from_bits(w as u64)).unwrap_or(0.0)
}

/// `socketaddress.flowlabel`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_SOCKET_ADDRESS_FLOWLABEL(this: u64) -> f64 {
    field(this, "__flowlabel").map(|w| f64::from_bits(w as u64)).unwrap_or(0.0)
}
