//! `dns.resolveCname`/`resolveNs`/`resolvePtr`/`reverse` — the four members
//! of this module whose whole answer is a list of hostnames, decoded off a
//! single `Name`-typed RDATA field (`CNAME`/`NS`/`PTR` each wrap exactly one,
//! per `hickory_resolver::proto::rr::rdata::name`'s `name_rdata!` macro).
//!
//! `resolveCname`/`resolvePtr` have no dedicated `hickory-resolver` method —
//! `docs/reference/node/crates.md` §4.5 already says so — so both go through
//! the generic [`hickory_resolver::TokioResolver::lookup`] keyed by
//! `RecordType`; `resolveNs` has one (`ns_lookup`) and uses it. `reverse` is
//! the one member here that is NOT a forward query: it resolves an IP
//! address into PTR records via `reverse_lookup`, which builds the
//! `in-addr.arpa`/`ip6.arpa` name itself (`Name: From<IpAddr>`) rather than
//! taking one as text — `resolvePtr` is the forward form of the same RR type,
//! queried at whatever name string the caller supplies.

use super::client::{node_code, resolver, runtime};
use super::common::{callback_result, error_object};
use hickory_resolver::TokioResolver;
use hickory_resolver::proto::rr::{Name, RData, Record, RecordType};
use rts_core::entry;
use std::net::IpAddr;

/// `dns.resolveCname(hostname, callback)` → `string[]`.
pub(super) extern "C" fn resolve_cname(_e: u64, _this: u64, hostname: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    callback_result(hostname, callback, "queryCname", |host| {
        let resolver = resolver().map_err(|err| node_code(&err))?;
        cname_value(resolver, host)
    })
}

pub(super) fn cname_value(resolver: &TokioResolver, host: &str) -> Result<u64, &'static str> {
    let lookup = runtime().block_on(resolver.lookup(host, RecordType::CNAME)).map_err(|err| node_code(&err))?;
    Ok(name_array(lookup.answers(), |data| match data {
        RData::CNAME(name) => Some(&name.0),
        _ => None,
    }))
}

/// `dns.resolveNs(hostname, callback)` → `string[]` (name-server hostnames).
pub(super) extern "C" fn resolve_ns(_e: u64, _this: u64, hostname: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    callback_result(hostname, callback, "queryNs", |host| {
        let resolver = resolver().map_err(|err| node_code(&err))?;
        ns_value(resolver, host)
    })
}

pub(super) fn ns_value(resolver: &TokioResolver, host: &str) -> Result<u64, &'static str> {
    let lookup = runtime().block_on(resolver.ns_lookup(host)).map_err(|err| node_code(&err))?;
    Ok(name_array(lookup.answers(), |data| match data {
        RData::NS(name) => Some(&name.0),
        _ => None,
    }))
}

/// `dns.resolvePtr(hostname, callback)` → `string[]` — the FORWARD form; see
/// this file's module doc for how it differs from [`reverse`].
pub(super) extern "C" fn resolve_ptr(_e: u64, _this: u64, hostname: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    callback_result(hostname, callback, "queryPtr", |host| {
        let resolver = resolver().map_err(|err| node_code(&err))?;
        ptr_value(resolver, host)
    })
}

pub(super) fn ptr_value(resolver: &TokioResolver, host: &str) -> Result<u64, &'static str> {
    let lookup = runtime().block_on(resolver.lookup(host, RecordType::PTR)).map_err(|err| node_code(&err))?;
    Ok(name_array(lookup.answers(), |data| match data {
        RData::PTR(name) => Some(&name.0),
        _ => None,
    }))
}

/// `dns.reverse(ip, callback)` — reverse (PTR) lookup for a v4/v6 address.
/// Throws synchronously (`ERR_INVALID_IP_ADDRESS`) on a malformed `ip`,
/// matching Node's documented contract; see `crate::errors::invalid_ip_address`
/// for why this module CAN raise a real catchable error here where its own
/// async callback errors stay plain objects (the module doc's "Errors are
/// plain objects" section says why the two do not need to agree).
pub(super) extern "C" fn reverse(_e: u64, _this: u64, ip: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let Some(text) = entry::text_of(ip) else {
        crate::errors::invalid_ip_address("");
        return absent;
    };
    let Ok(address) = text.parse::<IpAddr>() else {
        crate::errors::invalid_ip_address(&text);
        return absent;
    };
    if callback == absent {
        return absent;
    }
    let outcome = resolver().map_err(|err| node_code(&err)).and_then(|resolver| reverse_value(resolver, address));
    match outcome {
        Ok(value) => {
            let null = entry::null_value();
            entry::call(callback, absent, null, value, absent, absent)
        }
        Err(code) => {
            let error = error_object(code, "queryPtr", &text);
            entry::call(callback, absent, error, absent, absent, absent)
        }
    }
}

/// `pub(super)`: `dns.Resolver#reverse` (`resolver_class.rs`) shares this
/// decode over its own resolver instance.
pub(super) fn reverse_value(resolver: &TokioResolver, address: IpAddr) -> Result<u64, &'static str> {
    let lookup = runtime().block_on(resolver.reverse_lookup(address)).map_err(|err| node_code(&err))?;
    Ok(name_array(lookup.answers(), |data| match data {
        RData::PTR(name) => Some(&name.0),
        _ => None,
    }))
}

/// A JS array of hostname strings, extracted from `answers` by `pick` (which
/// answers the wrapped `Name` for the one RDATA variant this call cares
/// about, `None` for anything else — the server chasing a `CNAME` before
/// answering an `NS`/`PTR` query, say). No trailing root dot: real Node's
/// `resolveCname`/`resolveNs`/`resolvePtr`/`reverse` never print one, where
/// `Name::to_utf8()` does for a fully-qualified name.
fn name_array(answers: &[Record], pick: impl Fn(&RData) -> Option<&Name>) -> u64 {
    entry::with_runtime(|context| {
        let items: Vec<u64> = answers
            .iter()
            .filter_map(|record| pick(&record.data))
            .map(|name| {
                let text = name.to_utf8();
                entry::make_string(context, text.trim_end_matches('.'))
            })
            .collect();
        entry::make_array_in(context, items)
    })
}
