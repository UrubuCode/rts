//! node:net — the IP-string classifiers: `isIP`, `isIPv4`, `isIPv6`. Pure
//! `std::net` parsing, zero I/O.

use super::blocklist::rules::ip_kind;
use crate::values::read;

/// `net.isIP(input)` → 4, 6, or 0.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_IS_IP(p: *const u8, l: i64) -> i32 {
    ip_kind(&read(p, l))
}

/// `net.isIPv4(input)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_IS_IPV4(p: *const u8, l: i64) -> i64 {
    i64::from(ip_kind(&read(p, l)) == 4)
}

/// `net.isIPv6(input)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_IS_IPV6(p: *const u8, l: i64) -> i64 {
    i64::from(ip_kind(&read(p, l)) == 6)
}
