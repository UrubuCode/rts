//! Shared helpers across every native under this module: reading an options
//! object's fields, building the plain-object error shape resolution answers
//! (see the module doc, "Errors are plain objects, not `Error` instances",
//! for why), boxing a plain string, parsing a DNS server address, and the two
//! callback-invocation shapes the record-type resolvers share (added
//! alongside [`super::client`] and the `resolve*` family beyond `resolve4`).

use rts_core::entry;
use std::net::IpAddr;

/// The plain object a failed lookup/resolve answers — see the module doc
/// for why it is not a real `Error`.
pub(super) fn error_object(code: &str, syscall: &str, hostname: &str) -> u64 {
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

/// A JS string built from a Rust one.
pub(super) fn string_value(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}

/// A boolean member of an options object, `false` when absent or `options`
/// is not an object at all.
pub(super) fn option_bool(options: u64, name: &str) -> bool {
    let absent = entry::undefined_value();
    if options == absent {
        return false;
    }
    let value = entry::with_runtime(|context| entry::get_member(context, options, name));
    value == entry::boolean_value(true)
}

/// A numeric member of an options object.
pub(super) fn option_number(options: u64, name: &str) -> Option<f64> {
    let absent = entry::undefined_value();
    if options == absent {
        return None;
    }
    let value = entry::with_runtime(|context| entry::get_member(context, options, name));
    entry::number_of(value)
}

/// A text member of an options object.
pub(super) fn option_text(options: u64, name: &str) -> Option<String> {
    let absent = entry::undefined_value();
    if options == absent {
        return None;
    }
    let value = entry::with_runtime(|context| entry::get_member(context, options, name));
    entry::text_of(value)
}

/// Parses a DNS server string (`ip`, `ip:port`, `[ipv6]`, `[ipv6]:port`) into
/// an address and an optional explicit port. `None` on anything that is not
/// one of those four forms — used both by `state.rs::set_servers`'s
/// validation (port discarded) and [`super::client::build_resolver`] (port
/// kept, for the actual per-server connection a `dns.Resolver` instance
/// opens).
pub(super) fn parse_server_addr(text: &str) -> Option<(IpAddr, Option<u16>)> {
    if let Some(rest) = text.strip_prefix('[') {
        let (host, after) = rest.split_once(']')?;
        let addr: IpAddr = host.parse().ok()?;
        let port = match after.strip_prefix(':') {
            Some(port_text) => Some(port_text.parse::<u16>().ok()?),
            None if after.is_empty() => None,
            None => return None,
        };
        return Some((addr, port));
    }
    // No brackets: a bare IPv4 or IPv6 literal, or `ipv4:port`. A bare IPv6
    // WITH a port must use brackets — `::1:53` already parses as a (different)
    // IPv6 address, so splitting an unbracketed string on `:` would be
    // ambiguous between "the address" and "the address, minus a port".
    if let Ok(addr) = text.parse::<IpAddr>() {
        return Some((addr, None));
    }
    let (host, port) = text.rsplit_once(':')?;
    let addr: std::net::Ipv4Addr = host.parse().ok()?;
    let port: u16 = port.parse().ok()?;
    Some((IpAddr::V4(addr), Some(port)))
}

/// The callback-style native shape every `resolve*(hostname, callback)`
/// member of `node:dns` (and `dns.Resolver`) with no options object shares —
/// `resolveCname`/`Mx`/`Ns`/`Ptr`/`Soa`/`Srv`/`Naptr`/`Caa`/`Tlsa`/`Txt`/`Any`
/// and `reverse`. `resolve4`/`resolve6` keep [`callback_with_ttl_option`]
/// instead: they alone take an options object, so folding the last-argument
/// overload in here would abstract over a shape only two functions have.
///
/// `query` does the lookup and decode; this owns the argument validation and
/// the callback-invocation shape both branches need.
pub(super) fn callback_result(hostname: u64, callback: u64, syscall: &str, query: impl FnOnce(&str) -> Result<u64, &'static str>) -> u64 {
    let absent = entry::undefined_value();
    if callback == absent {
        return absent;
    }
    let Some(host) = entry::text_of(hostname) else {
        let error = error_object("ERR_INVALID_ARG_TYPE", syscall, "");
        return entry::call(callback, absent, error, absent, absent, absent);
    };
    match query(&host) {
        Ok(value) => {
            let null = entry::null_value();
            entry::call(callback, absent, null, value, absent, absent)
        }
        Err(code) => {
            let error = error_object(code, syscall, &host);
            entry::call(callback, absent, error, absent, absent, absent)
        }
    }
}

/// The `(hostname, options?, callback)` shape `resolve4` and `resolve6`
/// share — `options.ttl` toggles between a bare array of address strings and
/// an array of `{address, ttl}` objects. Same last-argument overload
/// [`super::lookup::lookup`] uses: the 3rd slot present means the 2nd slot is
/// `options`, otherwise the 2nd slot is the callback and there are no
/// options.
pub(super) fn callback_with_ttl_option(hostname: u64, arg1: u64, arg2: u64, syscall: &str, query: impl FnOnce(&str, bool) -> Result<u64, &'static str>) -> u64 {
    let absent = entry::undefined_value();
    let (options, callback) = match arg2 == absent {
        true => (absent, arg1),
        false => (arg1, arg2),
    };
    if callback == absent {
        return absent;
    }
    let Some(host) = entry::text_of(hostname) else {
        let error = error_object("ERR_INVALID_ARG_TYPE", syscall, "");
        return entry::call(callback, absent, error, absent, absent, absent);
    };
    let with_ttl = option_bool(options, "ttl");
    match query(&host, with_ttl) {
        Ok(value) => {
            let null = entry::null_value();
            entry::call(callback, absent, null, value, absent, absent)
        }
        Err(code) => {
            let error = error_object(code, syscall, &host);
            entry::call(callback, absent, error, absent, absent, absent)
        }
    }
}
