//! node:net — the IP-string classifiers: `isIP`, `isIPv4`, `isIPv6`. Pure
//! `std::net` parsing, zero I/O.

use super::blocklist::rules::ip_kind;

/// `net.isIP(input)` — returns `4`/`6` for a valid dotted-quad/colon-hex
/// address, `0` otherwise.
#[rtse::function(module = "node:net", value = "isIP")]
fn is_ip(input: &str) -> i32 {
    ip_kind(input)
}

/// `net.isIPv4(input)`.
#[rtse::function(module = "node:net", value = "isIPv4")]
fn is_ipv4(input: &str) -> bool {
    ip_kind(input) == 4
}

/// `net.isIPv6(input)`.
#[rtse::function(module = "node:net", value = "isIPv6")]
fn is_ipv6(input: &str) -> bool {
    ip_kind(input) == 6
}
