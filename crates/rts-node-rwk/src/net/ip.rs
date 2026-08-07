//! `net.isIP`/`isIPv4`/`isIPv6` — pure string classification, `std::net`
//! parsing only, no I/O. Complete against `docs/reference/node/net.md` §2.

use rts_core_rwk::entry;
use std::net::{Ipv4Addr, Ipv6Addr};

/// `net.isIP(input)` — `4`, `6`, or `0`.
pub(super) extern "C" fn is_ip(_e: u64, _this: u64, input: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(text) = entry::text_of(input) else { return entry::make_number(0.0) };
    entry::make_number(classify(&text) as f64)
}

/// `net.isIPv4(input)`.
pub(super) extern "C" fn is_ipv4(_e: u64, _this: u64, input: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let held = entry::text_of(input).is_some_and(|text| text.parse::<Ipv4Addr>().is_ok());
    entry::boolean_value(held)
}

/// `net.isIPv6(input)`.
pub(super) extern "C" fn is_ipv6(_e: u64, _this: u64, input: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let held = entry::text_of(input).is_some_and(|text| text.parse::<Ipv6Addr>().is_ok());
    entry::boolean_value(held)
}

fn classify(text: &str) -> u8 {
    if text.parse::<Ipv4Addr>().is_ok() {
        4
    } else if text.parse::<Ipv6Addr>().is_ok() {
        6
    } else {
        0
    }
}
