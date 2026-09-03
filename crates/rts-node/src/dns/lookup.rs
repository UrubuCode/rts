//! `dns.lookup`/`dnsPromises.lookup` — the OS-facility resolution path. See
//! the module doc's "Two resolution paths" for why this goes through
//! `std::net` rather than the protocol client [`super::resolve`] uses.

use super::common::{error_object, option_bool, option_number, option_text, string_value};
use rts_core::entry;
use std::net::{SocketAddr, ToSocketAddrs};

/// `dns.lookup(hostname, options?, callback)`.
///
/// The last-argument overload real Node accepts: when the 3rd slot is
/// `undefined`, the 2nd slot is the callback and `options` is absent;
/// otherwise the 2nd slot is `options` and the 3rd is the callback. A call
/// with no callback at all does nothing and answers `undefined` — there is
/// nowhere else for a lookup's result to go.
pub(super) extern "C" fn lookup(_e: u64, _this: u64, hostname: u64, arg1: u64, arg2: u64, _a3: u64) -> u64 {
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
pub(super) extern "C" fn promise_lookup(_e: u64, _this: u64, hostname: u64, options: u64, _a2: u64, _a3: u64) -> u64 {
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
