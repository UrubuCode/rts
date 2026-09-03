//! `getServers`/`setServers`/`setDefaultResultOrder`/`getDefaultResultOrder`/
//! `setLocalAddress` — the per-process DNS bookkeeping both `dns` and
//! `dns.promises` read and write.

use super::common::{parse_server_addr, string_value};
use rts_core::entry;
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::{Mutex, OnceLock};

/// Not consulted by `lookup` (see the module doc, "Two resolution paths")
/// and, since [`super::resolve`], not consulted by `resolve4` either:
/// `resolve4` reads the servers `hickory-resolver`'s own `system-config`
/// feature discovers directly from the OS, not this struct — the same
/// divergence real Node has between `setServers()` (a per-process override)
/// and what `getServers()` reports before any override is made (the OS
/// list). Reflecting the OS list here as a starting value would need the
/// same OS query `hickory-resolver` already does internally and has no
/// public accessor for, so `getServers()` still starts empty, named rather
/// than approximated.
pub(super) struct DnsState {
    pub(super) servers: Vec<String>,
    pub(super) order: &'static str,
    pub(super) local_v4: Option<String>,
    pub(super) local_v6: Option<String>,
}

pub(super) fn state() -> &'static Mutex<DnsState> {
    static STATE: OnceLock<Mutex<DnsState>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(DnsState {
            // Real Node's default reflects the OS's configured resolvers
            // (`/etc/resolv.conf`, the Windows resolver stack). Reading that
            // without a new dependency is not available here, so this
            // starts empty rather than guessing a well-known public
            // resolver's address and presenting it as measured.
            servers: Vec::new(),
            order: "verbatim",
            local_v4: None,
            local_v6: None,
        })
    })
}

/// `dns.getServers()` / `dnsPromises.getServers()` — see [`DnsState`] for
/// why this never reflects the OS's own configured resolvers.
pub(super) extern "C" fn get_servers(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let servers = state().lock().unwrap().servers.clone();
    entry::with_runtime(|context| {
        let items: Vec<u64> = servers.iter().map(|s| entry::make_string(context, s)).collect();
        entry::make_array_in(context, items)
    })
}

/// `dns.setServers(servers)`. A malformed entry drops the whole call rather
/// than throwing — this module has no way to raise a catchable exception
/// from host code (`entry::throw` ends the process; it is not the
/// per-call validation error Node raises here) — so the list is left
/// unchanged instead of partially replaced, and the call answers
/// `undefined` either way, matching Node's `void` return.
pub(super) extern "C" fn set_servers(_e: u64, _this: u64, servers: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(entries) = array_texts(servers) else {
        return entry::undefined_value();
    };
    for item in &entries {
        if parse_server_addr(item).is_none() {
            return entry::undefined_value();
        }
    }
    state().lock().unwrap().servers = entries;
    entry::undefined_value()
}

/// The strings an array-shaped argument holds, `None` if it is not one.
/// `pub(super)`: `resolver_class.rs`'s `Resolver#setServers` reads the same
/// argument shape.
pub(super) fn array_texts(value: u64) -> Option<Vec<String>> {
    if !entry::is_array(value) {
        return None;
    }
    let length = entry::with_runtime(|context| entry::get_member(context, value, "length"));
    let count = entry::number_of(length)? as usize;
    let mut out = Vec::with_capacity(count);
    for index in 0..count {
        let key = entry::make_number(index as f64);
        let item = entry::get_indexed(value, key);
        out.push(entry::text_of(item)?);
    }
    Some(out)
}

/// `dns.setDefaultResultOrder(order)`. An unrecognized order leaves the
/// stored value unchanged (same "no throw available" reasoning as
/// [`set_servers`]).
pub(super) extern "C" fn set_default_result_order(_e: u64, _this: u64, order: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    if let Some(text) = entry::text_of(order) {
        let resolved = match text.as_str() {
            "ipv4first" => Some("ipv4first"),
            "ipv6first" => Some("ipv6first"),
            "verbatim" => Some("verbatim"),
            _ => None,
        };
        if let Some(order) = resolved {
            state().lock().unwrap().order = order;
        }
    }
    entry::undefined_value()
}

/// `dns.getDefaultResultOrder()`.
pub(super) extern "C" fn get_default_result_order(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    string_value(state().lock().unwrap().order)
}

/// `dns.setLocalAddress(ipv4?, ipv6?)`. Stored, and — see the module doc's
/// "Two resolution paths" — never applied: `lookup` goes through
/// `std::net::ToSocketAddrs`, which has no source-address parameter to hand
/// this to, and `resolve4` goes through `hickory-resolver`'s
/// system-configured resolver, which this crate does not rebuild per call
/// to bind it to a source address either. Inert bookkeeping, the same
/// posture `set_servers` takes.
pub(super) extern "C" fn set_local_address(_e: u64, _this: u64, ipv4: u64, ipv6: u64, _a2: u64, _a3: u64) -> u64 {
    let mut guard = state().lock().unwrap();
    if let Some(text) = entry::text_of(ipv4)
        && IpAddr::from_str(&text).is_ok()
    {
        guard.local_v4 = Some(text);
    }
    if let Some(text) = entry::text_of(ipv6)
        && IpAddr::from_str(&text).is_ok()
    {
        guard.local_v6 = Some(text);
    }
    entry::undefined_value()
}
