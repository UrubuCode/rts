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
    // RAISES, where this used to answer `undefined` for every bad argument —
    // `dns.setServers(['not-an-ip'])` was a silent no-op, so a program that
    // misconfigured its resolver carried on querying the old servers and had no
    // way to find out. `Resolver#setServers` already raised; the module
    // function, which is the spelling programs actually use, did not.
    let entries = match array_texts(servers) {
        Ok(entries) => entries,
        Err(fault) => {
            refuse_servers(fault);
            return entry::undefined_value();
        }
    };
    for item in &entries {
        if parse_server_addr(item).is_none() {
            crate::errors::invalid_ip_address(item);
            return entry::undefined_value();
        }
    }
    state().lock().unwrap().servers = entries;
    entry::undefined_value()
}

/// What is wrong with a `servers` argument, when something is.
///
/// Two failures and not one, because Node reports them differently and this
/// collapsed both into `None`: `setServers("1.1.1.1")` is
/// `ERR_INVALID_ARG_TYPE` naming `"servers"` as needing an Array, and
/// `setServers([123])` is `ERR_INVALID_ARG_TYPE` naming **`"servers[0]"`** as
/// needing a string. One `None` cannot carry the index, so the caller had to
/// guess — and `resolver_class`'s copy guessed "Array" for both while the
/// module's copy raised nothing at all.
pub(super) enum NotServers {
    /// Not an array. Node names the argument and the class it wanted.
    NotAnArray,
    /// Item `at` is not a string. Node names `servers[at]`, and the VALUE is
    /// carried because the message quotes it (`Received type number (123)`).
    ItemNotText(usize, u64),
}

/// The strings an array-shaped argument holds, or which rule it broke.
/// `pub(super)`: `resolver_class.rs`'s `Resolver#setServers` reads the same
/// argument shape, and now reports it the same way.
pub(super) fn array_texts(value: u64) -> Result<Vec<String>, NotServers> {
    if !entry::is_array(value) {
        return Err(NotServers::NotAnArray);
    }
    let length = entry::with_runtime(|context| entry::get_member(context, value, "length"));
    // A `length` that is not a number is not an array by any reading, so this
    // is the same refusal rather than a third one.
    let Some(count) = entry::number_of(length) else {
        return Err(NotServers::NotAnArray);
    };
    let count = count as usize;
    let mut out = Vec::with_capacity(count);
    for index in 0..count {
        let key = entry::make_number(index as f64);
        let item = entry::get_indexed(value, key);
        // `string_in`, not `text_of`: `text_of` coerces, so `setServers([123])`
        // became `["123"]` and was then refused for not being an IP address —
        // Node refuses it for not being a string, naming `servers[0]`. The
        // difference is invisible in the answer and visible in the code.
        match entry::with_runtime(|context| entry::string_in(context, item)) {
            Some(text) => out.push(text),
            None => return Err(NotServers::ItemNotText(index, item)),
        }
    }
    Ok(out)
}

/// Raises the refusal a bad `servers` argument owes, in Node's own wording.
///
/// Here rather than at each call site: two sites read this argument and the
/// whole reason the enum above exists is that they were reporting it
/// differently. One function means they cannot drift again.
pub(super) fn refuse_servers(fault: NotServers) {
    match fault {
        NotServers::NotAnArray => {
            crate::errors::invalid_arg_instance("servers", "Array", entry::undefined_value())
        }
        NotServers::ItemNotText(at, held) => {
            crate::errors::invalid_arg_type(&format!("servers[{at}]"), "string", held)
        }
    }
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
