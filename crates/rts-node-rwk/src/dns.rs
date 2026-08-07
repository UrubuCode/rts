//! `node:dns` (+ `node:dns/promises`) — hostname/address resolution.
//!
//! # Reuse-check
//!
//! `reuse-check` was run before writing this: `rts-cranelift` owns nothing
//! about DNS or sockets, and `rts-core-rwk`'s `entry` API (`entry/mod.rs`,
//! `entry/modules.rs`) exposes no name resolution of any kind — the nearest
//! thing to reuse was `entry::settled`, which every promise this module
//! returns goes through, and `entry::call`, which the callback forms use to
//! invoke a user function synchronously (the same primitive `events.rs`'s
//! `emit` calls a listener through). Nothing else here duplicates state or
//! numbering that another crate already owns.
//!
//! # Two resolution paths, one implemented
//!
//! Real Node splits `dns.lookup`/`lookupService` (OS `getaddrinfo`/
//! `getnameinfo`, threadpool-backed) from `dns.resolve*`/`reverse`/
//! `Resolver` (DNS-protocol, speaks to configured servers directly,
//! `hickory-resolver`-backed per `docs/reference/node/crates.md` §4.5). Only
//! the OS-facility path is implemented: `std::net::ToSocketAddrs` is the one
//! honest answer available without a new dependency, and it is what
//! `lookup` needs. The protocol path needs a DNS client this crate is not
//! allowed to add — refused by name below, not approximated.
//!
//! # Synchronous, wearing a callback
//!
//! There is no worker/thread pool here. `lookup` resolves in-line on the
//! calling thread and then calls the callback (or settles the promise)
//! before returning — the same divergence `fs.promises`'s module doc states
//! for file I/O, applied to name resolution: a caller "starting" several
//! lookups "concurrently" does them one at a time, and gets the right answer
//! at the wrong pace.
//!
//! # Errors are plain objects, not `Error` instances
//!
//! `entry::modules` exposes no way to construct a real `Error` (no
//! `new_error`/`error_class` accessor exists in `rts-core-rwk`'s entry API —
//! checked; the reachable class-building primitives are `make_prototype`/
//! `make_instance`, and building an `Error` from those would fabricate a
//! sibling of the one the language already provides for `new Error(...)`,
//! duplicating it rather than reusing it). So a lookup failure hands back a
//! plain object carrying `code`/`syscall`/`hostname`/`message` — which is
//! what a program checking `err.code === 'ENOTFOUND'` actually reads — and
//! `err instanceof Error` is `false` here where real Node's is `true`. Named
//! rather than silent.
//!
//! # Not implemented, by name
//!
//! - `resolve`/`resolve4`/`resolve6`/`resolveAny`/`resolveCaa`/
//!   `resolveCname`/`resolveMx`/`resolveNaptr`/`resolveNs`/`resolvePtr`/
//!   `resolveSoa`/`resolveSrv`/`resolveTlsa`/`resolveTxt`/`reverse`/
//!   `dns.Resolver` (and its `dns.promises` mirror) — every one of these
//!   needs a DNS protocol client (MX/TXT/SRV/etc. records are not something
//!   `std` resolves; `docs/reference/node/dns.md` §5.1 names
//!   `hickory-resolver` as the crate for this). This task may not add a
//!   dependency, so these are refused by name rather than faked over
//!   `lookup`'s A/AAAA-only answer.
//! - `lookupService(address, port, callback)` — needs `getnameinfo`-class
//!   reverse lookup (hostname from address) AND a service-name database
//!   (port → name, e.g. `80` → `"http"`); `std` has neither, and hand-rolling
//!   a partial `/etc/services`-style table would be exactly the "plausible
//!   wrong value" the honesty floor refuses (it would also be wrong on
//!   Windows, which has no such file).
//! - The 23 numeric `dns.<CODE>` c-ares error-category constants
//!   (`dns.NODATA`, `dns.FORMERR`, …) — they describe c-ares's own internal
//!   taxonomy and have no canonical value without linking c-ares (or an
//!   equivalent); the `E`-prefixed string codes programs actually branch on
//!   (`ENOTFOUND`, …) are what `lookup`'s error object already carries, so
//!   nothing here depends on the numeric table existing.
//! - `dns.ADDRCONFIG`/`V4MAPPED`/`ALL` ARE implemented as bit flags below,
//!   but they are inert bookkeeping: `lookup`'s options accept an
//!   `options.hints` number and never consult it, because `std::net`'s
//!   resolution has no hints-bitmask control to forward it to. A caller
//!   combining them with `|` gets a number back from that expression and
//!   nothing more.

use rts_core_rwk::entry::{self, Provided};
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::str::FromStr;
use std::sync::{Mutex, OnceLock};

/// Per-process DNS bookkeeping `getServers`/`setServers`/
/// `setDefaultResultOrder`/`setLocalAddress` read and write.
///
/// Not consulted by `lookup`: real Node's `dns.setServers()` only affects
/// the `resolve*`/`reverse` protocol path (see the module doc, "Two
/// resolution paths"), and that path is not implemented here either. So
/// this is bookkeeping a program can read back consistently, and nothing
/// more — matching what `setServers` affects in real Node (never `lookup`)
/// exactly, by affecting neither here.
struct DnsState {
    servers: Vec<String>,
    order: &'static str,
    local_v4: Option<String>,
    local_v6: Option<String>,
}

fn state() -> &'static Mutex<DnsState> {
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

/// The namespace `node:dns` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("lookup", lookup),
        ("getServers", get_servers),
        ("setServers", set_servers),
        ("setDefaultResultOrder", set_default_result_order),
        ("getDefaultResultOrder", get_default_result_order),
        ("setLocalAddress", set_local_address),
    ];
    let namespace = entry::make_namespace(context, members);
    let promises = promises_namespace(context);
    entry::put_member(context, namespace, "promises", promises);
    for (name, value) in [("ADDRCONFIG", 4.0), ("V4MAPPED", 8.0), ("ALL", 16.0)] {
        let held = entry::make_number(value);
        entry::put_member(context, namespace, name, held);
        entry::put_member(context, promises, name, held);
    }
    namespace
}

/// `node:dns/promises` (also reachable as `dns.promises`) — the promise
/// mirror of what `namespace` implements. Built separately from `namespace`
/// rather than derived from it, the same way [`super::path`]'s `win32`/
/// `posix` children are each their own function table: `lookup`'s two forms
/// (callback vs. promise) are different enough in shape (a settled
/// `Promise` vs. a synchronous call into a callback) to not share one
/// native body cheaply.
pub fn promises_namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("lookup", promise_lookup),
        ("getServers", get_servers),
        ("setServers", set_servers),
        ("setDefaultResultOrder", set_default_result_order),
        ("getDefaultResultOrder", get_default_result_order),
        ("setLocalAddress", set_local_address),
    ];
    entry::make_namespace(context, members)
}

use rts_core_rwk::entry::Context;

/// `dns.lookup(hostname, options?, callback)`.
///
/// The last-argument overload real Node accepts: when the 3rd slot is
/// `undefined`, the 2nd slot is the callback and `options` is absent;
/// otherwise the 2nd slot is `options` and the 3rd is the callback. A call
/// with no callback at all does nothing and answers `undefined` — there is
/// nowhere else for a lookup's result to go.
extern "C" fn lookup(_e: u64, _this: u64, hostname: u64, arg1: u64, arg2: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let (options, callback) = match arg2 == absent {
        true => (absent, arg1),
        false => (arg1, arg2),
    };
    if callback == absent {
        return absent;
    }
    let Some(host) = entry::text_of(hostname) else {
        let error = error_object("ERR_INVALID_ARG_TYPE", "getaddrinfo", "");
        entry::call(callback, absent, error, absent, absent, absent);
        return absent;
    };
    let family = wanted_family(options);
    let all = option_bool(options, "all");
    match resolve(&host, family) {
        Some(addrs) if !addrs.is_empty() => {
            let null = entry::null_value();
            if all {
                let array = lookup_all_array(&addrs);
                entry::call(callback, absent, null, array, absent, absent)
            } else {
                let (address, family) = &addrs[0];
                let address_v = string_value(address);
                let family_v = entry::make_number(f64::from(*family));
                entry::call(callback, absent, null, address_v, family_v, absent)
            }
        }
        _ => {
            let error = error_object("ENOTFOUND", "getaddrinfo", &host);
            entry::call(callback, absent, error, absent, absent, absent)
        }
    }
}

/// `dnsPromises.lookup(hostname, options?)`.
extern "C" fn promise_lookup(_e: u64, _this: u64, hostname: u64, options: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(host) = entry::text_of(hostname) else {
        let error = error_object("ERR_INVALID_ARG_TYPE", "getaddrinfo", "");
        return entry::with_runtime(|context| entry::settled(context, error, true));
    };
    let family = wanted_family(options);
    let all = option_bool(options, "all");
    match resolve(&host, family) {
        Some(addrs) if !addrs.is_empty() => {
            let value = match all {
                true => lookup_all_array(&addrs),
                false => single_result(&addrs[0]),
            };
            entry::with_runtime(|context| entry::settled(context, value, false))
        }
        _ => {
            let error = error_object("ENOTFOUND", "getaddrinfo", &host);
            entry::with_runtime(|context| entry::settled(context, error, true))
        }
    }
}

/// `options.family`, whether `options` is the `LookupOptions` object shape
/// or the legacy bare-number shorthand (`dns.lookup(host, 4, cb)`) — both
/// are real Node call shapes. `0`/anything else means "either family".
fn wanted_family(options: u64) -> i64 {
    if let Some(number) = entry::number_of(options) {
        return number as i64;
    }
    match option_text(options, "family").as_deref() {
        Some("IPv4") => 4,
        Some("IPv6") => 6,
        _ => option_number(options, "family").unwrap_or(0.0) as i64,
    }
}

/// A boolean member of an options object, `false` when absent or `options`
/// is not an object at all.
fn option_bool(options: u64, name: &str) -> bool {
    let absent = entry::undefined_value();
    if options == absent {
        return false;
    }
    let value = entry::with_runtime(|context| entry::get_member(context, options, name));
    value == entry::boolean_value(true)
}

/// A numeric member of an options object.
fn option_number(options: u64, name: &str) -> Option<f64> {
    let absent = entry::undefined_value();
    if options == absent {
        return None;
    }
    let value = entry::with_runtime(|context| entry::get_member(context, options, name));
    entry::number_of(value)
}

/// A text member of an options object.
fn option_text(options: u64, name: &str) -> Option<String> {
    let absent = entry::undefined_value();
    if options == absent {
        return None;
    }
    let value = entry::with_runtime(|context| entry::get_member(context, options, name));
    entry::text_of(value)
}

/// Resolves a hostname to `(address, family)` pairs via `std::net`, the one
/// honest backend available (see the module doc). `family` is `4`, `6`, or
/// `0` for either.
fn resolve(host: &str, family: i64) -> Option<Vec<(String, i32)>> {
    let target = format!("{host}:0");
    let answered: Vec<SocketAddr> = target.to_socket_addrs().ok()?.collect();
    let filtered: Vec<(String, i32)> = answered
        .into_iter()
        .filter_map(|addr| match addr {
            SocketAddr::V4(v4) if family != 6 => Some((v4.ip().to_string(), 4)),
            SocketAddr::V6(v6) if family != 4 => Some((v6.ip().to_string(), 6)),
            _ => None,
        })
        .collect();
    Some(filtered)
}

/// The `all: true` result — `LookupAddress[]`.
fn lookup_all_array(addrs: &[(String, i32)]) -> u64 {
    entry::with_runtime(|context| {
        let items: Vec<u64> = addrs
            .iter()
            .map(|(address, family)| {
                let object = entry::make_object(context);
                let address_v = entry::make_string(context, address);
                entry::put_member(context, object, "address", address_v);
                let family_v = entry::make_number(f64::from(*family));
                entry::put_member(context, object, "family", family_v);
                object
            })
            .collect();
        entry::make_array_in(context, items)
    })
}

/// The `all: false` (default) promise result — `{ address, family }`.
fn single_result((address, family): &(String, i32)) -> u64 {
    entry::with_runtime(|context| {
        let object = entry::make_object(context);
        let address_v = entry::make_string(context, address);
        entry::put_member(context, object, "address", address_v);
        let family_v = entry::make_number(f64::from(*family));
        entry::put_member(context, object, "family", family_v);
        object
    })
}

/// The plain object a failed lookup answers — see the module doc for why it
/// is not a real `Error`.
fn error_object(code: &str, syscall: &str, hostname: &str) -> u64 {
    entry::with_runtime(|context| {
        let object = entry::make_object(context);
        let code_v = entry::make_string(context, code);
        entry::put_member(context, object, "code", code_v);
        let syscall_v = entry::make_string(context, syscall);
        entry::put_member(context, object, "syscall", syscall_v);
        let host_v = entry::make_string(context, hostname);
        entry::put_member(context, object, "hostname", host_v);
        let message = format!("{syscall} {code} {hostname}");
        let message_v = entry::make_string(context, &message);
        entry::put_member(context, object, "message", message_v);
        object
    })
}

fn string_value(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}

/// `dns.getServers()` / `dnsPromises.getServers()` — see [`DnsState`] for
/// why this never reflects the OS's own configured resolvers.
extern "C" fn get_servers(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
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
extern "C" fn set_servers(_e: u64, _this: u64, servers: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(entries) = array_texts(servers) else {
        return entry::undefined_value();
    };
    for item in &entries {
        if parse_server(item).is_none() {
            return entry::undefined_value();
        }
    }
    state().lock().unwrap().servers = entries;
    entry::undefined_value()
}

/// Whether a server string (`ip`, `ip:port`, `[ipv6]`, `[ipv6]:port`)
/// parses to a real address — the one thing `setServers` validates.
fn parse_server(text: &str) -> Option<IpAddr> {
    let bare = text
        .strip_prefix('[')
        .and_then(|rest| rest.split(']').next())
        .unwrap_or_else(|| text.split(':').next().unwrap_or(text));
    IpAddr::from_str(bare).ok()
}

/// The strings an array-shaped argument holds, `None` if it is not one.
fn array_texts(value: u64) -> Option<Vec<String>> {
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
extern "C" fn set_default_result_order(_e: u64, _this: u64, order: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
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
extern "C" fn get_default_result_order(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    string_value(state().lock().unwrap().order)
}

/// `dns.setLocalAddress(ipv4?, ipv6?)`. Stored, and — see the module doc's
/// "Two resolution paths" — never applied: `lookup` goes through
/// `std::net::ToSocketAddrs`, which has no source-address parameter to hand
/// this to, and the protocol path that would use it is not implemented.
/// Inert bookkeeping, the same posture `set_servers` takes.
extern "C" fn set_local_address(_e: u64, _this: u64, ipv4: u64, ipv6: u64, _a2: u64, _a3: u64) -> u64 {
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
